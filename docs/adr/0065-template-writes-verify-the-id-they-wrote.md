# 65. A template write verifies the id it wrote, and a contested id refuses the delete

Date: 2026-08-23

## Status

Accepted. Issues [#183](https://github.com/pfa230/labeler/issues/183) and
[#184](https://github.com/pfa230/labeler/issues/184). Answers the two consequences
[ADR-0058](0058-duplicate-template-id-refuses-the-file.md) recorded and left open; that record stays
Accepted.

## Context

ADR-0058 made a duplicate template id refuse the colliding file instead of the server. Two files can
therefore declare one id while the service runs: the lexicographically first filename is served, the
other sits in `broken[]`. The four endpoints that write to the templates directory were never taught
about that state, and each of them then answered a request with a claim that was not true.

`POST` and `PUT /api/templates` write their file, reload, and answer with `registry.detail(&id)`.
When the write created a collision the registry did not know about, the reload succeeded, the id
resolved to *the other file*, and the handler returned `201`/`200` describing a template the caller
never sent while the caller's own file was quarantined. `PUT /api/templates/{id}/group` has the same
shape. Before ADR-0058 the same case was a loud `422` from the reload.

`DELETE /api/templates/{id}` resolved the file through the registry, unlinked it, pruned the
favorites for the id, and reloaded. The reload then promoted the file that had been in `broken[]`, so
the id was served again immediately after a `204`, from content the caller never asked to keep, with
the favorites already gone.

Both faults share one cause: a write endpoint assumed the id it acted on still mapped to the file it
touched. Underneath sat a second problem, older than ADR-0058. `create_template` guarded with
`registry.get(&id).is_some() || path.exists()`, which reads an in-memory set that files installed by
hand (`cp catalog/*.yaml {config}/templates`) never update, and `write_template_file` published with
`std::fs::rename`, which replaces its destination silently.

## Decision

**1. A write answers for the file it wrote, or it fails.** After its reload, each of the three write
endpoints confirms that the id resolves to the file it wrote *and* that the served content hashes to
what it wrote. The pathname alone cannot carry this: publishing reports no file identity, so a writer
replacing that name leaves a path comparison passing while the response describes content the caller
never sent. The registry already keeps a SHA-256 per id for its ETag, which is exactly the check.

**2. A failed confirmation is classified, not lumped together.** `409 TemplateIdCollision` requires
all of: the id is served from a different file, the written file still exists, still declares the id,
and still holds what was written. Only then are the two things the error asserts both true, that two
files claim the id and that the caller's write is the one refused. Anything else, the file gone,
renamed, re-identified, or replaced, is `500` with `template_missing_after_write`: there would be no
second file to name and no intact write to point at. Verifying "still ours" reads the file back,
because `load_from_dir` records a hash only for the winner.

**3. A duplicate that does not displace the write is not an error.** Filename order decides the id
(ADR-0058), so a colliding file sorting *after* the written one never takes it. The write succeeds
normally and the duplicate is reported in `broken[]` like any other refused file. The test is the
confirmation, never the existence of a duplicate.

**4. Nothing is rolled back.** A losing file stays on disk holding what the caller submitted, named
in the error and visible in `broken[]`, where the operator resolves it. An earlier draft had `POST`
unlink its own file so a non-`201` could promise "nothing was created". That promise needed an
identity check before the unlink, to avoid deleting a file another writer had put at that name, and
that check and the unlink are two operations with no portable way to fuse them, so the guarantee was
not implementable as stated. It also could not hold on the `500` paths, where the service does not
know what is on disk.

**5. A create decides against disk and publishes without replacing.** Every mutating template handler
re-reads the directory under the write lock before deciding. `POST` then blocks on two independent
things: an id held by any file the registry could serve it from, under any filename, and the
destination filename existing at all, whatever it contains. It publishes by writing a staging file
and `hard_link`ing it onto the destination, which fails with `AlreadyExists` rather than replacing,
and the staging file itself is created with `create_new`, so a planted name or a symlink under it
cannot receive the caller's bytes. Failing to remove the staging file afterwards does not change the
response: a `.tmp` file is not a template the registry would load.

**6. `DELETE` refuses a contested id.** While any other file declares the id, the request is refused
with `409 TemplateIdCollision` naming the files, before the unlink and before the favorites prune, so
a refused delete has no side effects. The two alternatives in #183 were rejected: cascading would
delete files the caller never named, and accepting the promotion would keep a `204` that does not
delete the template.

**7. The collision is a new code, not a new reason.** `TemplateIdCollision`, `409`, `details`
`{ template, files }` with bare filenames. ADR-0052 scopes `details.reason` to `RenderFailed`,
`InvalidRequest`, `UnsupportedLayoutItem` and `TemplateInvalid`, and a `409` is none of them, so the
code carries no reason. `TemplateExists` keeps its meaning and stays what `POST` answers for anything
it catches before writing.

## Consequences

- `DELETE /api/templates/{id}` can now return `409` where it returned `204`. It only fires in a state
  that already needed a filesystem fix, and the message names the files, but a client that treated
  delete as infallible sees a new status. The UI needs no change: its delete mutation already toasts
  the server's message for any non-2xx.
- A write can now return `409` *after* persisting the caller's bytes. That is the honest report of a
  lost id election, and the file is named in the error and listed in `broken[]`.
- `POST /api/templates` with an unreadable templates directory now reports `details.reason`
  `template_registry_io` rather than `template_write_failed`. The pre-write re-read reaches the fault
  first, and "reading the templates directory failed" is what actually happened; nothing was written.
  `template_write_failed` still covers a write that fails on a readable directory.
- `PUT` and the group update now act on the file that serves the id *at request time*, not the one a
  possibly stale registry remembered. Where the winner changed on disk since the last reload, the
  edit lands in the new winner and the response describes it, instead of editing a file that no
  longer serves the id.
- Every mutating template request costs one extra directory read. The templates directory is local
  and small, the read is synchronous like the rest of this path, and `GET` endpoints are unaffected.
- Cross-process safety is bounded and stated: every check is made against the directory as read under
  the write lock. Another process writing the directory during a request can still defeat it, which no
  locking protocol the service adopts can prevent while the other writer is a person with `cp`. The
  confirmation is the backstop that keeps such a race from being reported as success.
- The registry now records, per id, the files refused for declaring it, alongside the prose in
  `broken[]`. Only files that parse and validate can appear, which is the same set the winner rule
  considers; a file that fails earlier never claims an id, and the create guard's filename half is
  what covers it.
