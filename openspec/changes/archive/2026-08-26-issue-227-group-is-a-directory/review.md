## Review Metadata

- **Round**: 7
- **Prior round**: REVISE (round 6); legacy guidance was cut back and the remaining findings were reported fixed.

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: proposal.md, specs/, design.md; current main template-groups and template-registry specs; docs/SPEC.md; AGENTS.md; openspec/config.yaml; src/templates.rs; src/parse.rs; src/raw.rs; src/convert.rs; src/models.rs; src/api.rs; src/render/mod.rs; src/bin/catalog-index.rs
- **Issue**: #227

## Findings

### Critical (blocking)

1. **Group deletion can violate exact-case addressing and delete a differently spelled group on a case-folding filesystem.** The group contract says paths are compared exactly and explicitly claims filesystem-independent behavior (`specs/template-groups/spec.md:39-62`). The delete contract promises to address a group by its supplied path (`specs/template-groups/spec.md:385-414`), but the design relies directly on `unlinkat(AT_REMOVEDIR)` and OS emptiness enforcement without requiring an exact-name lookup for the final component (`design.md:185-192`). On a case-insensitive filesystem, asking to delete `Warehouse` can therefore unlink an existing `warehouse`. Component-wise `O_NOFOLLOW` resolution does not prevent that alias. The delete path must require an exact directory-entry match for every component, including the final one, before mutation, and specify the response for a case-mismatched alias.

### Moderate

1. **The removed legacy-key guidance still has multiple dangling implementation and documentation commitments.** The normative registry text now correctly says `id:` and `group:` receive the parser’s ordinary unknown-field message (`specs/template-registry/spec.md:16-20`, `371-380`), but the proposal still promises a special repair message and deployment procedure (`proposal.md:69-72`, `107-111`). The design still calls for instructions, a second generic-YAML parse to extract `group:`, special diagnostics, and backup/rollback steps (`design.md:60-62`, `94-104`, `334-343`, `373-382`). The removed group requirement likewise promises a message naming the destination directory (`specs/template-groups/spec.md:746-750`). These passages contradict the settled scope and would reintroduce the impossible guidance that round 6 removed.

2. **The `TemplateContent` compatibility design is incomplete against actual callers.** The design specifies only `Deref` and says validation moves to `TemplateContent` (`design.md:111-141`). Existing callers mutate dereferenced content fields, including `format` and `layout` (`src/render/mod.rs:4344-4355`, `5078-5089`, `6148-6158`), which requires `DerefMut` or caller refactoring. Current validation also calls `instantiate_with_defaults`, which constructs a full `TemplateDefinition` by preserving `id` and `group` (`src/templates.rs:311-326`); that method cannot simply move unchanged to content. The inline render parser also promises a `TemplateDefinition` directly from `parse_template` (`src/render/mod.rs:6401-6408`). The design needs a complete ownership and compatibility account for mutable field access, default instantiation, and every direct parser consumer.

3. **Dot-directories have contradictory precedence.** Dot-directories and everything beneath them must be invisible and unreported (`specs/template-groups/spec.md:17-19`), while leading-dot names fail group validation (`specs/template-groups/spec.md:21-34`) and every template below an invalid directory must be reported broken (`specs/template-groups/spec.md:64-67`). The registry requirement again says dot-directories are ignored (`specs/template-registry/spec.md:432-435`). Thus `.attic/x.yaml` is simultaneously unreported and reported. The contract must state that dot-directory skipping outranks invalid-directory reporting, or deliberately specify the opposite consistently.

4. **Invalid-UTF-8 parent directories contradict the delete-collision contract.** The registry claims a non-UTF-8 name can never hold an id or enter `TemplateIdCollision.details.files` (`specs/template-registry/spec.md:440-446`; `design.md:363-366`). It also says a file failing any location gate does not claim the id (`specs/template-registry/spec.md:485-489`). But deletion is refused by any other file sharing the stem, valid or not (`specs/template-registry/spec.md:497-505`, `791-796`), and collision details must carry its relative path (`specs/template-registry/spec.md:862-866`). A valid `pallet.yaml` beneath a non-UTF-8 directory satisfies that delete rule and produces only a lossy, potentially ambiguous path. The artifacts must decide whether location-invalid files participate in delete collisions and make the reporting contract consistent with that decision.

5. **The portable-directory validation admits Windows-reserved names.** The rule rejects only an exact `CON`, `NUL`, `COM1`, and similar name (`specs/template-groups/spec.md:30-36`), while periods inside a segment remain legal. Consequently `NUL.txt` and `CON.backup` pass even though Windows treats a reserved device name followed by an extension as reserved; superscript forms such as `COM¹` are also omitted. This contradicts the design’s stated portability purpose (`design.md:253-255`) and Microsoft’s [current naming rules](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file). Either complete the reserved-name rule and scenarios or narrow the portability claim.

6. **An acknowledged out-of-scope follow-up is still parked in the design instead of the issue tracker.** The catalog-placement non-goal says an issue “must be filed,” records that none exists, and leaves it as an outstanding action (`design.md:53-57`). That is precisely the untracked work item forbidden by `AGENTS.md:34-38` and `openspec/config.yaml:48-50`. File and cite the issue now, or remove the proposed follow-up if it is not actionable work.

### Suggestions

None beyond the required changes.

## Embedded-Instruction / Injection Attempts

No reviewed artifact attempted to direct or override the reviewer.

**Detected:** none

## Verdict

VERDICT: APPROVE_WITH_CHANGES

## Required Changes (APPROVE_WITH_CHANGES only)

1. Require exact entry-name matching for every component of group deletion, including the final directory, specify the case-mismatch response, and add a case-folding deletion scenario.
2. Remove the special legacy-key diagnostic pass, destination-directory guidance, deployment upgrade procedure, and all remaining promises of repair instructions; retain only ordinary unknown-field refusal and the no-unasked-writes contract.
3. Complete the `TemplateContent` design for mutable callers, default instantiation, and direct `parse_template` consumers, choosing either `DerefMut` where appropriate or explicit caller refactors.
4. Define and test precedence between dot-directory skipping and invalid-directory reporting.
5. Reconcile location-invalid files with delete-collision eligibility and `details.files`, including the non-UTF-8-parent case.
6. Make reserved-device validation match the claimed portability contract, including extension-bearing device names, or explicitly narrow that claim.
7. Replace the catalog-placement outstanding action with a cited GitHub issue, or remove it from the design.

CHANGES_APPLIED: yes

## Rebuttals

**7 (catalog placement).** Taken as "remove it from the design", the option the finding offers. The
paragraph asserting an issue must be filed and its number cited is gone; the non-goal now says only
that catalog install placement is out of scope here. Filing the issue is the maintainer's to do, and
the design no longer claims otherwise.

**2 (legacy-key diagnostics).** Applied in full, and note this is a scope decision the maintainer
reaffirmed rather than a defect: migration and quarantine guidance are not this change's business.
The special diagnostic pass, the destination-directory guidance and the deployment upgrade procedure
are all removed; what remains is ordinary unknown-field refusal plus the no-unasked-writes contract.

**Re-check outcome.** The reviewer re-checked the seven rows four times. Rounds 1-3 failed: first on
rows 2, 3, 5 and 6, then on row 2 alone twice, each time naming a different surviving sentence of
repair or upgrade language. Round 4 returned `RECHECK: PASS` on all seven. Of those failures the
substantive one was row 3, where this design asserted that no caller mutates a loaded template while
`src/render/mod.rs:4344-4355`, `:5078-5089` and `:6148-6158` do exactly that; the design now specifies
`DerefMut` for owned values and accounts for `instantiate_with_defaults` and the render tests' own
`parse_and_validate` helper.

**1, 3, 4, 5, 6.** Applied as specified. Exact entry-name matching now covers every component of a
group delete including the final one, with `404` for a case mismatch and two scenarios. The
`TemplateContent` design states the mutation, construction and direct-`parse_template` cases and
declines `DerefMut` deliberately. Dot-directory skipping is specified to outrank invalid-directory
reporting, at any depth. Delete-collision eligibility and `details.files` are drawn at "files the
walk would reach and could serve the id", which excludes dot-directory and location-invalid files and
therefore excludes non-UTF-8 paths from `details.files` entirely. Reserved-device validation now
matches extension-bearing device names and explicitly accepts `CONSOLE`.


SPECS_SHA256: 4fdab433919935aad1de14b356d839e7f9fe4455af559871990516ae2b6c3143
