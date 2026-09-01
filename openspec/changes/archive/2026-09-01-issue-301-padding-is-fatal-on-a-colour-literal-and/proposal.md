## Why

Issue [#301](https://github.com/pfa230/labeler/issues/301). Inside one field, the two forms a colour
may take disagree about surrounding whitespace. `color: " red "` is refused and quarantines the
template; `color: " {brand} "` loads and resolves. Nothing states which is intended, and the
disagreement is an accident of how the two forms reach their parsers: `DynamicValueVisitor::visit_str`
trims before it tests the reference form (`src/models.rs:345-352`), while a literal is handed to
`Color::from_str` untrimmed and `Color::from_str` does not trim (`src/models.rs:893`).

The refusal is also new. On the `main` before `54fc07f` (#291), `ink: " red "` loaded: the bespoke
visitor at `src/raw.rs:223` trimmed before parsing, and `resolve_dynamic_value_ink` trimmed a resolved
parameter at render. #291 named three template-facing breaks and this was not among them; the only
place it is written down is `openspec/changes/archive/2026-09-01-issue-291-one-colour-type/diff-review-7.md`,
a review artifact rather than a contract.

## What Changes

- **Surrounding whitespace is never significant in a colour.** It is stripped before a colour is read,
  and the rule holds identically on a literal, on a `{param}` reference, and on a parameter value
  resolved at render. `color: " red "`, `background: " #F0F "` and `stroke.color: " navy "` load and
  paint the colour they name; a render supplying `brand: " navy "` resolves.
- This makes colour stop being the outlier rather than introducing a new rule. Every other dynamic
  value already strips it: `DynamicValueVisitor::visit_str` trims for every `DynamicValue<T>` before
  every branch, so `" 80mm "` as a length literal already loads; `SizeValue`'s visitor trims before
  matching `content`, `fill` and `auto` (`src/raw.rs:351-358`); `parse_datetime_in_tz`, which
  `parse_datetime_override` delegates to, trims and only then refuses an empty string
  (`src/datetime_fmt.rs:38-41`).
- **This is a bug fix, not a break.** It restores the pre-`54fc07f` behaviour, so `text-ink` gains no
  migration note and the shipped refusal is recorded as a regression.
- **The as-authored read-back keeps the declared string, padding included.** `color: " red "` reads
  back through `GET /templates/{id}` as `" red "`. What is preserved is the content of the decoded
  YAML scalar — surrounding whitespace, name case, hex digit count — and not the file's YAML
  spelling: quoting, escapes and scalar style are gone before the service sees a colour, and
  `GET /templates/{id}/source` is where the file as written stays available. Whitespace
  insignificance is a rule about a colour's *identity* — `" red "` and `"red"` are one colour and both
  paint `#ff0000`. `spelling` is not identity; it is the string the template declared, and it already
  preserves distinctions that do not affect identity, name case (`Red`) and hex digit count (`#F0F`).
  Padding is one more of those.
  A *reference* carries no such record and keeps reading back canonically: `background: " {brand} "`
  reports `"{brand}"`, which is what it reports today. The delta states that asymmetry rather than
  moving either form.
- **All-whitespace is still refused.** `color: "   "` trims to empty and is refused exactly as
  `color: ""` already is, quarantining its template and naming the file, the layout path and the field.
- **A resolved chained reference stays a chained reference.** `resolve_dynamic_value_color` tests
  `s.starts_with('{') && s.ends_with('}')` on the untrimmed value today, so a resolved `" {other} "`
  would fall through to `unrecognised colour`. It is trimmed first and refused with
  `color_param_invalid` and the chained-reference message.

Not changed: which forms a colour may take, the sixteen names and their values, whether whitespace
*inside* a colour is significant (it is: `"re d"` and `"# f0f"` stay refused), any field's absence
semantics or default, which keys appear in a read-back, or how a colour reaches either output path.

## Capabilities

### New Capabilities

None. This change states a rule about a colour that `colour-vocabulary` already owns.

### Modified Capabilities

- `colour-vocabulary`: three requirements gain the whitespace rule. "A colour is a name, a hex string,
  or a parameter reference" states that surrounding whitespace is stripped before a colour is read and
  that an all-whitespace colour is refused as the empty string is. "Every field that takes a colour
  takes a parameter reference" states that a value resolved at render is trimmed before it is read,
  and before it is tested for the chained-reference form. "A colour is reported as authored wherever a
  template is read back" states what "as authored" preserves and what it does not: the decoded YAML
  scalar's content, including surrounding whitespace as it already includes name case and hex digit
  count, but not the file's quoting, escapes or scalar style.

`text-ink` and `shape-paint` are untouched. This changes neither field's absence semantics, its
default, nor which keys appear in a read-back.

## Impact

- `src/models.rs`: `Color::from_str` (`src/models.rs:893`) trims before it parses and keeps the
  argument as given as the `spelling`. The empty-string guard is ordered after the trim so `"   "`
  is refused. This one edit covers both the literal at load and the resolved parameter at render,
  because both reach it.
- `src/render/helpers.rs`: `resolve_dynamic_value_color` (`src/render/helpers.rs:223-251`) trims the
  resolved string before the `{...}` chained-reference test.
- `src/raw.rs`: no behaviour change, but the doc comment on `RawColor`'s deliberately-failing
  `FromStr` (`src/raw.rs:34-44`) states its purpose as letting `convert.rs` and `Color::from_str`
  "strictly reject whitespace at load time". That reason is gone; the hack survives for the other
  reason it already serves, carrying the untrimmed declared string to the model so the read-back
  keeps its padding, and the comment says so.
- `src/convert.rs`: unchanged. Its three `raw_color.0.parse::<Color>()` call sites (`:45` stroke,
  `:265` background, `:347` text) inherit the trim.
- Tests: two shipped assertions invert. `invalid_colour_strings_are_rejected`
  (`src/models.rs:1430`) lists `" red "` and `" #ff0000 "` among the refusals, and
  `color_param_with_whitespace_is_rejected_at_render_time` (`src/render/mod.rs:8951`) asserts the
  render-time refusal. Both are replaced by tests asserting the new rule, and `"   "` joins the
  refusal list.
- API: no endpoint, schema or error code moves. The read-back rule itself is unchanged, and a padded
  literal is new only in that its template now loads and so has something to be reported from.
  `color_param_invalid` keeps its meaning and both its messages.
- `docs/AUTHORING.md`: its colour paragraph (`docs/AUTHORING.md:500-506`) gains one clause saying
  surrounding whitespace is ignored.
- No shipped template under `catalog/` or fixture under `tests/fixtures/templates/` writes a padded
  colour, so nothing needs migrating.
