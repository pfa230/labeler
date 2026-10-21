# Spec Delta

## MODIFIED Requirements

### Requirement: Grid columns

Except for the result column, a batch grid's columns SHALL be fixed by the template, not by its rows, and SHALL be drawn before any row exists. The Import grid SHALL show the loaded CSV's headers in file order, then every declared parameter the CSV lacks, in declaration order.

#### Scenario: Import keeps the file's order

- **WHEN** a template declares `title`, `subtitle`, `code` and a CSV with headers `subtitle`, `title` is loaded
- **THEN** the Import grid's columns run `subtitle`, `title`, `code`

#### Scenario: Declared lists occupy columns

- **WHEN** a template declares `title` then list parameter `tags` and the CSV has only the `title` header
- **THEN** the grid shows `title` then an initially empty editable `tags` column

## ADDED Requirements

### Requirement: Batch result display

Import SHALL show its result column only while at least one row has a result, headed `Print result` for printing or `Download errors` for download validation. Each failed row SHALL offer a keyboard-operable cell action exposing its complete failure message. A successful Download SHALL show one batch success message and leave no per-row success marks.

#### Scenario: An untouched grid has no result column

- **WHEN** a CSV is loaded and no operation has produced row results
- **THEN** the grid has no result column

#### Scenario: Print failures remain readable

- **WHEN** printing produces row results, including a failure message longer than a cell can display
- **THEN** the column is headed `Print result` and the failed row's action opens its complete message with pointer or keyboard activation

#### Scenario: Download validation names its operation

- **WHEN** Download returns a validation failure against row 2
- **THEN** row 2 shows its failure under `Download errors`

#### Scenario: A successful download leaves no result column

- **WHEN** Download succeeds
- **THEN** the batch success message appears and the grid has no download-success marks or result column
