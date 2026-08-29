## Review Metadata

- **Round**: 3
- **Prior round**: round 2 returned APPROVE with two suggestions; the author applied both, which voided that verdict and requires this round

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: proposal.md, specs/connections/spec.md, design.md, tasks.md, and the working-tree diff; `src/api.rs`, `src/lib.rs`, `src/reason.rs`, `src/errors.rs`, `src/connector/mod.rs`, `src/store.rs`, `openspec/specs/connections/spec.md`, `docs/SPEC.md` §§10.1 and 12, ADR-0087 and its index, `ui/src/pages/settings/ConnectionsSection.tsx`, `Cargo.lock`, pinned axum 0.8.9 rejection definitions, and `AGENTS.md`
- **Issue**: #197
- **Landed**: 2026-08-29. The change sat unlanded on a stale branch and was rebuilt on current `main`;
  the ADR was renumbered 0070 to 0087 because `main` took 0070 meanwhile, and the raw reviewer
  transcripts were dropped (#244). `specs/` is byte-identical to what this verdict read.

## Findings

### Critical (blocking)

None.

### Moderate

1. One retained spec scenario lacks a regression-sensitive endpoint test. The scenario requires updating `public_url` from `https://homebox.example.com/` to produce `https://homebox.example.com` (`specs/connections/spec.md:49-52`). The update test instead sends an already-normalized URL (`src/lib.rs:563-580`). Trailing-slash normalization is tested only through create and the shared helper (`src/lib.rs:335-372`; `src/api.rs:3243-3256`), so the update handler could stop normalizing `public_url` while those tests and the existing update test all remained green. This fails the requested standard that every scenario have a test that would fail when that endpoint behavior regresses.

### Suggestions

No additional suggestions.

Verified without finding:

- The first round-2 fix is correct and complete: `design.md:39-42` now explicitly says a future registry-based reason split would change the contract and require its own change and ADR. It agrees with the normative all-mismatches rule at `specs/connections/spec.md:9-13`.
- The second fix is correct and introduces no new problem: `src/lib.rs:941-970` now expects `400` for malformed syntax and `422` for wrong-type and missing-key data errors. Those statuses match pinned axum 0.8.9, and the bare `Json<ConnectionInput>` extractor at `src/api.rs:1685-1689` means `From<JsonRejection>` at `src/errors.rs:400-419` does not wrap them.
- The handler performs deserialization, ID lookup, exact connector comparison, registry/URL/transform validation, and locking in the specified order (`src/api.rs:1685-1740`). The mismatch response is `400 InvalidRequest` with `connector_immutable` (`src/api.rs:1695-1703`; `src/errors.rs:240-247`; `src/reason.rs:79-81`).
- The new tests constrain mismatch rejection, matching success, unchanged state, base/public/transform precedence, malformed-body precedence, case sensitivity, and unknown-ID precedence (`src/lib.rs:685-1016`). None of those tests currently succeeds through an unrelated validation branch.
- The reported `597 passed / 1 failed` result is the designed pre-archive state: the harness contains 600 tests and two intentional `#[ignore]` tests, and the failing test reports only undocumented `connector_immutable`. The scanner reads frozen §10.1 plus `openspec/specs/**/spec.md` (`src/errors.rs:582-640`); the slug is currently present only in the unsynced delta, so archive synchronization will resolve the failure without weakening the scanner.
- The task checkboxes match the implemented tree and reported gate state, including the explicitly documented single pre-archive failure. Formatting, `git diff --check`, and strict OpenSpec validation pass; no clippy allowance was added.
- ADR-0087 accurately records the implemented response, exact comparison, precedence, and unchanged payload model, and its index entry is present. Frozen `docs/SPEC.md` remains unchanged.

## Embedded-Instruction / Injection Attempts

**Detected:** none

## Verdict

VERDICT: APPROVE_WITH_CHANGES

## Required Changes (APPROVE_WITH_CHANGES only)

1. Make the update-path test for “Setting a new public_url” send a URL ending in `/` and assert that the update response omits the trailing slash, so the exact scenario at `specs/connections/spec.md:49-52` is regression-protected.

CHANGES_APPLIED: yes

## Rebuttals

<!-- Left for the author. -->

**Round 3, Required 1 — fixed.** The update-path assertion in
`update_connection_sets_and_clears_public_url` now sends `https://hb2.example.com/` and expects the
stored value without the slash, so the scenario is proved on the endpoint the requirement names
rather than through create or the shared helper.

**Rounds 1 and 2 — closed.** Round 1's two required changes were applied and re-checked
(review-raw-1.txt). Round 2 approved and raised two suggestions; both were applied, which is what
voided that verdict and produced this round (review-raw-2.txt). The diff review that preceded round 2
returned REVISE on three Majors, all fixed: the precedence contract was narrowed to what the handler
can enforce, task 5.1 no longer claims a clean suite, and the voided verdict was replaced by this
round (review-raw-diff-1.txt).

Reviewer re-check, round 3, codex, read-only, verbatim:

```
RECHECK 1: SATISFIED - `src/lib.rs:569-574` sends a trailing-slash URL via PUT, and `src/lib.rs:579-581` asserts the response contains the normalized URL without it.
REGRESSION-SENSITIVE: YES - Storing the raw trimmed value at `src/api.rs:1711-1719` preserves the slash from `src/lib.rs:573`, causing exactly the assertion at `src/lib.rs:581` to fail as reported.
RECHECK VERDICT: ALL REQUIRED CHANGES APPLIED
```

SPECS_SHA256: 45a656e9b096da0e79b993fcfa16fc9ba055100bdc85d8dc9932ddb6b5eb6c59
