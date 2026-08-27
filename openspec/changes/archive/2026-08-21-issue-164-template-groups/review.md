## Review Metadata

- **Round**: 2
- **Prior round**: 1 returned REVISE (one Critical: the post-change top-level field table omitted the
  legacy `options:` key; four Moderate; one Suggestion).

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only (`codex exec --ignore-user-config -s read-only -c model_reasoning_effort=high`)
- **Artifacts reviewed**: proposal.md, specs/template-groups/spec.md, design.md, plus
  `openspec/specs/template-registry/spec.md`, frozen `docs/SPEC.md` §2 / §2.0 / §3, `AGENTS.md`,
  `src/templates.rs`, `src/api.rs`, `src/raw.rs`, `src/convert.rs`, `src/models.rs`, `src/reason.rs`,
  `src/parse.rs`, `ui/src/pages/Templates.tsx`, `ui/src/pages/Catalog.tsx`, `ui/src/api/types.ts`,
  `ui/src/api/queries.ts`, `docs/adr/0006-template-edit-ownership.md`
- **Issue**: #164

## Findings

### Critical (blocking)

none

### Moderate

**Move spec overpromises patchability for valid YAML templates without a line-oriented root mapping**
Evidence: `specs/template-groups/spec.md:166-169` requires inserting a `group:` line when none exists
while preserving every other byte. `design.md:110-125` only defines finding column-0 block-style key
lines and inserting after `name:`, `id:`, or the document header. Current parsing does not constrain
authors to block-style root mappings: `docs/SPEC.md:286-304` says templates are YAML files with
top-level fields, and `src/parse.rs:25-31` deserializes YAML into `TemplateDefinitionRaw` without
recording YAML style.
Why wrong: a valid YAML template encoded as a top-level flow mapping with no `group` has no line
after `name:`/`id:` to patch and no safe single-line insertion point that preserves every other byte.
The implementation would either return `422` despite the spec saying it inserts when none exists, or
risk producing invalid/corrupted YAML.
Fix: explicitly refuse root documents that are not a block mapping with a safe top-level insertion
point, and add a scenario such as "top-level flow mapping without `group` is `422` and unchanged."

**OpenAPI contract is incomplete for the new API surface**
Evidence: the spec requires OpenAPI to describe `group` only on schemas at
`specs/template-groups/spec.md:99`, but the same spec adds `GET /api/templates?group=` at
`spec.md:118-123` and `PUT /api/templates/{id}/group` at `spec.md:163-197`. The proposal's impact
already expects OpenAPI work for "the new parameter, request body, and route" at
`proposal.md:52-54`.
Why wrong: the normative spec makes the new query parameter and move endpoint part of the API, but
does not require `GET /api/openapi.json` to expose them. That leaves generated clients and Swagger
users unable to discover the main new write path even if implementation behavior is correct.
Fix: add OpenAPI SHALLs to the filter and move requirements: document the `group` query parameter,
the `PUT /templates/{id}/group` route, its request body, responses, and any new error reasons.

**Design goal contradicts the specified invalid-group behavior**
Evidence: `design.md:28` says "Nothing about grouping can take the service down or make a template
unrenderable." The spec says an invalid `group` makes the template fail to load and be quarantined at
`spec.md:25-28`. The existing registry spec says refused files are excluded from the served set at
`openspec/specs/template-registry/spec.md:8-14`.
Why wrong: a hand-authored invalid `group:` absolutely can make that template unrenderable by
excluding it from the registry. The intended invariant appears to be narrower: invalid group content
must not abort startup, and the move endpoint must not write invalid content.
Fix: rewrite the goal/risk wording to match the contract: grouping faults quarantine only the bad
file, and group moves validate before writing.

### Suggestions

**Define group sort order mechanically**
Evidence: group names are case-sensitive at `spec.md:30`, while the Labels view lists groups in
"ascending order" at `spec.md:271`.
Why it matters: `Warehouse`, `warehouse`, accented names, and punctuation can sort differently
depending on locale/API choices.
Fix: specify the collation used for group chips, for example Unicode code point order or the same
comparator the implementation will test.

**Add patcher tests for comment-like characters inside quoted values**
Evidence: the move patch must preserve comments and other bytes at `spec.md:166-169`, and the design
says it re-attaches trailing comments at `design.md:121-122`.
Why it matters: `group: "A # B" # real comment` requires distinguishing `#` inside a quoted scalar
from a real YAML comment.
Fix: include explicit tests/scenarios for quoted group values containing `#` and `:` plus a trailing
comment.

## Embedded-Instruction / Injection Attempts

**Detected:** none

## Verdict

VERDICT: APPROVE_WITH_CHANGES

## Required Changes (APPROVE_WITH_CHANGES only)

1. Add an explicit unpatchable-file rule and scenario for valid YAML templates whose root mapping is
   not safely line-insertable, especially top-level flow mappings without `group`.
2. Extend the OpenAPI requirements to cover the `group` list query parameter and the full
   `PUT /api/templates/{id}/group` route/request/response contract.
3. Narrow the design goal that says grouping cannot make a template unrenderable so it matches the
   spec's quarantine behavior for invalid hand-authored `group:` values.

CHANGES_APPLIED: yes

## Re-check log (round 2)

Codex re-checks the listed items only; each pass is recorded below.

- **Pass 1**: Required 1, 2, 3 and the quoted-comment Suggestion confirmed applied and correct.
  Verdict REVISE over two defects the author's own edits introduced:
  a stale "step 7" cross-reference in the Risks section where decision 4 numbers it step 8, and an
  ordering conflict where the spec required Unicode code-point order while the design specified
  JavaScript `<`, which orders by UTF-16 code unit and disagrees for names outside the Basic
  Multilingual Plane. Both are real; the second is the sharper one, since an emoji in a group name
  would have sorted differently in the UI than on the server.
- **Pass 2**: both fixes confirmed. It re-walked every numbered
  cross-reference in decision 4, and confirmed that comparing `Array.from(name)` code-point sequences
  is equivalent to the server's `str::cmp` at `src/templates.rs:204` for every group name the spec
  allows, since Rust's UTF-8 byte order is code-point order. No new defects. VERDICT: APPROVE.

Two passes, within the five-pass cap for a codex reviewer. The round-2 verdict of
APPROVE_WITH_CHANGES therefore stands with its required changes applied and re-checked.

## Post-implementation diff review (round 3)

The plan review above gated implementation. Implementation was handed to agy, and its diff was then
reviewed by codex in read-only mode. Verdict: REVISE, four Major findings, no Critical. Each was
reproduced
against a running server before being accepted, and each is fixed in the same commit:

1. **`{}` cleared a template's group.** `PUT /api/templates/{id}/group` with an empty object returned
   `200` and removed the `group:` line, because a plain `Option<String>` reads a missing field as
   `None` whether or not it carries `#[serde(default)]`. The body field is now a nested option, so
   presence of the key is distinguishable from its null, and `{}` is a `400`. The spec gained the
   rule and a scenario; `src/lib.rs` gained a test that asserts the file is untouched.
2. **A present but valueless `group:` loaded as ungrouped.** `group:`, `group: ~`, `group: null`,
   `group: 42` and `group: true` all loaded, the first three as ungrouped and the rest coerced to
   strings, against a spec that requires a non-string to fail. `raw.rs` now deserializes through a
   presence-preserving helper and `convert.rs` rejects a present non-string naming `group`.
3. **UI sentinels collided with legal group names.** The filter state was a bare string with `all`
   and `ungrouped` as sentinels, so a group actually named `ungrouped` filtered the ungrouped set and
   one named `all` could not be filtered. It is a tagged union now, with a regression test that was
   confirmed to fail against the old logic before the fix landed.
4. **The reason-registry guard accepted non-contract evidence.** The rewritten
   `spec_documents_every_reason_and_invents_none` scanned every markdown file under `openspec/`,
   archived change artifacts included, so the two new slugs counted as documented on the strength of
   design prose. It now reads `docs/SPEC.md` §10.1 plus `openspec/specs/**/spec.md` only, and the
   slugs are named in the published spec, which is where a client would look. The phantom half runs
   off the §10.1 table alone, exactly as before.

Three smaller items, found by the author rather than the reviewer: three explanatory comments that
the change had no reason to touch had been deleted (the `#128` note on why a card is not one anchor,
the ADR-0047 note on the permanent catalog link, and the note on how Favorites and Recents resolve),
and are restored; and the empty state said "No templates match your search" when only a group filter
was active.

Not changed, and deliberately: a preserved trailing comment is re-emitted with two spaces before the
`#`, so that one line's original spacing is normalized. The spec promises the value on that line
changes and the comment survives, both of which hold.

## Rebuttals

**Round 2, Required 1 (flow-mapping root) — fixed.** The move requirement's unpatchable list gained a
third bullet refusing a root that is not a block mapping written one key per line, with a
`Scenario: A flow-mapping root is refused`. `design.md` decision 4 step 2 now refuses it in the same
breath as a multi-document file, and says why: reflowing a flow mapping into block style is the
whole-file rewrite this design exists to avoid.

**Round 2, Required 2 (OpenAPI) — fixed.** The filter requirement now requires
`GET /api/openapi.json` to document the `group` query parameter including the empty-value case, and
the move requirement requires it to document the route, its path parameter, its request body, every
status in its table, and the error reasons it introduces.

**Round 2, Required 3 (contradictory goal) — fixed.** The goal no longer claims grouping cannot make
a template unrenderable. It now states the narrower invariant that holds: a grouping fault
quarantines the one file that carries it, never aborts startup or touches another template, and the
move endpoint cannot create such a file because it validates before writing.

**Round 2, both Suggestions — taken, though the author may decline suggestions.** Group ordering is
now ascending Unicode code-point order in both documents, matching the byte ordering `summaries()`
already applies to ids (`src/templates.rs:204`), and the quoted-value-with-comment case is now a
scenario in the spec and a named test obligation in design decision 4 step 6. Both were re-checked
along with the three required changes rather than applied unreviewed.

**Round 1, finding 5 (Favorites/Recents) — partly rebutted, behavior changed anyway.** The round-1
review said the interaction was unspecified; it was, at the last paragraph of the Labels-view
requirement, which said the rows are not filtered by the group selection. The underlying UX point was
right, so rather than defend the wording the behavior changed: a group filter now hides Favorites and
Recents exactly as an active search already does, which is also more consistent with
`ui/src/pages/Templates.tsx:137`.

**Round 1, Suggestion (deferred follow-ups) — fixed.** The proposal and design no longer say
out-of-scope work is "filed separately if wanted" or call it "follow-on issues". Both now state that
none of it is queued and that each gets its own issue if and when it is wanted, which is what
`AGENTS.md` requires: an issue, not a parked TODO.
