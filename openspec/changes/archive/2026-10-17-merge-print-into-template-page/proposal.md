Fixes #468

## Why

The operator usually edits a template while printing with it, but the template page (`/templates/:id`) and the print page (`/print/:id`) are separate, so every YAML fix means leaving the form and its preview and coming back. The print form already shows every declared parameter and a live preview, which makes the template page's Parameters list and thumbnail repeats.

## What Changes

- The template page `/templates/:id` gains the print form and its preview, and keeps its details (format, dimensions, unit, DPI), Variables used, the raw YAML editor and Delete.
- Saving YAML keeps the entered values, and the preview shows the saved template.
- **BREAKING**: `/print/:id` is removed. The Labels card opens `/templates/:id`, and its ⓘ details link goes.
- **BREAKING**: the template page's Parameters list, thumbnail and Use to print link are removed.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `ui`: **Template page** holds the print form and preview, the template's details, source editor and delete, and defines what a save does to the form and preview.

## Impact

- `ui/src/pages/TemplateDetail.tsx`, `ui/src/pages/print/PrintForm.tsx`, `ui/src/lib/livePreview.ts`: merged page and a preview keyed on the template version.
- `ui/src/pages/Print.tsx` and its test are removed; their tests move to the template page.
- `ui/src/app/App.tsx`, `ui/src/pages/Templates.tsx`: routes and the card link.
- No API change.
