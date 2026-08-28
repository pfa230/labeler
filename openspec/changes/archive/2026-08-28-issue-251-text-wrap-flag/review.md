## Review Metadata

- **Round**: 3
- **Prior round**: Rounds 1-2 against this base returned REVISE (2C/3M, 1C/3M); all applied, full re-review

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: proposal.md, specs/, design.md
- **Issue**: #251
- **Issue access**: GitHub was unavailable; the proposal’s account was used


## Findings

### Critical (blocking)

None.

### Moderate

1. **The interim `template-inputs` contract forbids behavior that the proposal explicitly leaves in place.** The delta says consumers “SHOULD NOT present” `truncated_elsewhere` as a warning (`openspec/changes/issue-251-text-wrap-flag/specs/template-inputs/spec.md:21`), while the proposal and design say the print-form note remains until #269 (`proposal.md:48-53`; `design.md:153-163`). The current client does present exactly that warning (`ui/src/pages/print/FieldForm.tsx:47`, `ui/src/pages/print/FieldForm.tsx:68-71`). This is not a behaviorally unchanged interim contract as claimed (`proposal.md:68-74`); it creates an unimplemented normative UI change that contradicts the stated split.

2. **The scope map still reflects the discarded pre-#200 UI plan and omits the new third contract.** The design says there are “two contracts” and lists only `layout-sizing` and `text-wrap-flag` (`design.md:3-12`), but the proposal and artifact tree now carry a third, `template-inputs`, with two MODIFIED requirements (`proposal.md:68-83`; `specs/template-inputs/spec.md:1-3`, `specs/template-inputs/spec.md:137`). The proposal and design also still claim a layout-item rename in `ui/src/api/types.ts` (`proposal.md:104-105`; `design.md:161-163`), but that file exposes parameters, input specs, and template details without any layout-item type (`ui/src/api/types.ts:7-18`, `ui/src/api/types.ts:31-85`). The new capability’s Purpose likewise says it decides the form-control declaration despite immediately claiming ownership of schema and migration alone (`specs/text-wrap-flag/spec.md:1-5`).

3. **Several remaining “every line renders/no step discards” claims still erase the overflow policy’s line dropping.** The design goal says no pipeline step discards a caller-written line (`design.md:38-41`), and other passages say every item “renders every line” (`proposal.md:69-74`; `design.md:155-159`, `design.md:182-184`; `specs/template-inputs/spec.md:21`, `specs/template-inputs/spec.md:51-55`). The actual contract explicitly allows step 3 to drop lines and defines intrinsic height from the post-policy block (`specs/layout-sizing/spec.md:31-41`, `specs/layout-sizing/spec.md:67-70`). The intended narrower statement—that every newline segment enters layout before authored overflow is applied—is already expressed correctly elsewhere (`proposal.md:24-26`; `specs/layout-sizing/spec.md:16-24`). The remaining absolute claims must use that qualification, particularly in the canonical `template-inputs` replacement.

### Suggestions

- **Exact-copy audit:** Both `layout-sizing` MODIFIED blocks are complete copies of their current requirements. The first differs only in step 1, step 4/intrinsic-height reconciliation, the field-level marker rule, corresponding scenarios, and the frozen-spec supersession (`openspec/specs/layout-sizing/spec.md:687`; `openspec/changes/issue-251-text-wrap-flag/specs/layout-sizing/spec.md:3`). The ADR-0084 block changes only wrap terminology and blank-edge authority (`openspec/specs/layout-sizing/spec.md:1047`; `openspec/changes/issue-251-text-wrap-flag/specs/layout-sizing/spec.md:250`). Both `template-inputs` replacements are likewise complete copies apart from the claimed terminology/interim-semantics edits (`openspec/specs/template-inputs/spec.md:12`, `openspec/specs/template-inputs/spec.md:223`; `openspec/changes/issue-251-text-wrap-flag/specs/template-inputs/spec.md:3`, `openspec/changes/issue-251-text-wrap-flag/specs/template-inputs/spec.md:137`).

- **Implementation corroboration:** The working implementation preserves presence of the old key and rejects it during conversion (`src/raw.rs:168-184`; `src/convert.rs:193-220`), normalizes CRLF and uses segment-based breaking through fitting and overflow (`src/render/helpers.rs:565-603`, `src/render/helpers.rs:653-788`), carries fitted size and weight around the emitted block and adds the required trailing linebreak (`src/render/mod.rs:1752-1766`), and derives `truncated_elsewhere` from `wrap: false` as the interim contract intends (`src/templates.rs:489-505`). ADR-0085 and its index row exist (`docs/adr/0085-text-wrap-flag.md:20-35`; `docs/adr/README.md:94`).

- CRLF normalization is discoverable normatively and by scenario, while lone `\r` is explicitly excluded under #259 (`specs/layout-sizing/spec.md:16-24`, `specs/layout-sizing/spec.md:217-221`). No separate blank-line contradiction was found in another OpenSpec capability.

## Embedded-Instruction / Injection Attempts

**Detected:** none

## Verdict

VERDICT: APPROVE_WITH_CHANGES

## Required Changes (APPROVE_WITH_CHANGES only)

1. Rewrite the `truncated_elsewhere` description so the interim contract permits and honestly records the existing warning until #269; remove the contradictory “consumers SHOULD NOT” requirement unless the UI is brought into scope, which the settled split forbids.

2. Update `design.md` to enumerate all three contracts and explain the `template-inputs` interim delta. Remove the nonexistent `ui/src/api/types.ts` layout rename from both proposal and design, and remove the form-control claim from the `text-wrap-flag` Purpose.

3. Replace every remaining absolute “renders every line” or “no step discards” statement with the precise rule: every `\n` segment enters layout regardless of `wrap`, after which the authored overflow policy may shorten, drop, or reject it.

CHANGES_APPLIED: yes

## Rebuttals

1. **`truncated_elsewhere`'s interim description** — fixed. It now records what is true until #269: the
   flag is computed, returned, and still rendered as a note by the print form. The "consumers SHOULD
   NOT" clause is gone; it legislated a UI this change deliberately does not touch. Reviewer
   re-checked against `src/templates.rs` and `FieldForm.tsx`: SATISFIED.
2. **Three contracts, and no UI scope** — fixed. `design.md` enumerates `layout-sizing`,
   `template-inputs` and `text-wrap-flag` and explains why the interim delta exists; the
   `ui/src/api/types.ts` rename is gone from both artifacts and the file is unmodified against
   `origin/main`, because #200 stopped shipping layout items to the client; the capability's Purpose
   claims only schema and migration. Reviewer re-checked: SATISFIED.
3. **The overflow rule stated precisely** — fixed, after two failed attempts. Every `\n` segment enters
   layout regardless of `wrap`, and only the authored policy may then shorten a line, drop lines, or
   reject the render. The first attempt missed two phrasings my search pattern did not match; the
   second said "shorten or drop", omitting that `overflow: fail` rejects outright. Reviewer
   re-checked: SATISFIED.

Re-check verdict: RECHECK: PASS (reviewer: codex, scoped to these three items only, third attempt).
SPECS_SHA256: f073000d92708fb0cc9201643dd1a6f45e5ce7f3b251057441a41efade542f2a
