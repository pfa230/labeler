## ADDED Requirements

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
