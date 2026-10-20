# Spec Delta

## ADDED Requirements

### Requirement: Labels view restoration

Returning to Labels through navigation or browser Back, or refreshing it, SHALL restore its search, category choice and scroll position within the current browser tab's session.

#### Scenario: Return to filtered Labels

- **WHEN** the operator searches for `bin`, chooses category `Warehouse`, scrolls, opens a template and returns with Back
- **THEN** Labels restores that search, category and scroll position
- **AND** refreshing keeps the same view

### Requirement: Import draft restoration

Returning to Import through navigation or browser Back, or refreshing it, SHALL restore its CSV text, loaded source for Reset, rows and edits, selected template, Copies, start slot, printer, preview-row selection and scroll position within the current tab's session. A new browser-tab session SHALL start without that private draft.

#### Scenario: Resume an edited batch

- **WHEN** the operator edits a loaded CSV batch, selects its run options and preview row, scrolls, navigates to a template and returns to Import
- **THEN** the assembled batch and those choices and scroll position are restored
- **AND** browser Back and refresh also restore the Import view

### Requirement: Shareable view selections

Labels links SHALL reproduce search and category selections, and Import links SHALL reproduce the selected template. The URL's selections SHALL take precedence over remembered selections. CSV contents, row edits and run options SHALL stay out of URLs.

#### Scenario: A shared Import link selects only its template

- **WHEN** an operator opens an Import link selecting template `bin` in a new tab session
- **THEN** `bin` is selected and no private batch is restored

#### Scenario: A Labels link overrides remembered filters

- **WHEN** the tab remembers category `Warehouse` and the operator opens a Labels link selecting category `Shipping`
- **THEN** the view selects `Shipping`
