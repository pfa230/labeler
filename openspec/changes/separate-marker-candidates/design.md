# Design

## Context

`arrange_flow_children` resolves every expanded child's `FlowChildInput` and arranges them (`base`). Under `overflow: trim`, when the first discarded child is an instance of a repeat with a `trim_marker`, it:

1. finds the repeat's first drawn instance (`group_start`),
2. builds the marker's data scope and `ExpandedItem`, measures it once with `measure_item_instance` against the flow frame, and resolves its input with `flow_child_input`; either can fail with its own error,
3. for each prefix length from `cut` down to `group_start`, arranges the prefix inputs plus the marker input; an arrangement error becomes `ItemOutOfFrame` at the repeat's path, and a candidate whose rect count falls short is rejected,
4. on the first accepted candidate, copies `base`'s rects over the prefix, so adopting the marker never moves an earlier sibling, and returns a `FlowArrangement` with a `MarkerInstance` (prefix length, expanded marker, its `Measured`),
5. when no candidate fits, fails with `ItemOutOfFrame` at the repeat's path.

`prepare_items` then prepares the prefix children and the marker at the arrangement's rects, using the marker's saved `Measured`; `intrinsic` uses only the arrangement's `assembled` extent.

## Goals / Non-Goals

**Goals:**

- One operation evaluates one candidate prefix and owns everything that decides whether it is accepted: the marker's measurement, its packed box and the candidate arrangement.
- Selection reads as a loop over candidates that stops at the first accepted one.

**Non-Goals:**

- Changing `FlowArrangement`, `MarkerInstance`, `prepare_items` or `intrinsic`. Preparation already draws the selected arrangement's rects with the saved measurement.
- Moving the marker's nested preparation (container frames, nested arrangement, stock check, QR or image loading) into evaluation. On main none of it decides acceptance, and running it during selection would report a marker's nested error before an earlier prefix sibling's preparation error, changing first-error order.

## Decisions

### One candidate operation returning `Result<Option<FlowArrangement>, AppError>`

A new `RenderContext` method evaluates the candidate that keeps a given number of expanded children before the marker. It performs steps 2 to 4 above for that one prefix length and returns:

- `Ok(Some(arrangement))`: the accepted candidate's `FlowArrangement`, prefix rects taken from `base`, with its `MarkerInstance`;
- `Ok(None)`: the packing rejection, when the arranged candidate does not draw every child;
- `Err`: the error today's code raises from the same step, unchanged in code, message and path.

The values every candidate shares (the repeat's item, original index, repeat name and marker text, the repeat's path, `base`, the resolved inputs, frame, flow, geometry values and children prefix) go in one argument struct, following `FlowChildArgs` and `PrepareItemArgs`, so the method stays under clippy's argument limit. `arrange_flow_children` keeps steps 1 and 5 and loops prefix lengths from longest to shortest, returning the first `Some`.

`Option` is the smallest type that separates a rejection from an error. A dedicated enum would add a type with the same two cases.

### The marker is measured inside each evaluation

Default: the operation measures the marker for its own candidate rather than receiving a measurement taken once before the loop. The issue asks for one operation that returns the candidate's layout and saved text fits, and #260's allocation makes the marker's measurement depend on the prefix. Measuring once outside would leave the measurement outside the boundary this change exists to draw.

On main the measurement does not depend on the prefix, so every candidate measures the same result. The loop always evaluates at least one candidate, so a failing marker measurement or packed box raises the same error, before any arrangement, as it does today.

### No new test

`repeating_container_overflow_trim_draws_first_two_instances`, `repeating_container_trim_marker_distinguishes_overflow_from_an_exact_fit` and `repeating_container_trim_marker_keeps_earlier_siblings_in_place` pin longest-prefix selection, a rejected candidate followed by an accepted shorter one, the no-marker exact fit and earlier sibling positions. An operation that turned a rejection into an error, picked the wrong prefix or stopped copying `base`'s rects fails one of them. They must pass unedited.

## Risks / Trade-offs

- [Re-measuring the marker per rejected candidate] → One marker measurement per dropped instance, bounded by the repeat's drawn instances and small beside the Typst compile.
- [A step moves out of its order] → The tasks compare the operation's steps with `arrange_flow_children` on `origin/main`, and every existing renderer, error and trim test must pass unedited.
