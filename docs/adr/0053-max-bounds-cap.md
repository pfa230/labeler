# 53. `max_w`/`max_h` cap an `auto` size, not substitute for its fallback

Date: 2026-08-12

## Status

Accepted. Issues [#152](https://github.com/pfa230/labeler/issues/152) and
[#150](https://github.com/pfa230/labeler/issues/150). Interacts with the dynamic-width deferral model
of [ADR-0051](0051-edge-relative-and-corner-placement.md) §7 and decision 12 (below).

## Context

`max_w`/`max_h` are documented in SPEC §4 as the upper bound that resolves an `auto` size. Three
passes read them for the `auto` case and disagreed:

| Pass | `max_w` | `max_h` |
| --- | --- | --- |
| `templates.rs` compile-time validation | substitutes (`max.or(fallback)`) | substitutes |
| `render/mod.rs` measure pre-pass (text and container) | ignored | ignored |
| `render/mod.rs` render, auto-length text | inherits the measure (uncapped) | caps |
| `render/mod.rs` render, dynamic-width auto container | ignored | caps |
| `render/mod.rs` render, fixed path | caps | caps |

Two filed issues are symptoms of the same defect. **#152**: on a dynamic-width `single`, validation
resolved an auto-width container to `max_w`, but `render_container_item` ignored `max_w` and used
`frame_width_units - left` instead, so the render-time inner box could be *wider* than the load-time
one. The load-time line-endpoint check ADR-0051 decision 12 added compares a child endpoint against
the load-time width, so a template that would have rendered correctly was rejected at startup, and one
invalid template aborts startup. **#150**: `measure_box_height` resolved an `Extent::Size` height as
`size.0[1].value().unwrap_or(frame_height - at_y)`, which is `None` for `auto` and so never consulted
`max_h`, while `render_text_item` resolved the same slot honoring it. Whenever `max_h` was below the
frame remainder, the measure pass picked a font size for a taller box than the text was rendered into,
and a `font_size: {min, max}` range could overflow.

Both copies of `resolve_size_value` (`templates.rs` and `render/mod.rs`) trace to the same line: the
`auto` arm resolved as `max.or(fallback)`, discarding the fallback entirely whenever a bound was
present. That is a *substitute*, not a cap, and it is the root of both issues, not two independent
bugs in two files.

## Decision

**A `max_*` bound caps the resolution of an `auto` size on its axis, in validation, measurement, and
rendering, on every format. It binds `auto` only: a numeric `size` component is never clamped by it,
in any layer, and this branch did not change that** (`SizeValue::Value` returns the authored number
untouched, unconditionally, in both `resolve_size_value` copies).

**1. The root fix: `min`, not `or`.** Both copies of `resolve_size_value`'s `auto` arm change from
`max.or(fallback)` to:

```rust
let resolved = match (max, fallback) {
    (Some(max), Some(fallback)) => max.min(fallback),
    (Some(max), None) => max,
    (None, Some(fallback)) => fallback,
    (None, None) => return Err(/* auto with no max_{label}, unchanged message */),
};
```

The error case, the `<= 0` rejection, and every message string are unchanged. Every other change in
this branch (measure honoring `max_h`, the container/text/qr/image measure arms honoring `max_w`, the
dynamic-container render branch honoring `max_w`) is downstream of getting this one line right in both
places it is duplicated.

**2. This is a loosening, and it runs opposite to the usual direction of a bounds fix.** Where a
`max_*` exceeds the space actually available, the size now resolves to the available space rather than
to the oversized bound, so it fits instead of overflowing into an `item must fit within layout bounds`
rejection — a cap larger than the room available is simply not binding. Concretely: a container at
`at.x: 90` with `max_w: 30` on a `width.max: 100` label previously resolved to 30 at validation, so
`90 + 30 > 100` was rejected, while the capped renderer would have given `min(100 - 90, 30) = 10` and
fit comfortably. **Templates that were previously rejected with `item must fit within layout bounds`
for this reason now validate and render at the available size.** This is worth stating plainly, since
most bounds work tightens what a template author can get away with; this one loosens it, as a
side effect of making the bound behave like a cap instead of a substitute.

**3. `auto` means two different things, and a cap must respect both.** On a fixed-width label, `auto`
means "fill the parent frame". On a dynamic-width label, `auto` means "shrink to content" — that is
the mechanism auto-length templates are built on (SPEC §3.1). A cap therefore has to *limit content*
on a dynamic label, not convert it to "fill" — a capped *empty* container that filled its cap would
reserve the cap's width and print blank tape, which is precisely the defect this branch removes for
`qr`. The two halves of the dynamic-container arm resolve differently, though: measure resolves
`min(content + padding, cap)` (content-capped, so an empty container measures to essentially nothing),
while render resolves `min(frame_width - left, cap)` (fill-capped, off the label's final width, not
the container's own content). The two coincide exactly when the container is the widest element on
the label, i.e. when it is what determined `frame_width` in the first place; when it is not, an empty
capped container can still *render* at a nonzero width up to its cap even though it *measured* to
nothing (`max_w_caps_a_dynamic_container_at_render` and `no_max_w_means_no_cap_anywhere` both exercise
an empty container and pin nonzero render widths, 5mm and 90mm respectively — the fill, not the
content). Only a container whose content actually exceeds the cap changes measurement; render is
capped regardless of content. This was the one genuine contradiction surfaced during implementation:
an early formula treated a capped auto container
as filling to its cap regardless of content, which agreed with the fixed-width reading of `auto` but
broke the "shrink to content" contract every other dynamic-width item honors. The ruling: content-capped
wins, because the container-measure formula was already content-driven before this branch touched it,
and `qr`/`image` (decision 4) is the outlier, not the precedent.

**4. `qr` and `image` are the deliberate exception, and it is principled, not an oversight.** Neither
item type has a narrower natural footprint to shrink to, so neither can be content-sized the way a
container or a text run can. Concretely their `auto` requires `max_w` and resolves to exactly it:
both validation and render pass `allow_auto_fill: false`, leaving a `None` fallback, so
`max.or(fallback)` was already `max` on the render side and an `auto` width without `max_w` is a hard
error rather than a fill. Rendering therefore already honored `max_w` for these two types before this
branch. Capping the *measurement* arm to match is what stops a capped code from sizing
the whole label to `width.max` and leaving the remainder blank: before this fix, a `qr` with
`size: [auto, 20], max_w: 30` on a `width: {min: 10, max: 100}` label measured to the full 100mm
budget (the measure arm ignored `max_w` and contributed the whole remainder) while rendering only a
30mm code, wasting 70mm of tape per label. Since an `auto` width on a `qr`/`image` is only reachable
*with* a `max_w` (the same `allow_auto_fill: false` makes `auto` with no `max_w` an error at both
validation and render), this arm's fallback is, in practice, always capped after this change.

**5. `render_container_item`'s dynamic branch keeps an explicit `min` rather than routing through the
capped `resolve_size_value` helper.** That helper rejects a resolved value of `<= 0`, and this branch
must tolerate a remainder of exactly zero: a container at `at.x: 90` whose label measures to 90 has no
room left, and that is a legitimate outcome of measurement, not an authoring error — the same
distinction ADR-0051 decision 13 already draws for a zero `to`-extent at render (rejected only when
negative, not when zero). Routing through the helper would reintroduce the `<= 0` rejection for a
case that must render an empty box instead. The cost is that cap semantics now live in two places for
this one axis; the render-time branch is tested directly rather than relying on the helper's tests to
cover it.

**6. Two alternatives were considered and rejected** (design spec §2):
   - **Align validation down instead of the renderer up.** Accept that on an auto-length label the
     width is content-driven and `width.max` is the only real bound, stop validation pretending
     otherwise, and document `max_w` as inert on a dynamic-width container. This has zero rendering
     impact and is the smaller change, but it leaves "cap this container" inexpressible on a
     dynamic-width label and keeps the same field meaning two different things depending on
     `format.width`. Rejected: `max_w` already caps on fixed-width labels, so making it cap
     everywhere is the reading of SPEC §4 that makes the field consistently useful.
   - **Narrow the ADR-0051 line check to the page frame** so it stops firing at container inner
     frames. Rejected as papering over the actual disagreement: it leaves the renderer and the check
     out of sync and sets the next check added at a container frame up for the same trap #152 hit.

**7. #152's resolution.** This does **not** make #152's own repro template render. Once the renderer
honors `max_w: 30`, the child line reaching `x: 50` genuinely does not fit inside a 30mm-capped
container, so the load-time rejection stops being false and becomes correct. #152 closes as "the check
was right, the renderer was lying" — not as "the check was too strict."

## Consequences

- Compile-time validation now resolves an auto width to `min(max_w, remaining-from-at.x at
  width.max)`, and render-time resolves a dynamic auto container's width to `min(max_w,
  remaining-from-at.x at the final width)`. Since the final width is at most `width.max`, the rendered
  width is at most the validated one — validation stays an upper-bound check, rendering the exact one,
  matching the model ADR-0051 §7 already established (validation bounds against `width.max`, render
  checks the actual width).
- **The loosening enlarges the validate-but-fail-at-render set.** A template like:

  ```yaml
  format: { type: single, width: { min: 10, max: 100 }, height: 12 }
  layout:
    - type: container
      at: [90.0, 0.0]
      size: [auto, 12.0]
      max_w: 30.0
      items:
        - type: line
          at: [0.0, 6.0]
          to: [-0.0, 6.0]
          thickness: 0.2
  ```

  now validates (the container resolves to `min(30, 100 - 90) = 10` at load, bounded by `width.max`,
  and the child divider fits against that bound), but at render the label may measure to exactly 90
  (the line contributes only its inset, `-0.0` giving 0), leaving the container a zero-width remainder
  in which the divider is degenerate. `check_line` rejects it with the standard explained render error.
  Before this branch the same template was rejected at *load* instead, because the uncapped validator
  resolved the container to 30 and `90 + 30 > 100`. This is not a new defect class — it is the
  dynamic-width deferral model working as designed, admitting more templates to the render-time check
  that ADR-0051 §7 already exists to enforce, as the price of no longer rejecting the many templates
  that would have rendered fine.
- A dynamic-label container with `max_w` now renders capped rather than filling the remaining width;
  an auto-width `qr`/`image` with `max_w` stops padding the label with blank tape; a `max_h` below the
  frame remainder now yields a smaller measured (and matching rendered) font, verified by rendering and
  comparing rather than by a passing test suite alone, since a font-size change is not otherwise
  distinguishable from a regression by a green run.
- `brother_24mm_weights.yaml` (`at.x: 1.5`, `max_w: 117`, `width.max: 120`) goes from an effective
  118.5mm to a 117mm text budget. It renders identically in the test suite (short placeholder data),
  but a title wider than 117mm would now shrink or ellipsize sooner than before.
- Two defects were found but deliberately not fixed here, filed as follow-ups per CLAUDE.md rather
  than folded in (a third, #155, is its own bullet below): [#153](https://github.com/pfa230/labeler/issues/153), the `.min(outer_budget)` clamp
  on the dynamic container's measured contribution is unreachable for any template that passes
  `validate()` (compile-time validation already narrows the same bound before recursing into
  children), dead defensive code with no reachable test case; and
  [#154](https://github.com/pfa230/labeler/issues/154), a pre-existing (not introduced by this branch)
  missing `.max(0.0)` on `inner_width`/`inner_height` in `templates.rs`'s non-rotated container
  validation, which can surface a misleading `max_{w,h} must be greater than 0` error blaming a bound
  neither the container nor its child ever set, when the real cause is the container's own padding
  exceeding its resolved size.
- **This branch's own width-axis loosening (decision 1) has a height-axis counterpart for `text` that
  it does not close, filed as [#155](https://github.com/pfa230/labeler/issues/155).** `text` is the one
  item type where `allow_auto_fill` differs between validation (`templates.rs:371-378`, keyed off the
  template's `is_dynamic_width`) and `render_text_item`'s non-auto-length path (`render/mod.rs:1128-1134`,
  always `false`), so a fixed-width `text` item on a dynamic-width single validates an `auto` height
  against the frame-height fallback (`min(max_h, frame_height)`) but renders it against no fallback at
  all (`max_h` alone, uncapped). A `max_h: 200.0` on a 40mm-tall label now validates and then fails
  every render with `an item resolves outside the frame`, where pre-branch the same template was
  rejected at load. `qr`/`image` and `container` are unaffected: both layers agree on the flag for
  those types. Not fixed here: the fix is a single shared height-fallback model for `text` across three
  currently-disagreeing fallbacks (validation's frame height, the auto-length path's
  `frame_height - at.y`, and the fixed path's `None`), which is a design decision, not a fix-wave edit.
- Templates that set neither `max_w` nor `max_h` are untouched: every template in `catalog/` and
  `tests/fixtures/templates/` that sets neither bound renders to byte-identical Typst source before and
  after this branch.
