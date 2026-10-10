# UI

## Purpose

Cross-cutting web UI behavior: palette accessibility, how a template's format is shown, the parameter controls of the print form and the Connect and Import grids and what they submit, the preview pane, the template page, and the CSV Import screen.

## Requirements

### Requirement: Accessible palettes

The light and dark palettes SHALL each meet WCAG 2.x level AA: text and non-text contrast, and no meaning conveyed by colour alone.

#### Scenario: Selection without colour

- **WHEN** a selected and an unselected template card are viewed in greyscale
- **THEN** the selected card is still told apart from the unselected one

### Requirement: Template format is shown

Template cards and the template detail page SHALL show whether a template is `single` or `sheet`, and for a `sheet` the number of positions it declares.

#### Scenario: Grid and detail agree

- **WHEN** one `sheet` template with 24 positions is viewed on the template grid and on its detail page
- **THEN** both show that it is a sheet of 24

### Requirement: Preview pane states

The preview pane SHALL show that a render is in progress, the rendered label or sheet, the service's error message when the render fails, or, for a `sheet`, a refusal with no request sent while the batch exceeds the 500-label cap. A failed or refused preview SHALL NOT disable Print or Download.

#### Scenario: A failed render never gates the run

- **WHEN** a preview request returns a non-2xx response with a message
- **THEN** the pane shows that message and Print and Download stay enabled

### Requirement: Batch grid preview follows the template format

The Connect and Import pages SHALL show a preview pane beside the batch grid whenever a template is chosen and the grid holds rows. The `data` previewed for a row SHALL be the data the submit path would send for it.

For a `sheet` template the pane SHALL render the whole batch Download would submit: every row in grid order, each repeated `copies` times, with the same `start_slot`, sent as `POST /api/render`. The grid SHALL render no row selector.

For a `single` template, and for a grid with no template chosen, each row SHALL carry a radio selector. The pane SHALL render the selected row through `POST /api/render/label`. When no row is selected, or the selected row was removed, the first row SHALL be previewed.

#### Scenario: A sheet previews the whole batch

- **WHEN** a `sheet` template's grid holds 3 rows, `copies` is 2 and `start slot` is 4
- **THEN** the preview request is `POST /api/render` with `start_slot: 4` and 6 labels ordered row 1, row 1, row 2, row 2, row 3, row 3
- **AND** a Download activated without further edits sends equal `labels` and `start_slot`

#### Scenario: A refused row stays selected

- **WHEN** the operator selects row 2 of a `single` grid and its render is refused
- **THEN** row 2 stays the row requested and the pane shows the failure

### Requirement: The sheet preview is live

The sheet preview SHALL update as the batch (template, labels, `start_slot`) changes, with no refresh control.

#### Scenario: An edit reaches the preview

- **WHEN** the operator edits a cell of a `sheet` grid
- **THEN** the pane comes to show the sheet with the edited value, with no refresh action

### Requirement: Screens show every declared parameter

A screen collecting label data SHALL offer one control per parameter in the template detail's parameter list (owned by `parameters`), in that order except where "Grid columns" orders the Import grid, whatever values are entered: no control appears or disappears as values change. A screen SHALL NOT judge whether a label is complete; Print and Download send it, and a label the service refuses SHALL be shown with its error, against its row in a grid.

#### Scenario: A branch-only parameter stays on screen

- **WHEN** a template reads `subtitle` only behind `orientation: horizontal` and the operator picks `vertical`
- **THEN** the `subtitle` control stays on screen, and a print with it blank succeeds

#### Scenario: A refused row is marked

- **WHEN** a grid run's row 3 holds `abc`, loaded from a CSV, for an `integer` parameter
- **THEN** the run fails and row 3 shows the `param_value_invalid` failure naming that parameter

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
| `list` | editor of text elements the operator can add, remove and reorder | read-only (see "List values in the grids") |

A print form field for a parameter with a default SHALL start blank and show the default as its hint, except a checkbox, which has no blank state and starts at its default. Grid cells SHALL be editable in place.

#### Scenario: A defaulted field starts blank

- **WHEN** the print form shows `title` declaring `default: "Untitled"`
- **THEN** the field is blank and shows `Untitled` as its hint

### Requirement: What a screen submits

A screen SHALL submit, per label, the value of every control holding one. A blank control SHALL be omitted, so the service applies the default or reads the parameter as empty (`parameters`). A checkbox SHALL always send its value. A `list` value SHALL be sent as an array, `[]` included, its elements in order without trimming, dropping empties or de-duplicating. The print form SHALL send its label repeated `copies` times; every screen SHALL print through `POST /api/print` and download through `POST /api/render`. After a print, the screen SHALL report how many labels were sent to the printer and show each failed label with its error.

#### Scenario: A blank defaulted field is omitted

- **WHEN** the operator leaves `title` (`default: "Untitled"`) blank and prints
- **THEN** the submitted `data` carries no `title`

#### Scenario: Cleared controls are omitted

- **WHEN** the operator clears an `integer` control and a `text` control, neither parameter declaring a default
- **THEN** the submitted `data` carries neither name

#### Scenario: Edits reach the array

- **WHEN** the list editor holds `A`, `B`, `C`, the operator moves `C` earlier, moves `A` later, and appends an element left empty
- **THEN** the submitted `data` carries `tags: ["C", "A", "B", ""]`

### Requirement: Grid columns

A batch grid's columns SHALL be fixed by the template, not by its rows, and SHALL be drawn before any row exists. The Connect grid SHALL show one column per declared parameter, in declaration order. The Import grid SHALL show the loaded CSV's headers in file order, then every declared parameter the CSV lacks, in declaration order, omitting every `list` parameter.

#### Scenario: Import keeps the file's order

- **WHEN** a template declares `title`, `subtitle`, `code` and a CSV with headers `subtitle`, `title` is loaded
- **THEN** the Import grid's columns run `subtitle`, `title`, `code`, and the Connect grid's run `title`, `subtitle`, `code`

### Requirement: Grid cells keep their values

A grid cell SHALL NOT change its value until the operator edits it, including a value its control cannot show, such as a `select` value outside `values` or a date in another shape; the row submits that value unchanged.

#### Scenario: An offset-bearing instant in a date cell

- **WHEN** a `date` cell holds `2026-09-01T12:00:00Z`
- **THEN** the row submits `2026-09-01T12:00:00Z` unless a date is picked

### Requirement: List values in the grids

No grid SHALL edit a `list` value. A Connect row materialized with a multi-valued column mapped onto a `list` parameter (owned by `connections`) SHALL hold that array read-only and submit it unchanged and unflattened. A row holding no value for a `list` parameter submits nothing for it.

#### Scenario: A mapped list is submitted as delivered

- **WHEN** a row holds a mapped `tags` array
- **THEN** the `tags` cell shows the elements read-only and the submitted `data` carries the array in the connector's order

### Requirement: Connect mapping offers every parameter

The Connect field mapping SHALL offer one mapping control per declared parameter, before any row exists. Mapping validity is owned by `connections`.

#### Scenario: Both branches' names are offered

- **WHEN** a mapping is built for a template whose branches read `subtitle` and `tracking_url`
- **THEN** both names are offered before any row is added

### Requirement: CSV Import screen

The Import screen SHALL parse and edit a CSV in the browser and submit its rows as labels. A CSV MAY be loaded before a template is chosen: its columns show as plain editable text, and parameter columns, the preview and the run controls appear once a template is chosen. The loaded rows SHALL survive a template switch, including values in columns the new template does not read.

#### Scenario: Rows survive a template switch

- **WHEN** a CSV with a `note` column is loaded, a template that does not declare `note` is chosen, and then one that does
- **THEN** every row still holds its `note` value

### Requirement: Template page

The template page SHALL show the template's thumbnail (`templates`) and list its `params` in declaration order, showing each published `default` (`parameters`).

#### Scenario: A tokened default is shown by value

- **WHEN** a parameter declares `default: "{vars.qr_base_url}"` and the store holds `qr_base_url = https://ex.co/`
- **THEN** the page shows `https://ex.co/` as that parameter's default
