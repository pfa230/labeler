## MODIFIED Requirements

### Requirement: Template page

The template page SHALL hold the template's print form and preview, its dimensions, unit and DPI, the variables it reads, its YAML source with an editor that saves it (`templates`), and an action that deletes it (`templates`). A successful save SHALL leave the form's entered values in place, and the preview SHALL then show the saved template.

Clone SHALL immediately read the template's source and create a copy through template create (`templates`), trying `<id>-copy` with name `<name> (copy)`, then `<id>-copy-2` with name `<name> (copy 2)` and successive integers when create reports a filename conflict. The copied source SHALL differ only at the `name` value; the original SHALL remain untouched. Success SHALL open the copy's detail page with its existing YAML editor active. Source or non-conflict create failures SHALL show an error on the original page without reporting success.

#### Scenario: A tokened default is shown by value

- **WHEN** a parameter declares `default: "{vars.qr_base_url}"` and the store holds `qr_base_url = https://ex.co/`
- **THEN** the page shows `https://ex.co/` as that parameter's default

#### Scenario: A save reaches the preview

- **WHEN** the operator has typed `A` into `title`, then edits the template's YAML and saves it
- **THEN** `title` still holds `A`
- **AND** the preview is rendered from the saved template

#### Scenario: Clone starts an independent copy in the editor

- **WHEN** the operator clones `pallet`, named Pallet, whose label text is the literal `HELIX`, and no copy id exists
- **THEN** `pallet-copy` is created as Pallet (copy), its YAML editor is active, and its unedited label renders identically
- **AND** the original source and all copied source outside the name value, including the description and comments, remain unchanged

#### Scenario: A taken copy id advances the suffix

- **WHEN** creating `pallet-copy` reports a filename conflict
- **THEN** Clone tries `pallet-copy-2` named Pallet (copy 2), continuing to the next integer on further filename conflicts
- **AND** no occupied id is overwritten

#### Scenario: A failed clone stays on the original

- **WHEN** fetching the source or creating a copy fails for a reason other than a filename conflict
- **THEN** the original page shows the error, does not navigate to a copy and does not report success
