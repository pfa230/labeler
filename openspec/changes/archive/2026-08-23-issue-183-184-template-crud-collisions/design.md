## Context

See proposal.md for motivation. What matters for the approach:

- `TemplateRegistry` (`src/templates.rs:89`) rebuilds from disk on every load and keeps
  `paths: HashMap<String, PathBuf>` (`src/templates.rs:62`), the winner file per id. A file refused
  as a duplicate is pushed into `broken: Vec<BrokenTemplate>` as `{ filename, error }`, where `error`
  is the rendered `Display` of `TemplateRegistryError::DuplicateId` (`src/templates.rs:146`). The
  fact "id `badge` is also declared by `zed.yaml`" therefore exists only inside a prose string.
- All four mutating template handlers hold `state.write_lock` (`src/api.rs:57`), which serializes
  them against each other but not against anything outside the process.
- `AppState::reload()` (`src/api.rs:136`) re-reads the whole directory and swaps the registry in. It
  fails only on I/O.
- `existing_template_file` (`src/api.rs:355`) resolves an id through the registry's path map, never
  by guessing a filename (#140).
- Every path involved comes from joining `state.templates_dir`: `sorted_dir_paths`
  (`src/templates.rs:72`) collects `entry.path()` from `read_dir(dir)`, and the handlers build
  `dir.join(format!("{id}.yaml"))`. Both sides therefore carry the same prefix, and plain `PathBuf`
  equality is a valid comparison without canonicalization.

## Goals / Non-Goals

**Goals:**

- Give the handlers a first-class way to ask "is this id declared by more than one file?".
- Make every mutating template handler decide against disk, not against a registry that may predate
  the files on it.
- Keep one ADR and one error vocabulary covering all four endpoints.

**Non-Goals:**

- Locking the templates directory against other processes. The window between our re-read and our
  publish stays open for a collider arriving under a different filename; the post-write check is the
  backstop, not a lock.
- Changing the load path, the winner rule, or the shape of `broken[]` on the wire.
- Cascading deletes, and any UI work beyond confirming the new `409` message surfaces.

## Decisions

### D1. The registry records refused duplicates per id

`TemplateRegistry` gains `duplicates: HashMap<String, Vec<PathBuf>>`, filled at the point in
`load_from_dir` that already detects the collision (`src/templates.rs:146`), plus an accessor
returning the refused files for an id. `broken[]` and its wire shape are untouched; this is a second,
structured view of the same event.

*Alternatives.* Parsing the id out of the `broken[]` message — rejected, the message is prose written
for an operator and would become a parsing contract. Re-scanning the directory inside the handler —
rejected, it duplicates parse-and-validate ordering that decides who wins, and the two copies would
drift exactly like the size-resolution pair `CLAUDE.md` already warns about.

Only files that parsed and validated are recorded: a file that fails earlier never contributes an id.
That is the same set the winner rule considers, so the handler's view matches the load path's.

### D2. Every mutating template handler re-reads the directory under the lock, before deciding

`create_template`, `replace_template`, `update_template_group` and `delete_template` call
`state.reload()?` immediately after taking `write_lock`, and read the registry after it. This is what
turns the stale-registry cause into a normal, already-specified outcome: the `POST` guard's
`409 TemplateExists` and the `DELETE` collision `409` both then reflect the directory as it is.

*Alternatives.* Doing it only in `create_template`, which is the one #184 names — rejected: #183's
`DELETE` reaches the same defect through the same stale-registry path (`cp` then `DELETE`), and two
handlers with two different freshness rules is the kind of split that gets one of them fixed and the
other forgotten. Reloading from a filesystem watcher so the registry is never stale — rejected as a
much larger change with its own failure modes; it would be its own issue, and it would not remove the
post-write assert.

*Cost.* One extra full directory read per mutating request. The read is synchronous, like everything
else on this path (`src/api.rs:133` comment), and the target is a single-user local templates dir.
Reads on `GET` paths are unaffected.

### D3. The write handlers assert the id resolves to the file they wrote

After the post-write `state.reload()?`, each of the three write handlers checks two things against
the registry instead of calling `detail(&id)` blind:

1. `registry.path(&id)` equals the path it wrote, and
2. `registry.content_hash(&id)` (`src/templates.rs:190`, already the SHA-256 of the file's raw YAML
   and the source of the ETag) equals the SHA-256 of the bytes it wrote.

The pathname alone cannot carry the promise. `write_template_file` renames onto a name and returns no
file identity, so another process replacing that same name with different valid content between our
rename and our reload leaves the path comparison passing while the response describes content the
caller never sent. The hash closes that: what the endpoint answers with is byte-identical to what it
was given, or it does not answer `2xx`.

The two ways the check can fail are not the same fault and do not share a status:

- **The written file lost the id to another file.** `registry.path(&id)` names a different file, and
  the written file is still on disk, still declaring the id, still holding the bytes we wrote. This is
  the collision: `409 TemplateIdCollision`. A non-empty `registry.duplicates(&id)` is *not* the test:
  a duplicate sorting after our filename never displaces us, and the confirmation simply passes.
- **Anything else.** The written file is gone, renamed, re-id'd, or holds content we did not write.
  Nothing collided in a way we can describe; the server lost the write. `500`,
  `TemplateMissingAfterWrite`, which already exists for exactly this shape. The rename case matters:
  another process moving our `zed.yaml` to `alpha.yaml` leaves the id served from a path we did not
  write while our own path is gone, and a `409` there would name a file that no longer exists.

Verifying "still on disk, still ours" cannot go through the registry: `load_from_dir` inserts into
`hashes` only after the duplicate check (`src/templates.rs:149`-`165`), so a refused file has no
stored hash and `content_hash` answers for the winner alone. The handler therefore reads its own file
back and compares. That read happens only on the failure path, after the cheap winner comparison has
already told us something is wrong.

Classifying the second case as a collision would produce a `409` naming a file that is not a
collider, and would promise the caller a quarantined copy of its write that does not exist. `update_template_group` runs the check only on the branch that actually wrote (`patched != yaml`),
hashing the patched bytes; the no-op branch writes nothing, reloads nothing, and makes no claim about
a file it did not touch.

All three endpoints share one post-write failure code, `TemplateIdCollision`; `TemplateExists` stays
`POST`'s answer for what it catches before writing. The specs state each endpoint's codes once, in
the capability that owns that endpoint, so there is one place to change if they ever move.

`TemplateMissingAfterWrite` therefore widens slightly: it covers the id resolving to nothing *and*
the written file being gone or superseded with no second claimant. Both mean the same thing to the
caller: the service cannot show them what they just wrote, and it is not because someone else owns
the id.

### D4. No endpoint undoes a write

A create that loses its id leaves its file on disk, refused, named in the `409`, and visible in
`broken[]`, exactly as a losing `PUT` does. There is no rollback path in any handler.

An earlier draft had `POST` unlink its own file so a non-`201` could promise "nothing was created".
That promise cost more than it was worth: it needed an identity check before the unlink to avoid
deleting a file another writer had put at that name, and that check and the unlink are two operations
with no portable way to fuse them, so the guarantee was not implementable as stated. It also needed a
second reload, a distinct failure code for a failed unlink, and it still could not hold on the `500`
paths, where the service does not know what is on disk. Three review rounds spent two Critical
findings on that machinery.

What actually prevents the mess is upstream of it: D2's re-read under the lock, and publishing with a
single no-replace filesystem operation instead of `exists()` then `rename`. `std::fs::rename` silently
replaces its destination, so the current code can overwrite a file another writer created after the
guard ran (`src/api.rs:421`-`430`, `src/api.rs:374`-`400`). Writing the temp file and then
`std::fs::hard_link`ing it onto the destination fails with `AlreadyExists` when the name is taken, is
atomic, and keeps the fully-written-before-visible property `rename` gives us; the temp name is
unlinked afterwards either way.

The staging file needs the same treatment, or the guarantee leaks out the back. `write_template_file`
builds a predictable `.{name}.{nonce}.tmp` and opens it with `std::fs::write`, which truncates an
existing file and follows a symlink (`src/api.rs:374`-`400`). An external writer that plants that name,
or a symlink under it, gets our bytes written through it. The staging file must therefore be created
with `create_new(true)` in the destination directory, retrying with a fresh name if that fails, so the
open both proves the name was free and refuses to follow anything.

That leaves exactly one *collision* case reaching the post-write check: a collider under a different
filename, sorting earlier, appearing between the re-read and the publish. It is answered like every
other lost election, and the operator sees both files. The check still catches the non-collision
failures D3 lists, which are answered `500` and are not specific to `POST`.

### D5. A new code, not a new reason

`TemplateIdCollision` / `409`, with `details` `{ template, files }`, where `files` holds bare
filenames. The registry stores `PathBuf`s, so the constructor takes the file names off them the way
`BrokenTemplate` already does (`src/templates.rs:105`-`108`): the templates directory's absolute location
is server configuration and must not leak into an error body. ADR-0052 scopes `details.reason`
to `RenderFailed`, `InvalidRequest`, `UnsupportedLayoutItem` and `TemplateInvalid`, so a `409` cannot
carry one; adding a `Reason` variant that no response can reach would also fight
`spec_documents_every_reason_and_invents_none`. `docs/SPEC.md` §10's code table is frozen and already
omits `TemplateExists`, so the new row lives in the delta spec, which is where clients are pointed.

`POST` answers `TemplateExists` for everything it detects before writing, where "the id is taken,
nothing was written" is precisely true, and `TemplateIdCollision` for the one case it can only detect
after. Callers keep the code they already switch on for the common case.

### D6. `DELETE` refuses rather than cascading

Refusing is the only option of the three in #183 that neither destroys files the caller did not name
nor leaves the caller with a `204` that did not delete the template. The operator's next step is a
filesystem edit, which is how the colliding file got there in the first place.

The check runs before the unlink and before the favorites prune, so a refused delete has no side
effects at all. It is a check against the directory as re-read under the write lock (D2), and that is
the whole of what a `204` can promise: another process creating a same-id file after the check is
outside the guarantee, exactly as it is for the write endpoints, and surfaces through the ordinary
reload channels rather than through this response.

### D7. The group endpoint's contract is changed where it lives

`PUT /api/templates/{id}/group` is specified by `template-groups`
(`openspec/specs/template-groups/spec.md:154`), including a response table that lists no `409`. Its
new behavior is therefore a MODIFIED delta on that requirement, not a sentence in the
`template-registry` delta, which would leave the two capabilities contradicting each other after
archive. `template-registry` states the shared rule and points at `template-groups` for that
endpoint's codes.

### D8. ADR-0065

`docs/adr/0065-template-writes-verify-the-id-they-wrote.md`, plus its row in `docs/adr/README.md`. It
does not supersede ADR-0058: it answers the two consequences ADR-0058 recorded and filed as #183 and
#184, and cites them. ADR-0058 stays Accepted.

## Risks / Trade-offs

- **A `409` on `DELETE` is a breaking change for an API-only consumer that used to get `204`** → It
  only fires in a state that already needed a filesystem fix, and the message names both files. The
  ADR records the break, as #181's did.
- **The extra reload adds a directory read to every template mutation** → Bounded by the size of a
  local templates dir; the same read already happens after every write.
- **An external process can still create a collision between our re-read and our no-replace publish**
  → The post-write assert catches it; only the pre-write `409` is best-effort. Closing the window entirely
  needs directory locking, which is out of scope.
- **The `PUT` `409` leaves the caller's content in a quarantined file** → Documented in the spec and
  named in the message; `broken[]` shows it. The alternative (silently discarding the caller's edit)
  is worse.
- **Tests that cannot fail** → Each of the four behaviors gets a test that is run against the current
  code first and observed to fail (wrong body returned, `204` returned) before the fix lands.
- **The group endpoint widens the change past the two issues' literal text** → Agreed with the user
  before planning; the spec covers all three write endpoints as one contract, which is one rule
  to state, test and change rather than three that can drift.
