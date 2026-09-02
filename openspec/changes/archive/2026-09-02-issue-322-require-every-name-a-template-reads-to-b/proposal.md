## Why

Implements [#322](https://github.com/pfa230/labeler/issues/322).

`params:` is meant to be the complete vocabulary a template reads, and two sites opt out of that rule.
`validate_interpolated_string` (`src/templates.rs:1424-1481`) checks a bare token's *syntax* and never
asks whether the name is declared, and an `image` item's `name:` is checked for its charset only
(`src/templates.rs:1539-1553`). A name read at either site resolves out of the request `data` map at
render (`resolve_parameters_mode` starts from `let mut resolved = data.clone()`,
`src/render/mod.rs:176`), so a template can print a name nobody declared and nothing says so until a
label renders blank or 422s.

Every other reference site already enforces the rule, with the message already written.
`check_param_ref` (`src/templates.rs:1376-1401`) guards `format` `width`/`height`, `font_weight`,
`color`, `stroke.color`, `background` and every dynamic extent; `validate_when_references`
(`:1403-1422`) guards every `when:` key. This is not a new rule; it is closing the last two sites that
do not follow it.

## What Changes

**BREAKING.** Pre-1.0: no migration, no deprecation window, no flag, no second spelling.

- A bare token in a `text` `value:`, a `qr` `value:` or an interpolated `image` `src:` naming a
  parameter the template does not declare SHALL fail validation when the template loads, naming the
  file and the name. **Existence only:** every declared parameter stringifies, so no type restriction
  applies here.
- An `image` item's `name:` SHALL name a declared `string` parameter, checked through
  `check_param_ref`, not merely be a legal bare name.
- A template breaking either rule is **quarantined** at startup and on `POST /api/templates/reload`,
  and the same content arriving through a template write is refused with `422 TemplateInvalid`. No
  template content becomes fatal to startup (#175).
- Unaffected: `{vars.*}`, `{sys.now}` and `{sys.now:<fmt>}`, which are not parameters; and a `default:`
  string, where a bare token is already refused outright (`src/templates.rs:1018-1027`). A declared
  parameter the caller leaves unset is still `422 MissingField` at render.

**Consequence: the inputs contract loses its undeclared branch.** Every name an input list can hold is
now a declared parameter, so the "control follows use" rule, the undeclared clause of `required`, the
layout-order half of the ordering rule and the `inputs.all` union's widening rule are removed from the
spec rather than left unreachable in code.

**Out of scope.** What a *request* may carry is #324. A `data` key naming no declared parameter is
still carried into the render data untouched and is still read by nothing; this change does not refuse
it, and no requirement here says it does.

## Capabilities

### New Capabilities

None. The one first-touch requirement (`image` binding, superseding a `docs/SPEC.md` §8 bullet) lands
in the existing `interpolation-tokens` capability, which already owns what a bound name may be.

### Modified Capabilities

- `interpolation-tokens`: the bullet "the parameter of that name if the template declares one in
  `params:`, otherwise a field of the request `data` map" loses its `otherwise` clause, and a bare name
  the template does not declare becomes a load-time refusal. An `ADDED` requirement carries the
  complete post-change contract for `image` binding and supersedes the `image` binding bullet of
  `docs/SPEC.md` §8, which `interpolation-tokens:16` currently records as surviving untouched. The
  retired-`{datetime}` scenario is restated, because a template printing `{datetime}` is now quarantined
  unless it declares one. The mapping requirement is restated so the field its scenario prints is
  declared, keeping the distinction between a name a template reads **directly** and a connector key it
  reaches only through the field mapping.
- `param-resolution`: the preview requirement still says a preview is "the one place the service
  supplies a value the template does not declare", and builds a placeholder for "every request field or
  declared parameter" a token reads. Both clauses describe a template that no longer loads. This is a
  **specification correction only**: no render-path or preview behaviour changes, because the placeholder
  set is built from `inputs.all`, whose every entry is now a declared parameter. What a *request* may
  carry stays owned by #324, and this delta says nothing about it.
- `template-inputs`: the undeclared branch is removed from the entry rules (`control` follows use,
  `required`, entry ordering), from the `inputs.all` union rule, and from every scenario that turns on
  a name the template does not declare, including two thumbnail scenarios.

## Impact

- **Code.** Production code in `src/templates.rs` only: `validate_interpolated_string` gains a
  declaration check on a bare token, the `image` arm of `validate_item_references` calls
  `check_param_ref(…, &["string"])`, and `derive_inputs_internal`'s undeclared branch goes. Nothing in
  production code in `src/render/mod.rs`, `src/api.rs` or `src/batch.rs` changes: this is a load-time
  rule, and the render path already resolves a declared parameter the way it will now always be reached.
- **API.** No new status, `error.code` or `details.reason`. A template that breaks the rule is
  quarantined under the existing `template_validation_failed` contract. The `InputSpec` shape is
  unchanged; only which names can appear in it changes. The `param-resolution` delta adds no rule and
  removes none: it deletes two clauses about undeclared names from a requirement whose behaviour is
  otherwise untouched.
- **Templates in this repo.** Nil: all 5 under `catalog/` and all 18 under
  `tests/fixtures/templates/` already declare every name they read, and none carries an `image` item.
- **Tests.** Inline YAML across test modules (`src/templates.rs`, `src/lib.rs`, `src/render/mod.rs`,
  and `tests/`) that reads an undeclared name must declare it; inline `type: image` layouts need a
  declared `string` parameter for their `name:`.
- **Documentation.** `docs/AUTHORING.md` is updated to reflect that all template fields must be
  declared and `{datetime}` is a standard declared parameter.
- **UI.** No change. Every name the service reports as an input is now a declared parameter, which is a
  narrowing of what the screens already render.
