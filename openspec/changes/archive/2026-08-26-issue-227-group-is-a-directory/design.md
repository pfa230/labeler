## Context

See `proposal.md` — Why. The constraints that shape the approach:

- `TemplateRegistry::load_from_dir` reads one directory and does not recurse (`src/templates.rs:77-92`).
  Everything downstream assumes a flat directory: `broken[].filename` is a bare basename, the
  duplicate-id winner is decided by filename sort, and `template_file_path` returns `{dir}/{id}.yaml`
  (`src/api.rs:399-412`).
- Parsing is two-stage and every `raw.rs` struct carries `deny_unknown_fields`, so removing `id` and
  `group` from the raw struct is what rejects them; no new check is needed, only a better message.
- The write endpoints already hold one `write_lock`, re-read the tree before deciding, and confirm
  after writing by comparing the served path and the SHA-256 the registry keeps for its ETag
  (`confirm_written_template`, `src/api.rs:578`). That machinery is reused unchanged for moves.
- `catalog/` is already nested and already asserts filenames unique tree-wide
  (`src/templates.rs:1876-1879`), which is the invariant this change promotes to a rule.
- Only 5 catalog templates exist and none carries a `group:` line, so the catalog side of this change
  is 5 mechanical edits.

**ADR**: this change adds **ADR-0073, "A group is a directory and a template's id is its filename"**,
which supersedes ADR-0061 (group as a YAML field) and ADR-0062 (the service may rewrite a single
template key), and revisits ADR-0058 (a duplicate id refuses the file) for tree-wide ids.

The same change adds ADR-0073's row to `docs/adr/README.md` and edits the Status cells of the 0061
and 0062 rows to record that 0073 supersedes them, following the convention the 0066 row already
uses. AGENTS.md requires the row in the same change as the decision, and a superseded ADR that still
reads `Accepted` in the index is how the next reader gets it wrong.

Numbering: 0071 is the highest row in the index, 0072 has landed on `main`, and 0067 and 0070 are
absent — 0070 is claimed twice across in-flight worktrees. Re-check the highest number on `main`
immediately before committing and renumber if 0073 has been taken meanwhile.

## Goals / Non-Goals

**Goals:**

- Moving a template between groups is one filesystem operation on one file, not an N-file rewrite,
  and renaming a group in #202 will be one directory rename. The property that matters is that no
  file's *content* is read or rewritten to move it; which syscall does the moving is settled under
  Decisions.
- Delete `patch_template_group` and every refusal class that existed only to serve it.
- Keep ids stable across a move, so favorites (`src/store.rs:159-163`) and job history keep resolving.
- Keep every cleanup rule on the near side of publication. A request that has written or relocated a
  file never undoes it; a request refused before that point leaves nothing behind. Those two are
  exhaustive and must not overlap, or the artifacts promise both to remove a directory and to keep
  the file inside it.
- Keep the load path's quarantine discipline: no template content, no directory name, and no filename
  can stop the service from starting.

**Non-Goals:**

- Renaming a group (#202). This change makes it a directory rename; #202 ships the route and the UI.
- A create-group route. A group is still created by naming it in a move.
- Changing where a catalog install lands. It keeps writing to the root of `templates/`, ungrouped, as
  it does today, even though `catalog/index.json` already carries `category` and `vendor` that would
  map onto a nested group. Filed as
  [#234](https://github.com/pfa230/labeler/issues/234) and out of scope here.
- Conditional replace (`If-Match` with an ETag). Only `If-None-Match: *` is added, because only
  create-if-absent is needed to replace `409 TemplateExists`.
- Migration, in any form, and any operator-facing procedure standing in for one. Neither the service
  nor any command it ships rewrites, moves, or renames anything under `{config}/templates`. A file
  carrying a legacy key is an invalid template and is reported as one.

## Decisions

### The registry walks the tree, and the relative path is the sort key

`load_from_dir` becomes a recursive walk that collects `(relative_path, absolute_path)` pairs and
sorts by the relative path's bytes. That keeps the existing guarantee — the id-collision winner is a
property of the tree's contents, not of `read_dir` order (#181) — with one extra property: the sort
key is now the same string the API reports, so `broken[]`, `details.files` and the collision winner
all speak one language.

The walk uses `symlink_metadata`, not `is_dir`, so a symlinked directory is not descended into. A
symlink cycle under `templates/` would otherwise recurse forever; `load_all_for_tests` already guards
the catalog copy this way (`src/templates.rs:1865`) and the reason is the same.

Directories whose name starts with `.` are skipped whole. That is the conventional reading of a
hidden directory, and it gives the operator a place to park files inside `templates/` — a pre-upgrade
copy, say — without them becoming templates.

*Alternative rejected*: `walkdir`. The walk is twenty lines, needs the symlink and dot-directory
policy spelled out anyway, and a new dependency for it is not worth the supply-chain surface.

### `BrokenTemplate.filename` becomes `path`

A basename no longer identifies a file, so the field is renamed rather than quietly redefined. The
same applies to `TemplateIdCollision.details.files` and to `file_label` (`src/api.rs:561`), which
becomes "path relative to the templates directory" and keeps its promise that the templates
directory's own location never appears in an error body. Both are breaking and both are listed in the proposal. There is no consumer to migrate: `broken` is
absent from `ui/src/api/types.ts` and appears nowhere in `ui/src`, so nothing has ever rendered it.
`src/main.rs:67` reads the field for its startup warning and is the one caller to update.

### `id` and `group` are removed from `raw.rs`, and the error message names the command

`deny_unknown_fields` then rejects them with no new validation code. The default serde message
("unknown field `id`") is exactly what such a file gets, and it is enough: the field is named and the
file is reported in `broken[]`. An earlier draft added a second read-only pass to name the `group:`
value and every legacy key at once; it is gone, along with the procedure it served. A file written
for the old model is an invalid template, and this change gives it no special class and no special
handling.

*Alternative rejected*: keeping `id:` as an optional key that must agree with the filename. It keeps
`template_id_mismatch` and the entire divergence class that ADR-0058 and #183/#184 keep adjudicating
alive permanently, in exchange for accepting a key that can only ever be redundant or wrong.

**Where the id then comes from.** `TemplateDefinitionRaw` owns `id` and `group` today
(`src/raw.rs:134-146`), and `parse_template` converts a raw value straight into a complete
`TemplateDefinition` (`src/parse.rs:25-33`), so removing the two fields leaves the parser unable to
produce a `TemplateDefinition` at all. The type splits in two:

- `TemplateContent` — everything a file declares: name, description, unit, dpi, format, params,
  layout, version. This is what `parse_template(yaml)` returns, and it is all a body can carry.
- `TemplateDefinition { id, group, content }` — a located template. Only the loader and the write
  endpoints build one, because only they know a path.

`TemplateDefinition` keeps exposing the content fields rather than making every consumer reach
through `.content`: the renderer, the batch path and the catalog indexer read `unit`, `dpi`, `format`,
`params` and `layout` directly today (`src/render/mod.rs:481-494`, `src/batch.rs:154-172`,
`src/bin/catalog-index.rs:30-85`), and a rename across all of them would be churn this change did not
set out to make. A `Deref` to `TemplateContent` gives them the field access unchanged, so the split
is visible only where identity is constructed. `validate()` moves to `TemplateContent`; the parts of
it that checked an embedded id or group cease to exist.

`Deref` alone is not the whole story, and the remaining cases are settled here rather than
discovered during implementation:

- **Mutation.** Callers do mutate a template they own: the render tests build one and then assign
  `format` and `layout` on it (`src/render/mod.rs:4344-4355`, `:5078-5089`, `:6148-6158`). Those are
  owned values, not registry-shared ones, so they get `DerefMut` alongside `Deref`. The registry
  still hands out `&TemplateDefinition` and templates are shared through `Arc`, so nothing can mutate
  a loaded template through it.
- **Construction.** `instantiate_with_defaults` (`src/templates.rs:311-323`) builds a whole
  `TemplateDefinition` field by field, id and group included, and must keep preserving identity: it
  becomes "clone the identity, rebuild the content", which is the shape the split wants anyway. Test
  literals convert the same way, a `TemplateContent` literal wrapped with an id and group. That is
  the one place the split is deliberately visible.
- **Direct `parse_template` callers.** `parse_template` returns `TemplateContent`, so every caller
  supplies identity from where it actually knows it: the loader from the path, the write endpoints
  from the route, `catalog-index` from the catalog file's path. The render tests' own
  `parse_and_validate` helper (`src/render/mod.rs:6401-6408`) returns `TemplateDefinition` today; it
  takes an id parameter, or returns content, whichever each call site needs. A caller that cannot
  name a source of identity is a caller that should not have had one.

Each caller then gets its id from the one place that has it:

- the registry walk, from the file's stem and its parent directory;
- the New Template page, from a dedicated id field beside the YAML editor, since the YAML no longer
  carries one (see the UI decision below);
- `PUT /api/templates/{id}`, from the path segment it already validates, so body validation
  (`src/api.rs:431-437`) validates content and the route supplies identity;
- `src/bin/catalog-index.rs:30-35`, from the catalog file's own path, which is also where its
  `category` and `vendor` already come from;
- `load_all_for_tests` and the render tests, from the copied tree.

`validate()` moves to `TemplateContent` except for the parts that check identity, which cease to
exist: there is no embedded id or group left to check (`src/templates.rs:326-335`).

### A move is `renameat` with `NOREPLACE`, falling back to `linkat` + `unlinkat`

Plain `rename` replaces the destination silently, so it cannot express "move, but never overwrite".
`renameat2(RENAME_NOREPLACE)` can: it is one syscall, it is atomic, and it fails with `EEXIST`
instead of clobbering. `rustix` exposes it as `renameat_with(RenameFlags::NOREPLACE)`.

An earlier round rejected it as "Linux-only and needs `libc` or `nix`" and preferred `hard_link` as
"std and portable". Both halves of that are void now: `rustix` is already a dependency for the
`*at` calls the no-symlink rule requires, and `linkat`/`unlinkat` are no more portable off Linux than
`renameat2` is. Worse, the link-then-unlink pair leaves the template at two paths in between, a window
that then has to be mitigated with the write lock and the post-move re-read. One atomic syscall has no
window to mitigate.

`RENAME_NOREPLACE` is unsupported on a few filesystems, which surface as `EINVAL` or `ENOSYS`. There
the move falls back to `linkat` + `unlinkat`, which has the two-path window described below; the
write lock and the post-move confirmation cover it, and a crash inside it leaves an ordinary
duplicate-id collision the load path already reports.

The path-based forms of both — `std::fs::hard_link`, `std::fs::remove_file` — are unusable here, and
this is the same mistake as `canonicalize`: they take whole path strings, so the kernel re-walks every
ancestor at call time and follows whatever those ancestors point at by then, discarding the
component-wise resolution that was supposed to contain the write. Every final mutation is therefore
fd-relative, addressing one name inside an already-open directory: `renameat_with(NOREPLACE)` for the
move (`linkat` + `unlinkat` where the filesystem lacks it), `renameat` for an in-place replace,
`mkdirat` for a created group directory,
`unlinkat(AT_REMOVEDIR)` for a group delete, and `unlinkat` for staging cleanup. The source file is
opened `O_NOFOLLOW` too, so a symlinked source is refused rather than unlinked out from under its
target.

That makes containment structural rather than checked: the directory handle already refers to the
directory it opened, so nothing planted afterwards can redirect the operation.

Between the link and the unlink the template exists at two paths. A reload landing in that window
would see a duplicate id and refuse one of them. On the fallback path two things close it: the move
holds the same `write_lock` every other write holds, and the post-move confirmation re-reads the tree
and compares the served path and content hash before answering. A crash inside it leaves a duplicate
on disk, which the load path reports as a collision — the same state a hand-copied file produces, and
the operator resolves it the same way. `RENAME_NOREPLACE`, where available, has no such window.

*Alternative rejected*: check-then-`rename`. That is a TOCTOU with a silent overwrite as its failure
mode, which is precisely what #184 spent its effort eliminating from the create path.

### Deleting a group is `unlinkat(AT_REMOVEDIR)`, and the OS enforces emptiness

Directory removal fails with `ENOTEMPTY` when anything is inside and `ENOENT` when nothing is, which
maps onto `409` and `404` with no separate emptiness check and therefore no TOCTOU. It is issued
fd-relative against the resolved parent, for the reason above; `std::fs::remove_dir` would re-walk the
path and is not used. The refusal
covers a stray non-template file too, which is the safe reading: the service will not delete a
directory whose contents it does not understand.

### Every write resolves its path component by component and refuses a symlink

Lexical validation contains nothing. `Outside` is a valid group name, and an operator who plants
`templates/Outside -> /etc` makes a create or a move write through it. Today this cannot happen
because `template_file_path` joins a sanitized filename directly beneath the root
(`src/api.rs:399-412`) so there is no intermediate component to subvert; introducing group
directories introduces exactly those components, and `stage_template_file` trusts the destination's
parent as given (`src/api.rs:462-483`).

So the write path walks the group path itself. Starting from an open handle on the templates
directory, each segment is resolved against the previous one, and the ordering matters:

1. list the parent for an entry whose name matches the segment **exactly**;
2. if one exists, open it with `O_NOFOLLOW | O_DIRECTORY`;
3. if none exists, `mkdirat` it — and read `EEXIST` as the filesystem aliasing a name we did not
   find, which is the case-alias refusal, not a race to retry.

Open-first would defeat that rule: on a case-folding filesystem the alias case is exactly the case
where `openat` *succeeds*, so `mkdirat` would never run, no `EEXIST` would ever surface, and the
service would silently reuse the aliased directory — the outcome the spec forbids. `mkdirat` also
fails with `EEXIST` on a symlink, so the create and the open agree there. A component that is a symlink is refused; the final file is created with
`O_NOFOLLOW` too, so a symlinked destination cannot be written through either.

Relative `*at` calls are what make this immune to a link planted mid-operation: a check on a path
string is stale the moment it returns, whereas a handle already refers to the directory it opened.
This is why the change reaches for `rustix` (or `libc`) rather than `std`: `std` exposes neither
`openat` nor `O_NOFOLLOW`, and none of its path-based mutators (`hard_link`, `remove_file`, `rename`,
`create_dir`, `remove_dir`) has an `*at` form. Every mutation under `templates/` goes through the
fd-relative calls.

*Alternative rejected*: `canonicalize` and compare the prefix. It resolves symlinks rather than
refusing them, so a link that stays inside `templates/` passes while still being a link, and the
check is a TOCTOU against anything planted afterwards.

*Alternative rejected*: refusing symlinks at load time only. The load path already skips symlinked
directories, but that is a read; it says nothing about where a write lands.

### Group paths are validated segment by segment, and never canonicalized

A group path from a request is trimmed, split on `/`, and each segment checked against the rules in
the spec. `.` and `..` are rejected as segments and `/` and `\` inside a segment are rejected, so the
joined path has no component that lexically escapes `templates/`. That is necessary and not
sufficient: containment is enforced by the component-wise resolution above, and this validation only
rejects the paths that are wrong on their face. As a belt-and-braces check the joined path is
asserted to contain no `ParentDir` component before it is used.

Length is checked in characters *and* in UTF-8 bytes: 64 characters and 255 bytes per segment, 255
characters and 1024 bytes for the whole path. A 64-character name of multi-byte characters would
otherwise pass validation and fail at the syscall with `ENAMETOOLONG`, turning a `422` the caller can
act on into a `500` it cannot.

Case folding for the sibling-clash check is Unicode simple lowercase mapping (`str::to_lowercase`).
That is deterministic and identical on every platform, and it is deliberately *not* claimed to match
what any given filesystem does: simple lowercasing is not full case folding, so `Größe`/`GRÖSSE` is
not caught, and case-insensitive filesystems disagree with each other on folding and normalization
anyway. Promising agreement with all of them would be a promise no implementation could keep. The
check refuses the clashes it can predict; the spec says exactly which, and a scenario pins the case
it does not cover.

The reserved-name and forbidden-character rules (`<>:"|?*`, `CON`, `LPT1`, the superscript `COM¹`
family, trailing dot, trailing space) exist so a group created on Linux can still be checked out or
copied onto Windows and macOS. The device-name list is the one Windows actually reserves, superscript
forms included, and is compared with any extension stripped; a list that omitted them would be a
portability claim the validation does not deliver.
They are stricter than the field they replace, which is the price of the name being a directory.

The case rule is enforced by listing the destination's parent directory and comparing the new
segment case-insensitively against the existing entries, under the write lock. On a case-sensitive
filesystem it is a policy check; on a case-folding one it is what prevents two groups from silently
becoming one.

### `PUT /api/templates/{id}` creates or replaces; `POST /api/templates` is deleted

The client names the resource, so `PUT` to its URI is the conventional create (RFC 9110 §9.3.4), and
`If-None-Match: *` is the conventional "only if absent" (§13.1.2). The pre-write re-read, the
exclusive staging file, the publish-only-if-absent rule and the post-write confirmation all carry
over from the removed `POST` requirement unchanged — only the route, the source of the id and the
refusal status change.

A `?group=` on a replace does not move the file. Silently ignoring it would let a caller believe it
moved a template; honouring it would make every save a potential move. It is a `400` naming the move
route instead.

*Alternative rejected*: `POST /api/templates/{id}`. It preserves today's two-endpoint split and the
`409`, but no RFC prescribes POST-to-a-named-resource, and it leaves two write verbs where one now
suffices.

### A create publishes exclusively; a replace publishes by replacing

The two are distinct operations in the code today and stay distinct: `publish_new_template_file`
hard-links and refuses an occupied destination (`src/api.rs:535-557`), `write_template_file` renames
over it (`src/api.rs:440-453`). An unconditional `PUT` classified as a create therefore has a defined
transition when the destination appears mid-request: the exclusive publish refuses, and the request
re-classifies once as a replace of that path. At most once — a second exclusive failure is a `500`,
not a retry loop, because something else is writing the tree faster than this request can act.

With `If-None-Match: *` there is no re-classification: the refusal is the answer, `412`.

### The New Template page grows an id field

`ui/src/pages/NewTemplate.tsx:6-19` seeds its editor with a placeholder whose first line is
`id: my-label` and posts the body through `useCreateTemplate` (`ui/src/api/queries.ts:65-70`). Both
halves break: the key is rejected and the route is gone. The page therefore gets a separate,
validated id input next to the editor, its placeholder loses the `id:` and `group:` lines, and it
submits `PUT /api/templates/{id}` with `If-None-Match: *` and an optional `?group=`, reading `412` as
"that id is taken" against the id field rather than as a generic failure. Without the header a
mistyped id would silently replace an existing template, which is exactly the accident the old
`409 TemplateExists` prevented.

The group is offered as the same tree control the move dialog uses, defaulting to ungrouped, so
creating a template directly into a group does not need a second step.

### Routing a path that contains slashes

`DELETE /api/template-groups/{*path}` uses an axum 0.8 wildcard, which captures the remaining
segments including `/`.

Axum percent-decodes that parameter through `percent_encoding::percent_decode`, whose iterator passes
an invalid sequence through literally instead of failing. Since `%` is a legal character in a group
name, `%ZZ` would arrive as the perfectly valid group name `%ZZ` rather than as an error, so the
malformed-sequence `400` the spec requires cannot come from the extractor. The handler reads the raw
encoded path from the request URI and rejects any `%` not followed by two hex digits before it looks
at the decoded value. `GET /api/template-groups` is a separate, non-wildcard route on the same
prefix. utoipa describes the wildcard parameter as a string whose value is the group path.

### `patch_template_group` is deleted, not repurposed

Its line-splitting machinery — terminator-preserving splits, top-level key detection, flow-root and
multi-document refusals (`src/templates.rs:465`) — existed to move a template between groups without
disturbing its bytes. Moving the file does that by construction, and with no migration command to
reuse it, nothing else in the service needs to edit one key of a hand-authored file. It goes, along
with `template_group_unpatchable` and the ADR that authorized it.

### Tests keep the catalog's structure instead of flattening it

`load_all_for_tests` (`src/templates.rs:1850`) flattens `catalog/` into one temp directory precisely
because the registry could not recurse. It now copies the tree as-is, so the suite exercises nested
groups against real templates and `catalog/tape/brother/brother_12mm.yaml` is the template
`brother_12mm` in group `tape/brother`. The unique-filename assert stays: it is now the id-uniqueness
rule rather than a workaround for flattening.

## Risks / Trade-offs

- **A downgrade hides every grouped template.** An older binary scans only the root of `templates/`
  and reports an empty list, not a broken one → `docs/DEPLOY.md` and ADR-0073's Consequences record
  that a downgrade is lossy. This is strictly worse than ADR-0061's rollback note, and saying so is
  the mitigation.
- **Every existing template becomes invalid the moment the new binary starts.** That is the chosen
  shape: visible breakage rather than any unattended rewrite of a live bind-mounted `/config` → the
  file is reported in `broken[]` like any other invalid template, and nothing in this change reduces
  the work of fixing it.
- **The two-path window during a move**, on filesystems that lack `RENAME_NOREPLACE` and fall back to
  link-then-unlink → write lock, plus the post-move confirmation; a crash in the window leaves an
  ordinary duplicate-id collision the load path already reports. On every filesystem that supports
  the flag there is no window at all.
- **Case-clash detection is a check, not a guarantee.** Another process creating a case-variant
  directory between the check and the link defeats it → the write lock covers this service; a second
  writer on the same config dir is outside what any of these endpoints promise, as the registry
  requirements already state.
- **Path and name limits vary by OS.** A 64-character segment cap and a 255-character path cap are
  conservative against every common filesystem, and are stricter than what Linux would accept →
  documented in the spec as validation, so a name is refused at the API rather than at the syscall.
- **Containment covers mutations, not reads.** The walk skips symlinked *directories*, but a
  symlinked template *file* is still read and served, and `GET /templates/{id}/source` returns the
  target's bytes (`src/templates.rs:103-125`, `src/api.rs:907-919`). That predates this change and is
  not altered by it → ADR-0073 states the boundary explicitly, so nobody reads the no-symlink rule as
  a promise that reads stay inside `templates/`.
- **The symlink policy needs `openat`/`O_NOFOLLOW`, which `std` does not expose.** → one new
  dependency (`rustix`) confined to the write path, with the alternative being a containment
  guarantee that is lexical and therefore false.
- **A name that is not valid UTF-8 has no faithful JSON form.** Two such names can collapse to one
  string under lossy conversion → they are refused rather than served, reported lossily with the
  reason stated, and can never hold an id, so they never reach a collision's `details.files` where
  identifying them would matter.
- **`broken[].filename` → `path` breaks any external consumer.** → listed as BREAKING in the
  proposal; the UI is updated in the same change and is the only consumer in this repo.
- **Nesting has no depth cap beyond the 255-character path limit.** A deep tree is a UI problem
  before it is a service problem → the tree control is built to render arbitrary depth, and the path
  cap bounds it in practice.

## Deployment note

There is no migration, no procedure, and no operator-facing repair guidance: those are out of scope,
and the artifacts state only what the service will not do. One fact belongs in `docs/DEPLOY.md`: a
downgrade is lossy, because an older binary does not recurse and a template in a directory is
invisible to it rather than broken.

## Open Questions

None. The nine questions the issue raised are all answered in `proposal.md` and the delta specs.
