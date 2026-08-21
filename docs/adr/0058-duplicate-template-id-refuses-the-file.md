# 58. A duplicate template id refuses the file, not the server

Date: 2026-08-21

## Status

Accepted. Issue [#181](https://github.com/pfa230/labeler/issues/181). Completes the quarantine model
[#175](https://github.com/pfa230/labeler/issues/175) introduced for parse and validation faults.

## Context

`TemplateRegistry::load_from_dir` treated the four ways a template file can fail inconsistently. A
parse failure and a validation failure each pushed a `BrokenTemplate` and moved on, so the server
started and served every other template. An I/O failure and a duplicate id each returned `Err`, which
`main.rs` turns into `fatal!` and an `exit(1)`.

Fatal is right for I/O: an unreadable templates directory means the service cannot do its job at all.
It is wrong for a duplicate id. One copy-pasted YAML file in `{config}/templates` — the exact mistake
an operator makes while editing labels by hand — took down every unrelated template with it, and the
only way back was shell access to the config dir. The service had no way to tell anyone what was
wrong beyond one line on stderr before it died.

Load order made this worse rather than better. `read_dir` returns entries in filesystem order, so
which of two colliding files was "first" was undefined; had the duplicate been non-fatal already, the
template that survived would have varied by machine and by directory history.

## Decision

**A duplicate id refuses the colliding file. The template already accepted for that id stays served,
and the server starts.**

**1. Refuse the later file, quarantine it like any other content fault.** The collision check keeps
its position in the load loop — after parse, after `validate()`, before the id is inserted — and
changes from `return Err` to the path parse and validation already take: build the same
`TemplateRegistryError::DuplicateId`, render its `Display` into a `BrokenTemplate { filename, error }`,
log a warning, continue. All three content faults are now structurally identical in the loop, and the
refused file surfaces through the channels #175 already built: the startup warning, `broken[]` on
`GET /api/templates`, and `broken_count` on `POST /api/templates/reload`. `POST
/api/templates/reload` therefore no longer returns an error for an on-disk duplicate; it succeeds,
reports the collision, and converges once the operator renames or deletes a file.

**2. The already-accepted template is never ejected.** A refused file affects nothing but itself.
This rules out the symmetrical alternative — quarantining *both* contenders, which is what the
pre-#175 code did for a duplicate in spirit by refusing to run — because a newly dropped bad file
must not be able to evict a template that is working.

**3. Filename order decides the winner.** Directory entries are collected and sorted before loading,
so "first wins" means "the lexicographically first filename wins", in byte order, so `Z.yaml` precedes
`a.yaml` on a case-sensitive filesystem. The served set, the id-to-file mapping and the broken list
are a pure function of the directory contents. Two alternatives were rejected:

  - **Prefer a file literally named `<id>.yaml`, then filename order.** This is what `POST
    /api/templates` writes, so an API-managed template could never lose its id to a hand-copied
    sibling. Rejected as a second rule to specify, test and explain, for a case (`cp badge.yaml
    badge-copy.yaml`) where filename order already gives the intuitive answer.
  - **Oldest mtime wins**, which literally implements "the file that was there first keeps the id".
    Rejected: mtimes are assigned by checkout, copy and image build, so the winner would not survive
    a redeploy — reintroducing exactly the non-determinism this decision removes.

**4. `TemplateRegistryError::DuplicateId` and `Reason::TemplateDuplicateId` both stay.** The variant
can no longer escape `load_from_dir`, exactly like `Parse` and `Validation`, and is kept for its
message. The reason must stay regardless: `docs/SPEC.md` §10.1 is frozen and still lists
`template_duplicate_id`, and `spec_documents_every_reason_and_invents_none` asserts the table and the
enum hold the same slugs in both directions, so dropping the variant would fail the build against a
document that can no longer be edited.

## Consequences

- Fatals at startup are now exactly the infrastructure faults: an unreadable templates directory or
  file, an unopenable store, an unbindable port. No template *parse, validation or id* fault can stop
  the service. A `*.yaml` the process cannot read — a dangling symlink, a `chmod 000` file, an entry
  on a volume that is not mounted yet — is still an I/O fault and still fatal.
- A deployment that relied on startup refusing to run with duplicate ids now starts and serves the
  winner. The startup warning and `broken[]` are the replacement signal, and this operational break is
  the point of #181.
- Adding a colliding file whose name sorts *earlier* moves the id to that file at the next load. The
  registry is rebuilt from disk each time and keeps no memory of the previous winner, so "never
  ejected" holds within a load, not across loads. The displaced file appears in `broken[]` with a
  message naming the id and the file it collides with, so the swap is visible rather than silent.
- `POST` and `PUT /api/templates` answer with `registry.detail(&id)` after their reload, so a write
  that creates a collision the registry did not know about can return `2xx` describing the *other*
  file's template while the caller's own file lands in `broken[]`. Before this ADR the same case was
  a loud `422` from the reload. Left as is here, deliberately: #181's accepted scope was the load
  path. Filed as [#184](https://github.com/pfa230/labeler/issues/184).
- `DELETE /api/templates/{id}` removes the file the registry loaded for that id, so a colliding file
  left on disk is promoted at the following reload and the id appears to survive the delete, with the
  favorites for it already pruned. The collision is visible in `broken[]` beforehand; making delete
  cascade or refuse is out of scope here and is filed as
  [#183](https://github.com/pfa230/labeler/issues/183).
- The catalog CI gate is unaffected: `template_ids_are_unique_and_match_filenames` reads the ids off
  disk itself and panics on a duplicate, independent of `load_from_dir`, so a duplicate id shipped in
  `catalog/` still fails loudly rather than being quarantined.
- The behavior contract now lives in `openspec/specs/template-registry/`, which supersedes the
  `docs/SPEC.md` §3 sentence "An invalid template aborts server startup", the §3 `id` row's
  uniqueness enforcement, the §2.0 reload bullet and its `422` sentence, the §2.0 `PUT` and `DELETE`
  bullets' accounts of a post-write `422`, the §2 endpoint table's success column for the reload, and
  the §10.1 row for `template_duplicate_id`.
- `template_duplicate_id` leaves the wire. No request can provoke a `TemplateInvalid` carrying it any
  more; it survives as the cause named in a `broken[]` message, and as an enum variant the frozen
  §10.1 table still requires.
