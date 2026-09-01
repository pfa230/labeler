## Context

See proposal.md — Why. What the code does today, since the change is one trim in the right place and
one comment that currently argues for the opposite:

- `Color` (`src/models.rs:853`) is `{ spelling: String, rgba: [u8; 4] }`. `Color::from_str`
  (`src/models.rs:893`) is the single parser: it refuses `""` first, then reads a `#`-prefixed hex
  string or one of the sixteen names, and stores `s.to_string()` as the spelling. It does not trim.
- `spelling` is read at exactly one non-test site, the `Serialize` impl at `src/models.rs:992`, which
  is what `GET /templates/{id}` reports. Rendering never reads it: the emitters go through `rgba()`
  and `hex()`.
- A literal reaches that parser from three call sites in `convert.rs`, all spelled
  `raw_color.0.parse::<Color>()`: `:45` (`stroke.color`), `:265` (`background`), `:347`
  (`text.color`). Each maps the error into `TemplateError::Validation` with the field as its path,
  which is what quarantines the template and names the file, the layout path and the field.
- A reference reaches the model through `DynamicValueVisitor::visit_str` (`src/models.rs:345`), which
  trims, tests `{...}`, and returns `DynamicValue::Ref` holding the inner name trimmed as well. So
  `" {brand} "` already loads, and it already reads back canonically as `"{brand}"` because
  `DynamicValue`'s serializer rebuilds the braces from the stored name (`src/models.rs:286`).
- `RawColor` (`src/raw.rs:10`) exists to carry the untrimmed literal to `convert.rs`. Its `FromStr`
  fails unconditionally on purpose (`src/raw.rs:34-44`) so that the visitor's `trimmed.parse::<T>()`
  fast path cannot fire and the untrimmed string survives into `RawColor::deserialize`.
- What that string is bounds what any read-back can promise. `RawColor::visit_str` (`src/raw.rs:23`)
  receives the **decoded** YAML scalar, so quoting style, escape sequences and scalar style are
  already gone before the service sees a colour: a field written with a YAML escape,
  `color: "\u0072ed"`, arrives as the three characters `red`, and nothing downstream could
  report it as anything else. What survives to the serializer is the decoded scalar's content —
  surrounding whitespace, name case, hex digit count — and that is the whole of what this change
  preserves. The file as written stays available through `GET /templates/{id}/source`.
- A resolved parameter reaches the parser from `resolve_dynamic_value_color`
  (`src/render/helpers.rs:223-251`): it tests `s.starts_with('{') && s.ends_with('}')` for a chained
  reference and then calls `s.parse::<Color>()`, both on the untrimmed value.
- Two shipped tests assert the refusal this change removes: `invalid_colour_strings_are_rejected`
  (`src/models.rs:1430`) lists `" red "` and `" #ff0000 "`, and
  `color_param_with_whitespace_is_rejected_at_render_time` (`src/render/mod.rs:8951`) asserts
  `color_param_invalid` for the same two values supplied as a parameter.

## Goals / Non-Goals

**Goals:**

- One place decides that a colour's surrounding whitespace is insignificant, reached by every path
  that reads a colour, so the load-time and render-time answers cannot drift.
- The declared string still reaches the serializer untrimmed, so `GET /templates/{id}` keeps
  reporting the colour with its surrounding whitespace, case and hex digit count intact.
- The comment that currently documents the opposite intent is corrected, because the next reader of
  `src/raw.rs:34-44` would otherwise conclude the hack is dead and delete it.

**Non-Goals:**

- No change to `DynamicValueVisitor`. It is generic over every `DynamicValue<T>`, so touching its trim
  moves `layout-sizing`, `interpolation-tokens` and `font_size` as well. It already does the right
  thing for the reference form.
- No change to what a colour may be, to the name table, or to any field's absence or default.
- No unification of the two read-back forms. A padded literal reports its padding and a padded
  reference reports `"{brand}"`; the spec states both rather than moving either.
- No new error code, reason or message shape. `color_param_invalid` keeps both its messages.

## Decisions

### The trim lives inside `Color::from_str`

`Color::from_str` captures the argument before it trims, and the two bindings are kept distinct for
the rest of the function:

1. `let spelling = s.to_owned();` — the string the template declared, padding included.
2. `let value = s.trim();` — what the colour *is*. Every subsequent step reads this binding and only
   this one: the empty check, the `#` test and hex-digit walk, the name table, and every error
   message it formats.
3. Both success paths construct `Color { spelling, rgba }` from the captured `spelling`. Today they
   build it from the argument (`src/models.rs:951` in the hex branch, `:977` in the name branch), so
   each is one substitution.

Shadowing the argument with `let s = s.trim()` would be wrong, and is the obvious way to write this:
after the shadow the untrimmed value is unreachable, both constructors would store the trimmed
string, and `color: " red "` would read back as `"red"` against the read-back requirement. The
capture must come first for that reason, not for style.

One edit then covers both sites the issue names as untrimmed, because both reach it: the three
`convert.rs` call sites and `resolve_dynamic_value_color`.
It also covers `Color`'s `Deserialize` impl (`src/models.rs:1016`), which is itself `v.parse::<Color>()`
— no non-test path deserializes a `Color` today, but a future one inherits the rule rather than
rediscovering it.

Alternative considered: trim at each call site, leaving `Color::from_str` strict. Rejected because it
is four places to remember and a fifth (`ColorVisitor`) that would be missed, to buy a strictness
nothing wants. The house pattern is the opposite one: `resolver.rs` keeps one classifier so
load-time and render-time cannot disagree, and this is the same shape of problem — #291 shipped this
regression precisely because the two paths trimmed differently.

Consequence worth naming: `Color::from_str`'s error messages quote `s` today, and they are re-pointed
at the trimmed binding along with everything else that judges the value. `" octarine "` is therefore
reported as `unknown colour 'octarine'`. That is the value the service actually judged, and quoting the padded form would invite the reader to think the padding
was the fault, which is the confusion this change exists to remove. No requirement constrains the
message text beyond naming the offending value and, for a render failure, the parameter.

### The empty check runs after the trim, not before it

`"   "` must be refused exactly as `""` is, and the cheapest way to get that is ordering: trim, then
the existing `is_empty()` guard, which then fires with its existing message. Nothing else is needed
and no new error variant appears. Ordering it the other way would let `"   "` fall through to the
name table and be reported as `unknown colour '   '`, a worse message for the same refusal.

`parse_datetime_in_tz` (`src/datetime_fmt.rs:38-41`) already does exactly this — trim, then refuse
the empty string — so the ordering is the codebase's, not this change's.

### `RawColor`'s deliberately-failing `FromStr` survives, and its comment is rewritten

The comment at `src/raw.rs:34-44` gives the hack's purpose as letting `convert.rs` and
`Color::from_str` "strictly reject whitespace at load time". That purpose is exactly what this change
deletes, so a reader who trusts the comment will delete the hack with it — and doing so silently
drops the padding from the read-back, because the visitor's `trimmed.parse::<T>()` fast path would
then succeed and build the `RawColor` from the *trimmed* string, so `color: " red "` would read back
as `"red"`. No test between here and the API response would catch that without one written for it.

So: keep the code, replace the reason. The hack survives to carry the declared string, padding
included, from the deserializer to the model — which is the second reason it always served and now
the only one. This is the change's main trap and it is
handled with a comment plus the read-back test below, not with a comment alone.

### `resolve_dynamic_value_color` trims once, at the top

The trim inside `Color::from_str` fixes the parse but not the chained-reference test that runs before
it, so `" {other} "` would reach the parser and fail as an unrecognised colour rather than as a
chained reference. The helper therefore binds the trimmed value once, immediately after it has a
string, and every subsequent test reads that binding: the `{...}` test, the parse, and the
`unrecognised colour '{s}'` message alike. Trimming twice (once here, once in the parser) is
harmless and is not worth a special-cased entry point to avoid.

### Whitespace is `str::trim`, deliberately

`str::trim` is Unicode `White_Space`, which is what `DynamicValueVisitor`, `SizeValue`'s visitor and
`parse_datetime_override` already apply. A colour therefore strips exactly what a size or a datetime
override strips, including a non-breaking space and a stray newline from a YAML block scalar. An
ASCII-only variant was rejected on the requirement's own terms: the point of this change is that
colour stops having a rule of its own, and a second whitespace definition would reintroduce one at a
smaller scale.

Note that the rule is only observable on a quoted scalar or a parameter value: YAML strips the
padding around an unquoted `color: red` before the service sees it.

### Tests: one per site, at the layer that proves it

Three sites where a colour is read and two forms it is reported in. Which layer each is asserted at
is the point, because a claim proved one layer below the one that carries it is not proved:

- **Literal at load** — a template writing `color: " red "`, `background: " #F0F "` and
  `stroke.color: " navy "` loads and its converted model carries the right `rgba` for each. This is
  the `convert.rs` layer, where the three call sites are.
- **Literal at render** — the same three padded fields, carried through to the emitted paint.
  "Loads and paints the right colour" is two claims, and the conversion test proves only the first:
  a model holding the right `rgba` still says nothing about what reaches the page. So the parsed
  template's layout items go through the source-emitting path the colour tests already use
  (`render_test_items`, `src/render/mod.rs:2349`, as `emitted_typst_source_color_fill_and_omission`
  at `:8797` does), and the emitted markup is asserted to carry `fill: rgb("#ff0000ff")` for the
  padded `text.color`, `fill: rgb("#ff00ffff")` for the padded `background` and
  `stroke: ... rgb("#000080ff")` for the padded `stroke.color`. All three emit through `Color::hex()`
  (`src/render/mod.rs:2015`, `:2247`, `:2255`), so the expected strings are exact rather than
  approximate.
- **Reference at load** — `color: " {brand} "` still loads as a reference to `brand` and still
  renders. Unchanged behaviour, pinned because the `RawColor` hack sits between it and the parser.
- **Resolved parameter at render** — `brand: " navy "` renders and paints `#000080`, replacing
  `color_param_with_whitespace_is_rejected_at_render_time`, and `brand: " {other} "` fails with
  `color_param_invalid` carrying the chained-reference message rather than the unrecognised-colour
  one. Asserting the message, not just the reason, is what distinguishes the fixed behaviour from the
  bug.
- **Read-back** — one HTTP test on `GET /templates/{id}`, over a template carrying both forms: a
  `text` item written `color: " red "`, whose response must report `" red "` with its padding, and a
  container written `background: " {brand} "`, whose response must report `"{brand}"`. Both halves
  are needed because the delta states both, and they differ: a literal keeps the string it was
  declared with and a reference is reported canonically. This has to be the HTTP layer, because what
  is preserved is a property of the serializer's output; a unit test on `spelling()` would pass
  against a model that never received the padding.
- **Refusals** — `"   "` joins `invalid_colour_strings_are_rejected` as `" red "` and `" #ff0000 "`
  leave it, plus a load-level test that a template writing `color: "   "` is quarantined with an
  error naming the file, the layout path and the field.

## Risks / Trade-offs

- **The `RawColor` hack is deleted by a later reader as dead code, and the read-back silently loses
  padding.** → The comment states the surviving reason, and the HTTP read-back test fails loudly if
  the hack goes.
- **The two rules read as a contradiction: whitespace is insignificant, yet it is reported.** → Both
  the requirement and this design state the distinction between a colour's identity and the string the
  template declared, with the `#F0F` precedent that already settles it. A reviewer who reaches
  for "normalize the spelling instead" gets the counter-argument in the spec: the same reasoning would
  license reporting `#F0F` as `#ff00ff`, which the requirement forbids.
- **Trimming in `Color::from_str` widens the rule to `Color::deserialize`.** → Intended, and inert
  today: no non-test path deserializes a `Color` (templates arrive as YAML through `RawColor`, and
  `LayoutItem` derives `Serialize` only).
- **A template that was quarantined for a padded colour starts loading.** → That is the fix. It can
  only turn a refusal into a render, never the reverse, so no template that loads today stops
  loading.

## Migration Plan

None. The change restores the behaviour that shipped before `54fc07f`, no template or fixture in the
repository writes a padded colour, and no persisted state encodes the old refusal. `text-ink` gains no
migration note, because a break that was never intended and never announced is a regression rather
than a contract that moved.
