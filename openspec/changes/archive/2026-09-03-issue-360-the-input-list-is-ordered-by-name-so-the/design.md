# Design: Template params as a sequence

## Context

See proposal.md: the input list is sorted by name because `params` has no order to keep. YAML mappings are parsed into `BTreeMap` at `src/raw.rs:188`, carried as `BTreeMap` through `src/templates.rs:55` and `src/models.rs:101`, and the input derivation at `src/templates.rs:512` sorts by name for lack of alternative. The UI then renders whatever order the service publishes (`FieldForm.tsx:61`, `TemplateDetail.tsx:286`), and the two batch grids walk that list (`Import.tsx:136`, `Connect.tsx:153`). The only way to reorder a form today is to rename its parameters.

A template is also the wire shape. The list summary and the detail both publish `params` as an object keyed by name (`src/models.rs:62`, `:101`), so the same unordered container appears in two places.

## Goals / Non-Goals

**Goals:**
- A template author controls form order by declaration order, in one place.
- One field, one shape, one reader: the same array on summary and detail, in the same order.
- Lookups stay as they are: the 13 `params.get`/`contains_key` sites do not become linear scans.
- A mapping-shaped `params:` and a duplicate `name:` are refused with a parse/validation error naming the file and the name, as a dropped spelling before 1.0 must be.
- Input-list ordering, params file shape, params wire shape, and duplicate-name refusal are one coherent contract.

**Non-Goals:**
- Migrating stored user templates, desugaring the map, or keeping an alias. Breaking before 1.0 is the finished job (`AGENTS.md`); `deny_unknown_fields` gives the parse error once the field is gone, and an ignored field is what this forbids.
- Changing `variables` (ascending stays correct: those are layout-read keys with no declaration site) or `param_defaults` (keyed for lookup, never iterated).
- Adding ordering code to the batch grids or the connector mapping screen: they already walk the input list.
- Changing `store.rs` or the SQLite schema: this change touches no persisted state.

## Decisions

### `params` is a sequence in the file and an array on the wire

Each entry is flat and carries its own `name:`, the way a `layout:` item carries its fields. The YAML `params:` key therefore holds a sequence (`- name:` items) rather than a mapping, and the JSON `params` field on both `GET /api/templates` (summary) and `GET /api/templates/{id}` (detail, plus create/replace/move responses) is that same array, in declaration order.

Why a sequence and not a map with the order beside it: a map keyed by name needs a second field to carry order, because `Object.entries` enumerates digit-leading keys numerically whatever order the JSON carries, so a template declaring `10` and `2` would display them reordered. That is two spellings of one fact plus a paragraph explaining that the first cannot be trusted. A sequence makes the order intrinsic and the hazard stops existing rather than being routed around. The issue explicitly rejects the map-plus-order spelling for this reason.

Alternatives:
- **Keep the map on the wire, add `param_order: string[]` beside it.** Rejected: two fields for one fact, plus the `Object.entries` hazard above, plus every reader must join them. The spec would need a paragraph explaining that the first field cannot be trusted for order.
- **Keep the map in YAML but preserve insertion order via `IndexMap` at parse.** Rejected: YAML mappings have no defined order in the spec, parsers may or may not preserve it, and an author cannot tell whether file order survived. The wire would still need a single authoritative order.

The file sequence and the wire array are the same logical value, so one conversion path serves both summary and detail.

### Internal container is ordered, lookups stay O(1)

`TemplateContent.params` must both iterate in declaration order (for input derivation, validation error precedence, and wire serialization) and answer `contains_key`/`get` efficiently for the 13 existing sites. The container is therefore order-preserving and keyed by name (`IndexMap<String, ParamSpec>` or a `Vec<ParamEntry>` plus a `BTreeMap` index built once at conversion). Nothing at those call sites changes shape: they continue to call `params.get(name)` and `params.contains_key(name)`. The conversion step builds the ordered container and refuses duplicate `name` there.

This avoids touching every call site to become a linear scan, and avoids making the wire container and the internal container diverge: both derive from one ordered source.

Input derivation then drops its `sort_by(|a,b| a.name.cmp(&b.name))` at `src/templates.rs:512` and walks the ordered params (or the collected `NameInfo` keyed by declaration order). Which parameter's error surfaces first on `src/templates.rs:1008`, `src/convert.rs:743`, `src/render/mod.rs:230` naturally becomes declaration order, because those loops iterate `params`.

### Duplicate `name` is a conversion-stage error, not a silent collapse

A `BTreeMap` collapsed duplicates by last-write-wins. A sequence with explicit `name:` must refuse two entries sharing a `name:` as a conversion-stage validation error naming the file and the name. This is the only new validation that the sequence shape requires, and it is decided at conversion inside `parse_template` (`src/parse.rs:25-34`), after `deny_unknown_fields` has already rejected a mapping-shaped `params:`. Every such failure (mapping shape, explicit null, duplicate name) therefore maps to `template_parse_failed`; only later `validate()` failures become `template_validation_failed` (`src/api.rs:640-645`). The conversion-stage precedent explicitly confirms this behavior (`openspec/specs/list-params/spec.md:62-72`), so the duplicate-name write refusal is `template_parse_failed`, not `template_validation_failed`, and the shared parse/validation classifier is not widened.

Where multiple entries are invalid, the error is for the declaration-order first entry; no path reports errors in name order. This covers the three iteration sites (`src/templates.rs:1008`, `src/convert.rs:743`, `src/render/mod.rs:230`) and any future loop over `params`. Reverse-alphabetical multi-error cases exercise conversion, template validation, and render-time coercion each.

### `params: {}` mapping no longer parses; `params: null` is not an empty list

`TemplateDefinitionRaw.params` becomes `Vec<RawParamEntry>` with `#[serde(default)]` rather than `Option<Vec<RawParamEntry>>`. Omission then defaults to an empty vector (no params), while explicit `params: null` fails deserialization at the `params` path, because Serde treats missing and explicit null alike for plain `Option` (`src/models.rs:173-177` is the existing illustration of that hazard). The mapping shape and the explicit null both fail at the `params` path as `template_parse_failed`, which the quarantine path reports as `TemplateError::Yaml`. The same content arriving through a template write is refused with `422 TemplateInvalid` and `details.reason` `template_parse_failed`. `deny_unknown_fields` on `RawParamEntry` still guards the entry shape. No explicit migration, no alias, no ignored field: this is the breaking change the issue states, covering 16 YAML files under `catalog/` and `tests/fixtures/templates/`, inline Rust fixtures, and `docs/AUTHORING.md`.

### Ownership partition for `docs/SPEC.md` §3.0

`docs/SPEC.md` §3.0 is partitioned explicitly and uniquely: its opening declaration/container example (whether `params:` is a mapping or a sequence, order, and examples) is now owned by `template-inputs: Template params are declared as a sequence and published as an array`; its per-entry/type table (which YAML attributes each type permits, request shape, omission behavior, and the type table itself) is owned by `datetime-params: A datetime parameter names an instant, not a rendering`; and its "Namespace rules and reserved names" list is owned by `interpolation-tokens: A bare name is a bare name, and no word is reserved`. The companion MODIFIED in `datetime-params` no longer claims the container shape and the duplicated paragraph is removed. The top-level field table entry for `params` in `docs/SPEC.md` §3 stays superseded by `template-groups` as modified herein, whose authority paragraph now names all three §3.0 owners. No frozen paragraph is claimed twice.

### Input-list ordering follows declaration order, not name, not layout

`template-inputs` currently says `Entries SHALL be ordered by name, ascending` with the justification `because params is a map keyed by name and authoring order is not retained`, and its scenario is `Entries are ordered by name, then by first use`. The second group was already gone (every entry names a declared parameter), but the first group stays until this change. Post-change the input list is ordered by `params` declaration order, for both `inputs.default` and `inputs.all`, and for `POST /api/templates/{id}/inputs`. `variables` stays ascending because it has no declaration site.

The UI consequence is immediate: `FieldForm.tsx:61` maps `activeInputs` in the order it receives; `TemplateDetail.tsx:286` renders the Parameters card in the order it receives; the batch grids already walk the input list. No UI sorting code is added.

## Risks / Trade-offs

- **Wire break on `params`:** clients reading `params` as a `Record<string, ParamSpec>` will fail to deserialize an array. Acceptable: before 1.0, and the issue states the wire becomes an array on both endpoints; no client retains record access to the old shape. OpenAPI schemas change from map to array; generated types in `ui/src/api/types.ts` change from `Record` to `{name,type,...}[]`. An omitted or empty `params:` is always `[]`, never omitted, preserving field presence.
- **Catalog and fixture rewrite is pervasive:** every YAML file and every inline `params:` in tests must move to `- name:` form. A single missed mapping keeps quarantining as broken, which the `TemplateRegistry` quarantine path makes visible rather than fatal, but `git grep 'params:'` must be swept. The rewrite is mechanical and verifiable by `cargo test` (parse) plus `GET /api/templates` `broken` list being empty.
- **Digit-leading name hazard is gone, but existing names like `10` were never valid:** parameter names match `^[a-zA-Z0-9_-]+$`; a leading digit is allowed, so `10` is a legal name and the `Object.entries` numeric-key reordering would have applied under a map shape. The sequence removes the hazard entirely rather than documenting it.
- **Error precedence changes:** where today the first error is the alphabetically first name, tomorrow it is the declaration-first name. callers depending on name order for error precedence will see a different first error; the spec now says declaration order is the answer and no path may depend on name order, so this is intentional.
- **Internal container choice is not a visible contract:** whether `TemplateContent.params` is `IndexMap` or `Vec+Map` is implementation detail as long as iteration is declaration order and lookups remain. Both satisfy the 13 call sites.
