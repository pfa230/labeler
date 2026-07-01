# 37. Effortless print form: copies routing + global default printer

Date: 2026-07-01

## Status

Accepted

## Context

The interactive print form forced friction that the primary "just print this" intent does not want:
no way to print more than one copy, and a printer had to be picked on every print (the Print button
was gated on an explicit printer selection, defaulting to "none"). The backend already had a
purpose-built `POST /print` (`PrintRequest { template, printer, fields, option, copies }`) that expands
to `run_batch(vec![label; copies])`, but the UI never called it — it used `POST /batch` with a single
label. Printers (`printers` table) had no notion of a default. This is the Phase-1 slice of the
print-first work (#108); the grid/routing restructure (#109/#110/#113) is separate.

## Decision

1. **Copies routes by template type, using the endpoint that fits.**
   - Tape/single templates → `POST /print` with an integer `copies` (its native contract).
   - Sheet templates → `POST /batch` with the label repeated `copies` times, because `/print`
     hardcodes `start_slot: 0` and cannot target a user-chosen slot. There is no `copies` field on
     `BatchRequest`; the client builds the repeated `labels` array.
   - **Copies affects Print only.** Download is unchanged (always the single-label file). The UI clamps
     copies to `1..=100` (matching `/print`'s `MAX_PRINT_COPIES`); the sheet/`/batch` path's only server
     ceiling is the existing `MAX_BATCH_LABELS = 500` (100 is a print-form UX cap, not a
     server-guaranteed limit on that path).

2. **Default printer is deployment-global, modeled as a read-only flag on the printer.**
   - A `is_default` column on the `printers` table (not a pointer in `app_settings`, not per-user):
     the default is a property of a printer, queryable with the printer list, with no dangling pointer
     when a printer is disabled or deleted. Global scope matches shared physical hardware.
   - **At most one default**, enforced server-side in one SQLite transaction under the write lock:
     verify the id exists (else no change, return 404), then clear all and set the one.
   - `is_default` is **read-only in the API**: `POST`/`PUT /printers` never set it (create → 0 via the
     column default; replace → `ON CONFLICT` preserves it because `upsert_printer` omits the column);
     the handlers echo the stored value, not the request's. It is mutated only by
     `POST /printers/{id}/default` (set) and `DELETE /printers/{id}/default` (clear).
   - The print form preselects the printer **enabled default → sole enabled printer → none**, as a
     one-shot initialization (guarded by an `initialized` flag, not `printer === undefined`, so an
     explicit "None" survives a printers refetch).

## Consequences

- The common flow shortens to enter/confirm fields → set copies → Print, with the printer already
  chosen. Verified by backend tests (copies == summary total; single-default exclusivity;
  404-leaves-prior-default; create-ignores / replace-preserves) and frontend tests (routing by type,
  clamp, preselect precedence, one-shot guard).
- Two clean divergences from the interim design spec, made deliberately: copies are expressed via
  `/print` (int) for tape rather than a uniform `/batch`-repeat, per the owner's call; and the clear
  endpoint is `DELETE /printers/{id}/default` (id-scoped, idempotent) rather than `DELETE
  /printers/default`, to avoid colliding with a printer whose id is literally `default`.
- `is_default` appears in the OpenAPI `Printer` schema as `readOnly`; create/replace requests that
  include it are silently ignored.
- Out of scope: per-user default printers; copies on Download; the grid/routing restructure
  (#109/#110/#113) and recents/favorites (#115).
