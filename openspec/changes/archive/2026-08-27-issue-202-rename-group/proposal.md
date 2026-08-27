## Why

Implements [#202](https://github.com/pfa230/labeler/issues/202).

A group can be created, listed, filled and deleted, but not renamed. Fixing a typo in `Warehosue`,
or recasing `shipping` to `Shipping`, currently means creating the correct group, moving every
template into it one `PUT /api/templates/{id}/group` at a time, and deleting the old directory.
Since #227 made a group a directory (ADR-0073), the operation the user actually wants is a single
`rename(2)`, and nothing in the API exposes it.

## What Changes

- **A group is renamed by name.** A new route renames one group's directory in place. It changes the
  final path segment only: `Shipping/Pallets` may become `Shipping/Euro`, never `Warehouse/Pallets`.
  Reparenting a group is out of scope and stays an issue for later.
- **No template is touched.** A rename moves no template file, rewrites no YAML byte, and changes no
  template id, so favorites and job history keep pointing at the same templates. Templates the
  caller cannot see, and quarantined files the registry refused, follow the directory like every
  other file in it.
- **Renaming onto an occupied name is refused, never merged.** A destination that already exists is
  a `409`, matching the delete route's refusal of a non-empty group. Merging two groups stays what it
  is today: moving templates one at a time.
- **Recasing uses the same no-replace operation.** On the case-sensitive Linux filesystems where the
  service deploys, `shipping` to `Shipping` is a rename to a free name and succeeds. A filesystem
  that aliases those spellings may refuse the operation because there is no portable, race-free way
  to prove that two path spellings are the same directory entry and then use an ordinary replacing
  rename safely; the service reports that refusal and never risks replacing a distinct group.
- **BREAKING (behaviour, not wire format): the case-conflict rule follows the filesystem.** Today the
  service refuses to create any group whose name differs from an existing sibling only by case, on
  every platform, by comparing names before it tries. On a case-sensitive filesystem that refuses
  something the platform allows and that this capability's own "Case is significant" rule blesses,
  since `Warehouse` and `warehouse` are specified as two groups. The prediction is dropped: the
  service attempts the creation exclusively and lets the filesystem answer, refusing only when the
  filesystem itself says the name is taken. A caller that relied on receiving
  `422 template_group_case_conflict` on Linux will now get `200` and a second group.
- **The Labels view renames the selected group from the group filter controls.** The rename reports
  its refusals rather than failing silently, and the active filter follows the group it was pointed
  at without an empty-grid transition.

## Capabilities

### New Capabilities

None. Groups are already a capability; this adds an operation to it.

### Modified Capabilities

- `template-groups`: adds a requirement for the rename route, its refusals and its post-rename
  confirmation; modifies **A group is a directory under the templates directory** so the
  case-conflict rule is answered by the filesystem at creation time rather than predicted before it;
  modifies **The Labels view browses and edits groups** to carry the rename affordance and the
  filter-follows-rename rule.

## Impact

- **API.** One new route under `/api/template-groups`, documented in `src/openapi.rs`. No existing
  route changes shape. `PUT /api/templates/{id}/group` and `DELETE /api/template-groups/{path}` are
  unchanged except that the first can now create a case-variant sibling where the filesystem permits
  one.
- **Code.** `src/api.rs` (route, handler), `src/fs_safe.rs` (a no-replace directory rename beside the
  existing no-replace file move, and the removal of the lowercase-equality pre-check in
  `check_sibling_name`), `src/templates.rs` (post-rename subtree path validation), `src/openapi.rs`.
  No new `details.reason` is expected: the route reuses `template_group_invalid`,
  `template_group_case_conflict`, `template_group_unsafe_path` and `template_registry_io`.
- **UI.** `ui/src/pages/Templates.tsx` adds a rename action alongside the existing flat group-filter
  toolbar and keeps the selected group's templates visible while cached paths and the selection are
  changed together. `ui/src/api/queries.ts` adds the mutation and cache update.
- **Decisions.** A new ADR amending the case clause of ADR-0073, plus its row in
  `docs/adr/README.md`. ADR-0073 is otherwise unaffected: a group stays a directory, an id stays a
  filename.
- **Not affected.** Template ids, favorites, job history, the render and batch endpoints, and
  `docs/SPEC.md`, which is frozen and never described groups.
- **Out of scope.** The current `template-groups` specification describes a nested group tree, while
  the implementation on main currently renders a flat toolbar of buttons labelled with full group
  paths. That pre-existing implementation gap is not fixed by #202; this change adds rename to the
  control that exists and does not plan or build a tree.
