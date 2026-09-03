## MODIFIED Requirements

### Requirement: The `options_not_supported` reason is withdrawn

This requirement supersedes the `docs/SPEC.md` §10.1 row `| InvalidRequest | options_not_supported | An option selection was sent for a template that declares none. |` and states its complete post-change contract. The frozen document is not edited; every other row of §10.1 and every other section of `docs/SPEC.md` remains authoritative.

The `options_not_supported` slug SHALL be withdrawn, and SHALL NOT be raised by any code path:

| Reason | Why it can no longer occur |
| --- | --- |
| `options_not_supported` | The `Reason::OptionsNotSupported` variant and the `normalize_option` branch at `src/render/mod.rs:1224-1229` are deleted here. That branch was already unreachable at HEAD — every production call site passes `None` (`src/api.rs:2677,2681`, `src/batch.rs:105-106`, `src/api.rs:1254` and thumbnail paths) and `LabelInput` at `src/models.rs:1255-1257` has no `option` field so a carried key is dropped rather than forwarded — and `LabelInput`/`RenderLabelRequest` now both carry `deny_unknown_fields`, so a future `option` key is refused as `json_malformed` in any case. Every CSV column on `POST /api/import/csv` is now a data column judged under `csv_data_column_unknown`, and `csv_option_column_unknown` is itself withdrawn (`request-data-keys`). |

Adding no slug and withdrawing one is a change to the reason set that `docs/SPEC.md` §10.1 makes part of the contract, and is recorded as a decision against ADR-0052. The `code` `InvalidRequest` and the `json_malformed` reason that replaces this path are unchanged.

#### Scenario: A withdrawn slug is unreachable

- **WHEN** the reason set is enumerated
- **THEN** `options_not_supported` is absent
- **AND** the registry test fails if it is reintroduced without a spec change

