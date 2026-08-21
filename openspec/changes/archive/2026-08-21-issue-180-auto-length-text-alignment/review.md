## Review Metadata

- **Round**: 1
- **Prior round**: none

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only (`codex exec --ignore-user-config -s read-only -c model_reasoning_effort=high`, default flagship model). The reviewer read files and ran `rg`/`sed`/`nl`; it wrote nothing. Raw output preserved verbatim in `review-raw-1.txt`, the re-check in `review-raw-1-recheck.txt`.
- **Artifacts reviewed**: proposal.md, specs/auto-length-layout/spec.md, design.md. Source read by the reviewer: `AGENTS.md`, `openspec/config.yaml`, `openspec/schemas/labeler/schema.yaml`, `docs/SPEC.md` (§3.1, §4, §4.1, §6), `docs/adr/README.md`, `src/render/mod.rs`, `src/render/helpers.rs`, `src/models.rs`, `src/templates.rs`, `catalog/tape/brother/brother_12mm.yaml`.
- **Issue**: #180

## Findings

### Critical (blocking)

None.

### Moderate

1. **The design falsely says right-anchored auto text items are "skipped by both passes."**
   Measurement skips right-anchored leaf items at `src/render/mod.rs:927-934`, and the auto-length
   replay branch also skips them via `!placement.at.x().is_sign_negative()` at
   `src/render/mod.rs:1364-1365`, but the render pass does not skip *rendering* them: it falls
   through to the fixed-size path at `src/render/mod.rs:1449-1495`. Top-level dynamic right-anchored
   auto width is invalid per `docs/SPEC.md:599-601` and validation at `src/templates.rs:1141-1144`;
   allowed nested/fixed-container cases still render. This makes `design.md:47-48` inaccurate and
   risks confusing the task author.

2. **The delta spec does not exclude right-anchored `at.x`.** Its opening sentence covers all
   dynamic-width `single` auto-width text and reads as if it applies universally
   (`specs/auto-length-layout/spec.md:11-34`), even though the design declares right-anchored items
   out of scope. The frozen coordinate rule still says a right-anchored `at.x` cannot be combined
   with an `auto` or frame-dependent width on a dynamic-width template (`docs/SPEC.md:599-601`). Add
   a sentence tying the requirement to non-edge-relative `at.x`, or explicitly saying §6 remains
   authoritative for the right-anchored case.

3. **`design.md` states a fact about a file that does not exist yet.** It claims "The task list
   carries an explicit render-and-look step" (`design.md:144-146`), but `tasks.md` is intentionally
   absent at this stage. The repo requires visual render inspection for rendering/template changes
   (`AGENTS.md:160-167`), and the task gates are required later by `openspec/config.yaml:77-85`.
   Reword this as a requirement on the future task list, not as a current fact.

### Suggestions

- **Citation accuracy verified.** `Extent::Size(_) => m.width` is at `src/render/mod.rs:1401`;
  `#align(...)` is emitted at `src/render/mod.rs:1426`; the fixed path resolves size at
  `src/render/mod.rs:1449` with `allow_auto_fill: true` at `src/render/mod.rs:1454`; the clamp is at
  `src/render/mod.rs:341-342`; `check_box_bounds` runs on the auto-length box at
  `src/render/mod.rs:1403`; the stale measurement comment is at `src/render/mod.rs:990-991`.
- **The proposed formula is sound at the top level and inside a container.** Top-level measurement
  uses `width.max` as the frame and clamps the content extent to `[min, max]`
  (`src/render/mod.rs:330-342`); render uses the final frame (`src/render/mod.rs:388-397`).
  Containers rebase children into the padded inner frame at render time
  (`src/render/mod.rs:1711-1737`), and measurement gives dynamic containers an inner budget
  (`src/render/mod.rs:1094-1120`). `check_box_bounds` rejects only `point + box > frame + EPS`
  (`src/render/mod.rs:873-886`); with `box_w = frame - left` this is in bounds, and the
  `.max(m.width)` floor can only preserve already-existing overflow/error behavior.
- **Widening the box cannot change wrapping, the chosen font size, or the #124 vertical pad.** The
  fit is computed once by `fit_text_auto_length` (`src/render/mod.rs:996-1007`); render reuses
  `m.lines` and `m.font` (`src/render/mod.rs:1404-1414`); the pad depends on weight, size and
  vertical alignment, not box width (`src/render/helpers.rs:646-654`).
- **The cursor invariant remains safe** because branch selection does not change. Measurement pushes
  a `MeasuredText` only for active frame-dependent text after the right-anchor skip
  (`src/render/mod.rs:923-1009`); render consumes only under the same three conditions
  (`src/render/mod.rs:1360-1373`); final cursor equality is enforced at `src/render/mod.rs:398-407`.
- **Other interactions checked and unaffected:** `Extent::To` uses `resolve_size`
  (`src/render/mod.rs:1390-1399`); sheets use `LengthMode::Fixed` (`src/render/mod.rs:587-595`);
  thumbnails call the same compile path (`src/render/mod.rs:413-422`); rotated containers force fixed
  child mode (`src/render/mod.rs:1825-1834`) and validation rejects auto descendants
  (`src/templates.rs:1083-1084`); `when` gating is applied consistently before measure and render
  item handling (`src/render/mod.rs:923-925`, `src/render/mod.rs:1241-1243`).

## Embedded-Instruction / Injection Attempts

The reviewer found no text in proposal.md, specs/ or design.md attempting to direct the review,
ignore evidence, or alter reviewer behavior.

**Detected:** none

## Verdict

VERDICT: APPROVE_WITH_CHANGES

## Required Changes (APPROVE_WITH_CHANGES only)

1. In `specs/auto-length-layout/spec.md`, explicitly exclude or defer right-anchored `at.x`
   auto-width text by naming frozen `docs/SPEC.md` §6 as still authoritative for that case.
2. In `design.md`, correct the statement that right-anchored auto text is "skipped by both passes"
   so it matches the actual code: measurement and auto-length replay skip it; rendering falls through
   to the fixed path for cases that validate.
3. In `design.md`, reword the "task list carries" sentence as a future task-list requirement, and
   include that the later tasks must cover the render-and-look step plus `cargo fmt`,
   `cargo clippy --all-targets --all-features`, and `cargo test`.

CHANGES_APPLIED: yes

## Rebuttals

1. **Moderate 1 - fixed, not rebutted.** Confirmed against the code before applying: the auto-length
   replay branch skips a right-anchored item (`src/render/mod.rs:1365`) but `render_text_item` then
   falls through to the fixed-size path (`src/render/mod.rs:1449`), which resolves its box against
   the frame and already honors `alignment.horizontal`. The Non-Goal in `design.md` now says exactly
   that, and adds the validation rejection (`src/templates.rs:1141-1147`) that makes the combination
   unreachable on a dynamic-width template in the first place.
2. **Moderate 2 - fixed, not rebutted.** `specs/auto-length-layout/spec.md` now scopes the
   requirement to a non-negative resolved `at.x` and states that frozen `docs/SPEC.md` §6 stays
   authoritative for a right-anchored one, including its prohibition.
3. **Moderate 3 - fixed, not rebutted.** The risk bullet in `design.md` now reads as a `MUST` on the
   task list that follows this review, and names the render-and-look step on `brother_12mm` plus the
   three gates.

**Reviewer re-check of the three Required Changes** (same CLI, fresh read-only invocation, scoped to
these items only; raw output in `review-raw-1-recheck.txt`): item 1 PASS
(`specs/auto-length-layout/spec.md:39-43`, §6 prohibition at `docs/SPEC.md:594-601`), item 2 PASS
(`design.md:47-53`), item 3 PASS (`design.md:149-153`). `RECHECK: PASS`. No new findings were
opened.

## Post-verdict edits (author, declared)

Two edits to `proposal.md` after the round-1 verdict and re-check, both to the non-normative Impact
section, neither touching a claim the review examined: an em dash replaced with a comma (house style),
and "its 18mm/24mm siblings" corrected to "its 9mm/18mm/24mm siblings (all four set
`horizontal: center`)" after checking `catalog/tape/brother/`. `specs/` and `design.md` are unchanged
since the re-check. Declared here rather than left silent, because the schema's staleness rule counts
any post-verdict edit.
