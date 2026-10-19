## ADDED Requirements

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
