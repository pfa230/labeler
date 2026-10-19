# Proposal

## Why

A packed fill child currently takes its container's whole inner extent, so a content-sized badge followed by a fill title overflows instead of sharing a row. Ordered remaining-space allocation gives the title the room earlier children leave.

Source issue: https://github.com/pfa230/labeler/issues/260

Fixes #260

## What Changes

- **BREAKING:** On the packing axis, a packed `fill` child consumes the current line's remaining space after earlier children and the pending gap; caps still apply.
- Apply the rule to rows and columns and use the allocated box for text fitting.
- Define initial intrinsic contributions and final smaller-box fitting without an ancestor sizing cycle.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `layout`: packed-fill availability and its measurement contribution.
- `text`: fitting packed text when final allocation is smaller than its initial measurement box.

## Impact

Touches `src/resolver.rs`, measurement and emission in `src/render/mod.rs`, existing flow/text tests, and flow guidance in `docs/AUTHORING.md`. No new template keys or dependencies.

Non-goals: flex weights, sibling reservation, equal sharing, changing `content` to consume a line remainder, an iterative sizing solver, and changing text or flow overflow policies.
