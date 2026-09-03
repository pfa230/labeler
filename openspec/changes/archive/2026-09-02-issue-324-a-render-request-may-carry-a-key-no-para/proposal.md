## Why

Implements [#324](https://github.com/pfa230/labeler/issues/324), the request half of the split #322
landed. `params:` is the complete vocabulary a template reads, and since #322 a template can read no
other name. The request side still disagrees: `resolve_parameters_mode` starts from
`let mut resolved = data.clone()` (`src/render/mod.rs:176`) and then iterates only `template.params`, so
a `data` key naming no declared parameter is copied into the render data and read by nothing. It is a
typo, a stale field name, or a column belonging to another template, and today the service renders the
label and says nothing.

The precedent is already here and already cited to the frozen spec: an unknown `option.<name>` CSV
column is refused rather than ignored (`src/api.rs:2715-2727`, "Per SPEC section E, an unknown
option.`<name>` column is an error, not silently ignored"). This applies that posture to `data` keys, so
the two stop disagreeing about what an unrecognized column means.

## What Changes

**BREAKING.** Pre-1.0: no migration, no deprecation window, no flag, no lenient mode.

- **Every key of a request's `data` map SHALL name a parameter the template declares.** A key that
  names none SHALL be refused on every path that renders or prints: `POST /api/render/label`,
  `POST /api/batch`, `POST /api/print` and `POST /api/import/csv`. What may be sent is the
  **declarations**, not the input list: a declared parameter no active item reads is a legal key,
  because `when:` narrows the input list and narrows nothing about what a caller may carry.
- **One new reason on the render and print paths, `data_key_unknown`.** On `POST /api/render/label` it
  is a top-level `400 InvalidRequest`. On `POST /api/batch` and `POST /api/print` it is one entry per
  offending label inside the existing `422 BatchInvalid` envelope, carrying that label's `index`, the
  code `InvalidRequest` and that reason, alongside every label failing for any other reason.
- **One new reason on the CSV path, `csv_data_column_unknown`,** a whole-request `400 InvalidRequest`
  raised before any label is built from the file, in both `download` and `print` mode. A column is a
  property of the file rather than of one row, which is why it is not reported per label. It is the
  last of the file's own refusals: `csv_header_invalid`, `csv_row_invalid`, `csv_empty` and
  `csv_option_column_unknown` all still precede it, so no file that fails today changes what it reports.
- **A failure names every unrecognized key, in ascending name order.** A `data` map is unordered, so
  "the offending key" is undefined when a label carries several and any iteration order would differ
  between two runs of the same request. One failure per label, listing all of them, sorted, and the
  same for CSV columns.
- **The failure is reported per label, inside the loop that already visits every label**, not as a
  whole-request preflight. A preflight would silently drop the frozen guarantee that `details.failures`
  lists *every* failing label: a batch whose label 0 carries a bad key and whose label 1 omits a
  required field would report only label 0. Atomicity is a guarantee about **output**: a failing
  request produces nothing, and whether a passing label was rendered before a later one failed is not
  observable and is not constrained.
- **`POST /api/print` with `copies: N` reports the failure N times**, at indices `0` through `N-1`,
  because `copies` expands the one `data` map into N labels. That is what the endpoint already does for
  every kind of label failure; the restated contract says so rather than leaving it to be inferred.
- **`POST /api/templates/{id}/inputs` stays lenient and keeps ignoring such a key.** The Import screen
  posts a row's whole `data` map there to derive that row's inputs (`ui/src/lib/labelInputs.ts:42`),
  including CSV columns the chosen template never declares. This is a scope boundary stated from both
  sides, not an exception carved out of a universal rule: the rule is scoped to the four paths that
  render or print, and the `template-inputs` requirement that today says resolution there "differs from
  rendering in exactly one way" now says two.
- **The frozen batch-validation contract is restated, because a label can now fail before any render.**
  `docs/SPEC.md` §2.2 says "Every label is render-validated first", §2.3's error table says
  `422 BatchInvalid` means "A rendered label is invalid", and §10's code table says "One or more
  `/batch` labels failed render-validation". All three describe render-validation as the only way a
  label fails. None has a home in `openspec/specs/`, so this is a first-touch `ADDED` requirement
  carrying the complete post-change contract for the three of them.

## Capabilities

### New Capabilities

- `request-data-keys`: what a request's `data` map may carry — every key names a declared parameter, on
  the four paths that render or print; the CSV data-column form of the same rule; the boundary that
  leaves `POST /api/templates/{id}/inputs` lenient; and the published home of `data_key_unknown` and
  `csv_data_column_unknown`.
- `batch-validation`: what `POST /api/batch` and `POST /api/print` check before they execute, and how
  those failures are reported. First-touch, superseding `docs/SPEC.md` §2.2's "Validate-then-execute"
  paragraph, §2.3's `422 BatchInvalid` error-contract row and §10's `BatchInvalid` code-table row.

### Modified Capabilities

- `template-inputs`: "The service computes an input list for a given label" says lenient resolution on
  that path "differs from rendering in exactly one way". A second way now exists, so the requirement
  states both and says which paths refuse the key it ignores.

## Impact

- **Code.** One shared name-collector, returning the sorted unrecognized names, and two error contracts
  built on top of it at four call sites: the `POST /api/render/label` handler (`src/api.rs`),
  `render_single_batch`'s per-label loop (`src/batch.rs:93`) and `render_sheet_pages`' per-label loop
  (`src/render/mod.rs:951`) raise `data_key_unknown`; `import_csv` (`src/api.rs`) raises
  `csv_data_column_unknown` beside the `option.` column check it mirrors. `render_sheet_batch` has no
  loop of its own (`src/batch.rs:167` delegates the whole slice), so the sheet path has nowhere else
  that both sees every label and reports each one's failure. See `design.md` — Decisions for why the
  shared piece stops short of building the error.
- **A signature widens.** The message names the template id, and `TemplateContent` has no `id`
  (`src/templates.rs:61-65`); the id lives on `TemplateDefinition`, which `Deref`s to the content.
  `render_sheet_pages` takes `&TemplateContent` and must take `&TemplateDefinition`. Its one production
  caller already holds one. `resolve_parameters_mode` is **not** widened: `derive_inputs_for_label`
  calls it from a bare `TemplateContent`, and the check does not belong inside resolution anyway.
- **API.** No new status and no new `error.code`; two new `details.reason` slugs under `InvalidRequest`.
  The `BatchFailure` shape is unchanged, and a batch entry carrying the `InvalidRequest` code is
  already how a bad datetime value is reported (`datetime-params`).
- **Not the web UI.** Every screen that renders or submits prunes each row's `data` to that row's input
  list first — `pruneDataForSubmit` in `Import.tsx:274`, `Connect.tsx:257` and `print/PrintForm.tsx:121`
  — and the input list is narrower than the declarations this change permits, so no screen can send an
  unrecognized key. No UI change is required and none should be invented. The `/inputs` leniency above
  is what keeps the Import screen's per-row derivation working.
- **Connectors.** A materialized row's `data` is keyed by the **template field** side of the field
  mapping, never by the connector's own field keys (`rowsFromMaterialized`,
  `ui/src/lib/connectorRows.ts:20-38`), and that map is built from the template's declared fields. The
  connector path therefore cannot introduce an undeclared key either.
- **API callers.** A `POST /api/render/label`, `/api/batch` or `/api/print` carrying a stale field name
  now fails instead of rendering a label missing that value. That is the point.
- **`POST /api/import/csv`.** Nothing in the UI calls it, so this lands on direct API users: a CSV whose
  sheet carries columns beyond the template's parameters is refused rather than partly used, and the
  operator must trim the export or declare the column. This was raised as an objection when the issue
  was scoped and overruled deliberately: silence about a key nobody reads is the thing being removed.
- **Unaffected.** The thumbnail and the server's own preview build their data from declared parameters
  and do not go through the four checked paths, so they are untouched. `param-resolution` needs no
  delta: it decides the value a declared parameter takes, and a key naming no parameter supplies no
  value to anything. `print-request-body` needs none either: it constrains the **top-level** body keys,
  and this change constrains the contents of `data`.
- **Templates in this repo.** Nil. No template is edited and no template's meaning changes.
- **Tests.** New HTTP-level tests per path, including the sheet path, which needs its own because its
  loop lives in `src/render/mod.rs`. Existing inline tests calling `render_sheet_pages` with a bare
  `TemplateContent` (`src/render/mod.rs`, `tests/acceptance_issue_263.rs:354`) must wrap it in a
  `TemplateDefinition`.
- **Documentation.** `docs/SPEC.md` is frozen and is not edited. `docs/AUTHORING.md` gains nothing: it
  describes templates, and this change constrains requests.
