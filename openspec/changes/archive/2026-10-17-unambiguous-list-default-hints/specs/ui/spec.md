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
| `list` | editor of text elements the operator can add, remove and reorder | not shown |

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
