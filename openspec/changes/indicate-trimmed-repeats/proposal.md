## Why

A flow-trimmed list can look complete on a printed label even though input entries were omitted. A trailing marker lets the reader recognize an incomplete list.

Source issue: https://github.com/pfa230/labeler/issues/358

Fixes #358

## What Changes

- Let a repeating container opt into a literal `trim_marker`, rendered using that container with its repeated value bound to the marker.
- When trimming first cuts that repeat, retain the longest fitting prefix of its entries followed by the marker inside the same strip, removing trailing entries to make room.
- Show no marker when the entries all fit.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `layout`: Add an optional trailing marker to a repeat cut by flow trimming.

## Impact

Touches container parsing/model conversion, repeat expansion, flow measurement/rendering, render tests and the authoring guide. Reuses the existing repeated subtree and flow arrangement; adds no dependencies or HTTP endpoints.

## Non-goals

Omitted counts and a separately reserved indicator area were declined during scoping. This change does not alter packed `fill` sizing (#260) or add markers to ordinary non-repeated children.
