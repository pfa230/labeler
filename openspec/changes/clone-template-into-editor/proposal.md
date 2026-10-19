## Why

Adapting a template currently requires manually copying its source into New template. Clone should create a separately named copy immediately and open that copy in the existing YAML editor.

Source issue: https://github.com/pfa230/labeler/issues/144

Fixes #144

## What Changes

- Add immediate Clone to template detail, generating `<id>-copy`, then numbered suffixes on filename conflicts, with a corresponding distinct display name.
- Preserve the original and all copied source outside the display-name value, including the description and comments.
- Create through the existing source/create endpoints, then open the copy's detail page with its YAML editor active; display source/create failures on the original page.

## Capabilities

### New Capabilities

### Modified Capabilities

- `ui`: Template page clone action and entry into the copy's YAML editor.

## Impact

`TemplateDetail.tsx`, the existing source/create queries and a name-value source edit using the yaml dependency planned in #406. No clone API, id schema, description rewrite or naming dialog.
