# Design

## Context

At `3dddfb20`, Labels filters and all Import draft state are route-local. Import distinguishes the textarea from `loadedSource` used by Reset and holds edited rows with stable IDs. `Shell` scrolls its `<main>` element rather than the window. The app uses `createBrowserRouter`/`RouterProvider` over shared `routes.tsx`, with the existing `UnsavedYamlGuard` implementing the Unsaved YAML navigation requirement. See the proposal and UI delta for scope.

## Goals / Non-Goals

**Goal:** Restore the current assembled screen, with URL selections and tab-private data stored in their respective places.

**Non-goal:** Create snapshots of every edit or restore server/query responses as saved application state.

## Decisions

- User decision: current-tab restoration only, including navigation, Back and refresh; no application persistence across browser sessions. Links carry Labels filters and Import's template selection, while CSV data stays out of URLs. The user accepted this policy during scoping on 2026-10-19.
- Default: use `sessionStorage` for one current draft per screen and remembered navigation targets. It already has the selected tab-session lifetime and survives reload: https://developer.mozilla.org/en-US/docs/Web/API/Window/sessionStorage. A component-only context loses refresh; `localStorage` persists beyond the chosen lifetime. Keep a small screen-specific helper, without a state library, versioned migration framework or alternative store.
- Default: use `q` for Labels search, `category` for a named category, `uncategorized=1` for the distinct Uncategorized choice, and `template` for Import's template ID. All/blank selections omit their keys. This preserves categories literally named `All` or `Uncategorized`. Use existing router search-parameter APIs and replace the current history entry when editing selections. Shell links use the remembered screen URL so ordinary navigation returns to the saved selection; direct/history URLs take precedence. Back selects its destination view using that URL and the screen's current tab draft, without turning field edits into undo history.
- The Import draft contains the textarea text, loaded Reset source, CSV headers, rows with their stable IDs and held values, parse issues, Copies, start slot, selected printer and preview-row ID. Persist committed changes, including grid edits through the existing `commitRows` path, rather than saving only on unmount. Initialize once on mount; normal Load/Reset and template-change handlers continue to own their mutations. Source inspection found that current Import preserves rows through template switches; restoration must not introduce a remount reset. Transient busy flags, previews, notifications and prior run annotations are recomputed or cleared rather than stored as the draft.
- Keep Labels/Import scroll coordinates with the view URL. Save the actual Shell main-scroll position and Import grid offsets; the grid has its own fixed 350px viewport. Use the existing grid API's scroll state and `scroll-to` action for grid restoration: https://docs.svar.dev/react/grid/api/actions/scroll-to/. Restore after destination data, rows and initial focus have mounted; restoring window scroll alone misses both visible scrollers. Keep this scoped to these screens.
- Extend existing screen/router tests for navigation and Back with URL precedence and an edited batch. Remount with the same session storage to test draft hydration. Exercise actual reload and main-scroll restoration in a browser, because remount-only tests cannot prove browser navigation or scroll geometry. Checks must fail on lost edits/options, a URL overriding the wrong selection, or a restored scroll offset applied before the screen has content.

## Risks / Trade-offs

Restoration combines the current tab draft with the destination URL's selections. It does not retain past versions of a batch. The browser owns the page-session lifetime; the application adds no cross-session draft store.

## Migration Plan

No server data migration. Add the tab draft and URL behavior through the loop after explicit enrollment.
