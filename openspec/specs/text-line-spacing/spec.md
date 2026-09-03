## Purpose

Gives template authors control over the line pitch of multi-line text: one bare number per text item fixing the baseline-to-baseline distance as a multiple of the font size, defaulting to 1.2 and replacing the font-derived default the renderer inherits from Typst.

## Requirements

### Requirement: A text item's line pitch is authored as `line_spacing`

A `text` layout item SHALL accept exactly the following fields, and no others: `value`, an interpolated string; placement `at` (default `[0, 0]` on non-packed items; a packed child carries neither `at` nor `to` and is positioned by its flow container under `flow-layout`), exactly one of `size` or `to`, and `max_w` / `max_h` bounds; `font_size`, either a fixed number or a `{ min, max }` range that auto-shrinks in 0.5pt steps and truncates with an ellipsis on overflow; `font_weight`, either a literal multiple of 100 between 100 and 900 or a `"{param}"` reference to an integer parameter resolving to such a value, defaulting to 400 when omitted; `color`, the glyph foreground colour defaulting to black; `wrap`, defaulting to `false`; `line_spacing`, the baseline-to-baseline distance of its lines as a multiple of the font size; `alignment`, with `horizontal` of left/center/right and `vertical` of top/center/bottom defaulting to `top`; `overflow`, of `ellipsis` (the default) or `fail`; and `when`, the conditional rendering gate map.

An item declaring no `line_spacing` SHALL be laid out and rendered at 1.2, identically to an item declaring `line_spacing: 1.2`. `line_spacing: 0.99` puts baselines 0.99 font sizes apart.

The value SHALL be a bare number. A string (`"1.2em"`, `"{{ pitch }}"`), boolean, object, array, explicit null, or any other non-number SHALL be refused before serving, with an error naming the file and the key. There is exactly one spelling of a value, so no unit suffix and no absolute lengths exist to collide: this also makes the field static-only, since a `{{ param }}` interpolation is a string and is refused as one.

The value SHALL be finite and greater than zero. A zero, negative, NaN or infinite value SHALL be refused at load, naming the item's layout path and the key. A template carrying any refused value SHALL fail to load: it is excluded from the served set and reported as broken through the same channel as every other content fault, under the existing rules of the `template-registry` capability, and SHALL NOT abort startup. The same refusal SHALL apply to a template submitted through the write endpoint, which validates before writing.

`line_spacing` SHALL NOT be accepted on any other layout item. `qr`, `image`, `line` and `container` reject the key as they reject every other unknown field, and a container's pitch is never inherited by a `text` child. Nothing inherits the field and no other new field is accepted anywhere: the `deny_unknown_fields` surface stays closed.

A template read back through the template API SHALL report the `line_spacing` an item declared, and SHALL omit the key for an item that declared none. An item declaring `line_spacing: 1.2` therefore reads back with the key, while an item declaring nothing reads back without it although both render identically.

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

#### Scenario: A zero, a negative and a non-finite value are each refused at load

- **WHEN** a template's `text` item declares `line_spacing: 0`, and three further templates declare `-0.5`, `.nan` and `.inf`
- **THEN** each template fails validation with an error naming the item's layout path and the `line_spacing` key, each is quarantined rather than aborting startup, and no render of any of them succeeds

#### Scenario: `line_spacing` on a container is refused

- **WHEN** a template declares `line_spacing` on a `container` (and likewise on a `qr`, `image` or `line` item)
- **THEN** the template fails validation with an unknown-field error naming that item's layout path

#### Scenario: An omitted pitch is omitted from the read-back

- **WHEN** a template whose `text` item declares no `line_spacing` is read back through `GET /templates/{id}`
- **THEN** that item carries no `line_spacing` key in the response
- **AND** an item declaring `line_spacing: 0.99` reports `0.99`

### Requirement: Pitch is the authored multiple, or 1.2, everywhere

For a text item at font size `s`, the pitch SHALL be `pitch(s) = line_spacing × s`, where `line_spacing` is the item's authored value or 1.2 when absent. 1.2 is the only default: the metric-derived pitch (`cap_height + 0.65em`, 1.3775em on the bundled Inter) is retired and SHALL NOT be kept as a fallback path. Pitch is the authored number or 1.2, everywhere, and the renderer computes whatever Typst `par` leading produces that pitch.

The fitter SHALL reserve and the emitter SHALL emit against the same pitch: the Typst source for a text block sets its paragraph leading to `pitch(s) − cap_height(s)` at the rendered size, so the block the fitter judged to fit is the block Typst lays out. That algebra inherits the box model the existing Typst-layout agreement proof covers: the fitter's cap-to-baseline stacking matches what Typst lays out to within that proof's tolerance, while the issue's 1.326em figure measured ink tops, which move with the glyphs each line carries rather than with the baselines. Pitch acceptance is therefore measured on content-controlled values: identical repeated lines, on which corresponding ink features sit exactly one pitch apart, with the fitter's block prediction additionally checked against Typst's laid-out height at authored leading.

On a single-line item the field SHALL have no effect: a one-line block is one cap-height box whatever the pitch, so an item rendering one line renders identically with and without it. A `wrap: true` item is one or two lines depending on its own content, so refusing the field there would refuse a legal template.

This requirement is breaking, and no migration is offered: every existing multi-line text item tightens from 1.3775em to 1.2em pitch with the bundled font, and every item bound by its box height picks up a larger fitted font size from the slack the tighter pitch frees. Templates change appearance without being edited. That is the intended outcome: it is what makes the default a number in the spec rather than a property of a font file.

#### Scenario: An authored pitch lands on the render

- **WHEN** a template sets `line_spacing: 0.99` on an item rendering `"Hxy\nHxy"`
- **THEN** the distance between the corresponding ink bands of the two rendered lines is 0.99 font sizes, measured on the render

#### Scenario: A pitch below the cap-height ratio still lands

- **WHEN** a template sets `line_spacing: 0.5`, below the bundled font's cap-height ratio, on an item rendering `"Hxy\nHxy"` in a box holding two lines at that pitch
- **THEN** the render succeeds and the distance between the corresponding ink bands is 0.5 font sizes, measured on the render

#### Scenario: Fitter and Typst agree at authored leading

- **WHEN** a block of one to three identical lines is laid out at each of `line_spacing` 0.5, 0.99, 1.2 and 1.5
- **THEN** the fitter's predicted block height matches the height Typst lays out for the same lines at the emitted leading, within the existing 1% agreement tolerance

#### Scenario: A tighter pitch grows a height-bound size

- **WHEN** two otherwise identical height-bound two-line items with a `font_size` range declare `line_spacing: 0.99` and `line_spacing: 1.5`
- **THEN** the tighter item settles at a larger font size than the looser one

#### Scenario: `line_spacing` on a single-line item changes nothing

- **WHEN** two otherwise identical items rendering a single line declare no `line_spacing` and `line_spacing: 2.0`
- **THEN** both render byte-identically

#### Scenario: A worked example documents the field

- **WHEN** `docs/AUTHORING.md` is read
- **THEN** a worked `text` example declares `line_spacing` with its meaning stated alongside
