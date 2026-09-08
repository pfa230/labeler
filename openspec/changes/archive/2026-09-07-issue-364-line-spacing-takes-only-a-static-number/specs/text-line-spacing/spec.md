## MODIFIED Requirements

### Requirement: A text item's line pitch is authored as `line_spacing`

A `text` layout item SHALL accept exactly the following fields, and no others: `value`, an interpolated string; placement `at` (default `[0, 0]` on non-packed items; a packed child carries neither `at` nor `to` and is positioned by its flow container under `flow-layout`), exactly one of `size` or `to`, and `max_w` / `max_h` bounds; `font_size`, either a fixed number or a `{ min, max }` range that auto-shrinks in 0.5pt steps and truncates with an ellipsis on overflow; `font_weight`, either a literal multiple of 100 between 100 and 900 or a `"{param}"` reference to an integer parameter resolving to such a value, defaulting to 400 when omitted; `color`, the glyph foreground colour defaulting to black; `wrap`, defaulting to `false`; `line_spacing`, the baseline-to-baseline distance of its lines as a multiple of the font size, either a bare number or a `"{param}"` reference resolved per render; `alignment`, with `horizontal` of left/center/right and `vertical` of top/center/bottom defaulting to `top`; `overflow`, of `ellipsis` (the default) or `fail`; and `when`, the conditional rendering gate map.

An item declaring no `line_spacing` SHALL be laid out and rendered at 1.2, identically to an item declaring `line_spacing: 1.2`. `line_spacing: 0.99` puts baselines 0.99 font sizes apart.

**Two spellings, and no third.** The value SHALL be either a bare number or a reference to a declared parameter written as the single-brace string `"{name}"`, the spelling `font_weight`, `color` and every dynamic dimension already use. Surrounding whitespace inside the braces is ignored, so `"{ pitch }"` and `"{pitch}"` name the same parameter. Every other value SHALL be refused before serving, with an error naming the file and the key: a boolean, an object, an array, an explicit null, a numeric string (`"1.2"`), a string carrying a unit (`"1.2em"`, `"1.2mm"`), and a doubled-brace string (`"{{ pitch }}"`). A doubled brace is not a second reference spelling and never was one: `interpolation-tokens` writes a token with single braces and gives `{{` and `}}` as literal braces instead, and `line_spacing` is not an interpolated string in any case. `"{{ pitch }}"` therefore falls outside the two spellings above and SHALL be refused like any other string.

**A bare number is bounded at load.** A literal SHALL be finite and greater than zero. A zero, negative, NaN or infinite literal SHALL be refused at load, naming the item's layout path and the key.

**A reference is checked for existence and type at load, and its value is not.** The referenced parameter SHALL be declared, and SHALL be of type `number` or `integer`. A reference to an undeclared parameter, or to a parameter of type `length`, `string`, `boolean`, `enum`, `datetime` or `list`, SHALL be refused at load with an error naming the layout path and the `line_spacing` key. `length` is refused deliberately, and this is narrower than the `length`/`number`/`integer` set the sibling numeric references accept: pitch is a unitless multiple of the font size, so a parameter carrying mm or in has no meaning here. Nothing about a reference's *value* is checked at load, including any `default` the parameter declares: a declared default that is zero, negative or non-finite SHALL NOT quarantine the template, and is answered at render under the requirement below.

**A referenced parameter is an input like any other.** A parameter named only by a `line_spacing` reference SHALL appear in the template's input list, with `interpolated` false, because `line_spacing` is a layout attribute and not a value the label prints. `template-inputs` owns that rule and is unchanged by this requirement: it already requires an entry for a name read only by a layout attribute, and gates the list on the same active-item walk every other reference is collected by, so a reference inside an item whose `when:` gate is false contributes no entry.

A template carrying any refused value SHALL fail to load: it is excluded from the served set and reported as broken through the same channel as every other content fault, under the existing rules of the `template-registry` capability, and SHALL NOT abort startup. The same refusals SHALL apply to a template submitted through the write endpoint, which validates before writing.

`line_spacing` SHALL NOT be accepted on any other layout item. `qr`, `image`, `line` and `container` reject the key as they reject every other unknown field, and a container's pitch is never inherited by a `text` child. Nothing inherits the field and no other new field is accepted anywhere: the `deny_unknown_fields` surface stays closed.

A template read back through the template API SHALL report the `line_spacing` an item declared, in the spelling it declared it: a bare number reads back as that number, and a reference reads back as the single-brace string `"{name}"`. The key SHALL be omitted for an item that declared none. An item declaring `line_spacing: 1.2` therefore reads back with the key, while an item declaring nothing reads back without it although both render identically.

This requirement supersedes the frozen `docs/SPEC.md` §4.1 `text` bullet (`docs/SPEC.md:488-500`) in full and carries the complete post-change field list above. The detailed rules of the sibling fields are owned where they already live and are unchanged by this requirement: `wrap` by `text-wrap-flag`, `color` by `text-ink` and `colour-vocabulary`, sizing and overflow by `layout-sizing`, `when` by `conditional-visibility`, and interpolation by `interpolation-tokens` and `param-resolution`.

#### Scenario: An absent pitch renders at 1.2, measured on the render

- **WHEN** a `text` item declaring no `line_spacing` renders the two identical lines `"Hxy\nHxy"` in a box wide enough for each
- **THEN** the distance between the corresponding ink bands of the two rendered lines is 1.2 font sizes, measured on the render rather than asserted from the fitter's arithmetic

#### Scenario: An explicit 1.2 is the default written out

- **WHEN** two otherwise identical two-line items declare no `line_spacing` and `line_spacing: 1.2`
- **THEN** both render identically, measured on the render

#### Scenario: A non-number is refused and quarantined

- **WHEN** templates carry a `text` item declaring `line_spacing: "1.2em"`, and further templates declare `"{{ pitch }}"`, `true`, `[1.2]` and an explicit null, alongside one valid template
- **THEN** the service starts and the valid template is served
- **AND** each offending template is not served, and is reported as broken with an error naming its file and the `line_spacing` key

#### Scenario: A numeric string is not a second spelling of a literal

- **WHEN** templates carry a `text` item declaring `line_spacing: "1.2"`, and a further template declares `"1.2mm"`
- **THEN** each template is refused before serving with an error naming its file and the `line_spacing` key, and neither is read as the number 1.2

#### Scenario: A zero, a negative and a non-finite value are each refused at load

- **WHEN** a template's `text` item declares `line_spacing: 0`, and three further templates declare `-0.5`, `.nan` and `.inf`
- **THEN** each template fails validation with an error naming the item's layout path and the `line_spacing` key, each is quarantined rather than aborting startup, and no render of any of them succeeds

#### Scenario: A reference to an undeclared parameter is refused at load

- **WHEN** a template's `text` item declares `line_spacing: "{pitch}"` and the template declares no parameter named `pitch`, alongside one valid template
- **THEN** that template fails validation with an error naming the item's layout path and the `line_spacing` key, it is quarantined rather than aborting startup, and the service starts and serves the valid template

#### Scenario: A reference to a parameter of the wrong type is refused at load

- **WHEN** templates each declare `line_spacing: "{pitch}"` against a `pitch` parameter of type `length`, `string`, `boolean`, `enum`, `datetime` and `list` in turn
- **THEN** each template fails validation with an error naming the item's layout path and the `line_spacing` key, and each is quarantined rather than aborting startup

#### Scenario: A reference to a `number` or `integer` parameter loads

- **WHEN** a template declares `line_spacing: "{pitch}"` against a `number` parameter, and a second declares it against an `integer` parameter
- **THEN** both templates load and are served

#### Scenario: A reference whose parameter declares an unusable default still loads

- **WHEN** a template declares `line_spacing: "{pitch}"` against a `number` parameter whose `default` is `0`
- **THEN** the template loads and is served, its `line_spacing` value having been checked for neither bound at load

#### Scenario: A parameter named only by a pitch reference gets an input control

- **WHEN** a template's only use of a `number` parameter `pitch` is `line_spacing: "{pitch}"` on an active `text` item, and its input list is read
- **THEN** the list holds an entry for `pitch`, carrying the numeric control its declared type calls for
- **AND** that entry reports `interpolated` false, since no item prints `pitch` as a value

#### Scenario: A pitch reference inside a gated-off item contributes no input

- **WHEN** a template's only use of `pitch` is `line_spacing: "{pitch}"` on a `text` item whose `when:` gate is false for the request, and its input list is read
- **THEN** the list holds no entry for `pitch`
- **AND** the same template read with the gate true holds one

#### Scenario: An omitted pitch is omitted from the read-back

- **WHEN** a template whose `text` item declares no `line_spacing` is read back through `GET /templates/{id}`
- **THEN** that item carries no `line_spacing` key in the response
- **AND** an item declaring `line_spacing: 0.99` reports `0.99`

#### Scenario: A reference reads back as the string it was declared as

- **WHEN** a template whose `text` item declares `line_spacing: "{pitch}"` is read back through `GET /templates/{id}`
- **THEN** that item reports `line_spacing` as the string `"{pitch}"`, not as a number and not as the parameter's default

#### Scenario: `line_spacing` on a container is refused

- **WHEN** a template declares `line_spacing` on a `container` (and likewise on a `qr`, `image` or `line` item)
- **THEN** the template fails validation with an unknown-field error naming that item's layout path

### Requirement: Pitch is the authored multiple, or 1.2, everywhere

For a text item at font size `s`, the pitch SHALL be `pitch(s) = line_spacing × s`, where `line_spacing` is the item's authored number, the value its `"{param}"` reference resolved to for this render, or 1.2 when the item declared nothing. 1.2 is the only default: the metric-derived pitch (`cap_height + 0.65em`, 1.3775em on the bundled Inter) is retired and SHALL NOT be kept as a fallback path. The renderer computes whatever Typst `par` leading produces that pitch.

**One resolution feeds both passes.** A reference SHALL be resolved once per rendered item, before that item is measured, and the single resolved value SHALL be what the fitter reserves against and what the emitter emits against. The fitter SHALL NOT substitute a placeholder for an unresolved reference and SHALL NOT measure at 1.2 while the emitter emits at the resolved value: pitch feeds the auto-shrink search and the block-height arithmetic directly, so the two passes disagreeing would break the guarantee below. The Typst source for a text block sets its paragraph leading to `pitch(s) − cap_height(s)` at the rendered size, so the block the fitter judged to fit is the block Typst lays out. That algebra inherits the box model the existing Typst-layout agreement proof covers: the fitter's cap-to-baseline stacking matches what Typst lays out to within that proof's tolerance, while the issue's 1.326em figure measured ink tops, which move with the glyphs each line carries rather than with the baselines. Pitch acceptance is therefore measured on content-controlled values: identical repeated lines, on which corresponding ink features sit exactly one pitch apart, with the fitter's block prediction additionally checked against Typst's laid-out height at authored leading.

**A supplied value is bounded at render, and never clamped.** A reference's resolved value SHALL be finite and greater than zero. A resolved value that is zero or negative SHALL refuse the render with `400 InvalidRequest` and `details.reason` of `line_spacing_param_invalid`, and the message SHALL name the item's layout path and the parameter. The render SHALL NOT proceed at 1.2, SHALL NOT clamp the value into range, and SHALL NOT panic. This reason is distinct from the type failure below so a client can tell a pitch of zero from a pitch that was not a number at all.

**A supplied value is read as every other value of its declared type is.** This requirement introduces no pitch-specific coercion: exactly the supplied forms numeric parameter resolution accepts for a `number` or an `integer` parameter are accepted here, and `line_spacing_param_invalid` is reserved for a value that *is* a number of that type and is not a usable pitch. A supplied value that resolution cannot read as the declared type SHALL refuse the render with `400 InvalidRequest` and `details.reason` of `request_body_invalid`, naming the parameter, and SHALL NOT reach the bound above or the fitter.

Two consequences of that reuse are stated here because they are observable and differ between the two accepted types:

- **A `number` parameter cannot carry a non-finite value at all.** A supplied NaN, and a supplied magnitude beyond the range a `number` carries, are lost in coercion and refused as a value that is not a number, with `request_body_invalid`. Pitch SHALL NOT be given a private answer to them: no non-finite value reaches the bound, the fitter or the emitter, by either route.
- **An `integer` parameter saturates instead, and which bound it saturates toward decides the answer.** A supplied JSON *number* beyond the range an `integer` carries saturates toward the bound it overshot. A large positive one, such as `1e300`, becomes the largest integer the parameter holds, which is finite and positive and therefore a legal pitch. It SHALL NOT be refused as a bad pitch, because it is not one: it is an enormous pitch, and the block it produces does not fit its box, so the item's `overflow` rule decides the outcome exactly as it does for an authored `line_spacing: 1e9`. A large negative one, such as `-1e300`, becomes the smallest integer the parameter holds, which is negative, and SHALL therefore be refused by the bound above with `400 InvalidRequest` and `line_spacing_param_invalid`. A magnitude written as a *string* is read by integer parsing rather than by saturation, so an oversized one — `"9223372036854775808"` — is not a value of the declared type at all and SHALL be refused with `400 InvalidRequest` and `request_body_invalid`. This requirement adds no magnitude ceiling; the only bounds on a pitch are finiteness and positivity.

**A missing parameter is an error, never a default.** When a reference's parameter is absent from the resolved request data and declares no `default`, the render SHALL be refused with `422 MissingField` naming the parameter, which is what every unresolvable reference already returns. Resolution SHALL NOT fall back to 1.2: `params[].default` is the mechanism for a value a caller omits, and 1.2 is the default for an item that declared no `line_spacing` at all, not for one whose reference did not resolve.

**A declared default is answered where it fails, and the two places differ.** `param-resolution` already decides a default that fails validation or coercion, and this requirement does not touch it: a `default` of `.nan` or `.inf` on a `number` parameter is refused there as the *template's* fault, `422 TemplateInvalid` with `details.reason` `param_default_unresolvable`, and never reaches an item. A `default` that coerces and is then an unusable pitch — `0` or a negative — resolves like any supplied value and meets the bound at the item, which cannot see where the value came from and reports it as `400 InvalidRequest` with `line_spacing_param_invalid`. The split is where each check lives, and it is stated rather than smoothed over: making the bound answer as the template's fault would mean tracking each value's provenance through resolution, and refusing the default at load instead would refuse a template for a default that every other reference to the same parameter answers at render.

Every refusal above applies on the single-label path and on the batch path alike, and the batch path stays **atomic** under the `batch-validation` capability: a label whose pitch is refused is a failing label, so the request SHALL return `422` with `error.code` `BatchInvalid` and one entry in `error.details.failures` at that label's `index`, and SHALL produce nothing — no ZIP, no PDF, no page of a sheet and no print job — however many other labels of the request would have rendered. That entry carries the `code` that label's own refusal carries and no other, and a `reason` exactly when that code is one that carries a reason: `InvalidRequest` with `line_spacing_param_invalid` for an unusable resolved pitch, `InvalidRequest` with `request_body_invalid` for a value that is not a number, `TemplateInvalid` with `param_default_unresolvable` for a default that could not be resolved, and `MissingField` with no `reason` at all for an absent parameter.

On a single-line item the field SHALL have no effect: a one-line block is one cap-height box whatever the pitch, so an item rendering one line renders identically with and without it. A `wrap: true` item is one or two lines depending on its own content, so refusing the field there would refuse a legal template.

The change from the metric-derived pitch to 1.2 was breaking and no migration was offered: every existing multi-line text item tightened from 1.3775em to 1.2em pitch with the bundled font, and every item bound by its box height picked up a larger fitted font size from the slack the tighter pitch freed. Templates changed appearance without being edited. That is the intended outcome: it is what makes the default a number in the spec rather than a property of a font file. Admitting the reference spelling is not itself breaking: a template declaring a bare number renders exactly as it did before.

#### Scenario: An authored pitch lands on the render

- **WHEN** a template sets `line_spacing: 0.99` on an item rendering `"Hxy\nHxy"`
- **THEN** the distance between the corresponding ink bands of the two rendered lines is 0.99 font sizes, measured on the render

#### Scenario: A supplied pitch lands on the render, at two values from one template

- **WHEN** one template setting `line_spacing: "{pitch}"` on an item rendering `"Hxy\nHxy"` is rendered twice, supplying `pitch` of 0.99 and then 1.5
- **THEN** the distance between the corresponding ink bands is 0.99 font sizes on the first render and 1.5 font sizes on the second, each measured on the render

#### Scenario: A pitch below the cap-height ratio still lands

- **WHEN** a template sets `line_spacing: 0.5`, below the bundled font's cap-height ratio, on an item rendering `"Hxy\nHxy"` in a box holding two lines at that pitch
- **THEN** the render succeeds and the distance between the corresponding ink bands is 0.5 font sizes, measured on the render

#### Scenario: Fitter and Typst agree at authored leading

- **WHEN** a block of one to three identical lines is laid out at each of `line_spacing` 0.5, 0.99, 1.2 and 1.5
- **THEN** the fitter's predicted block height matches the height Typst lays out for the same lines at the emitted leading, within the existing 1% agreement tolerance

#### Scenario: Fitter and Typst agree at a supplied leading

- **WHEN** a block of one to three identical lines is laid out with `line_spacing: "{pitch}"` at each supplied `pitch` of 0.5, 0.99, 1.2 and 1.5
- **THEN** the fitter's predicted block height matches the height Typst lays out for the same lines at the emitted leading, within the existing 1% agreement tolerance

#### Scenario: A tighter pitch grows a height-bound size

- **WHEN** two otherwise identical height-bound two-line items with a `font_size` range declare `line_spacing: 0.99` and `line_spacing: 1.5`
- **THEN** the tighter item settles at a larger font size than the looser one

#### Scenario: A tighter supplied pitch grows a height-bound size, proving the fitter saw it

- **WHEN** one template with a height-bound two-line item whose `font_size` is a `{ min, max }` range and whose `line_spacing` is `"{pitch}"` is rendered twice, supplying `pitch` of 0.99 and then 1.5
- **THEN** the first render settles at a larger font size than the second

#### Scenario: A supplied zero or negative pitch refuses the render

- **WHEN** `POST /api/render/label` renders a template declaring `line_spacing: "{pitch}"`, supplying `pitch` of `0` and then of `-0.5`
- **THEN** each render is refused with `400 InvalidRequest`, `details.reason` of `line_spacing_param_invalid`, and a message naming the item's layout path and `pitch`
- **AND** no image is produced by either request

#### Scenario: A supplied non-number refuses the render as a type failure

- **WHEN** `POST /api/render/label` renders a template declaring `line_spacing: "{pitch}"`, supplying `pitch` as a value numeric parameter resolution cannot read as a number
- **THEN** the render is refused with `400 InvalidRequest` and `details.reason` of `request_body_invalid`, naming `pitch`, and not with `line_spacing_param_invalid`

#### Scenario: A supplied non-finite pitch refuses the render as a type failure

- **WHEN** `POST /api/render/label` renders a template declaring `line_spacing: "{pitch}"` against a `number` parameter, supplying `pitch` as NaN and then as a magnitude beyond the range a `number` carries
- **THEN** each render is refused with `400 InvalidRequest` and `details.reason` of `request_body_invalid`, naming `pitch`
- **AND** neither request reaches the fitter, so no image is produced and no pitch is measured against

#### Scenario: An `integer` pitch resolves like a `number` one

- **WHEN** `POST /api/render/label` renders a template declaring `line_spacing: "{pitch}"` against an `integer` parameter, supplying `pitch` of `2`
- **THEN** the render succeeds and the distance between the corresponding ink bands of two identical lines is 2 font sizes, measured on the render

#### Scenario: An oversized positive `integer` pitch is an enormous pitch, not a refused one

- **WHEN** `POST /api/render/label` renders a two-line, height-bound item declaring `overflow: fail` and `line_spacing: "{pitch}"` against an `integer` parameter, supplying `pitch` as the JSON number `1e300`
- **THEN** the request is refused under the existing overflow rule, with `422` and `details.reason` of `text_does_not_fit`, and not with `line_spacing_param_invalid` or `request_body_invalid`

#### Scenario: An oversized negative `integer` pitch is a negative pitch

- **WHEN** `POST /api/render/label` renders a template declaring `line_spacing: "{pitch}"` against an `integer` parameter, supplying `pitch` as the JSON number `-1e300`
- **THEN** the render is refused with `400 InvalidRequest`, `details.reason` of `line_spacing_param_invalid`, and a message naming the item's layout path and `pitch`

#### Scenario: An oversized `integer` pitch written as a string is not a number of its type

- **WHEN** `POST /api/render/label` renders a template declaring `line_spacing: "{pitch}"` against an `integer` parameter, supplying `pitch` as the string `"9223372036854775808"`
- **THEN** the render is refused with `400 InvalidRequest` and `details.reason` of `request_body_invalid`, naming `pitch`, and not with `line_spacing_param_invalid`

#### Scenario: A non-finite declared default is the template's fault, not the caller's

- **WHEN** `POST /api/render/label` renders a template declaring `line_spacing: "{pitch}"` against a `number` parameter whose `default` is `.nan`, without supplying `pitch`
- **THEN** the render is refused with `422 TemplateInvalid` and `details.reason` of `param_default_unresolvable`, naming `pitch`, and not with `line_spacing_param_invalid`

#### Scenario: An unusable pitch fails the whole batch, whatever the other rows say

- **WHEN** `POST /api/batch` renders three labels from a template declaring `line_spacing: "{pitch}"`, the first and third supplying a usable `pitch` and the second supplying `0`
- **THEN** the request is refused with `422` and `error.code` `BatchInvalid`
- **AND** `error.details.failures` holds exactly one entry, at `index` 1, with `code` `InvalidRequest` and `reason` `line_spacing_param_invalid`, naming the item's layout path and `pitch`
- **AND** the response carries no ZIP and no PDF, and the two valid labels are not returned

#### Scenario: Each batch failure carries its own code, and a missing parameter carries no reason

- **WHEN** `POST /api/batch` renders two labels from a template declaring `line_spacing: "{pitch}"` against a `pitch` parameter with no `default`, the first supplying a usable `pitch` and the second omitting it
- **THEN** the request is refused with `422` and `error.code` `BatchInvalid`, and produces nothing
- **AND** the failure entry at `index` 1 carries `code` `MissingField` and no `reason` key at all
- **AND** rendering the same batch from a template whose `pitch` parameter declares a `.nan` default, with `pitch` omitted on the second label, gives that entry `code` `TemplateInvalid` and `reason` `param_default_unresolvable`

#### Scenario: An omitted parameter refuses the render rather than falling back to 1.2

- **WHEN** a template declaring `line_spacing: "{pitch}"` against a `pitch` parameter with no `default` is rendered without supplying `pitch`
- **THEN** the render is refused, and no label is produced at 1.2 or at any other pitch

#### Scenario: A declared default supplies an omitted parameter

- **WHEN** a template declaring `line_spacing: "{pitch}"` against a `pitch` parameter whose `default` is 0.99 is rendered without supplying `pitch`
- **THEN** the render succeeds and the distance between the corresponding ink bands of two identical lines is 0.99 font sizes, measured on the render

#### Scenario: An unusable declared default is refused at render, not at load

- **WHEN** a template declaring `line_spacing: "{pitch}"` against a `pitch` parameter whose `default` is `0` is loaded, and then rendered without supplying `pitch`
- **THEN** the template loads and is served, and the render is refused with `400 InvalidRequest`, `details.reason` of `line_spacing_param_invalid`, and a message naming the item's layout path and `pitch`

#### Scenario: `line_spacing` on a single-line item changes nothing

- **WHEN** two otherwise identical items rendering a single line declare no `line_spacing` and `line_spacing: 2.0`
- **THEN** both render byte-identically

#### Scenario: A worked example documents the field

- **WHEN** `docs/AUTHORING.md` is read
- **THEN** a worked `text` example declares `line_spacing` with its meaning stated alongside
- **AND** the `"{param}"` reference spelling is shown, naming the parameter types it accepts
