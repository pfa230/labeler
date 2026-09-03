## Why

Implements [#213](https://github.com/pfa230/labeler/issues/213).

Nothing in the template model holds more than one value.
`ParamType` has no plural, `ParamValue` has no sequence variant, and interpolation is substitution-only,
so a `data` value that is a JSON array falls to `value_to_string`'s catch-all and prints as raw JSON:
`{tags}` renders `["CONSUMABLE","KIDS","SPARES"]` (`src/render/helpers.rs:29-36`). There is no join, no
error, and no way for the count of anything on a label to be decided by the data rather than by the
author.

This change adds the **engine contract for a list-valued parameter and nothing else**: the type, the
value model, and what every *existing* consumer does when it meets a list. That is what unblocks the
repetition construct (#319), which is the piece the tag strip actually waits on. The form control
(#318), the CSV spelling (#320) and the connector column (#321) are filed separately and are out of
scope here.

## What Changes

**The type.** A `params:` entry may declare `type: list`. Its value is an ordered list of **strings**,
and only of strings: a `default:` element that is not a YAML string scalar, and a request element that is
not a JSON string, are both refused. An author wanting numbers writes `["1", "2"]`, which joins
identically. `default:` is a YAML sequence resolved by the same rules as every other type;
`description:` behaves as elsewhere. `min`, `max`, `multiline`, `values`, `format` and `time` are refused
at load, naming both the parameter and the offending attribute, turning on the key being *written* (an
explicit YAML null included) exactly as `datetime-params` already refuses its own forbidden set. There is
no element-type key and no `values`: the only thing that reads a list is a join into text. `[]` is
present and empty, never absent.

**Reading one.** The reader position after the colon gains an optional parenthesized argument, and a
reader carrying one is a join: `"{tags:join(', ')}"`. The separator is a single-quoted literal, because
the value is double-quoted in YAML. **The argument is what makes a token a join, not the word.** A bare
`join` with no argument stays an ordinary format name resolved from `datetime_formats`, so
`{sys.now:join}` behaves exactly as it does today and no stored setting is stranded. No word is reserved
in either position.

**Six refusals**, as #213 assigns them. The last row is widened by one case the issue's list did not
reach: an `image` item's `name:`, which binds a `data` field with no token and is a scalar slot like any
other.

| What | When |
| --- | --- |
| `{tags}` on a declared list | Load, quarantining the file, naming the token |
| An array under an undeclared `data` key reaching a scalar slot | Render, `422` naming the field |
| A join on a value the template does not declare as a list | Load, naming the token |
| `{tags:join}` with no argument | Load, naming the token |
| A `when:` key naming a declared list | Load, naming the key and the item's layout path |
| A numeric or colour `ref:`, or an `image` `name:`, naming a list | Load, naming the parameter and the context |

`when` gains **no** `contains` operator: refusing a `when` over a list rather than quietly never matching
is what keeps that door open at no cost, because a refusal is additive to relax later while
never-matching would silently flip live templates from hidden to shown.

**Scalar slots are more than tokens.** An `image` item's `name:` binds a `data` field directly rather
than through a token, and stringifies it during measurement and rendering alike
(`src/render/mod.rs:1636-1642`, `:2120-2125`). Naming a declared `list` there is refused at load with the
other `ref:` refusals; an array arriving under an undeclared name it binds gets the same render-time
refusal a token does, decided before the data-URI parse it would otherwise fail inside.

**Reported inputs.** A `list` parameter is reported **like every other parameter**: `InputControl`
gains a `list` variant and the input list carries an ordinary `InputSpec` for it, with the name, the
control, whether a value is required, and the resolved `default` when there is one, which for a `list`
is an array of strings. The template-detail response follows: `params` may report `type: "list"`, and
`param_defaults` may carry an array-valued `resolved`.

This reverses an earlier draft of #213, which said a list produces no `InputSpec` so the UI stayed
untouched. That exception cost six consecutive plan-review rounds, because the corpus assumes every
readable parameter has an entry: the thumbnail invents a placeholder for every token-read parameter, the
CSV Import screen offers every parameter a template can read, one derivation serves the thumbnail and
the catalog index, and `param_defaults` projects each resolved default into `inputs.default` and
`inputs.all`. Each round found another one. The uniform rule leaves all four holding **unchanged**, and
this change's `template-inputs` delta falls from six `MODIFIED` requirements to two.

**What this change owes `template-inputs` is therefore two things**: a `list` row in the control table
of "An input list describes the controls one label needs", and a `list` fill in the invention table of
"The thumbnail renders the default selection from placeholder data". The fill is a one-element list
holding the parameter's own name, which is the `text` rule applied to a type with no other sensible one,
so `{tags:join(', ')}` renders `tags` on a thumbnail.

**Out of scope, and #318 owns it:** the screens that draw the new control. Until #318 lands a client is
told a `list` input exists and has no editor for it, so a screen SHALL skip a control it cannot draw
without failing. That tolerance is the one UI-visible obligation this change carries. The consequence
behind it is the one #213 named all along: a print screen for a template reading a `list` submits
without it and gets `422 MissingField` naming a field it showed no control for, and a resolvable
`default:`, `default: []` included, avoids that.

`param-resolution`'s preview requirement is **not** modified. Its rule already requires a placeholder
for every parameter a token reads that the service has no value for, so a `list` is covered without
amendment, and only the invention table moves. A draft of this plan modified it anyway, which meant
republishing a clause that contradicts the service, `template-inputs` and `datetime-params` alike; that
defect is now **#325** and `design.md` records why this change leaves it alone.

**BREAKING**: `{tags}` over a JSON array prints `["CONSUMABLE","KIDS"]` today. After this it does not
render at all: a load refusal where the parameter is declared, a `422` where it is not. There is no
second spelling, no desugaring and no deprecation window.

**BREAKING**: a `string` parameter supplied a JSON array is refused rather than stringified into JSON
text. It is the only declared type whose request behaviour changes here; `boolean`, `integer`, `number`,
`length`, `enum` and `datetime` already refuse an array, and this change pins each with a test rather
than widening the shared coercion path.

**BREAKING**: a `default:` written as a YAML sequence on any type but `list` is refused at load rather
than held as the sequence's debug text. Refusing an array from a request while accepting one from a
declaration would break `param-resolution`'s rule that a default may not carry a value the request could
not have carried, so this change closes both halves or neither. That refusal is written in
`param-resolution`, which owns the sentence it qualifies, not in `list-params`.

### One clause #213 withdrew, and what survives of it

**There is no undeclared `when:` case to state.** An earlier draft of #213 said an array under an
undeclared key named by a `when:` makes that predicate false. That is withdrawn in the issue:
`validate_when_references` already refuses **every** `when:` key absent from `params:` at load
(`src/templates.rs:1403-1419`), reached for every item type through `validate()` (`:1107-1108`), so no
template carrying one ever loads and no request reaches that comparison. This change therefore restates
that existing refusal in the `conditional-visibility` delta, giving it a spec home for the first time,
and writes **no** rule for a state no template can reach. Whether an undeclared key should be legal at
all is #322's question, and #322 argues to extend this rule rather than relax it, so relaxing it inside
#213 would have been undoing what #322 sets out to do. What survives is the part that was never in
doubt: **a `when:` key naming a declared `list` is refused at load**, which alone serves the stated
purpose of keeping the door open for a future `contains`.

### The three things #213 left "for the change to settle"

- **Escaping a quote inside the separator: there is none, and a quote is a load refusal.** The literal
  runs from the first `'` after the `(` to the next `'`; a third before the `)` is refused, naming the
  token. Doubling is not a spelling. Relaxing this later turns a refusal into an accepted value, which
  is additive, whereas picking an escape now fixes a spelling nothing has asked for.
- **Whether `:` or `}` may appear inside it: `:` may, neither brace may, and a brace is not always
  refused at load.** The colon needs no rule of its own once the token is parsed by structure rather
  than by counting colons. Neither brace can be written, but *when* that is said follows from
  `scan_tokens`, which this change does not touch: a `}` inside a separator closes the token early, so
  the malformed token is refused at load, while a doubled `{{` makes the scanner produce no token at all
  and the string is then refused by the unmatched-brace rule already governing it, at load for a
  `default:` and at render as `400 InvalidRequest` with `interpolation_syntax` for a `text`, `qr` or
  `image src` value. Nothing prints either way. Making both load-time would mean extending brace
  checking to those three sites, which a standing `interpolation-tokens` requirement explicitly declines
  to do.
- **Whether an argument is admitted on a format name generally or only for `join`: neither, exactly.**
  The grammar admits an argument only after the word `join`, and it is the *argument* rather than the
  word that makes a token a join. A bare `join` stays an ordinary format name, so `{sys.now:join}` still
  resolves a `datetime_formats` entry of that name and no stored operator setting is stranded, which
  reserving the word would have done.

One decision #213 did not leave open is also called out, because it changes the contract a reviewer is
gating:

- **The undeclared-array refusal is `422 UnsupportedLayoutItem` with `details.reason`
  `field_value_not_scalar`.** #213 specifies `422` without naming a code; this is the only 422 code whose
  reason vocabulary already covers "the caller's `data` value is one this item cannot render"
  (`image_data_invalid`, `image_format_unsupported`). The alternative was `400 InvalidRequest`, which is
  what the declared-parameter path uses, and which would have contradicted #213's `422`.

## Capabilities

### New Capabilities

- `list-params`: the `list` parameter type: how a template declares one, which attributes it refuses,
  what a request may send for it, and where a list may not be used.

### Modified Capabilities

- `interpolation-tokens`: the reader position gains an optional argument and the join it spells, so the
  requirement "A colon attaches a format name, and only an instant takes one" is replaced; the
  JSON-scalar stringification rule stops printing an array as JSON text, on a token and on an `image`
  `name:` alike; and the bare-name requirement's clauses are restated around the new parens.

  That replacement is a `REMOVED` plus an `ADDED` under a new name rather than a `MODIFIED`, and the
  distinction #213 draws still holds: first-touch does not apply, because the requirement already lives
  in `openspec/specs/` and nothing here supersedes a frozen `docs/SPEC.md` section. What forces the pair
  is the **title**. A colon no longer attaches only a format name, so a `MODIFIED` would leave a
  requirement whose name contradicts its own body, and a rename inside a `MODIFIED` is not how archive
  resolves one: it locates a requirement by name. `datetime-params` set this precedent, replacing "A
  template declares a datetime parameter as an instant, not a rendering" the same way. The other two
  `interpolation-tokens` requirements keep their names and are ordinary `MODIFIED` deltas.
- `datetime-params`: its requirement "A datetime parameter names an instant, not a rendering" owns the
  complete parameter-type table, so the `list` row is added there.
- `conditional-visibility`: a `when:` key naming a declared list is refused at load; the existing
  refusals this capability never wrote down (an undeclared key, an empty map, a blank key or value, the
  scalar forms a condition may take, an `enum` literal outside its `values`) are stated for the first
  time, unchanged; and the requirement supersedes exactly one bullet of frozen `docs/SPEC.md` §5,
  claiming nothing about branch parity between the paths that evaluate a gate.
- `template-inputs`: two requirements. "An input list describes the controls one label needs" gains the
  `list` control, and the obligation on a screen to skip a control it cannot draw; "The thumbnail
  renders the default selection from placeholder data" gains the `list` fill. Its other requirements,
  including the lenient-resolution path, the one-derivation rule, the CSV Import screen and the
  template-detail report, hold **unchanged** under the uniform rule and are deliberately not restated:
  an unnecessary `MODIFIED` republishes clauses nobody reviewed a reason to touch.
- `param-resolution`: one requirement, the one owning how a declared default is resolved. It says a
  non-string default is "used as written" and that a literal default keeps the load-time checks it has
  today, both of which the sequence-default refusal qualifies, so that refusal is written there rather
  than in `list-params`. Its **preview** requirement is deliberately not modified; see below.

## Impact

- **Template model**: `raw.rs` (`RawParamType`, `RawParamSpec` refusals), `models.rs`
  (`ParamType::List`, `ParamValue::List`), `convert.rs` (`TryFrom<RawParamSpec>`, `convert_raw_default`)
  (the three files a new parameter type always moves together).
- **Token grammar**: `interpolation.rs` (`parse` becomes structural rather than colon-counting; a
  `Reader` replaces `format: Option<&str>`), and every caller that reads `Token::format`. `scan_tokens`
  is unchanged.
- **Load-time validation**: `templates.rs` (`validate_param_spec`, `validate_interpolated_string`,
  `validate_when_references`, `check_param_ref`, the `image` `name:` check) and the input-list
  derivation, which gains a `list` arm alongside the other declared types. `validate_item_references`
  gains a layout path it threads to its children, because two of the new refusals name one and nothing
  at this stage of validation carries one today; no existing message is changed. `placeholder_data`
  gains a `list` arm in its match over `InputControl` and needs nothing else, because a `list` now has
  an entry to be filled by.
- **Render**: `render/helpers.rs` (`interpolate`, `value_to_string`'s consumers), `render/mod.rs`
  (`coerce_param_value`, `is_item_active`, both `image` `name:` bindings, and `json_to_param_value`,
  whose wildcard arm would otherwise publish a list default as the string `["CONSUMABLE"]` without
  failing to compile).
- **API**: `ParamType`, `ParamValue` and `InputControl` are exposed, so `src/openapi.rs` carries the new
  variants; one new `details.reason` slug, `field_value_not_scalar`, whose documented home is this
  change's delta. `LabelInput.data` is already `HashMap<String, serde_json::Value>`
  (`src/models.rs:1235`), so the request envelope is unchanged.
- **UI**: `ui/src/api/types.ts` only, where `ParamValue` gains `string[]`, `ParamSpec["type"]` gains
  `"list"`, and the `InputControl` union gains `"list"` (`:7`, `:21`, `:30-39`), because the API now
  returns all three and those declarations are the checked-in statement of what it returns. The template
  detail page needs nothing: it already renders a declared default with `String(spec.default)`
  (`ui/src/pages/TemplateDetail.tsx:306`), which JavaScript renders for an array as its elements
  separated by commas, so a list default is legible there unchanged.

  **Components do change**, and the `template-inputs` delta is why: a client is told a `list` input
  exists and has no editor for it, so it SHALL tolerate a control it cannot draw. Left alone,
  `ParamInput.tsx` falls through to a plain text input for any unrecognised control, which would send a
  value guaranteed a `400`. Tasks 6.2 to 6.4 therefore reach `ParamInput`, `PrintForm`, `FieldForm`,
  `Import`, `Connect`, `LabelGrid` and `labelInputs.ts`, each to skip a `list` rather than draw it. The
  editor itself is #318.
