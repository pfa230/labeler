# Design

## Context

At `3dddfb20`, `parseCsv` preserves strings, Import removes list columns from `displayedFields`, and `pruneDataForSubmit` accepts only already-materialized arrays for lists. Grid edits already hold text, including temporarily invalid scalar values. See `proposal.md` for motivation and `specs/ui/spec.md` for the new cell contract.

## Goals / Non-Goals

**Goal:** Add list decoding at Import's typed-data boundary while retaining raw editable cells.

**Non-goal:** Change the print form's array-based list editor or the API's ordered-list semantics.

## Decisions

- User decision: use JSON arrays of strings, empty cells for omission, `[]` for explicit emptiness, visible editable list columns, row/column errors that block submission, and no new declaration attribute. The user accepted this policy during scoping on 2026-10-19.
- Keep the CSV parser and grid values as text. Resolve declared list cells against the currently selected template when deriving typed Import data, then use the existing submit pruning. Switching to a scalar parameter of the same name must recover the original text; eager replacement of stored text with arrays loses that representation. Ordinary comma or newline splitting cannot preserve all the requested elements unambiguously.
- Default: use `JSON.parse` and require an array whose elements are strings. Only a zero-length cell is omitted; nonempty whitespace without a JSON value is invalid. JSON whitespace and escapes are allowed inside a valid JSON value. Papa Parse already handles outer CSV quoting, with embedded CSV quotes doubled: https://www.papaparse.com/docs. No extra encoding library is needed.
- Remove list filtering from Import's column derivation and use the existing multiline text-cell presentation for list JSON, with compact example/empty-cell help beside Import. Do not introduce add/remove/reorder widgets inside grid cells.
- Derive encoding errors and decoded data from the same resolver used by previews and actions. A malformed cell remains in the grid, names its current 1-based data-row position and column, and blocks Print/Download, including action-handler submission from the latest rows snapshot. A preview that needs malformed data shows the encoding error without sending a render request; a single-row preview of a different valid row remains available. Do not drop invalid list values or treat them as absent.
- Leave `pruneDataForSubmit`'s existing print-form array handling intact; decode only on Import's path. Test the new boundary and extend existing Import/grid tests rather than introducing a parallel test harness. Tests must fail on dropped list columns, element coercion/loss, absent-versus-empty collapse, a request with malformed data, or rewriting raw cells across a list/scalar template switch.

## Risks / Trade-offs

JSON inside CSV needs two quoting layers; the inline example makes those layers concrete. Keeping raw text means template selection can reveal list-encoding errors in a previously scalar column, as required by typed interpretation.

## Migration Plan

No stored-data or API migration. Implement through the behavior-change loop after proposal delivery and explicit enrollment.
