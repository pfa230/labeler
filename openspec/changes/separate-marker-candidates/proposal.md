## Why

`arrange_flow_children` in `src/render/mod.rs` measures a repeat's trim marker once, before it tries any prefix, and then tests each candidate prefix by arranging it inline. Nothing names "evaluate this candidate", so #260 had to add allocation-dependent fitting after a marker was already selected, where a fitting failure aborted the render instead of trying a shorter prefix. This refactor gives candidate evaluation one boundary on main first, without changing behavior.

Source issue: https://github.com/pfa230/labeler/issues/498

Fixes #498

## What Changes

- Extract one operation that evaluates a marker candidate for a given prefix length: it measures the marker in the repeat's scope, resolves its packed box, arranges the prefix plus the marker and keeps the prefix's original placements. It returns the candidate's arrangement with the marker's saved measurement, a packing rejection, or the error the same work raises today.
- Have selection call that operation from the longest prefix down and return the first accepted arrangement, which preparation already consumes unchanged.
- Leave measurement bounds, text fits, longest-prefix selection, earlier sibling positions, error codes, messages, paths and first-error order unchanged.

## Capabilities

### New Capabilities

### Modified Capabilities

None. This is a behavior-preserving refactor; `.openspec.yaml` sets `skip_specs: true`.

## Impact

`src/render/mod.rs` only. No API, template schema, layout, rendering or error contract changes, and no new dependencies. Lands after `extract-flow-traversal` (#497) and touches none of its code.

Non-goals: remaining-space allocation, final smaller-box text fitting and deciding which allocation failures permit a shorter prefix (all #260, `ordered-packed-fill`); moving the marker's nested preparation, drawing or asset loading into evaluation; a general renderer error framework.
