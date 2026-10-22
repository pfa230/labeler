## 1. Server

- [x] 1.1 Add `Store::delete_variable` in `src/store.rs`.
- [x] 1.2 Add the `delete_variable` handler (`204`, under `write_lock`) with its `utoipa::path`, and register it beside `put_variable` in `src/api/settings.rs`.
- [x] 1.3 Add an API test in `src/lib.rs` for the "Delete a variable" scenario: PUT `site`, DELETE it twice, assert both `204` and that `GET /api/variables` omits `site`.

## 2. UI

- [x] 2.1 Add `useDeleteVariable` to `ui/src/api/queries.ts`, invalidating `["variables"]` on success.
- [x] 2.2 Give stored rows in `VariablesSection.tsx` a Remove button that switches to Confirm/Cancel, deleting only on Confirm and showing an error toast if the delete fails; suggested rows get no Remove.
- [x] 2.3 Add a `VariablesSection.test.tsx` case for the "Remove a variable" scenario: Remove sends no request, Confirm sends `DELETE /api/variables/site`.
