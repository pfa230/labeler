# 54. An `auto` size falls back to the space remaining from its anchor, not the whole frame

Date: 2026-08-12

## Status

Accepted. Issue [#155](https://github.com/pfa230/labeler/issues/155). Corrects one clause of
[ADR-0053](0053-max-bounds-cap.md) (§ Supersession, below) and reuses its `min(max_*, fallback)` cap
model unchanged.

## Context

`resolve_size_value`'s `auto` arm resolves to `min(max_*, fallback)` (ADR-0053). The **fallback**,
until this branch, was the whole frame dimension on that axis, regardless of where in the frame the
item sits. That is wrong: an item anchored at `at` has only `frame - at` of room on each axis; the rest
of the frame is space its own position has already claimed.

`text` had a second, independent defect layered on top: `templates.rs` validated a `text` item's size
with `allow_auto_fill: is_dynamic_width` — a fallback present only on dynamic-width templates — while
`render_text_item`'s fixed (non-auto-length) path always passed `allow_auto_fill: false`, no fallback at
all. Those are two different questions answered by one flag: validation asked "is this *template*
dynamic-width", render asked "does *this item's* width depend on the frame". A `text` with `max_h`
above a dynamic-width label's frame height validated (`min(max_h, frame) = frame`, and `frame` fits) and
then failed every render with `an item resolves outside the frame` (`min(max_h, None) = max_h`, which
doesn't). That is #155.

`container` did not have the second defect — both layers already agreed `allow_auto_fill: true` for it
— but shared the first: an auto-sized container at a nonzero anchor resolved against the full frame at
both layers and could be rejected by `validate_bounds` (`at + frame > frame`) for a shape that would
have fit against the space actually available.

## Decision

**An `auto` size falls back to the space remaining from the item's own anchor — `frame -
resolved_at` on each axis — at both the validation layer (`templates.rs::resolve_size`) and every
render path (`RenderContext::resolve_size`), on every format, for `text` and `container`.** `qr` and
`image` keep no fallback, per ADR-0053 decision 4: neither has a natural content footprint to shrink
to, so their `auto` requires an explicit `max_w` and resolves to exactly it.

**1. Both halves were required; either alone leaves the bug.**

- The fallback value (`frame - at` instead of `frame`) is what makes the *number* correct.
- Making `text`'s `render_text_item` fixed-path `allow_auto_fill` unconditionally `true` — matching
  validation, which was already `true` on dynamic-width templates and becomes `true` everywhere — is
  what makes the two *layers agree*.

Doing only the second with the old full-frame fallback produces agreement on a wrong number: both
layers say `frame`, and at a nonzero anchor that still overflows `validate_bounds` or the render-time
frame check. Doing only the first leaves the fixed render path still passing `allow_auto_fill: false`
and ignoring the fallback entirely, resolving straight to `max_h` with nothing to cap it against.

**2. The structural property, scoped to the types that have a fallback.** With the fallback at
`frame - resolved_at`, an `auto` axis satisfies `at + resolved <= frame` **by construction**: `resolved
<= max` when a cap is present, and `resolved <= fallback = frame - at` otherwise, so in either branch
`at + resolved <= frame`. `validate_bounds` therefore cannot fail on an `auto` axis of a `text` or
`container` — the two item types with a fallback. This is stated with that scope on purpose: it does
**not** hold for `qr`/`image`. They have no fallback, so an `auto` width there resolves to `max_w` alone
with nothing tying it to `at`, and `validate_bounds` can still — correctly — reject one that doesn't
fit. Excluding them is the price of not re-opening the `qr`/`image` contract ADR-0053 decision 4 just
settled, not an oversight.

**3. `allow_auto_fill` changes what it encodes.** Before this branch it meant, inconsistently, "is this
*template* dynamic" (validation, for `text`) or "is *this item's* width frame-dependent" (render, for
`text`) — two different questions sharing one flag, which is the direct cause of #155. It now encodes
one thing only, uniformly at both layers: **does this item type have a frame to fall back on**. `text`
and `container` do, unconditionally; `qr` and `image` don't, unconditionally. The flag survives (a
fully-uniform version that removes it, giving `qr`/`image` a fallback too, was considered — § Rejected
alternatives).

**4. `resolve_size` resolves both axes; several callers want only one, and that used to matter.** Before
this branch a caller that needed a single axis but happened to invoke `resolve_size` still resolved the
other one, silently, against whatever fallback the shared code path produced. With a constant `None`
fallback that was harmless — the unused axis either had an explicit value or errored identically either
way. Once the fallback derives from the anchor, it stops being harmless: an axis nobody asked for can
independently decide whether the template renders. Two concrete instances existed, both fixed as
preparation before the fallback itself changed:

- `render_container_item`'s R0 branch computed its height as `self.resolve_size(..).1`, which resolves
  width first. For a zero-remainder dynamic auto-width container, a position-aware width fallback of `0`
  would trip `resolve_size_value`'s `resolved <= 0.0` rejection through the height call — breaking the
  ADR-0053 contract, pinned by `a_zero_remainder_container_renders_an_empty_box`, that a zero remainder
  renders an empty box rather than erroring. The fix resolves the height alone, through
  `resolve_size_value` directly, with its own `frame_height_units - resolve_coord(at.y, ...)` fallback
  (`src/render/mod.rs`, `render_container_item`'s R0 branch); the width on that branch was already
  computed separately by its own `min`/`max` clamp and never went through the helper.
- `measure`'s fixed-width `text` branch (the `else` arm reached when `width_is_frame_dependent()` is
  `false`) called `self.resolve_size(..)` for the width alone but resolved the height too, so a
  `size: [w, auto]` text with no `max_h` errored in the measure pre-pass — before the item was even
  rendered — despite the pre-pass never touching its height. The fix resolves `size.0[0]` alone via
  `resolve_size_value` (`src/render/mod.rs`, `measure`'s `Text` case).

A third site, `measure_container_footprint`'s `Extent::Size` width arm, already resolved width only,
but with a `None` fallback — the fourth place in the codebase resolving an `auto` size, and the one an
earlier draft of this fix missed. It now takes the same anchor-derived fallback as the other three, so a
right-anchored auto-width container nested in a fixed-width parent (which escapes the
edge-relative-width-plus-dynamic-frame validation check, because its own parent isn't frame-dependent)
resolves the same way in the measure pre-pass as it does at render (`frame_width - left`), instead of
validating and then erroring with `size width is auto but no max_width provided`.

Every other single-axis (`.0`-only) caller is `Extent::To`, where `resolve_size` early-returns from
corner arithmetic before any fallback is built, so it cannot exhibit this failure mode and was left
alone.

**5. Six behavior changes, all loosenings, and the origin is not exempt.**

1. `size: [w, auto]` at a nonzero `at.y` with no `max_h` now works on a dynamic-width label, resolving
   to `frame_height - at.y` instead of being rejected by `validate_bounds`.
2. #155's own shape works: `max_h: 200` at `at.y: 0` in a 40mm frame resolves to `min(200, 40) = 40` at
   both layers and renders.
3. A container with an `auto` axis at a nonzero anchor gains the same, on every format — it had the
   identical latent defect; nothing had reached it until now.
4. `text` gains the fallback on fixed labels and sheets, on both axes (item 3, above): an `auto` height
   with no `max_h` resolves to `frame_height - at.y` instead of erroring, an oversized `max_h` resolves
   to the room left instead of a false `validate_bounds` rejection, and an `auto` **width** with no
   `max_w` resolves to `frame_width - at.x` instead of erroring — a genuinely new capability, since an
   auto-width `text` on a fixed single or a sheet slot previously had to name a `max_w`.
5. An `auto` width on a fixed-width label narrows by `at.x` for containers, the only type with
   `allow_auto_fill: true` there before this branch. A container at `at.x: 10` with an `auto` width
   previously resolved to the full frame and was rejected by `validate_bounds`; it now fills the
   remainder.
6. A fixed-width `text`'s `auto` height (no `max_h`) now measures on a dynamic-width label instead of
   erroring in the pre-pass, per decision 4's second instance.

The tempting shortcut — "an item at the origin is unaffected, since `frame - 0 == frame`" — is **false**.
The subtraction is a no-op at the origin, but items 2, 4, and 6 above are not about the subtraction:
they are about an axis *gaining a fallback it did not have before*, which fires at the origin exactly as
anywhere else. #155's own repro sits at `at: [0.0, 0.0]`. Only item 5 — the container width narrowing on
fixed labels — is genuinely inert at the origin, because that is the one change that was already
present, just not anchor-aware.

**6. `auto_resolve_bounds` and `extent_auto_bounds` are deleted.** `auto_resolve_bounds` narrowed
`layout_bounds.width` by `at.x`, on dynamic-width labels only — exactly what the new fallback now does
on both axes, at both layers, on every format: a strict superset. `extent_auto_bounds` existed to stop
that narrowing being applied a second time for an `Extent::To`, guarding the double-subtraction bug
ADR-0053 fixed (`resolve_to_extent` already subtracts `at` from `to` itself). With the narrowing moved
inside `resolve_size`'s `auto` fallback, and `resolve_size`'s `Extent::To` arm early-returning into
`resolve_to_extent` *before* that fallback is ever constructed, a `to`-extent item can no longer reach
the fallback code at all — there is nothing left for the deleted helper to guard against.
`a_to_extent_is_not_narrowed_twice_by_its_anchor` (`src/templates.rs`) tests this directly: a container
at `at.x: 20` with `to: [-0.0, h]` on a dynamic-width label resolves against `frame - 20`, not
`frame - 40`, both before and after the deletion.

## Supersession of ADR-0053

ADR-0053's implementation carried a test, `a_cap_smaller_than_the_padding_clamps_the_inner_box`,
asserting that a `max_w` cap smaller than a container's own padding — leaving no inner box at all —
made a nested auto-width child fail, and that the only thing the padding clamp changed was **which**
error fired: without the `.max(0.0)` clamp the child's edge-relative `at.x` resolved against a negative
frame and failed in coordinate resolution; with it, the frame was zero and the child failed the size
check instead. Its own comment recorded that "the child errors either way" and that there was "no
'renders successfully' green to reach."

That was true only as an artifact of `render_container_item`'s R0 branch resolving its **height**
through `self.resolve_size(..).1` — which resolves **width** first — for the parent container. A
zero-width fallback for the parent tripped `resolved <= 0.0` through that width-before-height coupling,
before the nested child was ever reached. Decision 4's first instance in this ADR removes exactly that
coupling: the R0 branch now resolves height alone, through `resolve_size_value`, never touching width as
a side channel. With the coupling gone, the nested child's own height resolves independently against
its own `Value(1.0)`, no error fires, and its auto width is computed by the *same* explicit
`min`/`max(0.0)` clamp `render_container_item`'s dynamic-width branch already uses for the top-level
zero-remainder case — the render succeeds, producing an empty box for the child.

That is not a new behavior this branch introduces; it is the zero-remainder contract ADR-0053 itself
states elsewhere (`a_zero_remainder_container_renders_an_empty_box`) applying to a case that used to be
unreachable because of the coupling, not because the zero-remainder rule stopped at one level of
nesting. The test was rewritten (same fixture: `max_w: 2.0`, `padding: 3.0` on every side, an
edge-relative nested child sized `[auto, 1.0]`) to assert the corrected behavior — the container still
renders at its `2mm` cap and no negative dimension reaches the emitted Typst source — rather than which
error message fired, since there is no longer an error to pin. ADR-0053's record that "the child errors
either way" is superseded: it described an artifact of control flow, not an intended or durable
behavior.

## Rejected alternatives (design spec §2.2, §2.3)

- **Make validation mirror render's strictness instead of giving render a fallback.** Key validation off
  the item's own frame-dependence (what render already used) rather than the template's, so
  `size: [20, auto]` with no `max_h` is rejected at load instead of 422-ing at render. This closes #155
  too, is smaller, and fails fast. Rejected: it leaves the misleading `at.y`-blind rejection in place (an
  auto height at `at.y: 10` with no `max_h` would still resolve against the full frame and fail
  `validate_bounds` for a shape that fits), and it leaves `auto` height on the fixed `text` path meaning
  "you must also supply `max_h`" for no reason once the fallback itself is correct. It treats the
  symptom — two layers disagreeing — rather than the cause: a fallback describing space that does not
  exist.
- **Apply the fallback to `qr` and `image` too, and delete `allow_auto_fill` entirely.** The fully
  uniform version gives every box item a `frame - at` fallback and removes the flag. Rejected: `qr` and
  `image` already pass `allow_auto_fill: false` at both layers and are self-consistent — they are not
  part of this bug — and ADR-0053 decision 4, on the branch immediately before this one, already settled
  that their `auto` deliberately requires `max_w` and resolves to exactly it, on the principled ground
  that neither has a natural content footprint to shrink to. Reopening that contract to make a flag
  disappear is churn, not correctness. `allow_auto_fill` survives, now with a position-aware fallback
  behind it for the two types that have one.

## Consequences

- `templates.rs::resolve_size` and `RenderContext::resolve_size` (`src/render/mod.rs`) are now
  byte-identical in structure (a free function vs. a method, `String` vs. `AppError` errors aside): both
  build `fallback` as `(frame_dim - resolve_coord(at, frame_dim)).max(0.0)` per axis when
  `allow_auto_fill`, `None` otherwise, per CLAUDE.md's note that the two resolvers must stay in sync.
- `templates.rs`'s `Text` validation arm passes `allow_auto_fill: true` unconditionally (was
  `is_dynamic_width`); `render_text_item`'s fixed-path `resolve_size` call passes `true` (was `false`).
  `Qr`, `Image`, and the container paths are unchanged: `qr`/`image` keep `false` everywhere;
  `container` was already `true` everywhere.
- `render_container_item`'s R0 branch and `measure`'s fixed-width `text` branch resolve one axis via
  `resolve_size_value` instead of both via `resolve_size`; `measure_container_footprint`'s width arm
  keeps resolving one axis but gains the anchor-derived fallback it was missing.
- `auto_resolve_bounds` and `extent_auto_bounds` (`templates.rs`) are deleted, along with the
  `auto_bounds.as_ref().or(layout_bounds)` argument threading at their four call sites (`Text`, `Qr`,
  `Image`, `Container` in `validate_layout_item`).
- Every template in `catalog/` and `tests/fixtures/templates/` renders to byte-identical output before
  and after this branch — verified by diffing all 13 shipped templates against an `origin/main`
  worktree, both `/render/label` PNGs and `/batch` PDFs — because none of them has an `auto` item at a
  nonzero anchor with no binding `max_*`; the ones that set `max_w`/`max_h` are already capped to the
  same number either way (e.g. `brother_24mm_weights`, where `max_w: 117` binds regardless of whether
  the fallback is `120` or `120 - 1.5`).
- Closes the last open loosening ADR-0053 named but deferred: its "Consequences" section filed the
  `text` height-axis fallback disagreement as #155 rather than fixing it in that branch, on the grounds
  that a shared fallback model across three then-disagreeing height resolutions was a design decision,
  not a fix-wave edit. This branch is that design decision.
- Two follow-ups ADR-0053 filed remain open and are unaffected by this branch: #153 (an unreachable
  `.min(outer_budget)` clamp on a dynamic container's measured contribution) and #154 (a pre-existing
  missing `.max(0.0)` on `inner_width`/`inner_height` in `templates.rs`'s non-rotated container
  validation).
