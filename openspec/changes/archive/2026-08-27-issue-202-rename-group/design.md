## Context

See `proposal.md` for motivation. What shapes the approach:

- Since #227 (ADR-0073) a group *is* a directory under `{LABELER_CONFIG_DIR}/templates/`, and a
  template's id is its filename stem. Renaming a group therefore touches no template file.
- The group write surface already exists: `PUT /api/templates/{id}/group` moves a template's file,
  `DELETE /api/template-groups/{path}` removes an empty group, `GET /api/template-groups` lists the
  tree. This change adds the one operation that is missing, and inherits their conventions:
  serialize on `state.write_lock`, re-read the tree before resolving a path, re-read and confirm
  after writing.
- `src/fs_safe.rs` already holds the primitives this needs: descriptor-relative entry listing
  (`list_dir_entries`), component-wise resolution that refuses symlinks (`resolve_group_dir`), and an
  atomic no-replace move built on `rustix::fs::renameat_with(.., RenameFlags::NOREPLACE)` with an
  `EXIST` branch (`src/fs_safe.rs:424-440`). The rename route is a new caller of that machinery, not
  a new mechanism.
- The `template-registry` capability requires that the service never rewrites, moves, or renames
  anything under `templates/` on its own, at startup or at any other time
  (`openspec/specs/template-registry/spec.md`, "The service never writes to the templates tree
  unasked"). Any design that writes to the tree to learn something about it is ruled out by that
  requirement, not merely inadvisable.
- The case-conflict rule shipped with #227 refuses a case-differing sibling on every platform, by
  lowercase-equality pre-check in `check_sibling_name` (`src/fs_safe.rs:74-90`). On the
  case-sensitive filesystems this service actually runs on, that refuses what the platform allows and
  what the same requirement's exact-comparison rule blesses.

**ADR.** This change adds `docs/adr/0076-the-filesystem-answers-the-case-question.md`, amending the
case clause of ADR-0073 and adding its row to `docs/adr/README.md`. ADR-0073 is not superseded: a
group stays a directory, an id stays a filename, and only the create-time case refusal changes.

## Goals / Non-Goals

**Goals:**

- One syscall renames a group, whatever it holds, with no intermediate state a crash can expose and
  no window in which a concurrent writer can turn the operation destructive.
- Recasing a group (`shipping` to `Shipping`) works on the case-sensitive Linux filesystem where the
  service deploys, without weakening the never-replace guarantee on other filesystems.
- A rename never merges, never overwrites, and never renames a group the caller did not name.

**Non-Goals:**

- Reparenting. `Shipping/Pallets` cannot become `Warehouse/Pallets` here. A later change may add it;
  the body key chosen below leaves room for that without a breaking change.
- Merging two groups. That stays moving templates one at a time.
- Predicting any filesystem's case folding or normalization. The service stops trying.

## Decisions

### `PUT /api/template-groups/{path}` with `{ "name": "<segment>" }`

The group already has a resource identity at that path, which `DELETE` addresses. `PUT` updates it.

The body carries `name`, a single segment, not `to` carrying a whole path. That is what makes
"rename only, never reparent" structural rather than a rule to enforce: `/` is already forbidden
inside a segment, so a path in `name` fails ordinary segment validation with no special case. If
reparenting is wanted later it arrives as a second key or a second route, and this contract does not
change.

*Alternatives.* `POST /api/template-groups/{path}/rename` reads as an action rather than an update
and adds a path shape the surface does not otherwise use. `{ "to": "<path>" }` covers rename,
reparent and promote in one route, which is the shape to adopt if reparenting is ever added, but
adopting it now would ship a contract whose reparenting half is specified as refused.

### The rename is no-replace, never check-then-rename

`renameat_with(.., RenameFlags::NOREPLACE)` is the operation. It fails with `EXIST` when the
destination name is taken, and that failure is the `409`.

Listing the parent first and then issuing an ordinary rename is **not** an acceptable substitute, for
two independent reasons. Linux `rename(2)` on a directory silently replaces an *empty* destination
directory and fails `ENOTEMPTY` only when the destination has contents, so an ordinary rename would
destroy exactly the empty group the delete route protects with a `409`. And `state.write_lock` is an
in-process mutex (`src/api.rs:64-68`); it does not stop an operator, a sync agent, or a second
process from creating the destination between the listing and the syscall. The check-then-act shape
is a TOCTOU hole with a data-loss outcome, and the codebase already avoids it in the move path.

`NOSYS`/`INVAL` from `renameat_with` means the platform has no no-replace rename. The existing move
path falls back to `linkat` + `unlinkat`, which works for a file and not for a directory, so this
route refuses with `500` instead. Refusing is correct: the alternative is an ordinary rename, which
is the destructive operation this decision exists to avoid.

### No portable same-dirent proof exists, so recasing stays no-replace

`(st_dev, st_ino)` identifies an underlying object, not the directory entry that names it. Two
sibling bind mounts can expose the same directory with the same device and inode under different
names, and those names are distinct groups. Directory enumeration tells us the spellings that existed
in one snapshot but does not supply a stable dirent handle that can be passed to a later rename.

More importantly, any metadata or listing criterion followed by ordinary `renameat` is still a
check-then-destructive-operation race. An external writer is not excluded by `state.write_lock` and
can replace either name between the observation and the syscall. The supported platforms expose no
portable operation that says both "these spellings are this one dirent" and "change its spelling
without replacing a distinct destination" atomically. The design therefore does not claim such a
criterion.

The fallback is deliberately limited and testable:

- a byte-identical name is the idempotent case and performs no syscall;
- every byte-different name, case-only or otherwise, uses the single
  `renameat_with(..., RenameFlags::NOREPLACE)` operation;
- on the case-sensitive Linux deployment filesystem, `shipping` to `Shipping` succeeds because the
  destination spelling is free;
- if a case-folding filesystem reports `Shipping` occupied by its alias of `shipping`, the operation
  returns `409` and leaves the name unchanged. If a filesystem supports an atomic no-replace recase
  and the postcondition observes it, it may succeed normally;
- no branch ever falls back from this refusal to ordinary rename.

This gives up guaranteed case-only rename on folding filesystems in exchange for a guarantee the
implementation can keep: it never replaces a distinct group and never uses inode equality as a
dirent identity test.

### The filesystem answers the case question; the service stops predicting

The lowercase-equality pre-check in `check_sibling_name` is removed. Existing exact-entry reuse stays
ahead of creation: list the parent, and when an exact name is present, open it with
`O_DIRECTORY|O_NOFOLLOW`. An exact directory is reused; an exact file or symlink is
`422 template_group_unsafe_path`.

Only when no exact entry is listed does the service attempt `mkdirat` exclusively. Success creates
the segment. `EEXIST` is not automatically a case conflict, because an external writer may have
created the exact directory after the listing. The branch re-lists and:

- safely opens and reuses an exact directory;
- refuses an exact file or symlink as `422 template_group_unsafe_path`;
- when no exact entry is present, resolves the requested spelling with no-follow semantics. If it
  still resolves to a directory, the filesystem supplied a non-exact alias and the request returns
  `422 template_group_case_conflict`, naming the existing group by the spelling stored in the parent,
  and reusing nothing. For this error-message lookup only, each listed sibling is compared by
  `(st_dev, st_ino)` with the resolved requested spelling; that comparison never authorizes reuse,
  rename, or any other mutation. If it resolves to a file or symlink, the request returns
  `422 template_group_unsafe_path`;
- if the requested spelling no longer resolves after `EEXIST`, treats the occupant as vanished and
  retries `mkdirat` exactly once;
- after a second `EEXIST`, performs one final re-list and no-follow classification. Exact directory,
  unsafe entry and non-exact directory alias keep the outcomes above; a second vanished occupant is
  an unstable concurrent race and returns `500 template_registry_io` rather than retrying again;
- maps any listing, open, or create I/O failure outside that bounded race to
  `500 template_registry_io`.

There is no unbounded retry. In particular, the branch does not call a third `mkdirat` after the
second `EEXIST`.

This is strictly better than the alternative considered first, a startup probe that creates and
removes a dot-prefixed entry to learn the filesystem's behaviour. The probe is **forbidden** by the
`template-registry` requirement that the service never writes to the templates tree unasked. It is
also wrong on its own terms: one probe at the root cannot answer for a tree whose subdirectories sit
on different mounts, and it fails in the direction that cannot be repaired, since a folding root with
a case-sensitive child would refuse a legal create before `mkdirat` was ever attempted. The
`EEXIST` branch has neither problem: it is per directory, it is the filesystem's own answer, and it
needs no prediction, so no prediction can be wrong. It also covers foldings no lowercase mapping
implements, `Größe` and `GRÖSSE` among them.

The change removes only the predictive lowercase branch from `check_sibling_name`; the exact-match
reuse behavior stays. The `EEXIST` handling after `mkdirat` gains the bounded re-list/retry state
machine above.

### Whole-path limits use the registry's snapshot boundary

`validate_group_name` enforces 255 characters and 1024 UTF-8 bytes on a *whole* path
(`src/templates.rs:770-780`), and `collect_group_paths` silently omits a directory whose full path
fails it (`src/templates.rs:268-269`). Lengthening one ancestor segment can therefore push the
renamed group, or any group beneath it, past the limit and out of the group tree.

Validating the new segment alone would let the rename happen and be noticed only by the post-rename
confirmation, answering `500` with a subtree already invisible. The handler instead walks the source
subtree before renaming, computes each discoverable group's post-rename path, and refuses `422` if
any in that snapshot fails whole-path validation. Nothing is renamed in that case.

That precondition has the same boundary the `template-registry` capability states for its tree
guarantees: it describes the snapshot the request acted on. The in-process lock serializes API
writes, not an operator, sync agent, or second process. A raw descriptor-relative subtree audit after
the rename checks the post-mutation snapshot before the registry confirmation. If a raced descendant
is then present and the longer ancestor makes its path exceed 255 characters or 1024 UTF-8 bytes, the
request returns `500` and leaves the already-published directory rename in place; it does not attempt
an unsafe rollback. A directory created after the audit is outside the response's guarantee and is
handled by a later ordinary reload. This is an explicit concurrency limitation, not a synchronization
claim.

### Source-path failures follow the write-endpoint rule

The `template-registry` capability distinguishes write endpoints from the group delete: a symbolic
link in a request-supplied group path is `422 template_group_unsafe_path` for a write, and `400` only
for `DELETE /api/template-groups/{path}`. Rename mutates the directory tree, so it follows the write
mapping: unsafe source component is `422`, missing exact component is `404`, malformed encoding or an
invalid decoded source path is `400`.

One thing is fixed rather than inherited: the current message calls a `NOTDIR` component a symbolic
link, which is wrong when the component is a regular file. The rename route separates the two
messages, and the spec carries a scenario for it.

### The existing group-filter toolbar owns the rename affordance

`ui/src/pages/Templates.tsx` does not currently contain the nested group tree described by the
normative `template-groups` requirement. It renders a flat toolbar of buttons labelled with complete
group paths. That implementation gap predates #202 and is outside this change: the work SHALL NOT
build a tree or introduce a node component. Rename is offered for the selected real group alongside
that existing toolbar; selecting `Warehosue` and invoking rename is how the user reaches the action.
The current specification's tree/node clauses remain unchanged because this delta must carry the
complete requirement and #202 does not supersede them.

When the chosen filter is the renamed group or a descendant of it, the selection is rewritten from
the old path to the new one. The rewrite is by whole path segments, never by string prefix: renaming
`Shipping` must not touch a selection of `Shipping2`.

The sequencing must also keep the grid populated. Merely invalidating queries and immediately
changing `selectedGroup` is wrong: the current cached summaries still carry old `group` strings, so
the filter temporarily matches nothing. The component captures and continues rendering the
pre-rename template snapshot after API success, starts the template and group refetches, and does not
change the selected path yet. Once refreshed template data carries the renamed paths, one component
state transition installs the whole-segment-rewritten selection and releases the snapshot. Thus no
render pairs a new filter path with old summaries or an old filter path with new summaries. If the
template or group refresh fails, the error is reported, the captured snapshot remains visible, and
the old selection remains selected rather than showing a false empty group. The user can retry both
refreshes without repeating the rename; after they succeed and refreshed template data carries the
new paths, the same selection rewrite and snapshot release completes recovery.

## Risks / Trade-offs

- **A folding filesystem refuses a case-only no-replace rename** → answer `409` and leave the group
  unchanged. Safe recasing is not promised there; case-sensitive Linux still performs it.
- **A platform without `RENAME_NOREPLACE`** → refused with `500`. The alternative, an ordinary
  rename, silently destroys an empty destination group; refusing an operation is better than
  performing a destructive one. Linux and macOS both provide it, so no supported deploy is affected.
- **Behaviour change on Linux: `warehouse` may now be created beside `Warehouse`** → intended, and it
  is what the capability's exact-comparison rule already promises. A caller that depended on the
  `422` sees a `200` and a second group.
- **Removing the pre-check moves a non-exact alias refusal after a syscall attempt** → the bounded
  `EEXIST` state machine distinguishes an exact directory race, unsafe exact entry, vanished entry,
  persistent non-exact alias and I/O failure instead of labelling all five as a case clash.
- **The subtree walk costs a directory traversal per rename** → bounded by the group tree, which the
  service already walks on every registry load, and it happens once per rename rather than per
  request.
- **A rename racing a concurrent move into the same group** → service requests both take
  `state.write_lock`; external writers remain outside that lock. No-replace contains destination
  races, while the explicit pre- and post-mutation snapshots bound subtree path guarantees.
- **A deep tree renames as one syscall** → no partial state to reconcile, which is the point of the
  directory model; there is no best-effort branch and no per-template reporting to design.

## Migration Plan

No data migration. Nothing on disk changes shape, and no existing route changes its contract.

**Rollback is safe.** The only behaviour an older binary would disagree with is the create-time case
refusal, and that refusal is on creation, not on load: if a user created `Warehouse` and `warehouse`
side by side under the new binary and then rolled back, both directories still load and both are
still served as two groups. The older binary simply refuses to create a third such sibling.
