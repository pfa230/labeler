## Why

The print form's list-default hints collapse arrays to comma-joined text, making two elements indistinguishable from one element containing a comma and an empty list indistinguishable from one empty element. Show list defaults as compact JSON arrays so the hint communicates the declared elements and their order.

Source issue: https://github.com/pfa230/labeler/issues/351

Fixes #351

## What Changes

- Show print-form list defaults after the existing `default: ` prefix as compact JSON arrays, including brackets and quoted elements.
- Distinguish `["A","B"]`, `["A,B"]`, `[""]` and `[]`, preserving element order.
- Cover the rendered hints and retain the existing untouched-list omission and explicitly emptied-list submission behavior.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `ui`: Specify the list-default representation under Parameter controls.

## Impact

Implementation touches the hint expression in `ui/src/pages/print/FieldForm.tsx` and existing FieldForm/PrintForm tests. Published defaults, list controls, submission data and backend behavior retain their existing contracts. No setting, dependency or shared formatting layer is needed.

## Non-Goals

No template-detail display change, scalar-default reformatting, list-editor redesign, default prepopulation or CSV list support. The complete request in #351 is included; no follow-up issue is needed.
