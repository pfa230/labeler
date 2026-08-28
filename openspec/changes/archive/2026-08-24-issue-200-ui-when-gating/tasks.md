## 1. The derivation, in the service

- [x] 1.1 Lift the `{token}` brace scanner out of `interpolate` (`src/render/helpers.rs:43`) into a
      function yielding each token plus a well-formed flag; keep `interpolate` erroring on the flag so
      its behavior is unchanged, and cover the `{{`/`}}` escapes and an unterminated brace.
- [x] 1.2 Add `InputSpec` to `src/models.rs`: `name`, `control`, `slider`, `required`, `default`,
      `values`, `min`, `max`, `unit`, `description`, `interpolated`, `truncated_elsewhere`.
- [x] 1.3 Write the derivation in `src/templates.rs` beside `options()`: one walk over the `format`'s
      dynamic dimensions and the layout, gathering `when:` keys, `DynamicValue::Ref` attributes, the
      tokens of `text.value`, `qr.value` and `image.src`, and `image.name`; classify each name by the
      order `interpolate` resolves in, and drop `variable` and `datetime`-resolver names.
- [x] 1.4 Decide `control` declaration-first, use-second, with the `image` override for a `string`
      parameter an `image` item binds; set `slider` from both bounds being declared, `required` from
      the `hasServerDefault` rule, and `default` from the declared default or the type's fallback.
- [x] 1.5 Set `interpolated` from whether an active item reads the name as a value, and
      `truncated_elsewhere` ungated across the whole template.
- [x] 1.6 Order entries: declared parameters ascending by name, then undeclared names in first-read
      order.
- [x] 1.7 Add `inputs.default`, `inputs.all` and `variables` to `TemplateDetail` and its
      `From<&TemplateDefinition>` (`src/templates.rs:1823`).

## 2. Lenient resolution and the endpoint

- [x] 2.1 Give `resolve_parameters` (`src/render/mod.rs:38`) a strictness mode rather than a copy: in
      lenient mode a value that fails coercion is treated as absent and takes the ordinary omission
      fallback; strict mode is byte-for-byte what it does today.
- [x] 2.2 Add `POST /api/templates/{id}/inputs` in `src/api.rs`, taking `{ labels: [LabelInput] }` and
      returning `{ inputs: [[InputSpec]] }`; enforce the `/batch` label cap with the same code, return
      `200` with an empty array for empty `labels`, `404` for an unknown id, and never `422` for a
      value's content.
- [x] 2.3 Register `InputSpec` and the new route in `src/openapi.rs`.

## 3. Fold in the service's other walkers

- [x] 3.1 Delete `walk_placeholder`, `collect_data_tokens`, `placeholder_data` and `template_fields`
      (`src/render/mod.rs:2075-2155`).
- [x] 3.2 Build the thumbnail's placeholder data (`src/api.rs:942`) from `inputs.all`, inventing only
      for an entry that is `interpolated` and `required`: a 1×1 PNG for `image`, the entry's own name
      for `text`/`textarea`, and `min` or `1` for `integer`/`number`.
- [x] 3.3 Point `src/bin/catalog-index.rs:87` at the `required` names of `inputs.all`.
- [x] 3.4 Render a thumbnail for `avery5163_asset_tag` and **open the PNG**: confirm the horizontal
      branch draws, the QR is square, nothing is clipped, and the gate key was not overwritten by its
      own name. A test that returned bytes does not satisfy this task.

## 4. Rust tests

- [x] 4.1 Reference-site guard: over the whole test corpus, every parameter name validation checks, in
      both `templates.rs:263-299` and `:935-1015`, appears in that template's `inputs.all`.
- [x] 4.2 Whole-manifest fixture: all five item types, nested and sibling gates, `image.name`,
      `image.src`, a `font_weight` ref, a dynamic `size` per item type and a dynamic `format`
      dimension; assert `inputs.default` and `inputs.all` against literals.
- [x] 4.3 Endpoint matches the render: for several labels over that fixture, every reported entry is a
      name the render of the same label resolves, and every `data` name the render resolves is
      reported.
- [x] 4.4 Thumbnail closure: a required `string` both printed and gated on renders rather than failing
      for missing data; a required `length` renders from its `min`; a printed `enum` keeps its default.
      **Reopened by review.** The existing test does not bite: deleting either the `interpolated` or
      the `required` condition from `placeholder_data` (`src/templates.rs:114`) leaves all 601 tests
      passing. Prove each red before green, by making the mutation, watching the new test fail, and
      restoring it.
- [x] 4.7 A gate key that is not interpolated is never invented for: a declared `string` with a
      default, gating a container on its own default value, keeps the gated branch rendering. Delete
      the `interpolated` condition and this test must fail.
- [x] 4.8 A name the service resolves on its own is never invented for: a `text` item interpolating an
      `enum` parameter renders that enum's default, not the literal parameter name. Delete the
      `required` condition and this test must fail.
- [x] 4.5 Lenient versus strict: a blank `enum`, a non-numeric `integer` and an unparseable `datetime`
      each return `200` from the endpoint with the omitted-value list, and each still fails a render
      with `422 InvalidOptionValue`, `400 InvalidRequest` and `400 InvalidRequest`
      (`datetime_param_invalid`) respectively.
- [x] 4.6 An `option` key on a submitted label changes neither the input list nor the render.

## 5. The UI

- [x] 5.1 Replace `LayoutItem`, `Options` and the `options?`/`option?` fields in
      `ui/src/api/types.ts` with `InputSpec`, `inputs` and `variables`.
- [x] 5.2 Delete the walk and its four field queries plus `referencedVariables` from
      `ui/src/lib/templateFields.ts`, keeping `datetimeCellError`, the local date formatters,
      `reconcileRowOptions` and `hasServerDefault`.
- [x] 5.3 Write `useLabelInputs` on the `useLivePreview` pattern (debounce, key, LRU cache, abort),
      returning the previous list while a request is in flight and reporting whether one is pending.
- [x] 5.4 Rewrite `FieldForm` as a renderer of `InputSpec[]`: no `declaredParams` loop, no
      `fallbackFields`, no `fallbackOptions`; seed each control from `default`.
- [x] 5.5 `PrintForm`: validity is "no `required` entry is empty"; request a list for the seeded label
      before treating it as complete; block submission while a list is pending; submit only the names
      in the current list, omitting an empty value for a non-text control.
- [x] 5.6 `Import` and `Connect`: batch one request for uncached rows, block the run while any row's
      list is unresolved, keep columns as the union across rows, and render a cell inert when its name
      is not on that row's list.
- [x] 5.7 `LabelGrid` takes a per-row predicate for cell editability (`ui/src/components/LabelGrid.tsx:67`).
- [x] 5.8 `TemplateDetail` and the Connect mapping palette read `inputs.all` and `variables` off the
      detail response; `lib/preview.ts` fills samples by the thumbnail rule over `inputs.all`.
- [x] 5.9 On a failed list request, keep the last list or fall back to `inputs.all`, surface the
      failure, and do not block submission.

## 6. UI tests

- [x] 6.1 Rebuild `ui/src/lib/templateFields.test.ts` without layout fixtures; its `option:` fixtures
      described a wire shape the API cannot emit.
- [x] 6.2 `useLabelInputs`: debounce, cache hit, abort on supersede, previous list held while pending.
- [x] 6.3 `FieldForm` renders each control kind from an `InputSpec` fixture, including `integer` versus
      `number` and a slider.
- [x] 6.4 `PrintForm` blocks submission while a list is pending, and omits a deactivated name and an
      empty non-text value from the submitted `data`. **Reopened by review.** The pending gate is
      covered, but the pruning is not: making `pruneDataForSubmit` keep names absent from the active
      list (`ui/src/lib/labelInputs.ts:205`) leaves all 370 UI tests passing. Prove red before green.
- [x] 6.6 `pruneDataForSubmit` submits an empty value for a `text`, `textarea` **and `image`** control,
      and omits it for every other control. The current code drops an empty `image` value, which the
      approved spec does not say. Change the code, not the spec: the spec is the approved contract and
      editing it now would void the review verdict.
- [x] 6.5 A grid cell inactive for its row is not editable, not validated and not submitted, and its
      value returns when the name does.

## 7. Record and verify

- [x] 7.1 Write `docs/adr/0070-service-derives-the-input-list.md` and add its row to
      `docs/adr/README.md`.
- [x] 7.2 Run `cargo fmt`, `cargo clippy --all-targets --all-features` and `cargo test`; fix every
      finding at the root rather than with an `allow`.
- [x] 7.3 Run `npm --prefix ui run lint`, `npm --prefix ui run build` and the vitest suite.
- [x] 7.4 Start the service with `LABELER_CONFIG_DIR=./config-dev LABELER_NO_AUTH=true`, drive the
      print form for `avery5163_asset_tag` through both `orientation` branches, and confirm by eye
      that the fields shown follow the branch and that the rendered PNG matches them.
