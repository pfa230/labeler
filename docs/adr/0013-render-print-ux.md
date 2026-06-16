# 13. Render & Print UX decisions

**Status:** Accepted

## Context

ADR-0008 decided the web UI is a React SPA served by axum and named a "render/print form" as part of M5,
but left the screen-level behavior open. Building it (issue #20) surfaced concrete choices that interact
with the backend contract (`/api/render/label`, `/api/batch`, ADR-0011) and the option/interpolation
model (ADR-0005, ADR-0010, ADR-0012). They are recorded here so the Render & Print screen and the later
CSV/Settings screens stay consistent.

## Decision

- **Download vs print transport.** A single-template **Download** uses `POST /api/render/label`
  (`?format=png|pdf`), returning one raw file, not a `/batch` ZIP-of-one. A sheet Download uses
  `/api/batch` `mode=download` (the composed PDF). **Print** always posts a batch of one to `/api/batch`
  `mode=print` (so single and sheet print through the same path), and shows the `BatchSummary`.
- **Auto-generated form.** The field form is derived from the template's **referenced fields**, computed
  client-side by walking `layout` (text/qr `name`-or-`value` tokens plus `image.name`), **option-aware**
  (containers gated by `option` only contribute for the selected option set; options default to the first
  declared value). The same extraction drives the live preview.
- **Image fields** are entered as a file → base64 **data URI** (never free text), matching the backend's
  data-bound image contract.
- **Live preview** is debounced (~300ms), abortable (`AbortController`, stale responses dropped), and
  cached by a hash of the request, gated on the required fields being present. Single previews use
  `/render/label`; sheet previews use `/batch` download.
- **Request hygiene.** `option` is omitted entirely for templates with no declared options (the backend
  rejects any `option` there); `start_slot` is omitted for single templates and when 0; print rejects
  `format`. Print is disabled until a printer is selected and all required fields are filled.
- **Errors.** `/api/batch` data failures (`422 BatchInvalid`, `details.failures`) surface as form-level
  errors (the backend carries no per-field path); the `/render/label` download path surfaces generic
  error messages. Both also toast.

## Consequences

- Single Download yields a clean `name.png`/`name.pdf` rather than a one-entry ZIP; the reusable
  `/api/batch` path still backs printing and the CSV grid (next screen).
- The referenced-field/preview logic is shared with the Templates detail screen and will back the CSV
  grid and Homebox mapping (M7), so the option-aware extraction lives in one place (`templateFields.ts`).
- No backend change: the screen consumes existing endpoints.

## Alternatives considered

- **Single Download via `/batch` (ZIP-of-one).** Rejected: a one-file archive is clunky for "download
  this label"; `/render/label` returns the raw artifact.
- **Per-field inline mapping of `BatchInvalid`.** Not possible: the batch summary's failures are keyed by
  label index with no field name, so errors are form/label-level, with client-side required-field gating
  preventing most missing-field cases up front.
