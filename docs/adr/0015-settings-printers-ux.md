# 15. Settings & Printers screen UX

**Status:** Accepted

## Context

ADR-0008 named a settings/printers screen as part of M5; building it (issue #23) fixed a few UX choices
that interact with the backend contracts (`GET /api/settings`, `PUT /api/settings/{key}`, `/api/printers`
CRUD). They are recorded here so the screen stays consistent with the rest of the UI.

## Decision

- **Settings is a flat key/value editor.** One editable row per stored setting; each row owns its draft
  value and saves with a single `PUT /api/settings/{key}`. Keys not yet stored but expected (currently
  `qr_base_url`) are shown as suggested rows so the user can fill them, since the store does not auto-seed.
  An add-custom-setting row creates arbitrary keys, validated client-side against the server charset
  (`[A-Za-z0-9_.-]+`). There is no delete (the backend exposes upsert only).
- **Printers is a CRUD table.** Add/edit via a form (id, name, kind, config, enabled); delete via a
  two-step inline confirm (no `window.confirm`, so it is testable and non-blocking). The id is immutable
  on edit (it is the path key; editing posts a `PUT /api/printers/{id}`); creating posts `POST /api/printers`.
- **Printer kind is fixed to `cups`** in the form, the only production driver; its config is `{ uri }`.
  The kind select is present but disabled, leaving room for more driver kinds later without reworking the
  form. Client-side validation mirrors the server (id charset, non-empty name, non-empty uri) so common
  mistakes are caught before the request; server errors still surface inline and as a toast.

## Consequences

- No backend or API change; the screen consumes existing endpoints. SPEC gets a changelog entry only.
- Adding a second printer driver kind later means enabling the kind select and a small per-kind config
  sub-form; the `Printer.config` stays an opaque object in the client.

## Alternatives considered

- **`window.confirm` for delete.** Rejected: blocks the event loop and is awkward to test; an inline
  confirm row is clearer and testable.
- **A schema-driven settings form.** Rejected as premature: settings are free-form string pairs in Phase
  1, so a flat editor plus suggested keys is enough.
