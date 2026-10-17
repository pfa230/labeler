## MODIFIED Requirements

### Requirement: Preview pane states

The preview pane SHALL show that a render is in progress, the rendered label or sheet, the service's error message when the render fails, or, for a `sheet`, a refusal with no request sent while the batch exceeds the 500-label cap. A failed or refused preview SHALL NOT by itself disable Print or Download; the print form's independent numeric-bound gate still applies.

#### Scenario: A failed render never gates the run

- **WHEN** a preview request returns a non-2xx response with a message and the actions' other availability conditions are met
- **THEN** the pane shows that message and Print and Download stay enabled

### Requirement: Screens show every declared parameter

A screen collecting label data SHALL offer one control per parameter in the template detail's parameter list (owned by `parameters`), in that order except where "Grid columns" orders or omits columns of the Import grid, whatever values are entered: no control appears or disappears as values change. A screen SHALL NOT judge whether a label is complete; Print and Download send it subject to "Print form numeric bounds", and a label the service refuses SHALL be shown with its error, against its row in a grid.

#### Scenario: A branch-only parameter stays on screen

- **WHEN** a template reads `subtitle` only behind `orientation: horizontal` and the operator picks `vertical`
- **THEN** the `subtitle` control stays on screen, and a print with it blank succeeds

#### Scenario: A refused row is marked

- **WHEN** a grid run's row 3 holds `abc`, loaded from a CSV, for an `integer` parameter
- **THEN** the run fails and row 3 shows the `param_value_invalid` failure naming that parameter

## ADDED Requirements

### Requirement: Print form numeric bounds

The print form SHALL show a textual error identifying the violated bound beside each nonblank `number` or `integer` field outside its declared `min` or `max` (`parameters`), with the field marked invalid and its error associated accessibly. Print and Download SHALL be disabled while any such field is invalid, and attempts to activate them SHALL send no action request. Equality with either bound SHALL be accepted. Correcting or clearing a field SHALL remove its bound error; when no field remains outside its bounds, actions SHALL regain availability subject to their other conditions.

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
