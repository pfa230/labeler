# enum-validation Specification

## Purpose

Defines the error the service returns when a request supplies a value for an `enum` parameter that is not a member of its declared `values`, including the stable `code`, status, message, `details` shape, and per-row batch reporting.

## Requirements

### Requirement: An out-of-range enum value is rejected with InvalidEnumValue

*This requirement supersedes the enum-validation sentence in `docs/SPEC.md` §5 (`docs/SPEC.md:566-567`) reading "If a request supplies a value for an `enum` parameter that is not member of its declared `values` list, the request is rejected with `422 InvalidOptionValue`", the `InvalidOptionValue` row of the error-code table in `docs/SPEC.md` §10 (`docs/SPEC.md:683`) reading "`InvalidOptionValue` | 422 | Option selection not allowed by the template.", and the CSV import clause in `docs/SPEC.md` § CSV import (`docs/SPEC.md:1069`) reading "and a disallowed enum value fails the row (`BatchInvalid` / `InvalidOptionValue`)." It restates that code's complete post-change contract under the new name. It supersedes no other row of that table and no other part of §10 beyond the named sites, and it supersedes no row of `docs/SPEC.md` §10.1, because this code carries no `details.reason` and never has. Every other row of §10 and every row of §10.1 remains authoritative under the frozen spec. It supersedes nothing else in § CSV import. The rest of that section, including the clause of the same sentence at `docs/SPEC.md:1068` reading "Any declared parameter the CSV omits defaults to its declared `default` value", remains authoritative under the frozen spec.*

When a request carries a value for a parameter declared `type: enum` that is not a member of that parameter's declared `values` list, the service SHALL reject the request with the error code `InvalidEnumValue`.

The response SHALL be:

| Field | Value |
| --- | --- |
| `code` | `InvalidEnumValue` |
| `status` | `422` |
| `message` | `Invalid option selection` |

`message` stays byte-identical to the value the service returned under the previous code `InvalidOptionValue`. `status` stays byte-identical.

`details` SHALL be present and SHALL carry exactly the two keys the previous code carried, with byte-identical names and values:

- `selection`: a map from the offending parameter name to the supplied value as a string.
- `allowed`: a map from the offending parameter name to the parameter's declared `values` list in declared order.

No key is renamed, added, or removed. In particular `details.selection` / `details.allowed` are not reshaped into better-named keys; a reshape is a different change and is out of scope. The `details` object SHALL contain no `reason` key.

This error SHALL be raised through the strict (non-lenient) param coercion path. The lenient `POST /api/templates/{id}/inputs` path SHALL NOT raise it; it treats the out-of-range value as though the label did not carry that name at all, so the parameter takes its declared `default` if it has one and is otherwise absent, per `template-inputs` and `param-resolution`.

Inside `POST /api/batch`, `POST /api/print`, and `POST /api/import/csv`, the per-label failure SHALL be reported per `batch-validation`: the top-level response is `422 BatchInvalid` and `details.failures` holds an entry per failing label carrying `code` `InvalidEnumValue`, the same `message`, and no per-row `reason` (this code has none). The per-row `message` is `Invalid option selection` as above.

#### Scenario: A render request carrying a value outside an enum's values is rejected

- **WHEN** a template declares `orientation` with `values: [horizontal, vertical]` and a render request carries `data: { orientation: "sideways" }`
- **THEN** the response status is `422`
- **AND** `error.code` is `InvalidEnumValue`
- **AND** `error.message` is `Invalid option selection`
- **AND** `error.details.selection` is `{ "orientation": "sideways" }`
- **AND** `error.details.allowed` is `{ "orientation": ["horizontal", "vertical"] }`
- **AND** `error.details` carries no `reason` key

#### Scenario: A batch row failing the same way reports InvalidEnumValue in its per-row failure

- **WHEN** a `POST /api/batch` request carries two labels and the second label carries `data: { orientation: "sideways" }` for the same template
- **THEN** the response status is `422`
- **AND** `error.code` is `BatchInvalid`
- **AND** `error.details.failures[0].code` is `InvalidEnumValue`
- **AND** `error.details.failures[0].message` is `Invalid option selection`
- **AND** `error.details.failures[0]` carries no `reason` key

#### Scenario: Details remain exactly selection and allowed after rename

- **WHEN** a render request carries an out-of-range enum value and is rejected as above
- **THEN** `error.details` contains exactly `selection` and `allowed` and no other key, proving the rename did not reshape the object
