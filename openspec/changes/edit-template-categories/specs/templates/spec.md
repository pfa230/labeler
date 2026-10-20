## MODIFIED Requirements

### Requirement: Labels view categories

The Labels view SHALL offer a category filter of `All`, one choice per category declared by a served template, sorted, and `Uncategorized` while a served template declares none. Choosing a category SHALL narrow the grid to the templates listing it, and `Uncategorized` to those listing none. The filter SHALL compose with the search box, which matches id or name case-insensitively. An empty result SHALL say that nothing matches.

Each template card SHALL offer an Edit categories action that opens the single Labels-level editor with editable string entries, Add, Remove, Save and Cancel. Save SHALL apply "Category source edits"; Cancel SHALL close without writing. A successful save SHALL refresh the Labels summaries and filter choices; if the selected filter's choice disappears, selection SHALL return to All. A failed source read or save SHALL show its error and SHALL NOT report success.

#### Scenario: A template appears under each of its categories

- **WHEN** `pallet` declares `categories: [Shipping, Warehouse]` and `bin` declares none
- **THEN** choosing `Shipping` or `Warehouse` shows `pallet`, and choosing `Uncategorized` shows only `bin`

#### Scenario: An edited category reaches the filter

- **WHEN** the operator changes the only Shipping template's categories to Warehouse and saves from Labels while Shipping is selected
- **THEN** the summaries and category choices show the saved list and the selected filter returns to All
- **AND** choosing Warehouse includes that template

## ADDED Requirements

### Requirement: Category source edits

Editing categories from Labels SHALL accept a single unanchored, untagged block or flow root mapping whose `categories` key is absent or explicitly holds a direct sequence of literal strings, in block or flow style. A direct sequence and its entries SHALL have no anchors, aliases or explicit tags. Unsupported forms SHALL refuse the edit without writing and explain that the YAML editor is required, including a link to the template's YAML editor.

A successful edit SHALL insert the absent key or replace the category value with the entered strings; clearing all entries SHALL write `categories: []`. Bytes outside that insertion or value replacement SHALL remain unchanged, including unrelated comments, formatting and line endings.

#### Scenario: Categories change without rewriting the surrounding file

- **WHEN** the operator saves new categories for supported YAML with an absent key, an inline list, or a block list
- **THEN** the saved source declares the entered category strings and its bytes outside the category edit are unchanged
- **AND** clearing all entries saves an empty list

#### Scenario: A shared category anchor is refused

- **WHEN** `categories` defines an anchor reused by another field, references an alias, or carries an explicit tag, and the operator tries to edit it from Labels
- **THEN** the edit is refused with an explanation including a link to the template's YAML editor, no write is sent and no success is reported
