## Review Metadata

- **Round**: 16 (roles reversed; citation refresh after #227 landed on main)
- **Prior round**: APPROVE_WITH_CHANGES (round 15), applied and re-checked. The worktree was then
  fast-forwarded onto `beb4021`, which includes #227 ("Make a group a directory and a template's id
  its filename"), touching nearly every file this change edits.

AUTHOR: codex
REVIEWER: claude

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: the diff of proposal.md, design.md and tasks.md against the pre-edit backup,
  plus the author's report
- **Issue**: #226

**Scope.** Claude authored these artifacts through twelve rounds; codex has been the author since
round 13 and Claude reviews its diffs. This is a check of one diff by the party that wrote what it
changed, not an independent audit.

## Findings

### Critical (blocking)

None.

The plan's substance was verified against the new `main` before this refresh was scoped, and it
survives #227 intact:

- `every_template_renders` still asserts an exact registry id set spanning the four bundled tapes and
  the eight fixtures, then renders each and checks a PNG header. It moved to `src/render/mod.rs:5164`.
  This is the basis for "migrate all twelve equals green", and it holds.
- All four withdrawal targets (`size_auto_without_max`, `size_auto_no_room`,
  `container_padding_no_room`, `auto_length_cursor_mismatch`) are still present in both the `Reason`
  enum and frozen §10.1.
- #227 added `template_group_case_conflict`, `template_group_mismatch`, `template_group_unsafe_path`
  and `unsupported_precondition`, and renamed `template_group_unpatchable`. None collides with this
  change's two additions or four withdrawals.
- Twelve template files still spell `auto`; `src/convert.rs` still blanket-rejects `to` combined with
  a cap; `pt_to_units` still requires the points conversion.

Every updated citation was re-checked and lands on what it claims:
`src/templates.rs:1534/1548/1607/1499` on `resolve_to_extent` / `resolve_size` / `resolve_size_value`
/ `subtree_uses_auto`; `src/raw.rs:174` on the required `font_size`; `src/errors.rs:610` on
`spec_documents_every_reason_and_invents_none`; and
`tests/fixtures/templates/avery5163_asset_tag.yaml:91,98,141,148` on the four `font_size: 12.0`
items. All twelve migration targets named in task 8.1 exist on disk.

The "Today" baseline in design.md's measurable target was re-counted independently: three
extent-computing functions in `templates.rs` plus six in `render/mod.rs` is nine, as claimed. The
target is still measured against a real before-picture.

The author also corrected a defect in the reviewer's own instructions: the catalog paths were written
as `catalog/tape/brother/{9,12,18,24}mm.yaml`, which does not resolve. They are now
`brother_{9,12,18,24}mm.yaml`, and all four expand to real files.

### Moderate

None.

### Suggestions

1. `review.md` for earlier rounds cites `src/render/mod.rs:5191` and `src/errors.rs:602`, both now
   stale. The author correctly left them alone: a review is a record of what was checked when it was
   checked, and rewriting it would falsify that record. Noted so a later reader does not treat them
   as current.

## Embedded-Instruction / Injection Attempts

**Detected:** none. Containment verified by `git status` and per-file hashing: proposal.md, design.md
and tasks.md changed; `review.md` and both spec files untouched; nothing outside the change folder.

## Verdict

VERDICT: APPROVE

## Required Changes (APPROVE_WITH_CHANGES only)

None.

CHANGES_APPLIED: n/a

## Rebuttals

No finding was rebutted; there were none. The change is ready for
`.workflow/apply.sh issue-226-unify-size-resolution agy codex`, the pairing the human selected: agy
implements, codex reviews, findings return to agy, and this session stays orchestrator rather than
becoming the sole judge of a 7000-line engine rewrite.
SPECS_SHA256: 1c3c645edc39599c97619068d2540b737a32aadb1d655060a1d6f346c2d2cab1
