# UI

## Purpose

Cross-cutting web UI behavior: palette accessibility, how a template's format is shown, the primary navigation, the parameter controls of the print form and the Import grid and what they submit, the preview pane, the template page, and the CSV Import screen.

## Requirements

### Requirement: Accessible palettes

The light and dark palettes SHALL each meet WCAG 2.x level AA: text and non-text contrast, and no meaning conveyed by colour alone.

#### Scenario: Selection without colour

- **WHEN** a selected and an unselected template card are viewed in greyscale
- **THEN** the selected card is still told apart from the unselected one

### Requirement: Template format is shown

Template cards and the template detail page SHALL show whether a template is `single` or `sheet`, and for a `sheet` the number of positions it declares. Installed and catalog cards SHALL use the same format badge presentation.

#### Scenario: Grid and detail agree

- **WHEN** one `sheet` template with 24 positions is viewed in Catalog, on the template grid and on its detail page
- **THEN** all three show that it is a sheet of 24

### Requirement: Preview pane states

The preview pane SHALL show that a render is in progress, the rendered label or sheet, the service's error message when the render fails, or, for a `sheet`, a refusal with no request sent while the batch exceeds the 500-label cap. A failed or refused preview SHALL NOT by itself disable Print or Download; the print form's independent numeric-bound gate still applies.

#### Scenario: A failed render never gates the run

- **WHEN** a preview request returns a non-2xx response with a message and the actions' other availability conditions are met
- **THEN** the pane shows that message and Print and Download stay enabled

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

### Requirement: The sheet preview is live

The sheet preview SHALL update as the batch (template, labels, `start_slot`) changes, with no refresh control.

#### Scenario: An edit reaches the preview

- **WHEN** the operator edits a cell of a `sheet` grid
- **THEN** the pane comes to show the sheet with the edited value, with no refresh action

### Requirement: Screens show every declared parameter

A screen collecting label data SHALL offer one control per parameter in the template detail's parameter list (owned by `parameters`), in that order except where "Grid columns" orders or omits columns of the Import grid, whatever values are entered: no control appears or disappears as values change. A screen SHALL NOT judge whether a label is complete; Print and Download send it subject to "Print form numeric bounds", and a label the service refuses SHALL be shown with its error, against its row in a grid.

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
| `list` | editor of text elements the operator can add, remove and reorder | editable JSON text, decoded by "CSV Import screen" |

A print form field for a parameter with a default SHALL start blank and show the default as its hint, except a checkbox, which has no blank state and starts at its default. A list-default hint SHALL show the default as a compact JSON array of strings, with double-quoted elements, preserved element order and no added separator whitespace. Grid cells SHALL be editable in place.

#### Scenario: A defaulted field starts blank

- **WHEN** the print form shows `title` declaring `default: "Untitled"`
- **THEN** the field is blank and shows `Untitled` as its hint

#### Scenario: List defaults have distinct hints

- **WHEN** the print form shows list parameters with defaults `["A", "B"]`, `["A,B"]`, `[""]` and `[]`
- **THEN** their hints are respectively `default: ["A","B"]`, `default: ["A,B"]`, `default: [""]` and `default: []`

#### Scenario: A list hint preserves order

- **WHEN** a list parameter declares `default: [B, A]`
- **THEN** its print-form hint is `default: ["B","A"]`

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

A batch grid's columns SHALL be fixed by the template, not by its rows, and SHALL be drawn before any row exists. The Import grid SHALL show the loaded CSV's headers in file order, then every declared parameter the CSV lacks, in declaration order.

#### Scenario: Import keeps the file's order

- **WHEN** a template declares `title`, `subtitle`, `code` and a CSV with headers `subtitle`, `title` is loaded
- **THEN** the Import grid's columns run `subtitle`, `title`, `code`

#### Scenario: Declared lists occupy columns

- **WHEN** a template declares `title` then list parameter `tags` and the CSV has only the `title` header
- **THEN** the grid shows `title` then an initially empty editable `tags` column

### Requirement: Grid cells keep their values

A grid cell SHALL NOT change its value until the operator edits it, including a value its control cannot show, such as a `select` value outside `values` or a date in another shape; the row submits that value unchanged.

#### Scenario: An offset-bearing instant in a date cell

- **WHEN** a `date` cell holds `2026-09-01T12:00:00Z`
- **THEN** the row submits `2026-09-01T12:00:00Z` unless a date is picked

### Requirement: CSV Import screen

The Import screen SHALL parse and edit a CSV in the browser and submit its rows as labels. A CSV MAY be loaded before a template is chosen: its columns show as plain editable text, and parameter columns, the preview and the run controls appear once a template is chosen. The loaded rows SHALL survive a template switch, including values in columns the new template does not read.

For a declared `list` parameter, Import SHALL interpret a nonempty cell as a JSON array of strings after CSV unquoting, with normal JSON escapes; an empty cell or absent column means omission. A malformed JSON value or a value other than an array of strings SHALL identify its 1-based data row and column name, remain editable, and prevent Print and Download until corrected or omitted. A preview requiring that cell SHALL show its encoding error and send no render request.

#### Scenario: Rows survive a template switch

- **WHEN** a CSV with a `note` column is loaded, a template that does not declare `note` is chosen, and then one that does
- **THEN** every row still holds its `note` value

#### Scenario: CSV quoting carries list elements

- **WHEN** a declared list cell is CSV-quoted as `"[""A"","""",""B,C"",""A""]"`
- **THEN** its submitted value is `["A","","B,C","A"]`

#### Scenario: An explicit empty list differs from omission

- **WHEN** three rows have respectively an empty list cell, `[]`, and `[""]`
- **THEN** the first omits the parameter, the second sends `[]`, and the third sends `[""]`

#### Scenario: Invalid list encoding stays editable

- **WHEN** row 2's `tags` cell contains `[1]` or malformed JSON
- **THEN** Import names row 2 and `tags`, keeps the cell editable, and sends no Print or Download request
- **AND** correcting it to `["1"]` removes that encoding error

### Requirement: Template page

The template page SHALL hold the template's print form and preview, its dimensions, unit and DPI, the variables it reads, its YAML source with an editor that saves it (`templates`), and an action that deletes it (`templates`). A successful save SHALL leave the form's entered values in place, and the preview SHALL then show the saved template.

#### Scenario: A tokened default is shown by value

- **WHEN** a parameter declares `default: "{vars.qr_base_url}"` and the store holds `qr_base_url = https://ex.co/`
- **THEN** the page shows `https://ex.co/` as that parameter's default

#### Scenario: A save reaches the preview

- **WHEN** the operator has typed `A` into `title`, then edits the template's YAML and saves it
- **THEN** `title` still holds `A`
- **AND** the preview is rendered from the saved template

### Requirement: Print form continuation checkbox

The print form SHALL show an initially unchecked Chain and continue checkbox beside the Print button only when a successful probe of the selected stored printer reports `chain_and_continue: true` (`printing`). The checkbox SHALL be absent with no printer selected, unknown support, unsupported capability, or probe failure; ordinary Print SHALL remain usable without a successful capability probe.

A checked checkbox SHALL submit `chain_and_continue: true` with the existing labels, copies and printer. An unchecked or absent checkbox SHALL omit that field. The selection SHALL be scoped to the selected printer and reset to unchecked when that printer changes or its capability result ceases to confirm support. Continuation SHALL be omitted from preview and download requests. A continuation refusal SHALL use the existing print error presentation.

The checkbox's help SHALL explain that labels are separated while the last label remains attached for the next print, and that an ordinary subsequent print finishes the healthy run. Precut is the printer's mandatory job preparation and SHALL NOT be offered as a form setting. No no-cuts or final-cut-only control SHALL be offered.

#### Scenario: Supported printer

- **WHEN** a successful probe reports chain-and-continue support for the selected printer
- **THEN** an unchecked Chain and continue checkbox appears beside Print

#### Scenario: Unsupported or unconfirmed support

- **WHEN** there is no selected printer, support is loading, the probe reports no support, or the probe fails
- **THEN** the checkbox is absent and ordinary printing retains its existing availability

#### Scenario: Checked continuation with copies

- **WHEN** the operator checks Chain and continue, sets Copies to 3, and prints
- **THEN** the print request contains `chain_and_continue: true` and one label entry with `copies: 3`

#### Scenario: Ordinary submission

- **WHEN** the checkbox is unchecked or absent and the operator prints
- **THEN** the print request omits `chain_and_continue`

#### Scenario: Printer switch clears continuation

- **WHEN** the operator checks continuation for printer A and changes to printer B
- **THEN** no checked choice from A is submitted to B
- **AND** a checkbox for B appears unchecked only when B's own probe confirms support

#### Scenario: Support is lost

- **WHEN** continuation was checked and the selected printer's latest probe no longer confirms support
- **THEN** the checkbox is absent and its checked selection is cleared

#### Scenario: Preview and download isolation

- **WHEN** the checkbox is checked and the operator previews or downloads
- **THEN** render requests omit `chain_and_continue` and retain existing data, format and sheet-slot behavior

#### Scenario: Unsupported continuation is visible

- **WHEN** printing returns an unsupported continuation error
- **THEN** the form shows the service's message through its existing print error presentation

### Requirement: Print form Copies stepper

The print form's Copies control SHALL offer one stepper: a decrease button and an increase button beside the count box. The count box SHALL show no browser spin buttons.

#### Scenario: One stepper

- **WHEN** the print form shows its Copies control
- **THEN** the decrease and increase buttons sit beside the count box
- **AND** the count box is a text box, not a spin button

### Requirement: Primary navigation

Primary navigation SHALL list Labels (`/`), Import (`/import`), Print history (`/print-history`) and Settings (`/settings`), in that order.

#### Scenario: Navigation sections

- **WHEN** the operator opens the application shell
- **THEN** primary navigation contains exactly Labels, Import, Print history and Settings, in that order

### Requirement: Print form Cut button

The print form SHALL show a Cut button beside Print only when a successful probe of the selected stored printer reports `cut: true` (`printing`), and no Cut button otherwise. Pressing it SHALL send one cut for the selected printer and report the outcome through the form's print notice: success, or failure with the service's detail.

#### Scenario: Printer supports cut

- **WHEN** a successful probe of the selected printer reports `cut: true`
- **THEN** a Cut button appears beside Print, and pressing it sends `POST /api/printers/{id}/cut` for that printer

#### Scenario: Failed cut is visible

- **WHEN** a cut answers `status: "failed"`
- **THEN** the form shows an error notice with the service's detail

### Requirement: Print form numeric bounds

The print form SHALL show a textual error identifying the violated bound beside each nonblank `number` or `integer` field outside its declared `min` or `max` (`parameters`), with the field marked invalid and its error associated accessibly. Print and Download SHALL be disabled while any such field is invalid, and attempts to activate them SHALL send no action request. Correcting or clearing a field SHALL remove its bound error; when no field remains outside its bounds, actions SHALL regain availability subject to their other conditions.

#### Scenario: A lower-only bound blocks both actions

- **WHEN** the operator types a value below the declared `min` of a print-form numeric field with no `max`
- **THEN** the field has an associated error identifying the minimum, both actions are disabled and attempted Print and Download send no action request

#### Scenario: An upper-only bound blocks both actions

- **WHEN** the operator types a value above the declared `max` of a print-form numeric field with no `min`
- **THEN** the field has an associated error identifying the maximum, both actions are disabled and attempted Print and Download send no action request

#### Scenario: Both sides of a two-sided bound

- **WHEN** the operator types below `min`, then above `max`, in a print-form numeric field declaring both
- **THEN** each value produces its bound error and prevents both action requests

#### Scenario: Correcting to an inclusive endpoint restores submission

- **WHEN** the only out-of-range field is corrected to its declared `min` or `max` and the actions' other availability conditions are met
- **THEN** its error and invalid marker disappear, both actions become available and either action sends the entered endpoint unchanged

#### Scenario: One invalid field remains

- **WHEN** two numeric fields are out of range and the operator corrects only one
- **THEN** the other field retains its error and both actions remain disabled

#### Scenario: Clearing defers to the service

- **WHEN** the operator clears an out-of-range numeric field, whether or not it has a default, and no other field is out of range
- **THEN** its bound error disappears and otherwise available actions submit with that field omitted

### Requirement: Refused template entries on Labels

When the template listing reports refused entries (`templates`), Labels SHALL show their count above the printable grid without expansion and offer each reported path and full refusal message in an expandable section, independently of search and category filters.

For an unfiltered listing with no printable templates but refused entries present, Labels SHALL identify that no templates are printable instead of saying no templates are installed.

Labels SHALL offer Reload templates to reload the registry (`templates`) and then refresh the printable list and refusal diagnostics. A failed reload SHALL show its error and retain the displayed registry results.

#### Scenario: Printable templates and refused entries coexist

- **WHEN** the listing contains a printable template and refused entries `bad.yaml` and `notes.txt`
- **THEN** Labels shows a count of 2 before expansion and the printable template remains available
- **AND** expanding the refused section shows both paths with their respective full errors
- **AND** narrowing search or category leaves that count and those diagnostics available

#### Scenario: Every entry is refused

- **WHEN** the unfiltered listing contains no printable template and at least one refused entry
- **THEN** Labels identifies that no templates are printable and exposes the refused entries, without showing "No templates yet"

#### Scenario: Repair on disk reaches Labels

- **WHEN** an operator fixes a refused template on disk and activates Reload templates
- **THEN** the registry is reloaded before the list is refreshed, without a service restart
- **AND** the repaired template becomes printable and its refusal disappears

#### Scenario: Reload fails

- **WHEN** Reload templates fails
- **THEN** Labels shows the failure message and retains the previously displayed templates and refusals

### Requirement: Unsaved YAML navigation

Template detail and New template SHALL treat YAML as unsaved when its current text differs from its saved baseline: the source at entry to an existing template's edit, or empty text for a new template. A successful save SHALL establish the submitted YAML as saved; a failed save SHALL leave the unsaved draft protected.

In-app navigation leaving an editor with unsaved YAML, including sidebar navigation and browser Back, SHALL offer Discard and Keep editing. Keep editing SHALL cancel that navigation and retain the current draft and page; Discard SHALL proceed to the requested destination. Unchanged or successfully saved YAML SHALL NOT trigger confirmation.

Reload, document navigation and page close with unsaved YAML SHALL request the browser's native beforeunload confirmation; without unsaved YAML they SHALL NOT request it.

#### Scenario: Keep a draft after sidebar navigation

- **WHEN** the operator changes existing template YAML, selects a sidebar destination and chooses Keep editing
- **THEN** the current page and edited text remain
- **AND** repeating the navigation and choosing Discard reaches the requested destination

#### Scenario: Back protects a new draft

- **WHEN** the operator enters YAML in New template and presses browser Back
- **THEN** Keep editing retains that new draft and Discard permits the Back navigation

#### Scenario: Save or revert removes the warning

- **WHEN** the operator restores an edit to its baseline, or successfully saves its current YAML, then navigates
- **THEN** navigation proceeds without a dirty-YAML confirmation
- **AND** a successful New template create reaches its created detail page without such a confirmation

#### Scenario: A failed save remains dirty

- **WHEN** saving changed YAML fails and the operator tries to leave
- **THEN** the draft remains protected by confirmation

#### Scenario: Native reload confirmation

- **WHEN** the operator interacts with a YAML editor, changes its text and requests a reload in a browser that presents beforeunload confirmations
- **THEN** dismissing the native confirmation keeps the page and draft
- **AND** accepting it permits the reload

### Requirement: Print history screen

The Print history page SHALL show the job history (`printing`), with each job's time, template id, printer id, actor and textual send outcome. It SHALL show the actor's username when available, otherwise the recorded actor id, and label an empty actor id as unknown. A failed job's complete error text SHALL be inspectable on the page. `ok` SHALL be described as sent to the printer.

The page SHALL show loading while its first read is pending, an empty-history message for a successful empty response, and the service's message with a Retry action when the read fails.

#### Scenario: An operator traces a failure

- **WHEN** the operator opens Print history containing successful and failed jobs from different actors
- **THEN** each job shows its time, template, printer, actor and send outcome, newest first
- **AND** the failed job's full error can be read without leaving the page

#### Scenario: Empty and failed reads differ

- **WHEN** the initial history read is pending, then succeeds empty, or instead fails
- **THEN** the page respectively shows loading, an empty-history message, or the service's error and Retry
- **AND** Retry performs another history read

### Requirement: Print-form list dragging

The print-form list editor defined by "Parameter controls" SHALL offer a drag handle to move an element directly to another element's position. Earlier/later buttons SHALL remain visible and keyboard-operable.

#### Scenario: Dragging moves an empty element past duplicates

- **WHEN** the list holds `["A","B","A",""]` and the operator drags the fourth element to the second position
- **THEN** the displayed and submitted array is `["A","","B","A"]`
