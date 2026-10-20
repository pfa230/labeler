# Spec Delta

## MODIFIED Requirements

### Requirement: Sheet pagination

A `sheet` batch SHALL fill the sheet's `positions` in declared order with labels in request order, starting at `start_slot` on page 1 and at slot 0 on each later page, adding pages as needed.

#### Scenario: Batch overflows the first page

- **WHEN** 5 labels go to a 4-slot sheet with `start_slot: 2`
- **THEN** the PDF has 2 pages: labels 0 and 1 in slots 2 and 3, labels 2 to 4 in slots 0 to 2 of page 2

## ADDED Requirements

### Requirement: Sheet stock clipping

Every rendered sheet slot, including a thumbnail (`templates`), SHALL clip all ink to its stock outline.

#### Scenario: Decoration reaches a curved edge

- **WHEN** a full-slot colored background renders on an ellipse sheet
- **THEN** its central ink remains and its bounding-box corners are absent in both the one-slot PNG and each PDF slot

#### Scenario: Rounded rectangle clips only its curved corners

- **WHEN** a 60-by-40 rectangular sheet slot with `rounded: 5` renders a full-slot background
- **THEN** the corner at `[0, 0]` is clipped while the edge midpoint at `[0, 20]` and the center remain, in PNG and PDF
