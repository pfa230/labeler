## 1. Decision record

- [x] 1.1 Write `docs/adr/0076-the-filesystem-answers-the-case-question.md`: the create-time case
      refusal is answered by the filesystem at creation rather than predicted by lowercase comparison,
      amending the case clause of ADR-0073 without superseding it. Confirm 0076 is still free against
      `main` before using it.
- [x] 1.2 Add the ADR's row to `docs/adr/README.md`.

## 2. Group creation stops predicting case

- [x] 2.1 Remove the lowercase-equality refusal branch from `check_sibling_name` in `src/fs_safe.rs`,
      keeping its exact-match result, so a case-differing sibling is no longer refused before the
      filesystem is asked.
- [x] 2.2 Implement the creation state machine the spec sets out in `resolve_or_create_group`: list
      for a byte-exact entry and safe-open it as a directory; otherwise create exclusively; on an
      exists result re-list and re-classify; retry the exclusive create once when the occupant
      vanished; classify exact directory, non-exact alias, file or symlink, and I/O failure
      distinctly, with no third create attempt.
- [x] 2.3 Name the existing group in the `template_group_case_conflict` message by its stored
      spelling, selecting it by comparing each listed sibling by `(st_dev, st_ino)` against the
      resolved requested spelling. Assert in code and in a test that this comparison feeds the message
      only and authorizes no reuse, rename, or other mutation.
- [x] 2.4 Cover the creation branches with tests: exact reuse, exact directory created by a racing
      writer, vanished occupant retried once, non-exact alias refused without reuse, repeatedly
      vanishing occupant as `500 template_registry_io`, file or symlink as
      `422 template_group_unsafe_path`, and an I/O failure that is not reported as a case conflict.
      Each test must fail against the pre-change behaviour before it passes.

## 3. The rename route

- [x] 3.1 Add `PUT /api/template-groups/{path}` to the router and an `update_template_group_name`
      handler in `src/api.rs`, taking `{ "name": "<segment>" }`, serializing on `state.write_lock` and
      re-reading the tree before resolving, as the move and delete routes do.
- [x] 3.2 Resolve the source path by exact entry name per component, rejecting a malformed percent
      sequence with `400` before decoding, and reporting a symlinked or non-directory component as
      `422 template_group_unsafe_path` with messages that tell those two cases apart. This is a write
      endpoint, so it takes the `422` side of the `template-registry` rule, not the delete route's
      `400`.
- [x] 3.3 Validate the new name as a group segment, then walk the source subtree and refuse
      `422 template_group_invalid` when the renamed group's or any discoverable descendant's
      post-rename path would exceed the 255-character or 1024-byte whole-path limits, with nothing
      renamed.
- [x] 3.4 Add a no-replace directory rename to `src/fs_safe.rs` beside the existing no-replace file
      move, built on `renameat_with(.., RenameFlags::NOREPLACE)`. Map `EXIST` to `409`. Map
      `NOSYS`/`INVAL` to `500`: the file path's `linkat` fallback does not work for a directory, and
      an ordinary rename would replace an empty destination.
- [x] 3.5 Issue that single no-replace rename for every byte-different source and destination, with no
      identity check authorizing an ordinary replacing rename. Handle the byte-identical name as the
      idempotent `200` that renames nothing.
- [x] 3.6 After the rename, run the post-mutation subtree audit and the confirmation: re-read the
      tree, require the new path to be a group and the old path not to be, and answer `500` for a
      failed audit or confirmation without rolling the rename back.
- [x] 3.7 Return `200 { "group": "<new path>" }` and register the route, its request body and every
      status in `src/openapi.rs`.

## 4. Rename route tests

- [x] 4.1 Cover the success paths: a top-level rename, a nested rename changing only the last segment,
      descendants following the renamed group, template ids and favorites unchanged, template bytes
      unchanged, and a quarantined file following the directory and reported under `broken` at its new
      path.
- [x] 4.2 Cover the refusals: occupied destination, an **empty** destination directory that must not
      be replaced, a name carrying a slash, an invalid name, a body omitting the key, an unknown
      group, a case-mismatched source segment, a malformed percent sequence, a symlinked component,
      and a component that is a regular file.
- [x] 4.3 Cover recasing: `shipping` to `Shipping` succeeding where the destination name is free, and
      a `409` with nothing renamed where the filesystem reports the case-only destination as existing.
      Assert no ordinary replacing rename is attempted on either path.
- [x] 4.4 Cover the whole-path limits: the renamed group's own path crossing a limit, and a descendant
      crossing it while the renamed group's own path stays valid, both refusing with nothing renamed
      and every group still listed afterwards.
- [x] 4.5 Assert the empty-destination test fails against an ordinary `rename` and passes only with
      the no-replace call, so the guard cannot regress silently.

## 5. Labels view

- [x] 5.1 Add the rename mutation to `ui/src/api/queries.ts`, calling
      `PUT /api/template-groups/{path}` and surfacing the route's refusals distinctly.
- [x] 5.2 Make the rename action reachable from the group filter control for the currently selected
      real group, offered for a real group only and never for the synthetic `All` or `Ungrouped`
      entries. Take a name, not a path.
- [x] 5.3 On success, hold the pre-rename template snapshot and the old selection while the template
      and group queries refresh, then rewrite the selected path by whole path segments and release the
      snapshot together. The rewrite must match on segment boundaries so renaming `Shipping` leaves a
      `Shipping2` selection alone.
- [x] 5.4 On a failed refresh, report the failure, keep rendering the captured snapshot, retain the
      old selection, and let the user retry both refreshes without repeating the rename.
- [x] 5.5 Report a rejected name, an occupied name, and a group that has gone missing as the errors
      they are.
- [x] 5.6 Test the view: renaming from the filter control, the filter following the rename, the filter
      following a renamed ancestor, no intermediate render showing an empty grid, the failed-refresh
      branch and its retry, and the two refusal messages.

## 6. Verify

- [x] 6.1 Run `cargo fmt`, `cargo clippy --all-targets --all-features` and `cargo test`; fix root
      causes rather than silencing lints.
- [x] 6.2 Run the UI test suite and the UI build.
- [x] 6.3 Exercise the route against a running server on a real templates directory: rename a group
      holding templates and a nested subgroup, confirm on disk that the directory moved and no
      template file changed bytes, and confirm `GET /api/template-groups` and `GET /api/templates`
      report the new paths.
