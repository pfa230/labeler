# Plan review

AUTHOR: claude
REVIEWER: codex
VERDICT: APPROVE_WITH_CHANGES
ROUNDS: 1

## Findings

### Critical

1. The proposed clearing path leaves row selection alive. `design.md:65-68` clears only `selectedConnectionId` and assumes keyed children own all connection-scoped state. In reality, `selected` is owned by `Connect` (`ui/src/pages/Connect.tsx:58`) and passed into the keyed browser (`ui/src/pages/Connect.tsx:105-106`); the existing manual-change path explicitly calls `setSelected([])` (`ui/src/pages/Connect.tsx:67`). Old rows could therefore reappear after another connection is selected, violating `specs/default-connection/spec.md:101-105`.

### Moderate

2. The contract says every newly created connection appears in the picker (`specs/default-connection/spec.md:86-93`), but creation permits `enabled: false` (`specs/connections/spec.md:16-23`) while the picker offers only enabled connections (`specs/default-connection/spec.md:95-97`; `ui/src/pages/Connect.tsx:69`). The disabled-creation case is contradictory.

3. The Settings test plan does not prove the no-request requirement. The delta requires no connection-list request (`specs/connections/spec.md:43-47`), while `proposal.md:54-55` plans only a DOM-absence check. `design.md:110-112` incorrectly claims deleting the fetch stub will make a lingering query fail the test; connection requests run through React Query (`ui/src/api/connectors.ts:59-60`), whose errors are ordinary query state (`ui/src/pages/settings/ConnectionsSection.tsx:243,290-293`). Current Settings tests assert only rendered content (`ui/src/pages/Settings.test.tsx:42-63`), not fetch calls.

4. The design correctly notices that `/connection/i` also matches **default connection**, but incorrectly concludes collapsing the block prevents ambiguity (`design.md:95-99`). The moved control remains labelled `default connection` (`ui/src/pages/settings/ConnectionsSection.tsx:319-324`), and `Connect.test.tsx` has thirteen broad label queries, beginning at `ui/src/pages/Connect.test.tsx:94`. A mounted disclosure’s closed contents remain in the DOM for label-text queries.

## Required changes

1. Amend `design.md` so invalidating a selected connection also clears the parent-owned `selected` array, in addition to setting `selectedConnectionId` to `""`; retain keyed unmounting for browser/composer state. Add or extend a delta scenario and planned test that selects rows on one connection, deletes or disables it, selects another connection, and verifies no prior row selection returns.

2. Qualify `proposal.md` and `specs/default-connection/spec.md`: successful creates and edits update picker options without reload according to enabled state; enabled connections appear, disabled connections do not, and selection remains unchanged except when the selected connection becomes disabled or disappears. Add a disabled-creation scenario.

3. Amend the Settings test plan to retain a fetch spy, wait for the page’s queries to settle, and explicitly assert that no request targeted `/api/connections`; do not rely on an unexpected-query rejection.

4. Amend `design.md` to require changing all existing Connect-page picker queries from `/connection/i` to an exact label such as `/^connection$/i`, while scoping management-control queries separately.

The author applies these changes and NO further review follows.

CHANGES_APPLIED: yes
SPECS_SHA256: ccb596aa5e07161704082e000c489f587c2aa6ddc9e8d2a358636ebdaede431b
