# Design

## Context

Variables live in the SQLite `variables` table behind `Store::{get,set,all}_variable(s)`. `PUT /api/variables/{key}` validates the key and writes under `state.write_lock`. In Settings, each `VariableRow` saves on its own; the rows Printers renders already use an inline Delete → Confirm/Cancel step.

## Decisions

- **Idempotent `204`, no `404`.** `DELETE /api/variables/{key}` answers `204` whether or not the key was stored, like `DELETE /api/settings/{key}` and favorites. A `404` would add a not-found kind for variables and make a retried delete fail, while telling the client nothing it acts on.
- **No key validation on delete.** A key outside `[A-Za-z0-9_.-]` can never be stored, so deleting it is already a no-op `204`. Rejecting it with `variable_key_invalid` would be a second check guarding nothing.
- **`Store::delete_variable` returns `()`.** The handler ignores whether a row existed, so a `bool` like `delete_setting`'s would be unused.
- **Inline confirm, reusing the Printers pattern.** A row's Remove switches it to Confirm/Cancel; Confirm sends the delete and invalidates the `variables` query, so the row disappears (or, for `qr_base_url`, turns back into its suggested row). One-click removal was rejected: a misclick deletes a value every template reading it needs, making their renders fail until it is re-added. A suggested row is not stored and offers no Remove. Rows are keyed by key and stored-ness, so a removed `qr_base_url` remounts as a fresh suggested row: keyed by key alone, the row kept the deleted value and an enabled Save (review of 9bdb399), which one click wrote back. The "Remove a suggested key" scenario and its test pin this.

