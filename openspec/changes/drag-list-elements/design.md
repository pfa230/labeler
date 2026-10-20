# Design

## Context

At `3dddfb20`, `ParamInput` represents a list as a string array and already implements earlier/later buttons with focus management. List tests exercise exact values, order and button focus, and submission already preserves arrays. See the proposal and UI delta for the additional interaction.

## Goals / Non-Goals

**Goal:** Add direct pointer reordering while reusing the existing controlled array and keyboard controls.

**Non-goal:** Introduce a second list value model or keyboard drag mode.

## Decisions

- Default: keep both reorder buttons visible and retain their focus behavior. They already supply the required keyboard path; hiding them would require a replacement interaction without solving another requested problem.
- Default: use the browser's HTML drag-and-drop events on a dedicated grip and row drop targets, committing one reorder on drop. Track the source by array position within this editor and move that occurrence to the target index with array removal/insertion before calling the existing `onChange`. Values cannot identify occurrences because duplicates and empty strings are valid. A whole-row draggable surface would compete with the text input's editing surface; a grip leaves that input intact. No drag library or persistent element IDs are needed. Browser event behavior: https://developer.mozilla.org/en-US/docs/Web/API/HTML_Drag_and_Drop_API.
- Reuse existing input/submission behavior and button tests. Add focused drag-order assertions with duplicate and empty values, and exercise upward and downward moves through the real browser UI before inspecting the submitted array. Calling React handlers or dispatching test DOM events alone does not establish a usable native drag interaction.
- Add the drag requirement alongside the existing parameter-control requirement, referencing it rather than replacing its whole block. The new behavior needs no edits to list-value submission semantics or the independently proposed CSV grid controls.

## Risks / Trade-offs

The pointer gesture uses the browser's drag interaction; the visible buttons provide the same ordering operation through the existing keyboard path. Moving array positions retains the current positional component model.

## Migration Plan

No migration or API change. Implement through the loop after explicit enrollment.
