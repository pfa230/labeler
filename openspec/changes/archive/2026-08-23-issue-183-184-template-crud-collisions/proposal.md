## Why

Issues [#183](https://github.com/pfa230/labeler/issues/183) and
[#184](https://github.com/pfa230/labeler/issues/184), both follow-ups from #181 / ADR-0058.

Since a duplicate template id stopped being fatal and started refusing the colliding file, two files
can declare one id while the service runs. The template write endpoints were never taught about that
state, so each of them now answers a request with a claim that is not true:

- `POST` and `PUT /api/templates` write the file, reload, and answer with `registry.detail(&id)`.
  When the id resolves to *another* file, the caller gets `201`/`200` describing a template it never
  sent, while its own file sits in `broken[]` (#184). `PUT /api/templates/{id}/group` has the same
  shape and the same failure.
- `DELETE /api/templates/{id}` unlinks the file the registry loaded for the id and prunes the
  favorites, and the reload then promotes the colliding file. The id is served again immediately
  after a successful delete, from different content, with the favorites already gone (#183).

Both are one defect: a write endpoint assumes the id it just acted on still maps to the file it just
touched. They share one detection primitive, one new error code, and one ADR, so they are planned
and reviewed together and closed by one commit.

## What Changes

- The registry records, per id, the files it refused as duplicates, so a handler can ask whether an
  id currently collides on disk and name the other file. Today that fact exists only inside the
  free-text message of a `broken[]` entry.
- **BREAKING** `DELETE /api/templates/{id}` returns `409` while another file on disk declares that
  id, naming the file to fix first. Nothing is unlinked and no favorites are pruned. Previously it
  returned `204`, unlinked the winner, and let the collider take over the id.
- `POST /api/templates` re-reads the templates directory under the write lock before its
  id-already-exists guard, so the guard tests disk rather than a possibly stale in-memory registry.
  A collision that was invisible to the guard now yields the existing `409` with nothing written.
- **BREAKING** After their reload, `POST`, `PUT /api/templates/{id}` and `PUT
  /api/templates/{id}/group` confirm that the id is served from the file they wrote *and* that its
  content is what they wrote, rather than trusting the pathname alone. Failing that, they answer an
  error naming the files instead of `2xx` with another template's body: `409` when the id is served
  from a different file, `500` when the written file is no longer there or no longer holds what was
  written. No endpoint undoes its write. On the `409`, the caller's file is still on disk holding what
  it submitted, named in the error and reported in `broken[]`, which is where the operator resolves
  it; the `500` cases are precisely the ones where that can no longer be said.
- `POST /api/templates` publishes its file with a single no-replace filesystem operation instead of
  `exists()` followed by a rename, so it can no longer silently overwrite a file another writer
  created after the guard ran.
- One new `AppError` code, `TemplateIdCollision`, for "two files declare this id and the service
  will not guess which one you meant": `DELETE` refusing, and any of the three write endpoints
  finding after its write that the id is served from a different file. It carries no `details.reason`:
  ADR-0052 scopes `reason` to `RenderFailed`, `InvalidRequest`, `UnsupportedLayoutItem` and
  `TemplateInvalid`, and a `409` is none of them. `POST` still answers the existing `TemplateExists`
  for everything it catches before writing, which is where "the id is taken and nothing was written"
  is true.
- ADR-0065 records the decision and updates the `docs/adr/README.md` index; ADR-0058's two open
  consequences (the ones that filed #183 and #184) are answered by it.

## Capabilities

### New Capabilities

None. The write endpoints' behavior belongs to the capability that already owns the registry
contract and the load-path collision rules.

### Modified Capabilities

- `template-registry`: adds requirements for how the template write endpoints behave when an id
  collides on disk (`DELETE` refuses; a write confirms the id still resolves to its own file with its
  own content, and says so plainly when it does not), and for the new error code. Modifies the existing "A `422` from a template write
  means nothing was written" requirement, whose account of post-write outcomes and of what cannot
  block a delete is now wrong.
- `template-groups`: modifies "A template is moved between groups without rewriting the rest of its
  file", which owns `PUT /api/templates/{id}/group` and whose response table lists no `409`. The
  endpoint gets the same pre-read and the same post-write check, on the branch that writes.

## Impact

- `src/templates.rs`: `TemplateRegistry` gains the per-id refused-file record, populated in
  `load_from_dir` where the duplicate is already detected.
- `src/api.rs`: `create_template`, `replace_template`, `update_template_group`, `delete_template`.
- `src/errors.rs`: one new code constant and `AppError` constructor for the collision `409`. No new
  `Reason` variant, so `docs/SPEC.md` §10.1 being frozen does not bind here.
- `src/openapi.rs` / the `utoipa::path` annotations: the new `409` responses on the four endpoints.
- `ui/`: no code change expected. `del()` in `ui/src/api/client.ts` already throws the JSON error
  contract for any non-2xx, so the delete mutation surfaces the new `409`'s message unchanged;
  whether that message reads well in the UI is checked, not rewritten, by this change.
- No change to the load path, to `GET` endpoints, or to the shape of `broken[]` on the wire.
