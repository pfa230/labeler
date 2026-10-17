## MODIFIED Requirements

### Requirement: Template page

The template page SHALL hold the template's print form and preview, its dimensions, unit and DPI, the variables it reads, its YAML source with an editor that saves it (`templates`), and an action that deletes it (`templates`). A successful save SHALL leave the form's entered values in place, and the preview SHALL then show the saved template.

#### Scenario: A tokened default is shown by value

- **WHEN** a parameter declares `default: "{vars.qr_base_url}"` and the store holds `qr_base_url = https://ex.co/`
- **THEN** the page shows `https://ex.co/` as that parameter's default

#### Scenario: A save reaches the preview

- **WHEN** the operator has typed `A` into `title`, then edits the template's YAML and saves it
- **THEN** `title` still holds `A`
- **AND** the preview is rendered from the saved template
