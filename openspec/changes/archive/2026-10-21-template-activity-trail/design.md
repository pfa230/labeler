# Design

## Context

See proposal.md for motivation. `src/api/templates.rs` owns all four selected handlers. Create and replace parse YAML, publish files and reload the registry; delete removes a file, prunes favorites and reloads. A later step can fail after a file changes, so a failed response already does not mean an unchanged filesystem.

`Store` owns SQLite and its sequential migrations. `Principal` already supplies the user id and username or the no-auth identity; tokens act as their owner. For no-auth activity, use `actor_id: local` and `actor_name: local`, matching the existing auth contract and `/api/auth/me`, rather than introducing a second spelling for that identity. Existing `jobs` rows feed Recent templates. `AppError` exposes status, code and reason separately from its message and arbitrary details, which can contain submitted text. The UI uses BrowserRouter, a shared Shell, TanStack Query and `getJson`.

## Goals / Non-Goals

**Goals:** Attribute the selected template operations, persist their response outcomes, and make the retained history readable with bounded requests.

**Non-Goals:** No generic HTTP logger, event bus, filesystem/SQLite transaction scheme, token-level attribution, retention scheduler or new navigation framework. Proposal non-goals cover the unselected event families.

## Decisions

### Record the accepted template action set

The user selected template create, replace, delete and explicit registry reload. Create identifies who introduced a definition; replace attributes changed label behavior; delete attributes its removal; explicit reload identifies who activated files changed outside the API. A write's internal reload is part of that write, not another selected action. Authentication/origin rejections and requests that do not reach a selected handler are not completed template operations.

Use the existing four handlers with one small shared recording helper. Evaluate each handler's operation into a result before recording so early validation and filesystem errors are included. Read actor attribution from Principal. The helper stores the response outcome and, on failure, code/reason from AppError, then returns the original result when insertion succeeds. Do not serialize response bodies or wrap the whole router: the four known handlers already expose the required facts, and response parsing would re-read data that must not be retained.

### Separate retained records from print jobs

Add one activity table in the existing migration sequence and Store insertion/listing methods. Its columns mirror the activity entry contract; use an INTEGER PRIMARY KEY, a UTC timestamp, actor id and username snapshots, action, target, outcome and nullable error code/reason. Actor snapshots have no cascading user foreign key: the current user-delete operation must not erase attribution. No full error message/details column is needed: AppError's code/reason distinguish validation, absence, conflicts and internal failures without copying parser input.

The user chose to retain all entries. There is no pruning or maximum count. Reusing jobs would mix template mutations into the existing Recent query and existing print history, so this is a separate table. SQLite documents that [INTEGER PRIMARY KEY aliases the rowid](https://www.sqlite.org/rowidtable.html); with no deletion path, AUTOINCREMENT's deleted-id reuse protection adds nothing this history requires.

### Bounded newest-first reads

Routine default: `GET /api/activity?before_id=<id>` uses descending id order, exclusively below the supplied id. Omit the cursor for the newest page. Query at most 101 rows, return at most 100, and use the extra row only to decide whether `next_before_id` is the last returned id or null. This avoids a total-count query and prevents an Older button that points to an empty page. Returned cursors name retained entries, so new inserts do not shift older pages as offsets would.

Expose one ActivityEntry model and an ActivityPage containing entries and next_before_id; register the authenticated route and models in OpenAPI using the repository's registration structure at implementation time. Shared history follows existing equal-manager authentication; no actor filter is introduced. Decode a supplied cursor through the error envelope with the one new `activity_cursor_invalid` reason. Use an optional signed 64-bit query field, map its typed-query rejection into AppError and reject nonpositive values; the default rejection response would bypass the error envelope.

### Propagate a recording failure

The user selected an explicit `500 Internal` response if insertion fails, even after a successful file change. Its message says activity could not be recorded and the requested change may already have happened. Use the existing Internal constructor and server-side cause logging; do not introduce a retry queue, rollback protocol or synthetic second activity entry. If the operation was already failing, a recording failure still produces this Internal response. A successfully stored entry describes the operation result before any recording failure.

### A small Activity screen

**User decision:** Place Activity after Print history and before Settings: Labels, Import, Print history, Activity, Settings. Add `/activity` under the existing RequireAuth/Shell. Use the existing client/query conventions and palette with a simple semantic table. Display time, actor name, a human-readable action label, target, textual success/failure and the failure's stable code/reason. Keep cursor state in the page: Older follows next_before_id and Newest clears the cursor. No page stack, filters, live polling, persisted browsing state or additional result detail model is needed to reach all retained entries. Render loading, empty and failed reads explicitly.

## Risks / Trade-offs

- A failed request can already have changed files, including when recording fails. The selected error message exposes this limit; no cross-storage atomicity is promised.
- Retaining all entries grows the database. This is the user's chosen policy; each read remains bounded, and this change adds no maintenance control.
- Failure code/reason is less detailed than a parser message. This deliberately satisfies classification while excluding request secrets from persisted history.

## Migration Plan

Append an additive SQLite migration; do not backfill actions that were never recorded. Deploy with an initially empty Activity view and retain existing jobs. No user data rewrite or rollback migration is required by this change.
