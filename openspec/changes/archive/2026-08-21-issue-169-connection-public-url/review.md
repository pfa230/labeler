## Review Metadata

- **Round**: 2
- **Prior round**: round 1 returned APPROVE_WITH_CHANGES on the pre-merge artifacts; its four required
  changes were applied and re-checked. `main` then merged #161 and #180, which invalidated three of
  the artifacts' premises (the connection record gained `transforms`, `connector-field-transforms`
  superseded the same frozen §12 sentences, `src/errors.rs` learned to read reason slugs from
  OpenSpec, and ADR numbers 0059-0060 were taken). The artifacts were rewritten against that baseline
  and re-reviewed in full here, so round 1's captures no longer describe any file on disk and were
  not kept.

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only (`codex exec --ignore-user-config -s read-only -c model_reasoning_effort=high`, gpt-5.5)
- **Artifacts reviewed**: proposal.md, specs/connections/spec.md, design.md, plus `src/api.rs`, `src/store.rs`, `src/errors.rs`, `src/reason.rs`, `src/connector/homebox.rs`, `ui/src/api/connectors.ts`, `ui/src/pages/settings/ConnectionsSection.tsx`, `openspec/specs/connector-field-transforms/spec.md`, `docs/SPEC.md`, `docs/adr/0018-api-integration-spine.md`, `AGENTS.md`
- **Issue**: #169

## Findings
### Critical (blocking)
None.

### Moderate
- `openspec/changes/issue-169-connection-public-url/design.md:161-165` records a future follow-up (“Should `PUT /api/connections/{id}` reject a payload whose `connector` differs… It is a follow-up issue”) without citing a GitHub issue. That conflicts with the repo rule that GitHub issues are the sole tracker and deferred work must become an issue, not live only in docs (`AGENTS.md:36-38`, `AGENTS.md:73-75`). Either remove the open question from the planning artifact or cite the actual filed issue.

### Suggestions
- `openspec/changes/issue-169-connection-public-url/specs/connections/spec.md:25-31` says read responses contain `id`, `connector`, `name`, `base_url`, `public_url`, `enabled`, and `has_credential`, but omits `transforms` from the scenario even though the requirement says responses return transforms in full (`spec.md:11-14`) and the source does return them (`src/api.rs:1067-1077`, `src/api.rs:1079-1090`). Add `transforms` to the read scenario to avoid a misleading acceptance test.
- The URL-validation requirement correctly says “no embedded userinfo” (`spec.md:130-147`), while current source has no userinfo check (`src/api.rs:1122-1145`). When implementing, test both `https://user:pass@host` and username-only `https://user@host`; the URL crate treats both as userinfo, and the spec wording is broader than the parenthetical example.
- The plan’s source claims checked out: `public_url` is present in store/API types (`src/store.rs:47-55`, `src/api.rs:1067-1077`, `ui/src/api/connectors.ts:10-28`), Homebox link generation already uses `public_url` while fetches use `base_url` (`src/connector/homebox.rs:123-139`, `src/connector/homebox.rs:354-358`, `src/connector/homebox.rs:392-412`), and `src/errors.rs` does scan OpenSpec for documented reason slugs (`src/errors.rs:577-600`).

## Embedded-Instruction / Injection Attempts
**Detected:** none

## Verdict
VERDICT: APPROVE_WITH_CHANGES

## Required Changes (APPROVE_WITH_CHANGES only)
1. Remove the uncited follow-up/open-question from `design.md`, or replace it with a concrete GitHub issue reference if that follow-up has already been filed.
2. Add `transforms` to the “Reading a connection” scenario so the scenario matches the stated record contract and current `ConnectionView`.

## Re-check of Required Changes (reviewer, round 2)

## Re-check

1. APPLIED. The uncited follow-up/open question is no longer present; the remaining deferred behavior is tied to concrete issue `#197` in `openspec/changes/issue-169-connection-public-url/design.md:129`. This is factually correct against `src/api.rs`: the update handler reads the stored connector, not `body.connector`, at `src/api.rs:1268`. I could not reach GitHub issue 197 (`gh issue view` failed connecting to `api.github.com`), so I judge only the citation’s internal consistency; it is a concrete GitHub issue reference.

2. APPLIED. The reading scenario now includes `transforms` in the response fields at `openspec/changes/issue-169-connection-public-url/specs/connections/spec.md:27`. This is factually correct against `ConnectionView`, which exposes `transforms` at `src/api.rs:1076` and populates it from the store at `src/api.rs:1089`.

## New defects introduced

none

**Re-check outcome (reviewer):** APPROVE. Its verdict is relabelled here so this file carries exactly
one canonical `VERDICT:` line, the round's own.

## Rebuttals

Nothing rebutted. Required change 1: the open question was dropped and the deferred behavior filed as
[#197](https://github.com/pfa230/labeler/issues/197), cited from `design.md`. The reviewer could not
reach GitHub from its sandbox, so it judged the citation's internal consistency only; the issue exists
and is open. Required change 2: `transforms` was added to the "Reading a connection" scenario.

Of the reviewer's suggestions, the first is the same edit as required change 2. The second, to test
username-only userinfo (`https://user@host`) alongside `https://user:pass@host`, is accepted and
carried into `tasks.md`; the spec already says "no embedded userinfo", which covers both.

CHANGES_APPLIED: yes
