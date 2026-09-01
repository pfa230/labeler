## Why

[#266](https://github.com/pfa230/labeler/issues/266). `openspec/specs/template-inputs/spec.md`
presents a gate whose value is its own parameter's name — `when: { mode: mode }` — as a supported
authoring shape, in two requirements and in two scenarios that use it as the worked example. It is
not one. The thumbnail's placeholder rule fills a required, interpolated `text` entry with the
entry's own name (`src/templates.rs:187`), so such a gate is satisfied by the placeholder and by no
label a caller sends unless the caller types a field's own name into that field.

That is a malformed template rather than a service defect, and the service's behaviour is correct as
shipped. What is wrong is the record: the spec endorses the shape, and it says nothing about what the
shape costs. Since [#263](https://github.com/pfa230/labeler/issues/263) gave `flow` containers packed
children, two such gates on two packed siblings both activate under the placeholder and accumulate
past the container, so the thumbnail returns `422 UnsupportedLayoutItem` / `item_out_of_frame` on a
template that renders for every caller sending ordinary values; only a caller deliberately sending
each involved parameter's own name as its value activates both gates and reproduces the same overrun.
A reader hitting that 422 should find it in the spec, and today finds nothing.

## What Changes

- The thumbnail requirement's gate rationale keeps every rule it states — filling from `inputs.all`,
  the closure argument for it, and the fact that an invented value can decide a gate — and stops
  using a self-named `string` gate as the example of that rule. The worked example becomes a required
  `integer` filled with `1` against a gate on `1`, which is a shape an author writes.
- Both requirements gain a paragraph naming the self-named gate for what it is: a gate satisfiable
  only by the placeholder or by a caller entering the parameter's own name, with `enum` named as the
  shape an author writes instead. The renderer's behaviour is stated, not softened: the gate is
  satisfied and its branch draws.
- Both requirements record the limit. Two self-named gates on packed `flow` siblings accumulate past
  the padded inner box and fail with `UnsupportedLayoutItem` / `details.reason` of
  `item_out_of_frame`; the fault is the template's; and neither the thumbnail nor the preview works
  around it by withholding a fill, relaxing a gate or softening the `flow` `overflow: fail` contract.
- The two scenarios that assert the self-named shape are replaced. The closure property they existed
  to pin keeps a scenario in each requirement, restated over the `integer` fill. Three scenarios are
  added: in the thumbnail requirement, one stating that a self-named gate is activated by the
  placeholder and by a caller only through the same-name value, and one stating the packed-sibling
  overrun and the error it returns; in the screen requirement, one stating that same overrun and that
  the preview surfaces the failure rather than working around it.
- **No behaviour changes.** No file under `src/` or `ui/src/` is touched and no existing test is
  changed. Every sentence added describes what the shipped renderer already does.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `template-inputs`: two requirements are modified, both already present in `openspec/specs/`, so the
  first-touch rule does not apply and both deltas are `MODIFIED`.
  - **The thumbnail renders the default selection from placeholder data** — its gate rationale and
    its gate scenarios.
  - **A screen renders the reported inputs and decides nothing else** — its preview sample-fill
    rationale and its preview gate scenario.

## Impact

- `openspec/specs/template-inputs/spec.md`, through this change's delta and the archive sync.
- No source, test, template or UI file. `src/templates.rs`'s `placeholder_data` and
  `ui/src/lib/preview.ts`'s `sampleData` are described more accurately and are not edited.
- Gates: `cargo fmt`, `cargo clippy --all-targets --all-features` and `cargo test` must pass
  unchanged, and `.workflow/archive-merge-check.sh` must accept the sync, which is why a spec
  correction goes through the full loop rather than a hand-edit.
