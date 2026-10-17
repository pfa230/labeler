## 1. Preview version

- [ ] 1.1 Add `version` to `PreviewInput` and `previewKey`, and pass the template query's `dataUpdatedAt` from `TemplateDetail` through `PrintForm` into `useLivePreview`; verified by 2.4.

## 2. Merged page

- [ ] 2.1 In `TemplateDetail`, delete the thumbnail, Parameters list and Use to print link, add the "← All labels" back link, render `PrintForm` with `stale={isPlaceholderData}` after the header, and move Details, Variables used and Raw YAML below it as collapsed panels; verify with the tests in 2.3 and 2.4.
- [ ] 2.2 Remove the `print` and `print/:templateId` routes from `App.tsx`, delete `Print.tsx`, point the Labels card's main link at `/templates/:id` and delete its ⓘ details link; verify `Templates.test.tsx` passes with the card destination updated and the ⓘ link assertion deleted.
- [ ] 2.3 Move the `Print.test.tsx` cases for a control per published parameter, Print gated on a printer, the form for the URL's template, and an unknown id into `TemplateDetail.test.tsx` against `/templates/:id`, then delete `Print.test.tsx`; in `TemplateDetail.test.tsx` delete the Parameters list, thumbnail and Use to print cases, and make the default-by-value case read the form field's hint; verify `npm --prefix ui run test` passes.
- [ ] 2.4 Add a `TemplateDetail.test.tsx` case for **A save reaches the preview**: type into a field, save edited YAML, then assert the field keeps its value and a new preview request goes out after the save; verify it fails with `version` removed from `previewKey` and passes with it.

## 3. Gates

- [ ] 3.1 Run the cut pass on the diff, naming each new or moved test's failure condition; verify `npm --prefix ui run lint`, `npm --prefix ui run test`, `npm --prefix ui run build` and `npx --no-install openspec validate --all --strict --no-interactive` pass.
