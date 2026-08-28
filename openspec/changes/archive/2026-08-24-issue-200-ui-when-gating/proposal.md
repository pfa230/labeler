## Why

Fixes [#200](https://github.com/pfa230/labeler/issues/200).

The UI decides for itself which fields a template needs, by walking the layout tree
(`ui/src/lib/templateFields.ts`). That rule therefore exists twice, once in Rust at render time and
once in TypeScript, across a process boundary no test spans. The copies have drifted.

`templateFields.ts:201` gates a container's children on `it.option`. The API has not sent `option`
since ADR-0056: `convert.rs:107` rewrites `container.option` into `when` at load, and
`LayoutItem::Container` (`src/models.rs:665`) has only `when`. A live `GET /api/templates/{id}`
confirms it: containers carry `when`, and the response has no `options` key at all. So
`it.option ?? {}` is `{}`, `every()` over it is `true`, and the gate has never excluded an item.
`when:` on `text`, `qr`, `image` and `line` (ADR-0056, #162) was never read at all.

`ui/src/api/types.ts` is why nothing failed: it declares `option?` on the container and `options?` on
the template, neither of which the API can produce. `templateFields.test.ts` builds its fixtures with
`option:`, so the suite feeds the walker a shape that does not exist on the wire and would keep
passing if the gate were replaced with `return true`.

There are in fact three copies, not two. `walk_placeholder` (`src/render/mod.rs:2107`) is the
service's own field collector, used to build thumbnail placeholder data. It reads `image.src` tokens,
which neither other copy does, and it ignores `when:` entirely, so a thumbnail invents data for
branches it will not draw.

And the print form does not consult the walker for declared parameters at all: `FieldForm.tsx:68`
renders every one and `PrintForm.tsx:60` requires every one without a default, while the renderer is
lazy (`docs/SPEC.md` §5) and never asks for a parameter used only in an inactive branch.

Repairing the TypeScript copy fixes today's drift and keeps the mechanism that produced it. So does
shipping the conditions to the client and having it evaluate them: to decide a gate the client must
reproduce how the service coerces a value before comparing it (`resolve_parameters`,
`src/render/mod.rs:38-220`), which is a normalization table written twice and free to drift again.
The client should not be deciding anything.

## What Changes

- **The service answers "which inputs does this label need", and the UI renders the answer.** The UI
  stops walking layouts, stops evaluating `when:`, and stops normalizing values. It never learns that
  `when:` exists.
- **New `POST /api/templates/{id}/inputs`** takes the same label shape `/api/batch` takes,
  `{ labels: [{ data }, ...] }`, and returns one input list per label. Each entry is what the form
  needs to render one control: its name, the control kind, whether a value is required, the value the
  service would default to, and the control's metadata (`values`, `min`, `max`, `unit`,
  `description`, and whether the same name is truncated by a single-line item elsewhere in the
  template). The same cap `/api/batch` enforces applies.
- The label shape carries `data` and nothing else, because that is all the render paths read.
  `LabelInput` is `{ data }` (`src/models.rs:780`), it declares no `deny_unknown_fields`, and both
  render paths pass `None` for the option selection (`src/api.rs:2295`, `:2306`, `src/batch.rs:93-103`),
  so an `option` key on a submitted label is silently discarded today. The new endpoint ignores it the
  same way, which is what keeps its answer equal to what `/api/batch` will draw. #214 covers retiring
  the map from the UI, which is where the misleading half lives.
- **`GET /api/templates/{id}` gains `inputs` and `variables`**, so first paint needs no extra round
  trip. `inputs.default` is the list for a label carrying no `data`, which is what a form renders
  before it has values. `inputs.all` is the union across every branch, which is what the template
  detail page and the Connect field-mapping palette read, and what the thumbnail and the preview fill
  sample values from. `variables` lists the `{vars.*}` keys the layout reads.
- **Gating resolution is lenient, rendering stays strict.** A half-filled form must still get an
  answer, so the inputs path treats a value it cannot coerce (a blank `enum`, a non-numeric
  `integer`) as though the label had omitted the name, after which the ordinary defaulting rules
  apply. Rendering is unchanged and still rejects the same value with the code that path already
  returns: `422 InvalidOptionValue` for an out-of-range `enum`, `400 InvalidRequest` for an
  uncoercible number or boolean and for an unparseable `datetime` on `/api/render/label`, and
  `422 BatchInvalid` wrapping the label's own code inside `/api/batch`.
- **`walk_placeholder` and `template_fields` are deleted**, and the thumbnail and the catalog-index
  binary take their data from the same derivation. The thumbnail's placeholder set narrows to names an
  active item actually reads as a value and the service has no value of its own for, which stops a
  gate key or a defaulted enum being overridden by the literal text of its own name.
- **The print form, the CSV import grid and the connector grid ask only for the inputs the service
  reports.** A declared parameter used only inside an inactive branch is neither shown nor required,
  matching `docs/SPEC.md` §5's lazy missing-field rule. A parameter named as a `when:` key, and one
  read only by a layout attribute such as `size: "{width}"`, are reported as inputs so the operator
  keeps those controls. A value already entered for an input a later selection drops is kept in the
  screen's state so switching back restores it, but is no longer submitted: rendering coerces every
  declared parameter before it evaluates any `when:` (`src/render/mod.rs:350`), so a stale invalid
  value rejects a label the form calls complete. A cleared numeric, select, checkbox or date control
  is likewise omitted rather than sent as an empty string.
- **BREAKING (internal, UI only)**: `ui/src/lib/templateFields.ts`'s layout walk and its four field
  queries are removed. `ui/src/api/types.ts` drops `option` from the container variant and `options`
  from the template types, since the API produces neither.
- New ADR-0070.

No change to how the renderer evaluates `when:`, to `docs/SPEC.md` §5, to the status or `code` any
existing endpoint returns, or to the template YAML schema. Nothing about `when:` is narrowed, so `datetime-params`' rule that a `datetime`
parameter in a `when:` predicate compares against its bare ISO date is untouched.

Out of scope, filed as [#214](https://github.com/pfa230/labeler/issues/214): the per-label `option`
map is silently discarded, so the grids' option columns do nothing.

[#215](https://github.com/pfa230/labeler/issues/215) is subsumed rather than deferred, and should be
closed when this merges. It reported the preview inventing an invalid sample value for a declared
enum or numeric parameter. The thumbnail has the same defect through the same walker, and this change
rewrites that walker, so the invent-rule has to be decided here for the thumbnail regardless; writing
one rule for the thumbnail and leaving the preview on the broken one would mean specifying the defect
deliberately. #200 remains the single issue this change implements.

## Capabilities

### New Capabilities
- `template-inputs`: what an input list is, how the service derives one for a given label, the two
  places it is served, the lenient resolution the gating path uses, and how a screen renders the
  result. Supersedes two frozen paragraphs: the `GET /templates/{id}/thumbnail` bullet of
  `docs/SPEC.md` §2.0, whose placeholder rule narrows, and the paragraph of `docs/SPEC.md` "CSV
  import" describing the web UI's CSV Import screen.

### Modified Capabilities
- `datetime-params`: two requirements. "A datetime parameter defaults to the render instant of its
  request" gains the input list's treatment of a referenced `datetime` parameter, preserving its rule
  that such a parameter is absent from the request fields a caller must supply. "The print form and
  the row grids carry a datetime parameter" is scoped to a `datetime` parameter the service reports
  as an input for the current selection, since every control is now reported rather than derived.

## Impact

- Rust: `src/models.rs` (`InputSpec`, `TemplateDetail`), `src/templates.rs` (derivation),
  `src/render/mod.rs` (lenient resolution beside `resolve_parameters`; `walk_placeholder`,
  `collect_data_tokens`, `template_fields` and `placeholder_data` folded into the derivation),
  `src/render/helpers.rs` (the interpolation-token scanner becomes shared rather than
  re-implemented), `src/api.rs` (the new route, and the thumbnail's placeholder source at `:942`),
  `src/bin/catalog-index.rs` (`:87`, now reading the derived list), `src/openapi.rs` (registration).
- API: one new endpoint; two additive fields on the template-detail body, which `POST`/`PUT`
  `/templates` and `PUT /templates/{id}/group` return too. No existing field changes.
- UI: `ui/src/lib/templateFields.ts` reduced to the pieces that were never about the layout
  (`datetimeCellError`, `formatLocal*`, `reconcileRowOptions`); a new hook fetches input lists on the
  pattern `useLivePreview` already established (debounce, key, cache, abort); callers updated in
  `TemplateDetail.tsx`, `Connect.tsx`, `Import.tsx`, `print/PrintForm.tsx`, `print/FieldForm.tsx`,
  `lib/preview.ts`.
- Tests: `ui/src/lib/templateFields.test.ts`'s layout fixtures removed; Rust coverage for the
  derivation across every item type, `image.src`, attribute references and nested gates, and for the
  lenient/strict split.
- Docs: ADR-0070 plus its row in `docs/adr/README.md`.
