## MODIFIED Requirements

### Requirement: Batch grid preview follows the template format

The Connect and Import pages SHALL show a preview pane beside the batch grid whenever a template is chosen and the grid holds rows. The `data` previewed for a row SHALL be the data the submit path would send for it.

For a `sheet` template the pane SHALL render the whole batch Download would submit: every row in grid order, each carrying the `copies` value, with the same `start_slot`, sent as `POST /api/render`. The grid SHALL render no row selector.

For a `single` template, and for a grid with no template chosen, each row SHALL carry a radio selector. The pane SHALL render the selected row through `POST /api/render/label`. When no row is selected, or the selected row was removed, the first row SHALL be previewed.

#### Scenario: A sheet previews the whole batch

- **WHEN** a `sheet` template's grid holds 3 rows, `copies` is 2 and `start slot` is 4
- **THEN** the preview request is `POST /api/render` with `start_slot: 4` and 3 entries ordered row 1, row 2, row 3, each with `copies: 2`
- **AND** a Download activated without further edits sends equal `labels` and `start_slot`

#### Scenario: A refused row stays selected

- **WHEN** the operator selects row 2 of a `single` grid and its render is refused
- **THEN** row 2 stays the row requested and the pane shows the failure

### Requirement: What a screen submits

A screen SHALL submit, per label, the value of every control holding one. A blank control SHALL be omitted, so the service applies the default or reads the parameter as empty (`parameters`). A checkbox SHALL always send its value. A `list` value SHALL be sent as an array, `[]` included, its elements in order without trimming, dropping empties or de-duplicating. A screen SHALL send each label once, the print form its one label and Connect and Import each row, carrying the screen's Copies value as `copies` (`rendering`). Every screen SHALL print through `POST /api/print` and download through `POST /api/render`. After a print, the screen SHALL report how many labels were sent to the printer and show each failed label with its error.

#### Scenario: A blank defaulted field is omitted

- **WHEN** the operator leaves `title` (`default: "Untitled"`) blank and prints
- **THEN** the submitted `data` carries no `title`

#### Scenario: Cleared controls are omitted

- **WHEN** the operator clears an `integer` control and a `text` control, neither parameter declaring a default
- **THEN** the submitted `data` carries neither name

#### Scenario: Edits reach the array

- **WHEN** the list editor holds `A`, `B`, `C`, the operator moves `C` earlier, moves `A` later, and appends an element left empty
- **THEN** the submitted `data` carries `tags: ["C", "A", "B", ""]`

#### Scenario: The print form sends its copies once

- **WHEN** the operator sets Copies to 3 and prints
- **THEN** the print request holds one label entry with `copies: 3`

#### Scenario: A failed copies job marks its row

- **WHEN** a Connect or Import grid holds rows 1 and 2, Copies is 3, and the print summary reports indices 3, 4 and 5 failed
- **THEN** row 2 shows the failure and row 1 does not
