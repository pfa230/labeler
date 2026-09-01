## Why

[#310](https://github.com/pfa230/labeler/issues/310). A `params:` entry accepts an `enum:` key that
nothing reads. `RawParamSpec` declares it as the field `choices` (`src/raw.rs:87-92`), and no branch of
`ParamSpec::try_from` builds a type from it: only `values:` produces `ParamType::Enum`
(`src/convert.rs:583`). A comment at `src/convert.rs:581` records the situation ("`enum:` (`choices`) is
still parsed and still unused") and two assertions at `src/convert.rs:838-850` pin it rather than refuse
it.

The key therefore produces three different outcomes, none of them the one the author asked for:

- `type: enum` with `enum: [a, b]` builds `Enum { values: [] }`, which `validate()` rejects with
  "parameter 'x' enum values must not be empty" (`src/templates.rs:1330`). The author wrote the values
  and is told there are none.
- `type: integer` with `enum: [100, 400, 700]` parses, validates and renders with the constraint
  discarded and nothing said (`src/convert.rs:844-850`).
- `type: datetime` with `enum:` gets a pointed "enum is not supported on datetime parameters"
  (`src/convert.rs:542-547`), which implies the key is supported on some other type.

The only reason `enum:` parses at all is that `deny_unknown_fields` on `RawParamSpec` (`src/raw.rs:73`)
would otherwise refuse it, which is the behavior we are declining to have.

## What Changes

- **BREAKING.** `enum:` on a `params:` entry of **any** type becomes an unknown key. The template is
  quarantined at load with a `deny_unknown_fields` error naming the file and the key, and the server
  still starts. This replaces all three of today's outcomes at once: the misleading "values must not be
  empty", the silent discard on `integer`, and the pointed datetime message.
- Delete the `choices` field from `RawParamSpec` (`src/raw.rs:87-92`).
- Delete the datetime `enum` guard (`src/convert.rs:542-547`), which becomes unreachable once the key is
  unknown, and the sentence of the comment at `src/convert.rs:581` describing the key.
- Replace the two assertions at `src/convert.rs:838-850` that pin the ignoring with a test asserting the
  refusal on `type: enum` and on `type: integer`, the two shapes that behaved differently, plus a
  registry-level test that the quarantine names the file and the key.
- **The lost capability is dropped with no successor.** The `integer` row of the parameter-type table
  promised a constrained integer rendered as a dropdown. Neither half was ever implemented:
  `openspec/specs/template-inputs/spec.md:44` maps `integer` to the `integer` control with no `enum`
  branch, and `ui/src/components/ParamInput.tsx:110-118` renders a stepper. An `enum` parameter with
  string values already covers the picker case. No successor issue is filed and no replacement is built
  under this change.
- No deprecation window, no second spelling and no paragraph in `AGENTS.md` or `docs/AUTHORING.md`
  explaining the removed key, per the breaking-changes rule.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `datetime-params`: one requirement, already present in `openspec/specs/`, so the first-touch rule does
  not apply and the delta is `MODIFIED`.
  - **A datetime parameter names an instant, not a rendering** — it holds both places the key is
    contracted: the rejected-attribute list for `datetime` (`:261`), which drops `enum` and keeps its
    pointed message for the rest, and the parameter-type table (`:292`), whose `integer` row loses both
    the `enum` attribute and the "dropdown (if `enum` provided)" control. The requirement gains the
    post-change rule that `enum:` is not part of the schema for any parameter type.

`docs/SPEC.md:346` carries the same `integer` row and is frozen; it is not edited, and it is already
superseded for this table by the requirement above.

## Impact

- `src/raw.rs` (`choices` field), `src/convert.rs` (datetime guard, comment, tests), `src/templates.rs`
  (one new registry-level quarantine test and removal of stale `enum: [400, 700]` from
  `raw_template_deserializes_params_dynamic_values_and_when`'s inline fixture, which otherwise
  would now quarantine).
- `openspec/specs/datetime-params/spec.md`, through this change's delta and the archive sync.
- No UI file. `ParamInput.tsx` never read the key and its stepper is already what the post-change table
  specifies.
- No template file. No YAML under `catalog/` or `tests/fixtures/templates/` carries `enum:` on a
  parameter, so nothing in the repository is quarantined by this change.
- `options:` on a template is untouched: it desugars to an `Enum` params entry through its own path
  (`src/convert.rs:628-637`) and never reads `choices`.
- Gates: `cargo fmt`, `cargo clippy --all-targets --all-features`, `cargo test`.
