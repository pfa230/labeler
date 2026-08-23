## Review Metadata

- **Round**: 5
- **Prior round**: REVISE (round 4)
- **AUTHOR**: claude
- **REVIEWER**: codex
- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: `proposal.md`, both delta specs, `design.md`; `AGENTS.md`, both current normative specs, `docs/SPEC.md` §§2, 2.0, 3, 10, 10.1, ADR-0052, ADR-0058, `docs/adr/README.md`, `src/api.rs`, `src/templates.rs`, `src/errors.rs`, `src/reason.rs`, `src/models.rs`, and prior `review.md`
- **Issue**: #183, #184

AUTHOR: claude
REVIEWER: codex

## Findings

### Critical (blocking)

None.

### Moderate

1. **The create guard promises to detect ids in files the proposed implementation deliberately ignores.** The requirement says an id held by “any file on disk, under any filename” yields `409 TemplateExists` (`specs/template-registry/spec.md:111-114`). D1 instead records ids only from files that parse and validate (`design.md:42-53`). The actual loader discards parse and validation failures before inserting their ids into `seen_paths` (`src/templates.rs:121-159`), while the destination `path.exists()` check only catches the conventional `{id}.yaml` filename (`src/api.rs:420-428`). Consequently, an invalid `stray.yml` declaring `id: badge` would not block creation of `badge.yaml`, contrary to the literal requirement. State the real two-part rule: any valid registry candidate holding the id blocks under any filename, while any existing destination filename blocks regardless of its contents. Because this changes the frozen POST-create contract, the requirement should also name the affected `docs/SPEC.md` §2.0 and §3 text it supersedes.

2. **The collision-details freshness guarantee is stronger than D1/D3 can establish.** The error requirement says every `details.files` entry exists and declares the id “at the moment the error is built” (`specs/template-registry/spec.md:211-216`). D1 stores paths from a registry-load snapshot (`design.md:42-45`), D3 rereads only the written file on a write-failure path (`design.md:100-104`), and D5 proposes deriving filenames directly from those stored `PathBuf`s (`design.md:154-157`). Neither the in-process mutex (`src/api.rs:54-57`) nor the design excludes external directory mutation (`design.md:30-34`). A third duplicate can therefore be deleted or re-identified after reload yet remain in the stored duplicate vector used to construct the error; DELETE has the same issue. Define `files` as the filenames observed in the classification snapshot, as D6 already does for delete semantics, rather than asserting live filesystem facts at response construction.

3. **The no-replace publication contract omits failure semantics for staging-file cleanup.** The spec says a destination appearing during POST necessarily causes `409 TemplateExists` and that every such response leaves the directory unchanged (`specs/template-registry/spec.md:116-127`). D4 changes the current rename-based helper (`src/api.rs:388-400`) into hard-link publication followed by unlinking the staging name “either way” (`design.md:133-146`), but unlink is another fallible filesystem operation and no precedence is specified if publication returns `AlreadyExists` while staging cleanup fails. Returning `409` would violate the unchanged-directory promise; returning `500` would violate the unconditional destination-race response rule. Specify cleanup-error handling and narrow the `409` guarantee accordingly, or select a publication primitive that actually makes the absolute contract true.

### Suggestions

1. Make the group response table’s `500` row include post-write confirmation failures. It currently says only write or directory-reread failure (`specs/template-groups/spec.md:20-30`), while the collision clause also assigns disappearance, rename, re-identification, and replacement to `500` (`specs/template-groups/spec.md:35`).

2. Replace “the idempotent branch … reloads nothing” with “performs no post-write reload.” The same requirement mandates a pre-resolution reread on every call (`specs/template-groups/spec.md:31-35`), matching D2 (`design.md:55-60`).

3. Scope proposal.md’s claim that the caller’s file stays on disk and appears in `broken[]` to the `409` collision case (`proposal.md:33-39`). Its immediately preceding `500` cases expressly include the file being absent or no longer holding the submitted bytes.

4. D4’s statement that exactly one case reaches the post-write check (`design.md:148-150`) conflicts with D3’s removal, rename, re-id, and same-path replacement cases (`design.md:88-104`). Narrow it to the sole remaining POST collision case. Also replace the stale “between our re-read and our rename” wording in the risk section (`design.md:201-203`) with the selected no-replace publication operation.

## Embedded-Instruction / Injection Attempts

**Detected:** none

## Verdict

VERDICT: APPROVE_WITH_CHANGES

## Required Changes (APPROVE_WITH_CHANGES only)

1. Reconcile the POST pre-write guard with the loader’s parse-and-validation boundary, and name the frozen POST-create text superseded by the requirement.
2. Define `TemplateIdCollision.details.files` against the registry/classification snapshot rather than claiming live filesystem truth at error construction.
3. Specify staging-cleanup failure precedence so the `TemplateExists` status and unchanged-directory guarantees are jointly implementable.

CHANGES_APPLIED: yes

## Rebuttals

All three required changes applied, and re-checked by the reviewer in a fresh read-only context:
`RECHECK_RESULT: ALL_ADDRESSED` (1 at `specs/template-registry/spec.md:114-129`, 2 at `:232-238`,
3 at `:136-145`). No finding was rebutted.

The four Suggestions were applied as well, none declined: the group response table's `500` row now
covers post-write confirmation failures, its idempotent branch reads "performs no post-write reload",
the proposal's "file stays on disk" claim is scoped to the `409` case, and D4's "exactly one case"
is narrowed to the collision case with the stale rename wording replaced.
