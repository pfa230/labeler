# Tasks

## 1. Restore selections and drafts

- [x] 1.1 Connect Labels filters and Import template selection to URL parameters and remember their destinations for shell navigation. Extend existing router/screen tests to exercise Back and explicit-link precedence; assertions must fail on lost selections or a remembered filter overriding the URL.
- [x] 1.2 Persist and hydrate the current Import draft in session storage, including raw CSV text, Reset source, headers, stable row IDs/edits, parse issues, run options and preview selection. Keep normal Load/Reset/template-change mutations authoritative. Extend existing Import tests for navigation/remount hydration and subsequent Reset/submission; assertions must fail on lost edited values/options or a restored draft overwriting a deliberate mutation.

## 2. Restore the visible position

- [x] 2.1 Save and restore Labels/Import shell scroll positions and Import's grid offsets after destination content and initial focus mount. Exercise navigation away/return, Back and reload in a browser with filtered Labels and an edited batch long enough to scroll inside the grid; capture restored selections, row values and both scroll offsets in `.agent-runs/`. Include opening the same link in a fresh tab session to establish that selections are shareable and the private batch is absent.

## 3. Integration gates

- [x] 3.1 Run `npm --prefix ui run lint`, `npm --prefix ui run test`, `npm --prefix ui run build` and `npx --no-install openspec validate --all --strict --no-interactive`; require success before committing implementation.
