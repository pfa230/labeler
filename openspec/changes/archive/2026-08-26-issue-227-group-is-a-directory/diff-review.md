# Diff Review: issue-227-group-is-a-directory

## Metadata

<!-- Who wrote the implementation, and who reviewed it. They must differ. -->

AUTHOR: agy
REVIEWER: claude

- **Issue**: #227
- **Scope reviewed**: the full implementation diff in `.worktrees/issue-227` (43 files,
  +2251/-2038, plus `src/fs_safe.rs` and `docs/adr/0073-*.md` as new files)
- **Recorded by**: the orchestrating Claude session, which did not write the code. The apply ran
  through `.workflow/apply-with-agy.sh`, which implements but records no verdict of its own; the
  paired `.workflow/apply.sh` would have written this file.

## Findings

None blocking. Nothing was accepted on the implementer's report alone: every claim below was
re-derived here.

## What was verified independently

**Gates, re-run rather than trusted.** `cargo fmt --check` clean, `cargo clippy --all-targets
--all-features` zero warnings, `cargo test` 584 passing, `npm test -- --run` 389 passing across 48
files. Re-run again after the merge into `main`: 596 passing.

**Containment, mutation-tested.** `src/fs_safe.rs` resolves every path component with `openat` and
`O_NOFOLLOW` and mutates only fd-relative: `renameat_with(NOREPLACE)` for a move with
`linkat`+`unlinkat` as the fallback, `mkdirat`, `unlinkat(AT_REMOVEDIR)`. Removing `NOFOLLOW` from
the resolver turns `template_symlink_refusals_on_create_move_and_delete` red; restoring it turns it
green. The test can fail, which is what makes its passing evidence.

**The three rules the plan review argued hardest over.** The resolver lists for an exact entry name
before `mkdirat`, so `EEXIST` reads as the case-alias refusal rather than silently reusing an
aliased directory. Every location gate in the load path `continue`s before the id contest, so a
template under an invalid directory cannot displace a serviceable one. The unconditional `PUT`
re-opens its destination `O_NOFOLLOW` before re-classifying to a replace.

**Live behaviour, against a running service.** Installed a catalog template into `tape/brother`
(`201`); `GET /api/template-groups` returned `["tape","tape/brother"]`, listing the intermediate
directory. Then: `412` on conditional re-create, `400` on a non-`*` `If-None-Match`, exact filter 0
vs `nested=true` 1, `409` deleting a non-empty group, `404` on a case-mismatched delete, `400` on
`%ZZ`, `422` on a traversal group path. With namesakes planted under `.attic/` and `bad:name/`, only
the latter appeared in `broken[]` (by relative path) and the delete still returned `204`, which is
the contender rule.

**Render and look, performed not assumed.** Rendered the nested-group template to PNG and opened the
image: 838x128, text clean and inside the printable area. Task 8.7 is satisfied by this reviewer
having looked, not by the box being checked.

**Boundaries.** `docs/SPEC.md` untouched, `openspec/specs/` untouched at apply time, no commits from
the apply stage, and the specs digest still matched the reviewed contract.

## Coverage not reached

Stated so the next reader knows where the residual risk sits, rather than implied by silence:

- the UI diff beyond test density (`ui/src/pages/Templates.tsx` alone is +216 lines);
- ADR-0073's prose, and `docs/AUTHORING.md`'s rewritten body. Both were checked for the specific
  thing that would be wrong (no `id:`/`group:` teaching, correct supersession rows), not read whole;
- `ui/src/pages/NewTemplate.test.tsx` has three cases (success, `412`, `422`) and does not obviously
  cover the new id-field validation itself.

## Verdict

VERDICT: APPROVE
