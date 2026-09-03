# Proposal: Template params as a sequence — declaration order drives the form

Implements [#360](https://github.com/pfa230/labeler/issues/360).

## Why

An operator filling a label sees fields in ascending name order, never in the order the template author wrote them. A template that declares `title`, `subtitle`, `code` renders the form as `code`, `subtitle`, `title`, and the author has no way to say otherwise: renaming a parameter is the only lever on the form's reading order, which is the wrong lever. The reason is structural: `params:` is a mapping (`src/raw.rs:188` as `Option<BTreeMap<String, RawParamSpec>>`), so file order is discarded at parse, `TemplateContent.params` (`src/templates.rs:55`) and `TemplateDetail.params` (`src/models.rs:101`) carry it forward as a `BTreeMap`, and `src/templates.rs:512` sorts the derived input list by name because there is nothing else to sort by. The published contract records that choice explicitly (`openspec/specs/template-inputs/spec.md:134`).

The fix is to make `params` a sequence in the file and on the wire, so order is intrinsic.

## What Changes

- **Template file:** `params:` becomes a sequence of entries, each flat and carrying its own `name:`, the way a `layout:` item carries its fields:

  ```yaml
  params:
    - name: title
      type: string
      default: Untitled
    - name: code
      type: string
  ```

  A `params:` mapping no longer parses; it is quarantined with a parse error naming the file, as `AGENTS.md` requires of a dropped spelling before 1.0. Two entries sharing a `name:` are refused with a validation error naming the file and the name; the map used to collapse them silently.

- **Wire:** `params` becomes an array of those entries, in declaration order, on **both** the list summary (`GET /api/templates`) and the template detail (`GET /api/templates/{id}` and every response carrying that body: create/replace/move). One field, one shape, one reader. An omitted or empty `params:` is published as `[]`, never omitted.

- **Input list:** entries are ordered by declaration, not by name. The sentence justifying ascending order by the map's shape and its scenario "Entries are ordered by name, then by first use" (`spec.md:205`) keeps its name but asserts declaration order.

- **Error precedence:** where validation or conversion would report an error for more than one parameter, the error is for the declaration-order first parameter; no path reports errors in name order.

- **Out of scope / unchanged:**
  - `variables` stays ascending: those are keys the layout reads, with no declaration site to order by.
  - `param_defaults` stays keyed by name: it is looked up, never iterated.
  - Batch grids (`ui/src/pages/Import.tsx:136`, `ui/src/pages/Connect.tsx:153`) follow with no ordering code of their own: they walk the input list.
  - The existing handling of `list` entries is unchanged.
  - Which parameter's error surfaces first on the paths that iterate `params` (`src/templates.rs:1008`, `src/convert.rs:743`, `src/render/mod.rs:230`) moves from name order to declaration order, now normatively specified.

- **Breaking scope in this repo:** 16 YAML files under `catalog/` and `tests/fixtures/templates/`, inline fixtures across Rust tests, and worked examples in `docs/AUTHORING.md`. A template in a user's `LABELER_CONFIG_DIR` is theirs to rewrite.

A map keyed by name with the order beside it was rejected. Keeping the map means order must travel beside it in a second field, because `Object.entries` enumerates digit-leading keys numerically whatever order the JSON carries, so a template declaring `10` and `2` displays them reordered. That is two spellings of one fact plus a paragraph explaining the first one cannot be trusted. A sequence makes the order intrinsic and the hazard stops existing rather than being routed around.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `datetime-params`: **`A datetime parameter names an instant, not a rendering`** — container shape is repartitioned to `template-inputs`: `params:` is a sequence in declaration order, not a mapping; duplicate-name and mapping-shape refusals are governed there. Per-entry attribute rules and type table are unchanged.
- `template-inputs`: **`Template params are declared as a sequence and published as an array`** (ADDED, supersedes `docs/SPEC.md` §3.0 as repartitioned from `datetime-params`) — `params:` mapping is gone; params are a sequence of `{name, type, ...}` objects on disk and an array on the wire for both summary and detail, in declaration order, with duplicate-name refusal and declaration-order error precedence (no path reports errors in name order). An omitted or empty `params:` is published as `[]`, never omitted or as an object.
- `template-inputs`: **`An input list describes the controls one label needs`** — ordering changes from `by name, ascending` to `by declaration order`. Scenario `Entries are ordered by name, then by first use` keeps its name but asserts declaration order.
- `template-inputs`: **`The template detail reports what each declared default resolves to`** — scenario `The list endpoint is unchanged` now asserts that `GET /api/templates` summaries carry `params` as an array in declaration order (still with no `param_defaults`), fixing the contradiction with the array wire change.
- `template-groups`: **`A group is a directory under the templates directory`** — post-change top-level field table row for `params` changes from `map | Optional. Map of parameter name -> ParamSpec` to `sequence | Optional. Sequence of ParamSpec entries carrying name`.

## Impact

- `src/raw.rs`: `TemplateDefinitionRaw.params` from `Option<BTreeMap<String, RawParamSpec>>` to `Vec<RawParamEntry>` with `#[serde(default)]` where `RawParamEntry { name, spec }` carries `deny_unknown_fields`; `name` validated like any param name. Omission becomes an empty vector while `params: null` fails deserialization, so the explicit null is refused as `template_parse_failed` rather than becoming an empty list.
- `src/convert.rs`: `TryFrom<TemplateDefinitionRaw>` builds an order-preserving map for internal lookups (e.g., `IndexMap` or `Vec`+`BTreeMap` dual) while retaining declaration order for serialization; duplicate `name` refused.
- `src/models.rs`: `TemplateSummary.params` and `TemplateDetail.params` from `BTreeMap<String, ParamSpec>` to `Vec<ParamEntry>` (`name` + flattened `ParamSpec`) preserving order; `TemplateContent.params` becomes ordered map (`IndexMap` or `Vec`+index) so the 13 `params.get`/`contains_key` sites (`src/templates.rs:429,...`, `src/render/mod.rs:170,6203`) keep working.
- `src/templates.rs`: `derive_inputs_internal` ordering from `sort_by name` to declaration order; `validate_params` iteration follows declaration order.
- `src/openapi.rs`: schemas for `TemplateSummary`/`TemplateDetail` updated to array shape.
- `src/parse.rs` / errors: mapping-shaped YAML now fails at the `params` path with unknown-field/sequence error naming file.
- `catalog/` + `tests/fixtures/templates/` + `docs/AUTHORING.md` + Rust inline fixtures: rewritten to sequence form.
- `ui/src/api/types.ts`, `ui/src/pages/TemplateDetail.tsx:286`, `ui/src/pages/print/FieldForm.tsx:61`: consume `params` as array in declaration order; no extra sort.
- Tests: input-list ordering cases now assert declaration order with a conflicting layout first-use order; new cases for `params: null` refusal, duplicate-name refusal (`template_parse_failed` at conversion) and map-shaped rejection; `ui` params card, print form (`FieldForm.tsx:61`), Import grid (`Import.tsx:136`) and Connect grid (`Connect.tsx:153`) assert they preserve input-list order without sorting; reverse-alphabetical multi-error cases assert conversion, template validation, and render-time coercion each surface the declaration-order first error.

No migration, no alias, no second spelling. Stored user templates outside this repo are the caller's to rewrite; the SQLite schema (`store.rs`) is untouched because it holds printers and tokens, not templates.
