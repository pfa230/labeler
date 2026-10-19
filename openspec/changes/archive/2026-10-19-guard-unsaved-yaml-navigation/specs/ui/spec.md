## ADDED Requirements

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
