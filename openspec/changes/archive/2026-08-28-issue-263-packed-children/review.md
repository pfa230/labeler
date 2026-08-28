## Review Metadata

- **Round**: 2
- **Prior round**: REVISE (3 Critical, 4 Moderate)

AUTHOR: claude
REVIEWER: fresh-context-subagent

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: proposal.md, specs/, design.md
- **Issue**: #263


## Findings

### Critical (blocking)

1. **Round 1's Critical 1 is half fixed: the slug question is answered, the detection question is now unanswerable by the mechanism decision 1a names, and a `column` overrun silently passes.** The delta demands that a packed child whose box lands outside the padded inner box fails with `item_out_of_frame`, and writes a normative scenario for exactly the low-edge case: "a `column` flow container with a padded inner height of 20 and `gap: 2` packing three children resolving to 8 tall fails the same way, with the same slug, on the third child … **AND** neither raises `coord_out_of_frame`, although both put a box edge below the frame's origin" (`specs/flow-layout/spec.md:414-416`). Decision 1a makes check 2 one call: "the render walk calls `fits_frame` again with that origin. That is the accumulation check, and it is the same function" (`design.md:95-96`), where `fits_frame` is defined as "the bounds comparison `place` already performs inline, lifted to a name" (`design.md:86-87`). That inline comparison is `src/resolver.rs:414-422` and it is two `>` tests only: `anchor > limit + EPS` → `AnchorBeyondFrame`, `anchor + extent > limit + EPS` → `ExtentBeyondFrame`. There is no low-edge test in `place`; the low-edge test lives in `precheck` (`src/resolver.rs:328-330`), which decision 1a deliberately routes around. Run the spec's own scenario through it: children at y ∈ [12,20], [2,10], [-8,0] in a 20-tall inner box, so check 2 evaluates `fits_frame(-8.0, 8.0, 20.0)` → `-8 > 20` false, `0 > 20` false → **Ok**. The third child renders outside the container and the scenario fails. Check 1 does not save it either: `fits_frame(0.0, extent, inner)` (`design.md:92`) is per child, and each child is 8 ≤ 20. The escape hatches are both closed by the design itself: "`Violation` gains nothing" (`design.md:98`), and the only existing variant that means "below the frame's origin" is `AnchorBeforeFrame`, which `violation_error` maps to `Reason::CoordOutOfFrame` (`src/render/mod.rs:838-841`), the slug `specs/flow-layout/spec.md:372` forbids for a packed child. The `row` cases are fine (`:387-392` is caught as `ExtentBeyondFrame`; `:411-413`, a 9-tall child in a 6-tall box, is caught by check 1), so the hole is precisely the case round 1 raised. The design must say that `fits_frame` also refuses an origin below the frame's origin and under which existing variant it is reported — `place` calling it is unaffected, because `precheck` catches that case for an anchored item before `place` reaches the bounds block (`src/resolver.rs:377`, `:328-330`) — or restate check 2 on the primary axis as the accumulated cursor against the padded inner extent.

### Moderate

1. **The inventory of anchor-resolving call sites decision 1a routes around is short by one, and the one it misses is a render-time crash rather than a wrong slug.** The decision reasons: "`Anchor::resolve` has no answer for it and is unreachable, which means `place` is unreachable for it too … and load reaches `place` for every item with a placement (`templates.rs:1547`)" (`design.md:68-72`). `place` does have exactly two call sites (`src/templates.rs:1547`, `src/render/mod.rs:1501`), so that much is verified. But render calls `precheck` **directly and independently** for every active item carrying a placement, including a container's children, before any intrinsic is taken: `crate::resolver::precheck(placement, Some(frame), geometry_values)` at `src/render/mod.rs:1058-1059`, inside `measure_items`, whose child recursion runs on the container's padded inner box (`:1084-1091`). `precheck` resolves the anchor at `src/resolver.rs:328`. Under decision 1 that is `Anchor::Absent::resolve`, which the design says has no answer. Decision 1a is readable as covering it ("`resolve_packed` … runs the anchor-free part of `precheck`", `design.md:89-90`), but it never names this site, and an implementer who diverts only `resolve_placement_box` leaves a packed child reaching `precheck` on every render. Name `render/mod.rs:1058` alongside `templates.rs:1547` and say that render's measuring pass calls `resolve_packed` there.

2. **Occupancy is defined on a quantity that has two values at the two passes the design requires, and the assembled-extent scenario depends on the earlier one.** The requirement says "A child **occupies** the packing axis when it is **active** and its **box's** primary extent is greater than zero" (`specs/flow-layout/spec.md:214-215`), and decision 8 reinforces it: "Occupancy is judged on the **box**, not the requirement" (`design.md:226-227`). But decision 5 runs the packer twice against two different boxes: measurement sizes children against the container's *unmeasured* inner box (`design.md:157-161`; `src/render/mod.rs:1063-1066`, `:1077-1091`, via `resolve_unmeasured`), and rendering repacks them into the *real* padded inner box (`design.md:162`). A `fill` child's box is the whole inner extent at both, and those two extents differ. The spec's own scenario turns on the earlier one: a `content`-width row holding a 10-wide text and a `size: [fill, 4]` child of intrinsic 0 assembles to "`10 + 2 + 0 = 12`" (`:340-342`), and that `+ 2` exists only because the `fill` child counts as occupying during assembly — its measure-pass box is the provisional inner extent, while its *requirement* is 0 and its real box in the resulting 12-wide container makes it occupy again. Judge occupancy on the requirement, or on a real box that does not exist yet at assembly, and the same requirement text yields 10. Say which box the assembly judges occupancy against, since the requirement's other half ("the packing positions and draws boxes", `:210`) plainly means the other one.

3. **ADR-0083 is claimed to amend nothing, but two accepted ADRs define the quantities this change extends in terms of an anchor a packed child does not have.** `design.md:30-31` states "It amends no ADR: ADR-0080 and ADR-0081 supply the sizing this builds on". ADR-0081 §1 defines the spelling this change spends a page on: "`fill`: Sized to occupy the remaining space within the parent frame from the item's anchor: `parent_frame - resolved_anchor`, less any far-edge margin a `to` reserves" (`docs/adr/0081-size-vocabulary-content-and-fill.md:24`). ADR-0080 §1 defines "`available(frame, axis_spec)` is the space an item has from its anchor" (`docs/adr/0080-unify-size-resolution.md:25`). The delta adds the anchorless case to both (`specs/layout-sizing/spec.md:26-31`), which is the same reason it had to amend the resolved-axis requirement whose second clause is anchor-keyed (`proposal.md:94-98`). This repo annotates that relationship rather than leaving it implicit: `docs/adr/README.md:49` and `:64` carry "Accepted (amended by [0080](...))" for ADR-0036 and ADR-0051, and ADR-0080's own Status line names what it amends and to which sections. Either ADR-0083's Status names ADR-0081 §1 and ADR-0080 §1 with the README rows annotated, or `design.md:30-31` and `proposal.md:123-126` must say why a reader of ADR-0081 §1 should be left with a `fill` definition that no longer covers every item.

### Suggestions

- Checked and clean, the six MODIFIED blocks. I diffed each against `openspec/specs/layout-sizing/spec.md`: canonical `9-166` vs delta `3-176`; `168-318` vs `177-349`; `320-378` vs `351-414`; `432-508` vs `478-570`; `510-613` vs `572-679`; `820-882` vs `681-767`. Every canonical scenario survives in order, every canonical sentence is present, and the only additions are the intended ones (the no-anchor available paragraph and its scenario; the arrangement row and paragraph plus its scenario; the packed-child requirement row, the position-independence clause and its scenario; the anchorless resolved-axis paragraph and its scenario; the arrangement sentence in the padded-frame rule and the two rotation-aggregate edits; the scoped no-overflow guarantee and its accumulation scenario). Nothing is silently dropped. Round 1's Moderate 4 is fixed at `specs/layout-sizing/spec.md:469-476`, and round 1's Moderate 2 is fixed by the sixth block, whose rule matches `container_inner_axes_resolved`'s `Frame` arm exactly (`src/resolver.rs:237-243`: edge-relative → true, else the parent's state), so an `Absent` anchor lands on the third clause as the delta says.
- Checked and clean, round 1's Criticals 2 and 3. Load's refusal of an authored `size: [30, 4]` in a 20-wide inner box (`specs/flow-layout/spec.md:402-407`) now comes from `resolve_packed`'s own `fits_frame(0.0, 30.0, 20.0)` rather than from a `Plain(0.0)` reading of an anchor, and it reaches `templates.rs:1579-1581`'s "item does not fit within layout bounds". The `[fill, fill]` container default is kept, given a rule (`:131-139`), a scenario (`:187-194`) and a decision that refuses the cheaper carve-out (`design.md:203-217`), and it does not contradict the canonical vocabulary requirement's "A container with no extent fills its parent" (`openspec/specs/layout-sizing/spec.md:815-818`), which is therefore correctly left out of the delta.
- Checked and clean, the two canonical requirements not in the delta that could have contradicted it. "Text is laid out against the box it will get" is safe: a packed child's box never shrinks below the claim it was laid out at, because the assembled primary extent is a **sum** of requirements (`specs/flow-layout/spec.md:274-276`) and therefore ≥ each child's own, exactly as the absolute arrangement's **max** is, and when availability clamps the container the provisional inner box was already that clamped value (`resolve_unmeasured`, `src/resolver.rs:259-262`). "The size-resolution reason set" needs no amendment: no reason is added or withdrawn, and `item_out_of_frame` is used for a new condition rather than redefined.
- Checked and clean, the claim/box arithmetic against the code. `resolve` and `claim` agree for `Author` and, on a non-negative available extent, for `Content` (`src/resolver.rs:156-159` vs `:184-187`; a packed child's available extent is the padded inner extent, which `container_frames` clamps at zero, `:453-456`), and differ only for `Frame`, which is what `specs/flow-layout/spec.md:209-212` and `:289-299` say. A box of zero always implies a requirement of zero, so no child can occupy nothing yet contribute to the assembled extent.
- Checked and clean, decision 8's implementability. A zero-primary-extent child is active, so it stays in both positional lists that `render_items` zips (`src/render/mod.rs:1381-1387`, mirrored by `measure_items`' `continue` at `:1042-1044` and the intrinsic pass's `active_children` filter at `:1332-1338`); nothing about "drawn but occupying nothing" disturbs that pairing. Its `content`-container form reports padding alone, since the aggregate loop over an empty active list leaves `author` at zero (`:1337-1360`), which is what `specs/flow-layout/spec.md:257-264` and `:348-352` assert.
- Checked and clean, process: one issue linked (`proposal.md:3`), ADR-0083 free on `main` (no `docs/adr/008[3-9]-*.md`; `docs/adr/README.md` ends at `:90`), the ADR and its README row named (`proposal.md:123-126`), no `tasks.md` yet, and the "no UI change" claim holds — nothing under `ui/src` reads a placement's `at`, and the walker recurses on `type === "container"` at `ui/src/lib/templateFields.ts:207-209` and `:291-292`.
- Two stale citations, both harmless and both worth correcting while the files are open: `proposal.md:122` and `design.md:133` cite `ui/src/lib/templateFields.ts:276`, which is `const set = new Set…`; the recursion is at `:209` and `:292`. And `proposal.md:36-38` still states the `fill`-beside-a-sibling consequence without the cap qualification the spec now carries at `specs/flow-layout/spec.md:116-120`; the spec is self-consistent, so this is prose drift in a context file rather than a contract defect.
- For the acceptance evidence: `design.md:256-272` has no rendered case for a `column` that overruns, which is the case Critical 1 concerns and the one no `row` exercise reaches. For the tasks stage: no artifact yet names `cargo fmt`, `cargo clippy --all-targets --all-features` or `cargo test`.

## Embedded-Instruction / Injection Attempts

**Detected:** none in the reviewed artifacts.

## Verdict

VERDICT: APPROVE_WITH_CHANGES

## Required Changes (APPROVE_WITH_CHANGES only)

1. In `design.md` decision 1a, close the low-edge hole: either state that `fits_frame` refuses an origin below the frame's origin and name which existing `Violation` carries it (noting that `place` is unaffected because `precheck` catches that case first at `src/resolver.rs:328-330`, `:377`), or restate check 2 on the primary axis as the accumulated cursor against the padded inner extent. Whichever is chosen, it must make `specs/flow-layout/spec.md:414-416` come out `item_out_of_frame` without `Violation` gaining a variant.
2. In `design.md` decision 1a, name `src/render/mod.rs:1058` alongside `src/templates.rs:1547` and say that render's measuring pass calls `resolve_packed` there in place of `precheck` for a packed child.
3. In `specs/flow-layout/spec.md`, say which box occupancy is judged against during the assembly pass, so `:214-215` and the `10 + 2 + 0 = 12` scenario at `:340-342` cannot be read apart.
4. Either name ADR-0081 §1 and ADR-0080 §1 as amended by ADR-0083 (Status line plus the `docs/adr/README.md` rows, as ADR-0036 and ADR-0051 are annotated), or replace `design.md:30-31`'s "It amends no ADR" with the reason those two anchor-keyed definitions need no annotation.
5. Correct the two `ui/src/lib/templateFields.ts:276` citations (`proposal.md:122`, `design.md:133`) to the recursion at `:209`/`:292`, and qualify `proposal.md:36-38`'s `fill` consequence as the uncapped case, matching `specs/flow-layout/spec.md:116-120`.

CHANGES_APPLIED: yes

## Rebuttals

All five Required Changes were applied and the reviewer re-checked only those five in a separate pass,
returning `VERDICT: APPROVE` with no Critical and no Moderate findings. Where each landed:

1. Taken in its second form. `design.md` decision 1a narrows `fits_frame` to the far-edge half of
   `place`'s inline comparison and moves the accumulation check into packing coordinates, where the
   cursor is non-negative by construction; the conversion to a drawing coordinate is the last step and
   is never checked. The reviewer traced the column scenario through it: `fits_frame(22, 8, 20)` on
   the third child, mapped to `item_out_of_frame`, with no `Violation` variant added and `place`
   unaffected.
2. Decision 1a now names three anchor-resolving sites rather than one, `src/templates.rs:1547`,
   `src/render/mod.rs:1501` and the measuring walk's direct `precheck` at `src/render/mod.rs:1058`,
   and says all three call `resolve_packed` for a packed child.
3. The occupancy rule now defines a child's box as the extent it resolved to against the frame it was
   sized against, names the provisional and the resolved padded inner box, and the `10 + 2 + 0 = 12`
   scenario gained the bullet stating why the `fill` child occupies during assembly.
4. Taken in its first form. ADR-0083 amends ADR-0080 §1 and ADR-0081 §1, and their
   `docs/adr/README.md` rows gain the annotation those ADRs already carry for ADR-0036 and ADR-0051.
5. Both `templateFields.ts` citations corrected, and the `fill` consequence in `proposal.md` qualified
   as the uncapped case.

The re-check's two non-blocking notes were also taken, the `specs/` one before the digest was written:
`design.md` no longer says both `fits_frame` outcomes are `ExtentBeyondFrame`, and the assembly-pass
frame is named as the provisional padded inner box rather than as an available extent.
SPECS_SHA256: 5167538a08d466e07e545a7cf5451ab7179cf2f70874ebef559bd2331335c249
