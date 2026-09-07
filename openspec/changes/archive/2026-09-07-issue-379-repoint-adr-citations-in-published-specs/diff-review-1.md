TREE_SHA256: d802e2340a6c2f31d51dc951cd7fd99fb06b31478b7b2e7f4234d4d09df4efd3
SPECS_SHA256: a869d537a378142a2987af27a5434c769154f4035cc4b6f4c7a9fc55a8a3c34d

Adversarial review of 379 — checking the diff against its contract and AGENTS.md.Verified core mapping and word-level fidelity — now scanning for any remaining contract gaps.Review of `issue-379-repoint-adr-citations-in-published-specs` against `proposal.md:1-46`, `design.md:1-115`, `specs/*/spec.md` deltas, `tasks.md:1-56` and `AGENTS.md`.

**Prior review fixes verified [verified]:**

* `proposal.md:11` now reads "with one exception. At `colour-vocabulary:164` … `§6` anchor is dropped" — matches `design.md:83-87` and `specs/colour-vocabulary/spec.md:19` (`ADR-0092 §6` → `openspec/changes/archive/2026-08-31-issue-280-shape-paint-model`). Prior "keeps its wording" absolute is resolved.
* `proposal.md:40` now reads "six pointer swaps across five requirements, one of which carries two … holds both ADR-0059 and ADR-0084" — fixes prior "one pointer each" understatement. Verified: `openspec/specs/layout-sizing/spec.md:743` (ADR-0059) and `:781` (ADR-0084) both in `Text is laid out…` (`openspec/specs/layout-sizing/spec.md:689`), delta `specs/layout-sizing/spec.md:57` and `:97` indeed carry two swaps.
* `design.md:106` / `tasks.md:29` predicate is now `ADR-0(05[89]|0[6-9][0-9]|[1-9][0-9]{2})` — catches ADR-0058/0059. Verified pre-change `grep -rnoE` on `openspec/specs/` returns exactly 6 (`flow-layout:339,559` ADR-0082, `layout-sizing:743` ADR-0059, `:781` ADR-0084, `:993` ADR-0058, `colour-vocabulary:164` ADR-0092); post-change delta grep returns 0; `ADR-0045`/`ADR-0050` at `specs/layout-sizing/spec.md:45,96,105` retained.

**Delta fidelity [verified]:**

* Headers byte-identical: `specs/flow-layout/spec.md:3,162` vs `openspec/specs/flow-layout/spec.md:275,529`, `specs/layout-sizing/spec.md:3,302` vs `openspec/specs/layout-sizing/spec.md:689,986`, `specs/colour-vocabulary/spec.md:3` vs `openspec/specs/colour-vocabulary/spec.md:149`.
* Word-level diff (`git diff --word-diff=porcelain --word-diff-regex='[^[:space:]]+'` concatenated source `flow 275-431+529-651`, `layout 689-984+986-1020`, `colour 149-179` vs delta bodies) shows exactly 6 token replacements: `(ADR-0082)`×2 → ``(`openspec/changes/archive/2026-08-27-issue-226-unify-size-resolution`)``, `ADR-0059,` → `` `openspec/changes/archive/2026-08-21-issue-180-auto-length-text-alignment`,``, `ADR-0084` → `` `openspec/changes/archive/2026-08-28-issue-245-center-ink-reserve` ``, `ADR-0058` → `` `openspec/changes/archive/2026-08-21-issue-181-duplicate-id-not-fatal` ``, `ADR-0092 §6` → `` `openspec/changes/archive/2026-08-31-issue-280-shape-paint-model` ``. No seventh word changes. Line-count deltas (+3 in flow/layout) are re-wrapping per `design.md:51-55` and do not affect words.
* All five archive folders exist and contain the cited rationale (`openspec/changes/archive/2026-08-27-issue-226-unify-size-resolution`, `2026-08-21-issue-180-auto-length-text-alignment`, `2026-08-28-issue-245-center-ink-reserve`, `2026-08-21-issue-181-duplicate-id-not-fatal`, `2026-08-31-issue-280-shape-paint-model`).
* `layout-sizing:781` mixture retained: delta `specs/layout-sizing/spec.md:96-97` is `ADR-0045, ADR-0050 and `archive/2026-08-28…`` — as intended per `proposal.md:24` and `design.md:57-61`.
* `DELIVERABLE: spec-only` at `proposal.md:7` correct; `openspec validate --strict` passes; `git status --porcelain` shows only untracked change folder, no edits to `openspec/specs/`, `src/`, `ui/`, `docs/SPEC.md`, `docs/adr/` per `tasks.md:3.1`; gates listed in `tasks.md:54-56` match `AGENTS.md:108-111`.

**Non-blocking note:**

Branch `issue-379-repoint-the-six-adr-citations-in-publish` is behind `origin/main` (`HEAD` at `7ef3b42` vs `origin/main` at `dd78fcc`/`16cb923`). `AGENTS.md` requires rebase onto `origin/main` before merge so diff-review covers landing tree; published specs for these three capabilities are byte-identical at the six sites on both refs (`grep` positions unchanged), so no content divergence, but rebase remains required before landing.

No blocking findings.

VERDICT: APPROVE
