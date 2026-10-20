# Design

## Context

At `3dddfb20`, `LabelGrid` always includes a flex-growing Status column. `Import.run` clears prior annotations, assigns print outcomes and BatchInvalid errors to rows by stable ID, and shows a toast for a successful download. Cell edits clear the edited row's annotation; structural edits, Copies and start-slot changes clear all annotations. See the proposal and UI delta for presentation scope.

## Goals / Non-Goals

**Goal:** Make row results conditional and attributable while keeping their existing data and invalidation paths.

**Non-goal:** Add a batch-operation history or a second result model.

## Decisions

- User decision: keep the existing single download-success message without per-row success marks. The user accepted this recommendation during scoping on 2026-10-19.
- Default: derive column visibility from whether any current row has an annotation. Keep its current layout when visible; removing the empty column solves the observed wasted share without adding a width setting or another panel. Existing invalidation naturally removes the column when the last result disappears.
- Default: keep the most recent run's operation (`print` or `download`) in Import and pass the result heading explicitly to the grid. Use `Print result` for print outcomes or validation errors and `Download errors` for download validation failures. The run already clears earlier results before another operation, so no per-row operation field or mixed-history model is needed.
- Keep existing row status/mapping and complete error messages. A failed cell uses a button and native popover to expose the whole message outside the grid's constrained cell, with wrapping and the existing palette tokens. Native button activation and popover dismissal supply the keyboard path without a new dialog framework or React open-state model: https://developer.mozilla.org/en-US/docs/Web/API/Popover_API/Using. A clipped string or hover-only tooltip does not provide that full keyboard-operable detail action.
- Preserve the current clearing rules and map expanded-label failures through the existing Copies/row-ID logic. No download-success annotation is created. The tab-restoration proposal excludes transient result annotations, so this display needs no draft persistence.
- Extend the existing grid/Import tests for initial absence, operation headings, existing successful/failed print rows, Download validation and successful-download absence, and clearing the last annotation. Exercise a long failure through the actual browser with pointer and keyboard; assertions must fail if any message is cut off or the detail action cannot be operated. Reuse current invalidation tests and add only missing coverage of the visibility transition they now control.

## Risks / Trade-offs

The result column can appear or disappear as results change. Full messages remain available through the cell action rather than requiring a wider data grid.

## Migration Plan

No API or stored-data migration. Implement through the loop after explicit enrollment.
