## ADDED Requirements

### Requirement: Primary navigation

Primary navigation SHALL list Labels (`/`), Import (`/import`) and Settings (`/settings`), in that order.

#### Scenario: Navigation sections

- **WHEN** the operator opens the application shell
- **THEN** primary navigation contains exactly Labels, Import and Settings, in that order

## MODIFIED Requirements

### Requirement: Batch grid preview follows the template format

The Import page SHALL show a preview pane beside the batch grid whenever a template is chosen and the grid holds rows. The `data` previewed for a row SHALL be the data the submit path would send for it.

For a `sheet` template the pane SHALL render the whole batch Download would submit: every row in grid order, each carrying the `copies` value, with the same `start_slot`, sent as `POST /api/render`. The grid SHALL render no row selector.

For a `single` template, and for a grid with no template chosen, each row SHALL carry a radio selector. The pane SHALL render the selected row through `POST /api/render/label`. When no row is selected, or the selected row was removed, the first row SHALL be previewed.

#### Scenario: A sheet previews the whole batch

- **WHEN** a `sheet` template's grid holds 3 rows, `copies` is 2 and `start slot` is 4
- **THEN** the preview request is `POST /api/render` with `start_slot: 4` and 3 entries ordered row 1, row 2, row 3, each with `copies: 2`
- **AND** a Download activated without further edits sends equal `labels` and `start_slot`

#### Scenario: A refused row stays selected

- **WHEN** the operator selects row 2 of a `single` grid and its render is refused
- **THEN** row 2 stays the row requested and the pane shows the failure

### Requirement: Parameter controls

Each parameter's control SHALL be labelled with its `description`, else its `name`, and SHALL follow its `control`:

| `control` | Print form | Grid cell |
|---|---|---|
| `text` | single-line text box | same |
| `textarea` | multi-line text box | same |
| `integer`, `number` | numeric box within the declared `min`/`max`, whole numbers for `integer` | same |
| `checkbox` | two-state checkbox starting at the parameter's default | same |
| `select` | a blank first option (showing the default when there is one), then the declared `values` | same, plus a held value outside `values` |
| `date`, `datetime` | date or date-and-time picker | same |
| `image` | image file chooser, with a clear action while it holds a value | read-only mark that a value is held |
| `list` | editor of text elements the operator can add, remove and reorder | not shown |

A print form field for a parameter with a default SHALL start blank and show the default as its hint, except a checkbox, which has no blank state and starts at its default. Grid cells SHALL be editable in place.

#### Scenario: A defaulted field starts blank

- **WHEN** the print form shows `title` declaring `default: "Untitled"`
- **THEN** the field is blank and shows `Untitled` as its hint

### Requirement: What a screen submits

A screen SHALL submit, per label, the value of every control holding one. A blank control SHALL be omitted, so the service applies the default or reads the parameter as empty (`parameters`). A checkbox SHALL always send its value. A `list` value SHALL be sent as an array, `[]` included, its elements in order without trimming, dropping empties or de-duplicating. A screen SHALL send each label once, the print form its one label and Import each row, carrying the screen's Copies value as `copies` (`rendering`). Every screen SHALL print through `POST /api/print` and download through `POST /api/render`. After a print, the screen SHALL report how many labels were sent to the printer and show each failed label with its error.

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

- **WHEN** an Import grid holds rows 1 and 2, Copies is 3, and the print summary reports indices 3, 4 and 5 failed
- **THEN** row 2 shows the failure and row 1 does not

### Requirement: Grid columns

A batch grid's columns SHALL be fixed by the template, not by its rows, and SHALL be drawn before any row exists. The Import grid SHALL show the loaded CSV's headers in file order, then every declared parameter the CSV lacks, in declaration order, omitting every `list` parameter.

#### Scenario: Import keeps the file's order

- **WHEN** a template declares `title`, `subtitle`, `code` and a CSV with headers `subtitle`, `title` is loaded
- **THEN** the Import grid's columns run `subtitle`, `title`, `code`

## REMOVED Requirements

### Requirement: List values in the grids

**Reason**: Inventory connector capability removed.
**Migration**: Supply list values through REST/MCP or the print form.

### Requirement: Connect mapping offers every parameter

**Reason**: Inventory connector capability removed.
**Migration**: Supply label data through REST/MCP or CSV import.
