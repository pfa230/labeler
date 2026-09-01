## Why

Issue [#291](https://github.com/pfa230/labeler/issues/291). Two colour types are on `main` and they
disagree about what a name means. `Ink` (#282, ADR-0091, `src/models.rs:831`) reads `red` as
`#ff4136`; `Color` (#280, ADR-0092, `src/raw.rs:40`) reads `red` as `#ff0000`. A template that writes
`ink: red` on a text item and `background: red` on the container behind it paints two different reds,
and nothing in the template says so. `gray`, `yellow` and `green` diverge the same way, and `eastern`
exists on one side only.

The second half is the name. "Ink" already means something else in this repo: the marks a glyph makes
(ADR-0043, ADR-0050, ADR-0084). Only ADR-0071 uses it for colour. CSS, SVG and Typst all call the
thing a colour.

## What Changes

- **BREAKING.** One colour type survives, named `Color`. `Ink` is deleted from `src/`. The survivor
  carries `Ink`'s capabilities rather than `Color`'s: a named colour, a hex string in 3, 4, 6 or
  8 digits, or a `{param}` reference, and it keeps the authored spelling.
- **BREAKING.** One named-colour table: the sixteen CSS Level 1 names at the values already in
  `src/raw.rs:94-111`, matched case-insensitively. Typst's palette goes. `eastern` and `orange` stop
  being colour names, and for text every remaining name except `black` and `white` changes value.
  Shape paint keeps the values it has. The table moves out of code and into a requirement.
- **New capability.** `stroke.color` and `background` accept `"{param}"` references, exactly as text
  already does: checked at load against the template's declared parameters, resolved per render,
  reported as a template input.
- **BREAKING.** The authored field on a text item is `color`. `ink:` is removed outright, with no
  alias and no deprecation window: it hits `deny_unknown_fields`, the template is quarantined at
  load with a path-carrying error, and the server still starts.
- **BREAKING.** The render-time failure reason `ink_param_invalid` becomes `color_param_invalid`.
  It now covers three fields, none of them called `ink`.
- **BREAKING.** A colour is reported through the template API as the author wrote it, on every field
  that takes one. `background: red` reads back `"red"`, not `"#ff0000ff"`. Which keys appear does not
  move: an omitted `text.color` or `background` stays absent from the response, and only
  `stroke.color`, whose block always carries a colour, reports its `black` default. This is the one place
  where honouring #291's "it keeps a `spelling`" costs something: it withdraws the canonical
  `#rrggbbaa` normalization #280 shipped for shape paint. `GET /templates/{id}/source` still returns
  the authored YAML verbatim, and the design records the alternative (canonical hex everywhere, no
  spelling) for the reviewer to weigh.
- ADR-0093 records why the type is `Color`, superseding ADR-0091's vocabulary and naming clauses and
  ADR-0092's clauses 5 and 6.

Not changed: where a colour may be written (no new field takes one), how paint is layered, stroke
geometry, rounding, the bilevel path, or how `text.color` behaves when absent (black).

## Capabilities

### New Capabilities

- `colour-vocabulary`: what a colour is, wherever one is written. The three accepted forms, the
  sixteen names and their values, the invariant that a name means one colour on every field, the
  parameter-reference form with its load-time check and per-render resolution, how a colour is
  reported when a template is read back, what a bad colour does to its template, and what a colour
  does on each output path.

### Modified Capabilities

- `text-ink`: the field is `color`, not `ink`; `ink` is an unknown field. Its vocabulary, parameter,
  input, quarantine and output-path requirements move to `colour-vocabulary`, which states them once
  for all three fields.
- `shape-paint`: its colour-vocabulary requirement moves to `colour-vocabulary` and gains the
  parameter-reference form; its canonical read-back requirement is withdrawn; `stroke.color` and
  `background` accept a reference as well as a literal.

## Impact

- `src/models.rs`: `Ink` deleted; `Color` becomes `{ spelling, rgba }` with `FromStr`, `Serialize`,
  `Deserialize` and the utoipa schema the merged type needs. `Color` stops being `Copy`.
- `src/raw.rs`: `parse_color` and `deserialize_dynamic_ink` collapse into one dynamic-colour
  deserializer used by `text.color`, `stroke.color` and `background`; `TextRaw.ink` becomes `color`.
- `src/convert.rs`, `src/templates.rs`, `src/render/mod.rs`, `src/render/helpers.rs`: the model
  carries `DynamicValue<Color>` on three fields, so load-time reference validation, input derivation
  and render-time resolution each gain the two shape fields.
- `src/errors.rs`, `src/reason.rs`: `ink_param_invalid` renamed.
- `src/openapi.rs`: `Ink` deregistered; `Color`'s schema description covers all three forms.
- API: `GET /templates/{id}` reports colours as authored; `POST /templates/{id}/inputs` reports
  parameters referenced by shape paint.
- No shipped template or test fixture writes `ink:`, so nothing under `catalog/` or
  `tests/fixtures/templates/` needs migrating.
- `docs/AUTHORING.md`: its colour paragraph (`docs/AUTHORING.md:500-504`) still teaches that shape
  paint uses CSS values whereas text `ink` uses Typst's palette. It is rewritten to the one
  vocabulary, documents `color` on a text item, and records that `ink` is gone.
- `docs/adr/0093-*.md` and its row in `docs/adr/README.md`.
