## 1. Registry: know which files were refused for an id

- [x] 1.1 Add a per-id record of refused duplicate files to `TemplateRegistry` (`src/templates.rs`),
      filled at the existing duplicate branch in `load_from_dir`, with an accessor returning the
      refused files for an id. Do not change `BrokenTemplate` or the `broken[]` wire shape.
- [x] 1.2 Unit test: a directory with `alpha.yaml` and `zed.yaml` both declaring one id records
      `zed.yaml` as refused for that id, and an id with no duplicate records none. Run it against
      1.1 reverted first and confirm it fails.

## 2. Publishing a new template file without overwriting anything

- [x] 2.1 In `write_template_file` (`src/api.rs`), create the staging file with `create_new(true)`,
      retrying with a fresh name if that fails, so the open cannot truncate an existing file or
      follow a symlink out of the templates directory.
- [x] 2.2 Add a no-replace publish path used by `create_template`: `hard_link` the staging file onto
      the destination, mapping `AlreadyExists` to `AppError::template_exists`, then unlink the
      staging name. A failed unlink logs and does not change the response. `replace_template` and
      the group update keep replacing their existing file.
- [x] 2.3 Test: a create whose destination filename appears after the pre-write guard answers `409`
      and leaves the other writer's content intact. Prove it red against the current
      `exists()`-then-rename path first.

## 3. Error vocabulary

- [x] 3.1 Add the `TemplateIdCollision` code constant and `AppError` constructor (`src/errors.rs`),
      `409`, `details` `{ template, files }` with bare filenames only, and no `details.reason`
      (ADR-0052 scopes `reason` to four codes and this is not one).
- [x] 3.2 Test: the constructor's body carries `error.code`, `details.template`, `details.files` with
      no directory part, and no `details.reason`.

## 4. Write endpoints confirm what they wrote

- [x] 4.1 Add the post-write confirmation shared by `create_template`, `replace_template` and
      `update_template_group`: after the reload, the id must resolve to the file just written and
      `content_hash` must equal the hash of the bytes written.
- [x] 4.2 Implement the failure precedence from the spec: `409 TemplateIdCollision` only when the id
      is served from a different file *and* the written file still exists, still declares the id and
      still holds the written bytes (read it back; the registry stores no hash for a refused file);
      every other failure is `500 TemplateMissingAfterWrite`. No endpoint deletes its write.
- [x] 4.3 Wire the confirmation into all three handlers, in `update_template_group` only on the
      branch that wrote, and add the pre-resolution `state.reload()?` under the write lock to all
      four mutating handlers.
- [x] 4.4 Tests, each proven red first. What the endpoints can reach on their own is covered over
      HTTP: a later-sorting duplicate leaves the write returning its normal `2xx` with the caller's
      content, a create losing to a file the registry had not loaded is a `409` with nothing written,
      and a `PUT` for a vanished file is a `404` from the re-read. The post-write arms need the
      directory to change *between* the write and the reload, which a request cannot stage: the
      classification is covered by unit tests on the confirmation (collision, external rename,
      replaced content), and one HTTP test drives a real post-write `409` through `PUT` using a
      test-only mid-write hook, so a handler that stops confirming fails a test.

## 5. Delete refuses a contested id

- [x] 5.1 In `delete_template`, refuse with `409 TemplateIdCollision` when the re-read directory
      shows another file declaring the id, before the unlink and before the favorites prune.
- [x] 5.2 HTTP tests, proven red first: `DELETE` on an id declared by two files returns `409`, both
      files survive, the id is still served and its favorites are intact; a file refused for an
      unrelated id still does not block a delete; the delete succeeds once the collider is gone.

## 6. Create guard against disk

- [x] 6.1 Make `create_template`'s pre-write guard test the re-read registry plus the destination
      filename, per the spec's two-part rule, and replace its stale comment citing #184 as the open
      hole.
- [x] 6.2 Test: a template file copied into the directory without a reload makes a `POST` for that
      id answer `409 TemplateExists` with nothing written.

## 7. API surface

- [x] 7.1 Add the `409` responses to the `utoipa::path` annotations for `POST /templates`,
      `PUT /templates/{id}`, `PUT /templates/{id}/group` and `DELETE /templates/{id}`, and check
      `src/openapi.rs` needs nothing further.
- [x] 7.2 Confirm the UI's delete flow surfaces the new `409` message through the existing
      `ApiError` path (`ui/src/api/client.ts`), and change nothing if it does.

## 8. Decision record

- [x] 8.1 Write `docs/adr/0065-template-writes-verify-the-id-they-wrote.md`, citing #183, #184 and
      ADR-0058, and recording the two operational breaks (`DELETE` can now `409`; a write can `409`
      after persisting).
- [x] 8.2 Add its row to `docs/adr/README.md`.

## 9. Gates

- [x] 9.1 Run `cargo fmt`, `cargo clippy --all-targets --all-features` and `cargo test`, and fix
      what they flag without `#[allow]`.
- [x] 9.2 Exercise the collision paths against a running server with `LABELER_NO_AUTH=true`: two
      files with one id, then `DELETE`, `POST`, `PUT` and the group update, confirming each response
      names the files and `GET /api/templates` reports `broken[]` as the spec describes.
