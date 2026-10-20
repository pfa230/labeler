# Spec Delta

## MODIFIED Requirements

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

### Requirement: Grid columns

A batch grid's columns SHALL be fixed by the template, not by its rows, and SHALL be drawn before any row exists. The Import grid SHALL show the loaded CSV's headers in file order, then every declared parameter the CSV lacks, in declaration order.

#### Scenario: Import keeps the file's order

- **WHEN** a template declares `title`, `subtitle`, `code` and a CSV with headers `subtitle`, `title` is loaded
- **THEN** the Import grid's columns run `subtitle`, `title`, `code`

#### Scenario: Declared lists occupy columns

- **WHEN** a template declares `title` then list parameter `tags` and the CSV has only the `title` header
- **THEN** the grid shows `title` then an initially empty editable `tags` column

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
