# Tasks

## 1. Decode Import list cells

- [x] 1.1 Add declared-list JSON decoding at Import's typed-data boundary while preserving raw cell text and existing print-form arrays. Extend the existing CSV/input tests for outer CSV quoting, JSON escapes, ordered/duplicate/empty elements, absent/empty cells versus `[]`, and malformed or non-string-array values; assertions must fail on changed decoded values or a missing encoding error.

## 2. Expose and submit list columns

- [x] 2.1 Include declared lists in Import columns, reuse editable multiline text cells for their JSON, and add concise inline spelling/quoting help. Extend existing grid/Import tests to show and edit a list column and verify its submitted array; a hidden column or submitted string must fail.
- [x] 2.2 Use the same typed resolution for previews and actions, report list errors by data row and column, block malformed action requests, and retain editable values through template switches. Extend existing Import tests for correction, Print/Download blocking, preview data/error handling, and a list-to-scalar-to-list switch with an edited cell; fail on any request containing silently omitted invalid data or lost raw text.

## 3. Integration gates

- [x] 3.1 Run `npm --prefix ui run lint`, `npm --prefix ui run test`, `npm --prefix ui run build` and `npx --no-install openspec validate --all --strict --no-interactive`; require success before the implementation commit.
