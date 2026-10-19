## Why

Labels can filter categories but cannot edit them without opening the full YAML source. Category editing must preserve the surrounding handwritten file, so an ordinary parse-and-reserialize operation does not meet the request.

Source issue: https://github.com/pfa230/labeler/issues/406

Fixes #406

## What Changes

- Add an Edit categories action to Labels cards, with an ordered string-list editor, Save and Cancel.
- Read the stored source and change only its category value, or insert the missing key, through the existing whole-source PUT.
- Support direct lists in block/flow YAML and clearing to `[]`; refuse indirect/tagged category forms with an explanation and a link to the YAML editor.
- Refresh saved category summaries/filter choices and keep failed edits visible without reporting success.

## Capabilities

### New Capabilities

### Modified Capabilities

- `templates`: Labels category editing, source preservation and supported/refused source forms. This domain already owns Labels category controls.

## Impact

`ui/src/pages/Templates.tsx`, source/write queries, a small source-edit helper and a direct `yaml` v2 UI dependency. No new endpoint, Rust parser, template-schema change or storage migration.
