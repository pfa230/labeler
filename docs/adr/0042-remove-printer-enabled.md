# 42. Remove the printer `enabled` flag

Date: 2026-08-02

## Status

Accepted. Supersedes the affected parts of
[ADR-0007](0007-printer-architecture-and-transport-model.md) (the printer record shape),
[ADR-0015](0015-settings-printers-ux.md) (the add/edit form fields),
[ADR-0031](0031-inbound-print-webhook.md) (a disabled printer as a pre-dispatch failure), and
[ADR-0037](0037-effortless-print-form.md) (the preselect rule stated in terms of enabled).
Issue [#126](https://github.com/pfa230/labeler/issues/126).

## Context

`Printer.enabled` was a soft off-switch: `run_batch` returned `409 PrinterDisabled` when it was
false, and that was its only enforcement point. It was never argued for on its own merits. It
arrived as part of the record shape in ADR-0007 (`{ id, name, kind, config, enabled }`), and ADR-0015
described the UI as a CRUD table over that record, so the form mirrored the model verbatim. The
`connections` table carries an inherited `enabled` for the same reason.

Reviewed against actual use, it does not pay for itself:

- The obvious trigger — a printer out of tape — is resolved by replacing the tape. Walking to
  Settings, disabling, fixing, and re-enabling is slower than the fix it is meant to cover.
- The residual case is a printer down for days where an automated `/api/print` caller would rather
  have a clean `409` than an IPP timeout. That is real but rare, and it does not justify a permanent
  control in the add-printer card — the one moment when the answer is guaranteed to be "enabled".
- Every picker had to carry a filter (`FieldForm`, `Connect`, `Import`, and the print-form preselect)
  purely to hide records the user had switched off. Four filters for a state almost nobody sets.

Hiding the control in the UI while keeping the field would have left the concept alive in the API,
the schema, the error contract and those filters. The concept is what is wrong, not its placement.

## Decision

**Remove `enabled` from the whole stack**: the `Printer` struct and its `default_true` serde helper,
the `printers.enabled` column (dropped by an appended migration), the `run_batch` gate, the
`PrinterDisabled` code and its constructor, the `409 "Printer disabled"` OpenAPI rows, the
TypeScript type, the form control, the printers-table column, and all four UI filters.

**Requests that still send `enabled` are accepted and ignored.** `Printer` does not set
`deny_unknown_fields`, so serde drops the unknown key with no code on our side. Rejecting with `400`
would break working callers in order to announce the removal of a field they cannot act on.

**The `409` responses on `/batch` and `/print` are rewritten, not deleted.** Both routes still return
`409` for `MediaMismatch`, so removing the row outright would have made the published contract
understate the API.

**`Connection.enabled` is out of scope.** It is a separate concept with its own UI and its own
justification, and shares only a name.

## Consequences

- **A printer that is disabled today becomes printable on upgrade.** No migration preserves the
  intent, because the intent no longer exists. Recorded in the SPEC changelog in those words.
- `PrinterDisabled` leaves the error contract. SPEC treats `code` strings as stable, so this is a
  documented breaking change — acceptable precisely because nothing can emit it once the gate is gone.
- Every configured printer now appears in every picker, and the print-form preselect becomes
  **default → sole printer → none**.
- The add-printer card loses its only non-text control, which also removes the tab-order wart noted
  in #125 (the checkbox sat between `name` and `address`).
- A migration test (`store::migration_tests::migration_drops_enabled_and_keeps_printers`) exercises
  the upgrade path an existing deployment takes: seed a disabled printer at the prior schema version,
  migrate to latest, assert the printer survives and the column is gone. `cargo test` otherwise only
  ever builds fresh databases, so nothing else would catch a bad migration.
- If a durable "out of service" state is ever wanted, it should be designed as one (with a reason, a
  timestamp, and a visible state in the printer list), not resurrected as a bare boolean checkbox.
