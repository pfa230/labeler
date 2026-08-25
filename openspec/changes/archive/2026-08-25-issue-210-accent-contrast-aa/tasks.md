## 1. Palette

- [x] 1.1 In `ui/src/theme.css:5-8` (`:root`), set `--accent: #b8420f` and add `--accent-ink: #ffffff`.
- [x] 1.2 In `ui/src/theme.css:10-13` (`.dark`), add `--accent-ink: #16140f`. Leave `--accent`
      `#f0784f` and both `--accent-soft` values as they are.
- [x] 1.3 Delete `--accent-deep` from both blocks.

## 2. Call sites

- [x] 2.1 Repoint `ui/src/components/FormatBadge.tsx:12` from `var(--accent-deep)` to `var(--accent)`.
- [x] 2.2 Replace all nineteen `var(--accent-ink, #fff)` with `var(--accent-ink)` across
      `Login.tsx:62`, `Templates.tsx:330,452,467,486,505,591`, `Catalog.tsx:75,262`, `Connect.tsx:368`,
      `TemplateDetail.tsx:121,205`, `Import.tsx:559`, `Setup.tsx:62`, `PrintersSection.tsx:220`,
      `NewTemplate.tsx:67`, `ConnectionsSection.tsx:202`, `EmptyTemplates.tsx:28`, `PrintForm.tsx:234`.
- [x] 2.3 Confirm `rg 'accent-ink' ui/src` returns no literal fallback anywhere, which is what
      `specs/ui-colour-palette/spec.md` requires of the label colour being a palette property.
- [x] 2.4 Confirm `rg 'accent-deep' ui/src` returns nothing once section 3 is done.

## 3. Tests that name the retired token

- [x] 3.1 Repoint `ui/src/components/FormatBadge.test.tsx:80`, `ui/src/pages/Templates.test.tsx:147,149`
      and `ui/src/pages/TemplateDetail.test.tsx:245,247` to `var(--accent)`.
- [x] 3.2 In `ui/src/setupTests.ts:148`, widen `noBadgeStyling`'s regex from `--accent-deep\b` to
      `--accent\b`. Verify the three assertions it guards still pass: `Catalog.test.tsx:123`,
      `TemplateDetail.test.tsx:224`, `PreviewPane.test.tsx:30`.

## 4. Theme test

- [x] 4.1 In `ui/src/theme.test.ts`, replace the three `accent-deep` references: `REQUIRED:36` takes
      `accent-ink`, `FOREGROUNDS:42` takes `accent`, and the comparison at `:57` compares `accent`
      against `info`.
- [x] 4.2 Add: `--accent-ink` over `--accent` at 4.5:1 or better, in both palettes.
- [x] 4.3 Add: `--accent` over `--surface` at 3:1 or better, in both palettes, the non-text-component
      ratio for the selected template card's border.
- [x] 4.4 Add: `accent-deep` absent from both palettes.
- [x] 4.5 Prove the suite is capable of failing: with 4.1-4.4 in place, temporarily restore
      `--accent: #e4572e` and confirm the accent-text assertions go red, then revert. A green suite
      against the old palette means the assertions are not reading what they claim to.

## 5. ADR

- [x] 5.1 Re-check the next free ADR number against `main` AND against every sibling worktree's
      untracked files, which `main` alone does not show. Done: issue-197 had already taken 0070 from
      the same base, so this change takes 0071. `0067` is an unused gap and stays unused.
- [x] 5.2 Write `docs/adr/0071-one-accent-colour-with-a-defined-ink.md`, Status `Accepted`, issue #210.
      Record: the measured failures, that the darkening is chosen over the viable dark-ink alternative
      and why, that no dark ink reaches AA on `#b8420f` so the light ink must be a light one and
      white is the chosen one among several that pass, and that the one-accent rule is held by review
      rather than by assertion.
- [x] 5.3 Mark it as partially superseding ADR-0066: that record's `--accent-deep` token decision only,
      not its icon, count or border decisions.
- [x] 5.4 Add the new row to `docs/adr/README.md`, and amend ADR-0066's row to
      `Accepted (the --accent-deep token decision superseded by [0071])`.

## 6. Gates

- [x] 6.1 `cd ui && npm run test`.
- [x] 6.2 `cd ui && npm run lint` and `npm run build`.
- [x] 6.3 `cargo fmt`, `cargo clippy --all-targets --all-features`, `cargo test` from the repo root.
      No Rust changes here, but the gates run before any change is reported.

## 7. Look at it

- [x] 7.1 Run the UI (`LABELER_CONFIG_DIR=./config-dev cargo run` plus `cd ui && npm run dev`) and
      screenshot six screens in **both** themes: the template grid with one card selected **and** one
      favourited in the same shot; a template detail page; the catalog with an installed entry;
      Connect with a resource selected; a settings section; and Login. Two states needed setting up
      rather than clicking: the favourite is set through `PUT /api/favorites/{id}` because the star's
      click does not take under CDP, and the catalog's `installed` chip is produced by placing a
      catalog id in the config dir. The catalog's own Install button cannot do it from a Vite dev
      server: fetching the YAML from GitHub is fine (`catalog.ts:1-3`, that host sends
      `access-control-allow-origin: *`), but the POST back to our API is state-changing, and under
      `LABELER_NO_AUTH=true` `middleware.rs:174-178` refuses a state-changing request whose `Origin`
      does not match, which the dev server's port does not. Connect has no grid row selection to
      shoot; see 7.3.
- [x] 7.2 Screenshot the mobile header at a narrow viewport in both themes. It is `md:hidden`
      (`Shell.tsx:118-134`) and no desktop shot covers `Shell.tsx:134`.
- [x] 7.3 Open every shot and check, against intent rather than against "it rendered": primary button
      labels read cleanly on the new fill; the selected card is still obviously selected; the favourite
      star still reads as the accent and not as a neutral; the format badge and the group chip still
      read as two different things. The grid has no selection stripe to check: `select={false}` means
      the vendor never sets `.wx-selected` (see design.md). What the grid does paint in the accent is
      the focused-cell outline and the column-resize grip, via `--wx-color-primary`.
- [x] 7.4 Confirm the deepened orange still reads as this app's accent rather than as a new brand
      colour. If it does not, that is a finding for the human, not a task to silently re-tint around.

## 8. Review and integrate

- [x] 8.1 Adversarial review of the **diff** (separate from the plan review already in `review.md`),
      on a different model, against the issue's acceptance criteria and `AGENTS.md`. agy implemented,
      Claude reviewed the diff, and codex ran three further passes over the artifacts the diff review
      forced changes to; all recorded under "Post-implementation rounds" in `review.md`. It found the
      ADR-0070 collision, an unshot favourite star, and four `--wx-*` accent mappings the artifacts
      wrongly described as live.
- [x] 8.2 `/opsx:archive`, syncing the delta into `openspec/specs/`. Review the archive diff: it
      rewrites `openspec/specs/` after the last review pass.
      Done: the delta synced into `openspec/specs/ui-colour-palette/spec.md` (new capability, 5 ADDED
      requirements, 12 scenarios) and the result was diffed against the delta before the move.
      `openspec validate --all --strict` passes 11/11 with the new capability included.
- [x] 8.3 One commit covering the stylesheet, call sites, tests, ADR, README row and the archived
      change, with `Fixes #210`. Push the branch, wait for a green run, then merge to `main`.
