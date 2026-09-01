## Why

Issue [#305](https://github.com/pfa230/labeler/issues/305). The template model has one spelling for a
declared input (`params:`) and one for conditional visibility (`when:`). The loader silently accepts
two dead spellings beside them and converts each into the survivor: `TemplateDefinitionRaw::options`
(`src/raw.rs:196`) is folded into `params` as an `enum` entry at `src/convert.rs:628`, and
`ContainerRaw::option` (`src/raw.rs:303`) is read as `when` at `src/convert.rs:300`.

A template written against the model ADR-0056 replaced therefore loads, validates and renders exactly
as if it had been written correctly, and nothing tells its author otherwise. `AGENTS.md` §*Breaking
changes, until 1.0* forbids exactly that: "a field read and ignored is what this forbids". Nothing
depends on either spelling. No YAML in this repository writes one — not `catalog/`, not
`tests/fixtures/templates/`, not `config-dev` — and the UI never sends one.

## What Changes

- **BREAKING.** The top-level `options:` field is deleted. A template declaring it is refused at load
  as an unknown top-level key, quarantined with a path naming the file and the key, and the server
  still starts and still serves every other template. There is no alias, no desugaring, no
  deprecation window and no warning path.
- **BREAKING.** The `option:` key on a `container` item is deleted. A container declaring it is
  refused the same way, with the error naming that item's layout path. `when:` is the only
  conditional-visibility key, on a container as on every other item.
- **BREAKING, over HTTP.** The set of YAML bodies `PUT /api/templates/{id}` accepts narrows by the
  same two keys, because that endpoint parses its body through the same parser before it writes
  anything (`src/api.rs:771` → `parse_and_validate`, `src/api.rs:639`). A body carrying either
  spelling is answered `422` with `error.code` `TemplateInvalid` and `error.details.reason`
  `template_parse_failed`, in a message naming the rejected key. Nothing is written: an existing
  template is left byte-for-byte unchanged, and a create-only write (`If-None-Match: *`) creates no
  file. That is the `template-registry` requirement *A `422` from a template write means nothing was
  written* applying unchanged to a newly refused body.
- The refusals are what `deny_unknown_fields` already gives once each field is gone. Removing the
  field *is* the whole implementation; there is nothing to migrate and nothing to warn about.

Not changed, and not back-compat:

- `TemplateContent::options()` (`src/templates.rs:90`) and `models::Options` (`src/models.rs:374`).
  They are a derived view over the `enum` entries in `params`, feeding `validate()`
  (`src/templates.rs:1110`) and the preview-only option-selection plumbing. Only `RawOptions`
  (`src/raw.rs:204`) goes, with its field.
- The renderer's internal option-selection argument (`normalize_option`,
  `default_option_selection`, the `option:` parameter on the `render/mod.rs` signatures). No request
  model reaches it, and `openspec/specs/param-resolution/spec.md` specifies it as preview-only
  behaviour. It is a live requirement, not a leftover.
- The `option` key a caller may put on a `LabelInput` and have swallowed (`src/models.rs:1222`,
  `ui/src/api/types.ts:101`, `ui/src/pages/Import.tsx:224`). That is #214.
- A CSV `option.<name>` column, folded into `data` at `src/api.rs:2733` and specified in
  `param-resolution`. Untouched.
- The `options_not_supported` failure reason (`src/reason.rs:67`), which answers for the request-side
  `option` map, not for either deleted spelling.

## Capabilities

### New Capabilities

- `conditional-visibility`: which key gates a layout item's visibility. `when:` is the only one, on
  every item type including `container`; `option:` is an unknown field wherever it appears, and its
  refusal is the ordinary quarantine. Supersedes the frozen `docs/SPEC.md` §5 insofar as that section
  names the key, and only insofar as it does: §5's evaluation semantics stay authoritative and are
  restated in the requirement so it stands alone.

### Modified Capabilities

- `template-groups`: the requirement *A group is a directory under the templates directory*
  (`openspec/specs/template-groups/spec.md:321`) carries the post-change table of top-level template
  fields, and its `options` row still reads "Optional, legacy… Still accepted; not for new
  templates". That row is struck. It is the only place in `openspec/specs/` naming the spelling, and
  leaving it would be a spec contradicting the loader.

## Impact

- `src/raw.rs`: `TemplateDefinitionRaw::options` and the `RawOptions` newtype go;
  `ContainerRaw::option` goes. Both structs already carry `deny_unknown_fields`.
- `src/convert.rs`: the `raw.options` fold into `params` goes; `self.when.or(self.option)` becomes
  `self.when`.
- Tests: four new ones, two per spelling. At the registry layer, a template carrying a top-level
  `options:` and one carrying a container `option:` are each quarantined with an error naming the
  key. At the HTTP layer, a `PUT /api/templates/{id}` carrying each spelling is answered with the
  complete error envelope above and leaves the templates directory untouched. No existing test
  asserts the desugaring — the repository holds none, contrary to the issue's expectation — so there
  is nothing to delete.
- No route, no handler, no OpenAPI model, no error code, no `details.reason` slug, no UI code and no
  YAML in this repository changes. What does change on the API is the contract of one existing
  response: which YAML bodies `PUT /api/templates/{id}` accepts, and therefore which ones it answers
  `422 TemplateInvalid` / `template_parse_failed` to. `GET`, `DELETE`, the render paths and every
  response schema are untouched.
