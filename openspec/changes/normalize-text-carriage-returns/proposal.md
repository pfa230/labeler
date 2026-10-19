## Why

Text fitting normalizes CRLF but leaves a lone carriage return in a measured segment. Treating every CR, LF and CRLF as a line terminator gives text the same layout regardless of its newline spelling.

Source issue: https://github.com/pfa230/labeler/issues/259

Fixes #259

## What Changes

- **BREAKING:** Normalize lone CR as well as CRLF to LF before text segmentation and measurement.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `text`: Recognize CR, LF and CRLF as equivalent line terminators.

## Impact

Changes the normalization in `render/helpers.rs` and extends existing text-fit and tape-render coverage. No schema fields, dependencies or HTTP changes.

## Non-goals

Other Unicode separators, CSV parsing and unmapped-character refusal (#254).
