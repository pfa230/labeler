## ADDED Requirements

### Requirement: Print form Cut button

The print form SHALL show a Cut button beside Print only when a successful probe of the selected stored printer reports `cut: true` (`printing`), and no Cut button otherwise. Pressing it SHALL send one cut for the selected printer and report the outcome through the form's print notice: success, or failure with the service's detail.

#### Scenario: Printer supports cut

- **WHEN** a successful probe of the selected printer reports `cut: true`
- **THEN** a Cut button appears beside Print, and pressing it sends `POST /api/printers/{id}/cut` for that printer

#### Scenario: Cut not confirmed

- **WHEN** no printer is selected, the probe is loading or fails, or it reports `cut: false`
- **THEN** no Cut button is shown

#### Scenario: Failed cut is visible

- **WHEN** a cut answers `status: "failed"`
- **THEN** the form shows an error notice with the service's detail
