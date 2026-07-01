# 38. Print-first landing: grid as the print picker

Date: 2026-07-01

## Status

Accepted. Issues #110, #114, #108.

## Context

Evidence from #108: the landing page (`/`) was management-first. It rendered the template grid, but
each card linked to `/templates/{id}` (the manage/edit detail page). Printing a label was a separate
flow behind the "Print" nav entry, which opened a standalone page whose only content was a template
`<select>` dropdown. The purpose-built visual picker (the thumbnail grid) already existed, but it
pointed at management rather than the primary user intent: printing.

The result was that the most common action (open the app, pick a label, print it) took a nav click to
"Print" plus a dropdown selection, while a rich grid of tappable cards sat on the landing page wired to
the secondary action. #109 had already made `/print/{id}` a linkable, deep-linkable route, so the
picker could point straight at the print form.

#114 separately asked for search by name (the grid search filtered by id only).

## Decision

**One grid at `/`, print-primary.**

1. `/` is the single template grid. There is no second grid and no "manage mode".
2. Tapping a card opens that template's print form: `/print/{encodeURIComponent(id)}`.
3. Management is secondary: a small details icon at each card's top-right corner links to
   `/templates/{id}` (the existing detail/edit page). "New template" stays as a grid button.
4. The standalone `/print` dropdown page is removed. `/print` (no id) redirects to `/`;
   `/print/{id}` remains the print form. A "← All labels" link on the print form is the escape hatch
   and the recovery path from an unknown-id error.
5. Nav shrinks from five entries to four: **Labels** (`/`), Import, Connect, Settings. The separate
   "Print" and "Templates" entries collapse into "Labels".
6. The grid search filters by name or id, case-insensitive (#114).

Card link structure: a relatively-positioned wrapper `<div>` holds two sibling links. the main card
`<Link>` (whole-surface, to the print form) and an absolutely-positioned details `<Link>`. The main
link is not the wrapper, because a nested `<a>` inside an `<a>` is invalid HTML.

## Consequences

- One-tap print entry: opening the app and tapping a card is the first step of the target print
  flows, with no nav detour or dropdown.
- **Intentional loss of cross-template in-progress field preservation.** The old `/print` `<select>`
  kept entered field values when switching templates, because the page did not remount. With the
  select gone, switching templates means navigating back to the grid, which unmounts `PrintForm` and
  discards entered values. This is accepted: the print-first flow optimizes the common case
  (pick → fill → print); cross-template field carry-over was a side effect of the dropdown, not a
  designed feature. The covering test is removed, not replaced.
- **Card accessibility and touch-target requirements.** The details link must be a ≥44px touch target
  (`h-11 w-11`), sit above the card link (`z-10`), show a visible focus ring, and carry a unique
  accessible name per card (`aria-label={`${template.name} template details`}`). The main card link
  carries `aria-label={`Print ${template.name}`}` so its accessible name is unambiguous versus the
  details link. Repeated identical "template details" labels are poor for assistive tech.
- `/print/{id}` deep links are unaffected: they still render the print form directly.
- No SPEC.md change: this is SPA navigation behavior; SPEC documents endpoints and template schema,
  not SPA routing. No backend, API, or OpenAPI changes.

## Alternatives rejected

- **Two grids** (a `/print` picker plus a `/templates` manage grid): duplicate surface for the same
  set of templates, doubling maintenance and confusing which grid does what.
- **Keep `/` = Templates with a beefed-up `/print`**: the primary intent (printing) stays a nav click
  away behind the "Print" entry, leaving the rich picker wired to the secondary action.
- **Long-press to manage**: an undiscoverable gesture for a primary secondary action, and unavailable
  to keyboard and pointer users.
