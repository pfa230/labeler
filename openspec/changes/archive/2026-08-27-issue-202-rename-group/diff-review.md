# Diff review: issue-202-rename-group

AUTHOR: agy
REVIEWER: claude

- **Issue**: #202
- **Rounds**: 3 (initial review, two fix rounds)
- **Implementation**: run via `.workflow/apply-with-agy.sh`, so this file is written by hand rather
  than by `.workflow/apply.sh`, which is what records it on the `apply.sh` path. Same fields, same
  contract: the diff was written by agy and reviewed by an agent that did not write it.

## Round 1

Three Moderate findings against the implementation diff.

1. The segment-boundary rewrite in `ui/src/pages/Templates.tsx` was correct but untested. The test
   named for the property renamed `Shipping/Pallets` to `Shipping/Boxes` and never constructed a
   prefix-sharing sibling, so a regression to a bare `startsWith` would have passed the suite.
2. The specs delta's "no intermediate render shows an empty grid" scenario had no assertion anywhere
   in the UI suite.
3. `update_template_group_name` bound the safely resolved source directory descriptor as
   `_src_dir_fd`, discarded it, and walked the subtree twice by re-stated string path, which the
   `template-registry` capability warns undoes the resolution.

## Round 2

Findings 1 and 3 verified fixed. Finding 2's fix was not real.

The new test passed with the transition snapshot gutted to `[]` at `ui/src/pages/Templates.tsx:665`,
a state in which the empty-grid message is exactly what should render. Its assertions ran in the same
tick as the click, before the mutation resolved and before the transition state existed, so they
described the pre-click state and could not fail. The same test also asserted against the string
`"No templates in this grid."` where the rendered string is `"No templates in this group."`, an
assertion that could never fail either.

## Round 3

Re-verified by mutation, not by reading the report:

- `snapshot: []` at `Templates.tsx:665` now fails
  `keeps pre-rename templates rendered continuously during transition without showing empty grid`;
  restoring `snapshot: currentTemplates` passes. Inversion confirmed in both directions.
- Removing `+ "/"` from the predicate at `Templates.tsx:660` fails
  `preserves prefix-sharing sibling group selection when ancestor name is renamed`, and only that
  test; restoring it passes.
- The dead `"in this grid"` assertion is gone.
- `src_dir_fd` is used, `collect_subgroup_rel_paths_fd` is `openat`-relative with `NOFOLLOW`, and the
  string-based helper is removed from `src/templates.rs`.

## Gates, run by the reviewer rather than reported by the implementer

`cargo fmt --check` clean; `cargo clippy --all-targets --all-features` zero warnings; `cargo test`
606 passed, 0 failed; `ui` suite 34/34 in `Templates.test.tsx`. Tasks 27/27.

## Live exercise of task 6.3

The task was checked with no evidence in the implementer's log that a server had been run, so the
reviewer performed it against `cargo run --bin labeler` on a seeded config dir holding
`Warehosue/bin-tag.yaml`, `Warehosue/Pallets/euro.yaml` and a deliberately broken file:

- rename returned `200 {"group":"Warehouse"}`; groups became `["Warehouse","Warehouse/Pallets"]`;
- template ids unchanged and both files byte-identical by md5 before and after;
- the refused file followed the directory, reported at `Warehouse/broken.yaml`;
- `409` onto an occupied name, and `409` onto an **empty** existing directory with that directory
  surviving, which is the data-loss case proven against a real filesystem;
- `Warehouse` to `warehouse` returned `200`, the recasing this issue exists for;
- `422` for a slash in the name, `400` for a body missing the key, `404` for an unknown group.

## Outstanding, out of scope, reported to the human

`ui/pnpm-lock.yaml` carries a 246-line addition of `@svar-ui/react-grid@2.7.3`. `ui/package.json`
already declares that dependency and the lockfile at the base ref contains no reference to it, so
this repairs a stale lockfile on `main` rather than doing this issue's work. Kept or reverted at the
human's direction, not the reviewer's.

VERDICT: APPROVE
