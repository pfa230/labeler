# Design

## Context

Rendering runs in two passes today. `measure_items` measures every item against its unmeasured box and keeps each `text` item's fit in a `Measured` tree. `render_items` then walks the layout again and, for each child, builds its data scope and path, resolves its final box (`resolve_placement_box`, or the flow arrangement's rect from `arrange_flow_children`), and calls `render_single_item`. That function mixes preparation with drawing: it resolves the font weight a second time, checks the box against the sheet stock (`check_stock`), resolves and validates line endpoints (`resolve_point`, `check_line`), resolves a QR payload and builds its SVG, resolves an image source, decodes or reads it and registers it with the `ImageCollector`, and, for a container, derives the child frame and slot map and recurses into `render_items`. Each step writes its markup as soon as its own checks pass.

Because the walk is depth-first and interleaved, the first error a failing render reports depends on that order. For example, an undecodable image in `layout[0]` is reported before an out-of-frame box in `layout[1]`.

## Goals / Non-Goals

**Goals:**

- One function family prepares an item: final box, stock and endpoint checks, text fit and weight, QR SVG, image bytes, flow arrangement with its trim marker, and nested children. It has no side effects outside its return value.
- One function family emits an item: it formats lengths and writes Typst markup from the prepared tree. It does not read template data and does not resolve, place, arrange, fit or check anything.

**Non-Goals:**

- Changing `measure_items`, `Measured` or `intrinsic`. Measurement against the unmeasured box and preparation against the final box are different questions; the container intrinsic's arrangement against the provisional child frame is not a duplicate of preparation's arrangement against the final inner frame.
- Refitting text at the final box, allocating fill, or making a prepared failure retryable. Those are #260's rules.
- New helpers for the data-scope and path construction repeated in measurement.

## Decisions

### A prepared tree, emitted afterwards

`render_items` keeps its signature and becomes `prepare_items` followed by emission of the result. A prepared node holds the `&LayoutItem` it came from, its `PlacedBox`, and what the item kind needs to be drawn:

- `text`: the resolved weight and the `TextFit`, cloned from `Measured`. Emission still derives the leading from that fit, as `render_text_item` does today.
- `qr`: the SVG markup, or nothing for an empty payload.
- `image`: the decoded bytes and format, or nothing for an empty source.
- `line`: its two resolved, checked endpoints.
- `container`: its prepared children and the canvas and inner frames from one `container_frames` call.

A flow container's prepared children are the drawn prefix followed by the trim marker when one was selected, so emission draws children in order with no marker branch.

Keeping the `render_items` signature leaves both production callers and every test call site untouched (14 in `src/render/mod.rs`, one of them the shared `render_test_items` helper, and one in `src/templates.rs`), which is how this change shows it preserved behavior. Changing callers to two calls would touch every one of those tests for no behavioral reason.

The alternative of writing markup into a scratch buffer and discarding it on failure keeps the tangle: a caller still cannot ask for a child's box or fit without running its drawing code, and an image would already be registered with the collector.

### Every user-facing failure moves to preparation, in today's order

Preparation performs each fallible step the renderer performs today, in the same depth-first order: a flow container arranges its children before preparing any of them; a non-flow container places and prepares each child before the next; a `text` resolves its weight and then checks the stock; a `qr` resolves its payload, checks the stock and then builds its SVG; an `image` resolves its source and then decodes or reads it. Emission keeps only `format_length`, `derived_leading_pt` and `writeln!` failures, all of which are internal errors no template or request can cause.

Leaving image decoding and QR building in emission would be a smaller move, but it changes which error a render reports when an earlier sibling's content fails and a later sibling's box fails, which the issue rules out.

### Side effects stay in emission

Preparation loads image bytes but emission registers them with `ImageCollector`, so a prepared node that is never emitted leaves no file in the compiled document. Preparation builds container child contexts (data scope and slot map) itself; emission needs only the context's unit and image collector.

Emission stays on `RenderContext` instead of a new emitter type. The boundary is the prepared node each emission function receives; a separate type would add a component without fixing a failure.

### No new test

The existing renderer, fixture and catalog tests pin the emitted Typst, the rendered output and the reported errors, including error order (`one_code_two_reasons_for_unrelated_failures`, `an_image_render_failure_names_the_instance_path`, the flow overflow and trim-marker tests). Moving responsibilities must leave all of them passing unedited. A test that preparation has no side effects would guard only a risk no run has shown on main; #260's marker-selection tests will exercise preparing without drawing.

## Risks / Trade-offs

- [A fallible step lands in the wrong phase or order] → The tasks review each moved call against `render_single_item` and `render_items` on main, and existing error tests must pass unedited.
- [Cloning each `TextFit` into the prepared tree] → One small vector of lines per text item, far below the Typst compile it feeds; borrowing from `Measured` cannot reach a trim marker's measurement, which preparation owns.
