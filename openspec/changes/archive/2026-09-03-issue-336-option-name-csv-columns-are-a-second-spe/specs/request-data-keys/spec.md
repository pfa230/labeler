## REMOVED Requirements

### Requirement: A CSV data column names a declared parameter
**Reason**: The `option.<name>` column category is withdrawn; every CSV column is a data column. No
header carries an option.
**Migration**: Rename any `option.<name>` header to the declared parameter name.

## ADDED Requirements

### Requirement: A CSV data column names a declared parameter: every header is a data column

`POST /api/import/csv` SHALL refuse a CSV whose header carries a data column naming no parameter the
template declares. The refusal SHALL be a **whole-request** `400 InvalidRequest` with
`error.details.reason` `csv_data_column_unknown`, raised **before any label is built from the file and
before anything is rendered**, in both `mode=download` and `mode=print`. No label is rendered, no blob
is produced and no print job is dispatched.

It is a whole-request failure rather than a per-label one because a column is a property of the file
and not of one row: every row of the file carries it, so reporting it once per row would report the
same defect as many times as the sheet is long.

**Every unrecognized column is named, in ascending order**, on the same terms the requirement above
states for a label's keys: one failure, naming all of them sorted ascending by Unicode code point,
together with the template id the request named.

**The order of the file's own refusals is fixed, and this rule joins the end of it.** In order:

1. an unparsable header, or one with an empty or duplicate column name — `csv_header_invalid`;
2. an unparsable data row — `csv_row_invalid`;
3. a header with no data rows under it — `csv_empty`;
4. a data column naming no parameter the template declares — `csv_data_column_unknown`.

Steps 1 to 3 are unchanged and every file that fails one of them today SHALL keep reporting what it
reports today. A file with an unparsable row and an unrecognized column reports `csv_row_invalid`.

**The whole file is parsed before any column is judged, and that is deliberate.** This requirement
does not claim the refusal precedes parsing: a column set read from a header the parser has not
accepted is not a column set anyone can act on, and hoisting step 4 above steps 1 to 3 would
re-label files that fail today. What it claims is what a caller can observe — that no **label** is
built from the file, no render is attempted, and the whole request fails with one status. A
`ParsedCsvRow` is an intermediate of parsing the file, not a label: no label exists until the rows
have been turned into label inputs, and that happens after step 4.

**`csv_data_column_unknown` is a new entry in the error contract, and this requirement is its published
home.** The frozen `docs/SPEC.md` §10.1 is not edited and every row already there remains authoritative;
this requirement extends that registry by exactly one row, under `InvalidRequest`.

| Code | Status | Reason | When |
| --- | --- | --- | --- |
| `InvalidRequest` | `400` | `csv_data_column_unknown` | A CSV data column names no parameter the template declares. |

#### Scenario: An unrecognized data column is refused

- **WHEN** `POST /api/import/csv?template=shelf` is sent a sheet whose header is
  `title,sku_legacy` for a template declaring `title` and not `sku_legacy`
- **THEN** the response is `400` with `error.code` `InvalidRequest` and
  `error.details.reason` `csv_data_column_unknown`
- **AND** the `message` names `sku_legacy` and the template id `shelf`
- **AND** no ZIP or PDF is produced

#### Scenario: Print mode refuses it too

- **WHEN** the same file is sent with `mode=print` and a valid printer
- **THEN** the response is the same `400` and no print job is dispatched

#### Scenario: Every unrecognized column is named once

- **WHEN** the header is `title,zeta,alpha` and neither `zeta` nor `alpha` is declared
- **THEN** one failure is reported whose `message` names `alpha` then `zeta`, regardless of how
  many data rows the file holds

#### Scenario: An unparsable row is reported ahead of an unrecognized column

- **WHEN** a file carries an unrecognized data column and also a row the CSV parser rejects
- **THEN** the response reports `csv_row_invalid`, unchanged from today

#### Scenario: A header with no rows is reported ahead of an unrecognized column

- **WHEN** a file carries an unrecognized data column and no data rows
- **THEN** the response reports `csv_empty`, unchanged from today

#### Scenario: A column `option.size` naming no declared parameter fails as csv_data_column_unknown

- **WHEN** the header is `title,sku_legacy,option.size` where neither `sku_legacy` nor `option.size` is declared
- **THEN** the response reports `csv_data_column_unknown` naming both `option.size` and `sku_legacy` in
  ascending order

#### Scenario: A file naming only declared parameters imports

- **WHEN** the header names only declared parameters
- **THEN** the file imports exactly as it does today

### Requirement: The `csv_option_column_unknown` reason is withdrawn

This requirement supersedes the `docs/SPEC.md` §10.1 row `| InvalidRequest | csv_option_column_unknown | An option.* CSV column names an option the template does not declare. |` and states its complete post-change contract. The frozen document is not edited; every other row of §10.1 and every other section of `docs/SPEC.md` remains authoritative.

The `csv_option_column_unknown` slug SHALL be withdrawn, and SHALL NOT be raised by any code path:

| Reason | Why it can no longer occur |
| --- | --- |
| `csv_option_column_unknown` | Every CSV column is a data column; no header carries an option. The `Reason::CsvOptionColumnUnknown` variant at `src/reason.rs:94` and the `csv_option_column_unknown` branches at `src/api.rs:2727-2730` are deleted here. |

Adding no slug and withdrawing one is a change to the reason set that `docs/SPEC.md` §10.1 makes
part of the contract, and is recorded as a decision against ADR-0052.

#### Scenario: A withdrawn slug is unreachable

- **WHEN** the reason set is enumerated
- **THEN** `csv_option_column_unknown` is absent
- **AND** the registry test fails if it is reintroduced without a spec change
