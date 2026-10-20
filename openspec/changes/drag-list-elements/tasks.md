# Tasks

## 1. Add direct reordering

- [x] 1.1 Add drag grips and row drop handling to the print-form list editor, moving the source occurrence to the target index through the existing array `onChange`. Retain visible earlier/later controls and focus management. Extend existing list tests to assert exact arrays after non-adjacent moves in both directions with duplicates and an empty element; wrong occurrence selection, value loss or incorrect insertion must fail.
- [x] 1.2 Verify real browser dragging in both directions, keyboard reordering through the existing buttons, and the resulting submitted array from the print form. Record interaction and request evidence under `.agent-runs/`; synthetic handler calls alone do not complete this task.

## 2. Integration gates

- [x] 2.1 Run `npm --prefix ui run lint`, `npm --prefix ui run test`, `npm --prefix ui run build` and `npx --no-install openspec validate --all --strict --no-interactive`; require success before committing implementation.
