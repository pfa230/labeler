## 1. Browser baseline

- [x] 1.1 Before dependency edits, use temporary run tooling to capture the built UI's Labels, expanded Template detail, populated/editable Import grid and Settings forms at the two widths and themes in design.md. Capture Tab-focus states on Labels and Settings plus grid focus, and record current resolved ring/outline/border/placeholder styles. Keep scripts, browser/version, fixture data and PNGs in `.agent-runs/` so the same views can be reproduced after migration.

## 2. Tailwind 4 migration

- [x] 2.1 Update package/lockfile to Tailwind 4 and its matching Vite plugin, replace the stylesheet entry, and remove the redundant PostCSS/Tailwind configs and direct pipeline-only dependencies. Migrate the inventoried rounded/outline/flex-shrink usages and preserve actual default-dependent ring/border/placeholder/button behavior, `.dark` tokens and vendor grid overrides; verify the build uses the Vite plugin and the package/config diff contains only this migration.
- [x] 2.2 Repeat the baseline browser captures with identical data/state and produce before/after comparisons plus a written finding for every visual difference; fix migration regressions and retain evidence that layout, grid editors, both themes and keyboard indicators preserve their appearance and colors. Unit tests alone do not complete this task.

## 3. Clean-install gates

- [x] 3.1 Run `npm --prefix ui ci`, UI lint/tests/build and `npx --no-install openspec validate --all --strict --no-interactive`; retain results in `.agent-runs/`. Reuse existing palette/grid tests rather than adding tests that assert class spellings.
