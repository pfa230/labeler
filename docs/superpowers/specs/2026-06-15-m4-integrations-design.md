# M4 — Integrations and import (design)

Status: approved (2026-06-15)
Issues: #14 (variable layer + QR base-URL + settings), #21 (CSV import). #22 (inbound print
webhook) deferred out of M4 until the consuming surfaces are better understood.
New ADR: ADR-0010 (variable / interpolation layer).

## Goal

Two integration capabilities for M4:

1. A **named-variable / interpolation layer** so template content can reference request data and
   stored settings (`{field}`, `{settings.qr_base_url}`), closing the CAPABILITIES §3.1 "no
   named-variable layer" gap. The motivating case is a QR whose payload is a configurable base URL
   joined with an item id.
2. **CSV import**: render one label per CSV row, downloaded as a ZIP or sent to a printer.

## 1. Interpolation / variable layer (#14, ADR-0010)

### Syntax

- `{field}` substitutes the value of `data["field"]` from the request, via the existing
  `value_to_string` (numbers/bools stringify as elsewhere).
- `{settings.<key>}` substitutes a value from the settings store.
- `{{` and `}}` emit literal `{` and `}`.
- An unresolved `{field}` (key absent from `data`) or a missing `{settings.<key>}` is an error:
  `422 MissingField`, with the offending token in `details`.

Substitution only. No formulas, conditionals, or `{field|default}` fallback — explicitly deferred.

### Where applied

Text content and QR content only. (Image `src`, lengths, and other fields are untouched.)

### Model change

`Text` and `Qr` items today carry `name: String`, meaning "the data key whose value is the
content." Add an alternative `value: String` holding an interpolated template. Exactly one of
`name` / `value` is required per item; supplying both or neither is a validation error.

- `name` remains the terse single-field shorthand, so every existing template parses unchanged.
- `value` unlocks templates like `"{settings.qr_base_url}/{id}"`.
- `name: id` is semantically equivalent to `value: "{id}"`.

This subsumes the "id-field mapping" half of #14: a template references whichever key holds the id
directly (`{id}`, `{asset_id}`), so no separate mapping config is needed.

Three layers updated together (per CLAUDE.md two-stage-parse rule): `raw.rs` (accept `value`),
`models.rs` (`Text`/`Qr` gain `value: Option<String>`, keep `name: Option<String>`), and the
`TryFrom` in `convert.rs` (enforce exactly-one-of).

### Wiring

`render_single_label`, `render_single_label_pdf`, and `render_sheet_labels` gain a settings-map
argument (`&BTreeMap<String, String>`), carried on `RenderContext`. The interpolator resolves
`{field}` from `data` and `{settings.*}` from that map. Handlers load settings once per request via
a new `Store::all_settings() -> Result<BTreeMap<String, String>, StoreError>`.

A single `interpolate(template, data, settings) -> Result<String, AppError>` helper (in
`render/helpers.rs`) is used by both the text and QR render paths. Name-based items keep their
current resolution (`value_to_string(data[name])`); value-based items run through `interpolate`.

### Validation interaction

Name uniqueness validation applies only to name-based items. Value-based text/QR items are
anonymous (like lines) and do not participate in name-uniqueness checks.

## 2. Settings API (#14)

- `GET /settings` → JSON object of all settings (`{ "qr_base_url": "https://..." }`).
- `PUT /settings/{key}` with body `{ "value": "..." }` → upsert; returns the stored pair.

Generic key/value over the existing `settings` table. The only documented key in M4 is
`qr_base_url`; the generic shape leaves room for later integration settings (e.g. Homebox URL/token)
without schema churn. Routes are top-level for now; `/api` namespacing arrives in M5 with the rest.

End-to-end QR-URL story: `PUT /settings/qr_base_url` → a QR item with
`value: "{settings.qr_base_url}/{id}"` → one starter template updated to demonstrate.

## 3. CSV import (#21)

### Endpoint

`POST /import/csv`, raw `text/csv` request body. The header row names the fields; each subsequent
row is one label's `data` (all values are strings). Query parameters:

- `template` (required) — template id.
- `mode` = `download` (default) | `print`.
- `printer` — required when `mode=print`; ignored otherwise.
- `format` — optional, same meaning and validation as `/print`.

Uniformly per-row: one artifact per row, no multi-page combining. The format follows the same rule
as the corresponding single-render path — `download` mode renders the template's natural format
(PNG, or PDF via `format`), `print` mode renders to the selected driver's accepted format, reusing
`/print`'s existing format resolution.

### Download mode

Returns `application/zip` (a new `zip` crate dependency) containing N files named by 1-based row
index zero-padded to the row count width (`001.png`, `002.png`, …). **Atomic**: the first row that
fails (missing interpolation field, render error, invalid option) fails the whole request with
`422` and the row index in `details`; no partial ZIP. A ZIP with silent gaps is worse than a clear
failure.

### Print mode

Dispatches N print jobs, one per row, so a continuous-tape printer auto-cuts between labels.
`record_job` is called per row. Response is a JSON summary:

```json
{ "total": 40, "succeeded": 38, "failed": [ { "row": 12, "error": "MissingField: id" } ] }
```

Continue-past-failure here, because already-sent jobs cannot be unwound; per-row errors are
collected rather than aborting.

### Out of scope (v1)

- Option selection per CSV row (single fixed option set not supported in import yet — noted as a
  follow-up if needed).
- Multipart upload. Raw `text/csv` body only; the React UI (M5) posts the file's text.
- Sheet/batch composition from CSV rows (that is #28's territory).

### Errors

- Empty body or header-only CSV (no data rows) → `400 InvalidRequest`.
- Malformed CSV → `400 InvalidRequest`.
- Unknown `template` → `404 TemplateNotFound`; unknown/disabled `printer`, `format`+constraints →
  same codes as `/print`.

## Error contract additions

No new error codes are strictly required: interpolation failures reuse `MissingField`; CSV shape
errors reuse `InvalidRequest`. If a distinct interpolation code proves useful during implementation
it will be added as an `AppError` constructor (stable `code` string) and recorded in SPEC.

## Testing

- Interpolation helper: `{field}` hit, missing `{field}` → error, `{settings.x}` hit + miss,
  `{{`/`}}` literal, mixed string. One positive + one negative per branch.
- Model validation: exactly-one-of `name`/`value` (both → error, neither → error).
- Render: a `value`-based text and a `value`-based QR produce the interpolated content (single +
  sheet paths).
- Settings API: `PUT` then `GET` round-trip; `GET` empty.
- CSV import: download ZIP has N entries with correct names; bad row fails atomically with row
  index; print summary reports per-row success/failure (using the `fake` driver).

## Docs to update in the same change

- `docs/SPEC.md` + its changelog: `value`/interpolation, `/settings`, `/import/csv`.
- `docs/adr/0010-variable-interpolation-layer.md` (new).
- `docs/CAPABILITIES.md` §3.1: mark the named-variable gap addressed.
- `docs/PLAN-phase-1.md`: M4 entries; mark #22 deferred.
- `openapi.rs`: register new request/response models and routes.
