## Context

`/templates/:id` (`TemplateDetail.tsx`) holds a header with Use to print and Delete, the thumbnail, a Details list, a Parameters list, Variables used and the Raw YAML editor (`RawYamlSection`). `/print/:id` (`Print.tsx`) is a back link, a heading, loading and error states, and `PrintForm`. A YAML save invalidates the `["template", id]` query, so `useTemplate` refetches and the page receives the new detail. `PrintForm` resets entered values only when the template id changes (`PrintForm.tsx:24-28`), so values already survive a save.

The live preview does not. `useLivePreview` caches renders under `previewKey`, which is the template id, format, data and start slot (`livePreview.ts:14-16`), and its effect runs only when that key changes. After a save with unchanged values the key is unchanged, so the preview keeps the render of the previous version.

## Goals / Non-Goals

**Goals:**
- One page per template at `/templates/:id` carrying the print form, preview and the template's tools.
- A save refreshes the preview without touching entered values.

**Non-Goals:**
- Previewing an unsaved draft. It needs a render endpoint that accepts inline YAML.
- Guarding an unsaved draft against navigation (#143).

## Decisions

**The page lives at `/templates/:id`.** The page is the template's home, not one action on it. The path matches the API resource `/api/templates/{id}` and the sibling routes `/templates/new` and `/templates/catalog`. Catalog's View and `NewTemplate`'s post-create navigation already point there. The `print` and `print/:templateId` routes go from `App.tsx`, with no redirect. An old address falls to the `*` route, which already sends it to `/`. The Labels card's main link moves to `/templates/:id`, and its ⓘ details link is deleted because it now duplicates the main link.

**`TemplateDetail` hosts the merged page; `Print.tsx` is deleted.** `TemplateDetail` already has the header, Delete, Details, Variables used and `RawYamlSection`. It loses the thumbnail, Parameters list and Use to print link, and renders `PrintForm` with `stale={isPlaceholderData}` after the header. Details, Variables used and Raw YAML follow below the form as collapsed `<details>` panels, so the form and preview stay at the top. Hosting it in `Print` instead would move ~300 lines of editor and delete state into it for the same result. The "← All labels" back link moves over from `Print`.

**The preview key carries the template version.** `PreviewInput` gains `version`, included in `previewKey`. `TemplateDetail` passes the template query's `dataUpdatedAt` through `PrintForm`. It is the value the page already uses to version its thumbnail. A save refetches the detail, the version changes, and the preview renders again with the same data. Remounting `PrintForm` on a new version would also refresh the preview, but it discards entered values, which the spec forbids. Clearing the preview cache from the editor would need a channel from `RawYamlSection` into `PrintForm`'s hook, more plumbing for the same effect. A refetch with no save (window refocus) also changes `dataUpdatedAt` and costs one extra preview render. That is acceptable.

**`RawYamlSection` stays keyed by the route param**, for the reason its existing comment gives: `useTemplate` serves the previous detail as placeholder data during a switch.

**Tests move with the code.** `Print.test.tsx` cases for behavior that stays (a control per published parameter, Print gated on a printer, form for the URL's template, error and back link for an unknown id) move to `TemplateDetail.test.tsx` against `/templates/:id`. The `/print` redirect case is deleted with its route. In `TemplateDetail.test.tsx`, the cases for the Parameters list, the thumbnail and Use to print are deleted. "Shows each published default by value" is rewritten to read the default from the form field's hint. "Refetches the thumbnail after a save" becomes **A save reaches the preview**: after a save, the entered value is still in its field and a new preview request goes out. It must fail when `version` is left out of `previewKey`.

## Risks / Trade-offs

- [The page gets longer] → The template panels sit below the form and start collapsed.
- [Bookmarks to `/print/:id` land on Labels] → Accepted. The breaking-change rule allows no second spelling.
