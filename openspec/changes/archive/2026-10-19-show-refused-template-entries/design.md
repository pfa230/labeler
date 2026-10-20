## Context

See proposal.md for motivation. `useTemplates` already retains `broken`, but Labels reads only `templates`. The backend returns `{ path, error }`; the client incorrectly requires an obsolete `reason` and makes `error` optional. Listing reads the loaded registry; its GET never rescans disk, and `POST /api/template-registry/reload` is the Labels screen's rescan path. A successful reload can still contain refused entries.

## Goals / Non-Goals

Use the existing registry results and reload API in Labels. The user chose diagnostics and reload with on-disk repairs. Browser source repair, file-management actions, backend changes, polling and a shared diagnostics framework are outside this change.

## Decisions

### Native disclosure above printable results

Default: render one native details/summary section when `broken` is nonempty, with "Refused entries (N)" in its closed summary and a path/error list inside. Place it above the printable result sections and outside their search/category conditions. These entries can include directories and invalid ids, so render plain diagnostics rather than template cards or links to print/detail. Rendering only filtered templates would continue hiding the refusals; a separate diagnostics route would hide the count from Labels.

Use each API `error` directly and fix `BrokenTemplate` to require `path` and `error`, with no compatibility adapter. The existing response omits `broken` when empty, so derive the diagnostic list from `data.broken ?? []`.

For an unfiltered listing with no printable templates and nonempty refusals, show "No printable templates" alongside the diagnostics. Keep the existing EmptyTemplates component for a genuinely empty listing. A local conditional is sufficient; changing that shared component would also affect Import unnecessarily. Filtered zero-result messaging remains owned by Labels view categories.

### Explicit registry reload followed by list refresh

Default: put Reload templates in the Labels header, including when no refusals are present, so external additions/removals can be loaded from the same control. Add one mutation beside the existing query hooks using `sendJson` with an undefined body for the bodyless POST and type its existing response shape as `{ count: number, broken_count: number }`. Await the POST before invalidating/refetching the templates query. A GET alone returns the old registry and cannot restore a repaired file.

Disable the control while its mutation is pending. Report a POST failure through the existing toast/error presentation, leaving the cached list and diagnostics intact. A subsequent GET failure uses the existing list-query error presentation. Do not clear results on activation or infer that a successful POST means every file is valid; display the refreshed listing. No separate success notice or stored copy of registry results is needed.

## Risks / Trade-offs

- Repairs require filesystem access, including fixing invalid filenames or removing non-template entries. This is the user's selected scope; an editor would additionally need to work without a served template detail and still could not repair every refused entry.
- The view reflects the last loaded registry until an operator reloads. The explicit control addresses recovery without adding an automatic refresh mechanism.
