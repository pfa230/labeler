## Context

See proposal.md for motivation. `RenderContext::expand_items` already associates each expanded repeat instance with its authored index, element index and scoped data. `resolver::arrange_flow` returns the drawn prefix in `FlowResult.rects`; measurement and rendering each call that arrangement. The existing `repeating_container_overflow_trim_draws_first_two_instances` test exercises the silent cut.

## Goals / Non-Goals

Use the existing repeated subtree and packing rules to produce the marker. Do not create a second indicator layout language, a truncation-count namespace or different measurement/render decisions.

## Decisions

- **User decisions:** Show a simple marker, without an omitted count, trailing inside the strip. Drop additional trailing tags when the marker needs their space.
- **Routine default: `trim_marker: "..."` on the repeated container.** The marker is a synthetic instance of that same container with its repeated parameter bound to the literal string. This reuses the tag's text, paint, size and padding. A separate text-item declaration would duplicate styling and geometry already supplied by the repeated subtree; hard-coding a font/box would bypass the template's tag styling. The string is data, so it is not interpolated a second time. Validate the key's dependency on `repeat` through existing load validation, adding it to raw/domain conversion together.
- **Arrange normally first.** If the first trimmed child is an instance of a repeat with a marker, measure a synthetic marker in that repeat's scope. Starting at the normal cut, remove trailing instances of that repeat until the prefix plus marker fits. Keep preceding siblings and the existing trimmed suffix policy. Reuse `arrange_flow` for each candidate so row, column, wrap and gaps retain their existing meaning. If no candidate fits, use the existing `item_out_of_frame` failure with the authored repeat path. Simply appending the marker fails in the exact-full-strip example: there is no room until a tag is removed.
- **Share the selected arrangement between intrinsic-size calculation and output.** Factor only the repeat-marker selection needed by the two existing arrangement call sites; retain the ordinary flow resolver. Rendering an independent overlay would neither reserve space nor contribute the marker's assembled extent.

## Risks / Trade-offs

- Reusing the subtree means its authored content and gates also apply to the marker value. Document the example as a text tag printing the repeated value; no new styling controls are needed.
- Marker geometry can differ from an input instance. Measure it as its own instance rather than reusing the removed tag's measurements.
- The issue audit and this scoping session inspected source; no representative PNG has yet been rendered. Implementation must render and inspect the overflowing and exactly fitting examples as required by the issue and repository, without adding an unverifiable checkbox task.
