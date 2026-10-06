# UI

## Purpose

Cross-cutting web UI behavior: the colour palette, the template format badge, how the print form and the Connect and Import grids render the inputs the service reports and what they submit, the preview pane, the template page, and the CSV Import screen.

## Requirements

### Requirement: Palette contrast

The light and dark palettes SHALL each meet every pairing below, computed by the WCAG 2.x relative-luminance formula, and every ground listed SHALL have a defined colour in both palettes.

| Foreground | Over | Minimum |
|---|---|---|
| accent (all accent text, a single glyph such as the favourite star included) | accent tint, info tint, card surface, page ground | 4.5:1 |
| info (sheet badge text) | accent tint, info tint, card surface, page ground | 4.5:1 |
| accent ink (label of an accent-filled control) | accent | 4.5:1 |
| accent as a non-text mark (selected card border) | card surface | 3:1 |

The UI test suite SHALL assert these pairings from the colour values in the shipped stylesheet, and SHALL fail naming any colour an assertion needs that the stylesheet lacks.

#### Scenario: A palette edit that breaks a pairing fails the suite

- **WHEN** the shipped accent is changed so that accent text over the accent tint falls below 4.5:1 in either palette
- **THEN** the UI test suite fails

### Requirement: One accent colour per palette

Within a palette, accent text and the fill of accent controls SHALL be the same colour, and the palette SHALL offer no second accent shade for either role. The label colour on an accent fill SHALL be a palette value, so it changes when the palette changes; the two palettes MAY resolve the accent and its label to different colours.

#### Scenario: Switching palette changes the button label

- **WHEN** the palette changes from light to dark
- **THEN** every accent-filled control's label changes with it and still meets 4.5:1 against the dark accent

### Requirement: Selection is not conveyed by tint alone

A selected template card SHALL carry the accent tint as its fill and an accent border meeting 3:1 against the card surface, so the selection stays visible to a user who cannot tell the tint from the surface.

#### Scenario: Selection survives an indistinguishable tint

- **WHEN** a selected and an unselected card are compared with the tint rendered identical to the surface
- **THEN** the selected card is still marked by its accent border

### Requirement: Format badge

The format badge SHALL separate `single` from `sheet` by three cues, each sufficient alone:

| Cue | `single` | `sheet` |
|---|---|---|
| Icon | exactly one cell | four or more cells in at least two rows and two columns |
| Colour | accent text and border on the accent tint | info text and border on the info tint |
| Text | `single` | `sheet · N`, N being the number of positions the template declares |

A one-position sheet SHALL read `sheet · 1`. The icon SHALL be hidden from assistive technology, so what it conveys is exactly the visible text. The border, drawn in the text colour, SHALL delineate the chip, because the ground behind it can equal its fill. The info colour SHALL differ from the accent, success and error colours.

#### Scenario: Colour removed

- **WHEN** a `single` and a `sheet` badge are rendered in greyscale
- **THEN** they remain distinguishable by icon cell count and by text

#### Scenario: Chip over its own fill

- **WHEN** a `single` badge sits on a selected card tinted with the accent tint
- **THEN** its border still delineates the chip

### Requirement: Where the badge appears

The badge SHALL appear on exactly two surfaces, identical on both: an installed template's card on the template grid, and the `Format` row of its detail page. Three places name a format in prose and SHALL carry no badge, icon, colour or count: the catalog listing (the plain word), the detail page's `Dimensions` row (a sheet's sentence ends in `sheet`), and the preview pane's `Open sheet preview` fallback link.

#### Scenario: Grid and detail agree

- **WHEN** one `sheet` template is viewed on the template grid and on its detail page
- **THEN** both badges show the same icon, colours and text including the position count

### Requirement: Preview pane states

The preview pane SHALL show exactly one of, in this precedence:

| State | Shows |
|---|---|
| Rendering (debouncing, in flight, or waiting for row inputs) | `rendering preview…` |
| Refused (sheet grid only) | the refusal text, not prefixed |
| Failed (any non-2xx render) | `Preview failed: <the service's message>` |
| Rendered, `single` | the returned image |
| Rendered, `sheet` | the returned PDF embedded, with an `Open sheet preview` link where it cannot embed |
| Idle | `Fill the required fields to preview.` |

A failed or refused preview SHALL NOT change whether Print or Download is enabled.

#### Scenario: A refused render never gates the run

- **WHEN** a preview request returns a non-2xx response with a message
- **THEN** the pane reads `Preview failed: <message>` and Print and Download keep the state the grid's own validity gives them

### Requirement: Batch grid preview follows the template format

The Connect and Import pages SHALL show a preview pane beside the batch grid whenever a template is chosen and the grid holds rows. The `data` previewed for a row SHALL be the data the submit path would send for it.

For a `sheet` template the pane SHALL render the whole batch Download would submit: every row in grid order, each repeated `copies` times, with the same `start_slot`, sent as `POST /api/batch` with `mode: "download"` and no `printer`. The grid SHALL render no row selector.

For a `single` template, and for a grid with no template chosen, each row SHALL carry a radio selector. The pane SHALL render the selected row through `POST /api/render/label`, whatever that row's validity. When no row is selected, or the selected row was removed, the first valid row SHALL be previewed; when there is none, no request is sent and the pane is idle.

#### Scenario: A sheet previews the whole batch

- **WHEN** a `sheet` template's grid holds 3 valid rows, `copies` is 2 and `start slot` is 4
- **THEN** the preview request is `POST /api/batch` with `mode: "download"`, `start_slot: 4` and 6 labels ordered row 1, row 1, row 2, row 2, row 3, row 3
- **AND** a Download activated without further edits sends equal `labels` and `start_slot`

#### Scenario: A selected invalid row is still previewed

- **WHEN** the operator selects row 2 of a `single` grid, clears a required value in it, and the render is refused
- **THEN** row 2 stays the row requested, the pane shows the failure, and Download is disabled by the invalid row

### Requirement: Sheet preview refusals

For a `sheet` template the preview SHALL send no request and show a refusal while the grid holds what Print and Download refuse:

| Condition | Pane text |
|---|---|
| one invalid row | `Fix row 2 to preview the sheet.` |
| several invalid rows, 1-based, in grid order | `Fix rows 2, 5 to preview the sheet.` |
| more than 500 labels after `copies` expansion | `Over the 500-label limit; reduce the batch to preview the sheet.` |

The named rows SHALL be exactly the rows the run refuses. Invalid rows take precedence over the cap. The refusal SHALL lift and the sheet render as soon as the condition clears. While any row's inputs are still pending, the pane SHALL show rendering and name no row, because validity is judged only against inputs reported for every row.

#### Scenario: Repairing the row renders the sheet

- **WHEN** the one invalid row of a `sheet` grid is filled in
- **THEN** the refusal clears and one batch request holding every row is sent

#### Scenario: Pending inputs show rendering, not a refusal

- **WHEN** one row's inputs are pending and the template's default inputs would mark that row missing a value its own inputs do not require
- **THEN** the pane shows rendering, names no row and sends no request until the inputs arrive

### Requirement: The sheet preview is live

The sheet preview SHALL re-render whenever the resolved batch (template, labels, `start_slot`) changes, with no refresh control. A request SHALL be sent only after the batch has been unchanged for about 300 ms, so edits within that interval produce one request and a re-render with an unchanged batch produces none. A change while a request is in flight SHALL abort it, and its response SHALL be neither shown nor reported as a failure.

#### Scenario: An in-flight render is superseded

- **WHEN** a batch preview is in flight and the operator edits a cell
- **THEN** the request is aborted, a new one carries the edited batch, and the pane shows neither the aborted PDF nor a failure

### Requirement: Screens decide from the reported inputs

A screen collecting label data SHALL decide which controls it offers, and whether a label is complete, only from the input lists the service reports (owned by `parameters`): it SHALL NOT inspect the layout, evaluate `when:`, or derive required-ness from anything but `required`. A label SHALL be incomplete exactly when an entry marked `required` holds no value, and an entry carrying `default_error` SHALL show that error's message against the entry.

No screen SHALL treat a label as complete, or submit it, while the list for its current values has been requested and not received. The print form is the exception on failure: when its list request fails it SHALL keep the last list it received (or `inputs.default`), show the failure, and stay submittable.

#### Scenario: A pending list blocks submission

- **WHEN** the operator switches `orientation` and the list for the new values has not arrived
- **THEN** the print form cannot be submitted until it does

### Requirement: What a screen submits

A screen SHALL submit exactly the names in the label's current list (a grid row's own list), less any name the print form is deferring. A held value for a name the list omits SHALL be retained, restored when the branch returns, and not sent. An empty value SHALL be omitted for controls `integer`, `number`, `select`, `checkbox`, `date` and `datetime`, and sent as `""` for `text`, `textarea` and `image`. A `list` value SHALL be sent as an array, `[]` included.

#### Scenario: A value from a deactivated branch is not submitted

- **WHEN** the operator enters `subtitle`, switches `orientation` to a branch whose list omits it, then switches back
- **THEN** the intermediate submission carries no `subtitle`, and the value is shown again after switching back

#### Scenario: Cleared numeric versus cleared text

- **WHEN** the operator clears an `integer` control and a `text` control
- **THEN** the submitted `data` omits the integer's name and carries the text's name as `""`

### Requirement: Print form inputs

The print form SHALL first render the template detail's `inputs.default`, then request the list for the data it would submit (pruned, deferred names omitted), re-requesting after a short debounce whenever a value changes and keeping the previous list on screen until the new one arrives. Each entry SHALL render in the list's order, under its `description`, else its `name`, with this control:

| `control` | Control |
|---|---|
| `text` | single-line text box |
| `textarea` | multi-line text box |
| `integer`, `number` | numeric box with the entry's `min`/`max`, stepping by 1 for `integer`, freely for `number`; a range slider, showing its value and `unit`, only when `slider` is true and the entry has a `default` |
| `checkbox` | tick box labelled `Unset`, `Enabled` or `Disabled` |
| `select` | the entry's `values`, showing `Select...` while unset |
| `date`, `datetime` | date or date-and-time control |
| `image` | file chooser, with `image selected` once a file is held |
| `list` | the list editor |

Controls SHALL be seeded with the published `default` unchanged, except that a bare `YYYY-MM-DD` seeded into a date-and-time control SHALL be widened to `YYYY-MM-DDT00:00`. An entry with no `default` SHALL start visibly unset: an unset tick box SHALL NOT read as unchecked and an unset select SHALL NOT show its first option. An entry with `truncated_elsewhere` true SHALL carry the note `Also used on a single-line item, which shows only the first line.`

#### Scenario: Switching a parameter changes what the form asks for

- **WHEN** the operator switches `orientation` from `horizontal` (reads `subtitle`) to `vertical` (reads `tracking_url`)
- **THEN** the form stops showing and requiring `subtitle` and starts showing and requiring `tracking_url`

#### Scenario: A broken default

- **WHEN** a `text` entry carries `default_error`
- **THEN** its control is empty and enabled, no deferral checkbox is offered, the error's message is shown, the form is incomplete until a value is typed, and the label then prints

### Requirement: Deferring to the template's default

The print form SHALL offer, for every entry publishing a `default`, a checkbox labelled `Use default: <published default>` whose accessible name is `Use default for <name>`. It SHALL be checked whenever the entry first appears, including when a later list brings the entry in. While checked, the entry's control SHALL be disabled, show the seeded default (an `image` shows none), and its name SHALL be omitted from the submitted `data` and from the list request. Clearing it SHALL enable the control with its seeded value, which is then submitted like any other. Re-checking SHALL discard what was entered, restoring the seeded value and clearing an image chooser's selection. Deferral state SHALL follow the entry across branch changes. Selecting a different template SHALL reset values and deferral from the new template's `inputs.default`, carrying nothing across a shared name.

#### Scenario: A declared default starts deferred

- **WHEN** the form first paints a `title` entry publishing `default: "Untitled"`
- **THEN** its checkbox is checked and reads `Use default: Untitled`, the control is disabled, and submitting sends no `title`

#### Scenario: Re-checking discards the edit

- **WHEN** the operator clears the checkbox, types `Kitchen`, and re-checks it
- **THEN** the control no longer holds `Kitchen` and submitting sends no `title`

#### Scenario: Switching templates carries nothing across

- **WHEN** templates A and B both publish `title`, the operator edits it under A, then selects B
- **THEN** `title` is deferred to B's default and submitting sends no `title`

### Requirement: List editor

The print form SHALL edit a `list` entry as one row per element, in order, each with a single-line text box, a remove control and move-earlier and move-later controls, plus a `+ Add` control appending an empty element. An undefaulted `list` entry SHALL hold `[]` from the moment it appears, so it never makes the form incomplete. The form SHALL submit the elements in row order without trimming, dropping empties or de-duplicating.

The first row's move-earlier and the last row's move-later controls SHALL do nothing, report themselves unavailable to assistive technology, and stay focusable. After a move, focus SHALL follow the moved element; after a removal it SHALL go to the remove control of the row now in that position, else the previous row's, else `+ Add`. Accessible names SHALL carry the entry's `name` and, per element, its position (`tags 2`, `move tags 2 earlier`, `remove tags 2`, `add tags`), and the editor SHALL be a group named for the entry. While deferred, every control in the editor SHALL be disabled and its rows show the published default.

#### Scenario: An untouched undefaulted list submits the empty list

- **WHEN** the form paints `tags: { type: list }` read by an active item and the operator submits untouched
- **THEN** the form is submittable and `data` carries `tags: []`, as does the initial list request

#### Scenario: Edits reach the array

- **WHEN** the editor holds `A`, `B`, `C`, the operator moves `C` earlier, moves `A` later, and appends an element left empty
- **THEN** the submitted `data` carries `tags: ["C", "A", "B", ""]`

### Requirement: Grid columns

A batch grid's columns SHALL be fixed by the template, not by its rows, and SHALL be drawn before any row's list arrives. The Connect grid SHALL show one column per name in the template detail's `inputs.all`, in that order. The Import grid SHALL show the loaded CSV's headers in file order, then every `inputs.all` name the CSV lacks, in `inputs.all` order, omitting every `list` entry. A grid SHALL NOT mark a column or cell by whether a row's list reports its name.

#### Scenario: Import keeps the file's order

- **WHEN** a template declares `title`, `subtitle`, `code` and a CSV with headers `subtitle`, `title` is loaded
- **THEN** the Import grid's columns run `subtitle`, `title`, `code`, and the Connect grid's run `title`, `subtitle`, `code`

#### Scenario: A gate no row satisfies still shows the fields behind it

- **WHEN** `tags` and `location` are read only behind `orientation: horizontal`, and connector rows arrive with no `orientation`
- **THEN** the grid shows `orientation`, `tags` and `location` columns, the `tags` and `location` cells are editable, and no row is refused for them

### Requirement: Grid cells follow each row's own list

Each row SHALL be validated and submitted against the list the service reports for that row; the grid SHALL request the lists for its rows in one request. A cell whose name that row's list omits SHALL stay editable with the control its `inputs.all` entry publishes, SHALL keep its value, and SHALL be neither validated nor sent until the row's values activate the branch that reads it. Grids SHALL NOT seed defaults or offer deferral: a cell holds only what the CSV, the connector or the operator put there.

#### Scenario: Typing the gate's value brings its branch into play

- **WHEN** the operator types a `tags` value while the row's `orientation` is unset, then types `horizontal` into `orientation`
- **THEN** the row is validated against a list reporting `tags`, and submits both `orientation` and the earlier `tags` value

### Requirement: Grid cell controls

Every editable grid cell SHALL show its control at rest, editable without a click or double click, chosen from the entry's `control`:

| `control` | Cell |
|---|---|
| `text` | single-line text box |
| `textarea` | multi-line text box; Enter inserts a line break that reaches the submitted value |
| `select` | `(none)`, the entry's `values`, and the value the cell holds when outside both; nothing else can be entered |
| `checkbox` | tick box cycling unset (indeterminate), checked, unchecked, unset; unset submits no key |
| `integer`, `number` | numeric box with the entry's `min`/`max`, step 1 for `integer` and any for `number`, marked invalid outside the bounds or when not a number; `slider` is ignored |
| `date`, `datetime` | date or date-and-time control |
| `image` | read-only: `image` when it holds a value; the value is still validated and sent |
| `list` | read-only display text of a held array (owned by `connections`), else `—` |
| no entry for the cell | read-only `—` |

While a batch is in flight every cell SHALL render read-only as plain text. Editable cells SHALL draw a bordered box on the surface colour at rest, so they are told apart from read-only cells without hovering, and each cell SHALL expose to assistive technology whether it is editable. Tab and Shift+Tab move between controls; arrow keys and Escape act inside the control only.

#### Scenario: A checkbox cell returns to unset

- **WHEN** the operator activates an unset checkbox cell three times
- **THEN** it reads checked, then unchecked, then unset, and the row then submits no key for it

#### Scenario: A select cell keeps a delivered value

- **WHEN** a CSV puts `enormous` into a `select` cell whose `values` are `small`, `medium`, `large`
- **THEN** the cell offers `(none)`, `small`, `medium`, `large` and `enormous`, holds `enormous`, and the row submits it

### Requirement: Grid cells preserve values their control cannot show

A cell holding a value its control cannot display (a `select` value outside `values`, a `date` not shaped `YYYY-MM-DD`, a `datetime` not shaped `YYYY-MM-DDTHH:MM[:SS]`, a non-numeric `integer`/`number`, a `checkbox` value other than true/false/1/0) SHALL show the control empty or unset beside the raw value as text. A cell whose value contains line breaks SHALL show its first line followed by `+N` for the remaining lines, with the full value as its tooltip. Rendering or focusing such a cell SHALL NOT change the stored value; only an explicit edit replaces it.

#### Scenario: An offset-bearing instant in a date cell

- **WHEN** a `date` cell holds `2026-09-01T12:00:00Z`
- **THEN** it shows an empty date control beside `2026-09-01T12:00:00Z`, and the row submits `2026-09-01T12:00:00Z` unless a date is picked

#### Scenario: A newline is not collapsed

- **WHEN** a `text` cell holds `first`, a newline, `second`
- **THEN** it shows `first` with `+1`, and the row submits the value with its newline unless the operator edits it

### Requirement: Grid row validation

For each entry in a row's list except `list`, the grid SHALL flag the cell with:

| Cell | Message |
|---|---|
| `date`/`datetime`, non-blank, not one of the three datetime forms (owned by `parameters`) or not a real calendar date and time | a message naming the fault, such as `Invalid datetime; use YYYY-MM-DD or YYYY-MM-DDTHH:MM` |
| blank, entry carries `default_error` | the error's message |
| blank, entry `required` | `required` |

The flag SHALL appear as `⚠ <message>` beside the still-operable control. A flagged row SHALL block Print and Download; a run attempted anyway reports `Fix the highlighted rows before running.`, and one attempted while any row's list is pending reports `Resolving row inputs; please wait.` Until a row's list arrives it SHALL be judged against `inputs.default`.

#### Scenario: Two rows on different branches

- **WHEN** one row sets `orientation = horizontal` and another `vertical`
- **THEN** the first is flagged only for a missing `subtitle` and the second only for a missing `tracking_url`, and both columns are editable on both rows

### Requirement: List values in the grids

No grid SHALL edit a `list` value. A Connect row materialized with a multi-valued column mapped onto a `list` parameter (owned by `connections`) SHALL hold that array read-only, submit it unchanged and unflattened whenever the row's list reports the name, and keep it otherwise. A row holding no value for a `list` entry its list reports submits nothing for it, and the run fails per label as rendering reports it.

#### Scenario: A mapped list behind an unsatisfied gate

- **WHEN** a row holds a mapped `tags` array read only behind `orientation: horizontal`, and the operator types `vertical`
- **THEN** the `tags` cell still shows the elements read-only and the submitted `data` carries `orientation` and no `tags`; typing `horizontal` instead submits the array in the connector's order

### Requirement: Connect mapping offers every input

The Connect field mapping SHALL offer one mapping control per name in `inputs.all`, before any row exists, so a mapping can target a name only some branch reads. Mapping validity is owned by `connections`.

#### Scenario: Both branches' names are offered

- **WHEN** a mapping is built for a template whose branches read `subtitle` and `tracking_url`
- **THEN** both names are offered before any row is added

### Requirement: CSV Import screen

The Import screen SHALL parse and edit a CSV in the browser and submit through `POST /api/batch`, never calling `POST /api/import/csv`. A CSV MAY be loaded before a template is chosen: its columns show as plain editable text with no validation, and parameter columns, validation, the preview and the run controls appear once a template is chosen. The loaded rows SHALL survive a template switch, including values in columns the new template does not read. A parameter that no item, condition or attribute reads SHALL get no column.

#### Scenario: A parameter read in one branch is offered for every row

- **WHEN** a template reads `subtitle` only behind `orientation: horizontal` and a CSV row carries `orientation: vertical`
- **THEN** `subtitle` has a column, it is editable on that row, and the row is not refused for it

### Requirement: Template page preview

The template page SHALL preview the template with sample data filled over `inputs.all`, through `POST /api/render/label` for `single` and through `POST /api/batch` with one label in `download` mode for `sheet`. Every `select` entry SHALL get the first of its `values`. Every other entry that is `interpolated` and `required` SHALL get:

| `control` | Sample |
|---|---|
| `text`, `textarea` | the entry's `name` |
| `integer`, `number` | its `min`, else `1` |
| `checkbox` | `false` |
| `date`, `datetime` | the current instant in RFC 3339 with `Z` |
| `image` | a 1×1 PNG data URI |
| `list` | `[<name>]` |

The page SHALL NOT withhold a sample or alter the request to avoid a failure; a failed render SHALL show in the pane as any other failure.

#### Scenario: A sample decides a gate

- **WHEN** a required `integer` `copies` with no `min` is read by an item, and a container gated on `copies: 1` reads `subtitle`
- **THEN** the preview sends `copies: 1` and `subtitle: "subtitle"`, and the gated branch renders

### Requirement: Template page parameters

The template page's Parameters card SHALL list `params` in declaration order. For a parameter declaring a `default:` it SHALL show the declared text and, from `param_defaults`, either `(resolved: <value>)` or the error's message in the error colour. A parameter declaring no default SHALL show neither.

#### Scenario: A broken default shows both

- **WHEN** a parameter declares `default: "{vars.base}"` and the store holds no `base`
- **THEN** the card shows `default: {vars.base}` and the failure's message
