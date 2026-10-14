## Why

A template cannot gate an item on whether a parameter is empty: `when:` only compares against a non-empty value, and a condition on an absent parameter is false. The live `brother_12mm_item` template therefore needs a `show_code` boolean beside its `code` field, so two inputs carry one intent and a code typed without ticking the box prints nothing.

Fixes #437

## What Changes

- A `when:` condition MAY be the mapping `{ empty: true }` or `{ empty: false }` instead of a scalar. `empty: true` matches when the parameter is absent or reads as the empty string; `empty: false` matches otherwise.
- Scalar conditions keep their meaning, including false on an absent parameter.
- Every other mapping-valued condition stays refused at load, as does an `empty:` that is not a boolean.

Existing mechanisms that do not cover the need:

- Scalar `when:` equality cannot express "not empty", refuses `""`, and is false on an absent parameter.
- A `content`-sized container in a flow row collapses its content when the value is empty, but its padding and `background` still paint, so the black code block would remain as a sliver.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `layout`: "when gates an item" gains the `{ empty: <bool> }` condition form and its load-time refusals.
- `parameters`: "A value comes from the request or the declared default" stops saying an absent parameter makes every `when:` predicate false, since `{ empty: true }` matches it.

## Impact

- `src/raw.rs` (`deserialize_when_map`), `src/models.rs` (the `when` field type), `src/templates.rs` (`validate_when`, `validate_when_references`), `src/render/mod.rs` (`is_item_active`), and the tests and fixtures that build `when` maps.
- `docs/AUTHORING.md`, section "Containers, `when:` and rotation".
- No API, UI or error-slug change: a malformed condition is the existing `TemplateInvalid` load refusal naming the key's path.
- Live templates (operator data, not this repository): after deployment `brother_12mm_item` and `brother_12mm_item_split` can drop `show_code`/`show_code2` and gate on `code`/`code2`.
