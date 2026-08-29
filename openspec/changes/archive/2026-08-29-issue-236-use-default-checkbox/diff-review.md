# Diff review

AUTHOR: claude
REVIEWER: codex

VERDICT: APPROVE

- **Change**: `issue-236-use-default-checkbox`
- **Issue**: #236
- **Rounds**: 2 (REVISE, then APPROVE on a scoped re-review of the fixes)

## How this was run

`.workflow/apply.sh` could not drive it end to end: `run-stage.sh:67` sends `/opsx-apply <change>` and
the claude CLI answers `Unknown command: /opsx-apply. Did you mean /opsx:apply?`, so the implement stage
was a no-op. The two roles were therefore run separately, with the pairing intact and named here as
`openspec/config.yaml` allows for a diff reviewed some other way.

An earlier implement attempt on `agy` produced a partial diff and then hit its account quota
(`Individual quota reached... Resets in 38h2m17s`), so the implementation was completed by a headless
`claude` subagent. No part of it was reviewed by its own author.

## Round 1 — REVISE

codex raised seven blocking findings against the partial diff: deferred names were still submitted
because `PrintForm` never passed the deferral map to `pruneDataForSubmit`; the input-list request still
received raw `value.data`; a template change retained the previous template's values and deferral; an
entry arriving on a later branch was neither seeded nor concretely deferred; an in-flight `FileReader`
callback could undo a re-check by spreading a stale snapshot; the required `PrintForm` integration tests
were absent and two existing tests still encoded the old behaviour; and ADR-0090 and its index row were
missing.

All seven were fixed. Each new test was proven red against the old behaviour before green.

## Round 2 — REVISE

codex raised two further blocking findings, both real and both missed by every earlier pass:

1. Parameter names may legally collide with `Object.prototype` members. `deferred[name]` was truthy for
   an inherited `constructor`, `name in deferred` was true for one, and assigning `__proto__` on a plain
   object literal creates no own entry at all, so such a field was always omitted or wrongly treated as
   already initialised.
2. A *failed* first input-list request after a template switch left `useLabelInputs` serving the previous
   template's list, and the arrival merge then seeded template A's defaulted entries into template B.

Both were fixed: own-property helpers (`hasOwnKey` / `getOwnKey` / `setOwnKey`) on every
parameter-name-keyed access, and a `templateId` recorded against the held list so another template's list
can never reach the merge. Red was proven per defect rather than in aggregate.

## Round 3 — APPROVE

Scoped to the two fixes rather than the whole change. codex confirmed both FIXED with file:line evidence,
including that `setOwnKey` creates an enumerable own `__proto__` property that survives `JSON.stringify`
into the request body, and that the stale-list path is unreachable by a slow request, a placeholder
template or a remount. Regressions: NONE. New blocking issues: NONE.

## Gates

Verified independently of the implementer, from the worktree, with real exit codes:

| Gate | Result |
| --- | --- |
| `cargo fmt --check` | 0 |
| `cargo clippy --all-targets --all-features` | 0, no warnings |
| `cargo test` | 0, 704 passed |
| `npm test` (ui) | 0, 427 passed across 49 files |
| `npm run lint` (ui) | 0 |
