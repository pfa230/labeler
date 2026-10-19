## Context

See proposal.md for motivation. `Store::record_job` stores `id`, UTC `ts`, template/printer ids, `ok`/`failed`, optional error and `user_id`. Dispatch calls it once per attempted job after the IPP send. The table has no foreign key to users, templates or printers, and `local` is an actor rather than a user row. The actor id does not distinguish a session from its owner's token. `recent_templates` is a separate per-actor grouped query limited to six templates.

`ui/src/app/App.tsx` owns guarded routes, `Shell.tsx` owns primary navigation, and React Query hooks already use `getJson` with the common error/auth behavior. `Shell.test.tsx` asserts the exact navigation order. No history endpoint or page exists.

## Goals / Non-Goals

Expose the existing log through one read endpoint and one page. Do not change dispatch, recording, actor attribution, Recent templates, schema, retention or historical identity snapshots. No search/filter controls, pagination, export, reprint, live printer-state polling, job-detail endpoint or general activity feed is required; #94 owns activity beyond printing.

## Decisions

- **User decisions:** Everyone sees all recorded jobs. Print history is a navigation item between Import and Settings. The existing equal-user auth gate applies; there is no new role or per-user history filter.
- **Routine defaults:** `GET /api/jobs` returns a plain array of the existing rows, ordered `ts DESC, id DESC`, through the existing store/API patterns. A single read of the log serves the issue; no observed log-size failure justifies cursor/query contracts or count queries. Database errors use existing Internal reporting. Register the response schema/path in `src/openapi.rs`.
- LEFT JOIN users only to obtain the current username, retaining the recorded actor id. Do not join away jobs whose associated user, template or printer no longer exists. Historical names cannot be recovered from the recorded ids; display the username when present and the recorded id otherwise (`local` included), with unknown for old empty actor ids. Adding username/template/printer snapshots would change recording and need a migration without supplying missing historical names.
- Convert the recorded timestamp to a UTC RFC 3339 value in the response, and display an unambiguous localized date/time. SQLite's [date/time documentation](https://www.sqlite.org/lang_datefunc.html) confirms that `datetime('now')` stores UTC in `YYYY-MM-DD HH:MM:SS` form. Returning that unqualified string to browser date parsing could show the wrong instant; no stored-data migration is needed.
- Add `useJobs` with the existing client/query pattern and a `/print-history` page within the current auth/shell route tree. Use an ordinary semantic table with contained horizontal scrolling when necessary and native disclosure for long failure details. Textual Sent/Failed outcomes reuse current palette semantics. No new design system or dependency is needed.
- Reuse query loading/error/refetch state for the page and Retry. Read on page entry using existing query freshness behavior; no interval or print-mutation changes. An `ok` row records successful send/acceptance, not later physical completion, so label it Sent to printer rather than Printed.

## Risks / Trade-offs

Current usernames are resolved at read time, and deleted users have only their recorded id. The UI must not invent historical names. An error returned by the service is shown using the existing message handling, while an empty response is a distinct successful state.

Reuse existing store/API and Testing Library setups. Seed jobs from two actors with a same-second tie, a failed job and `local`; assert field values/order so an accidental caller filter, inner join or shortened error fails. UI fixtures must exercise a real failed row and Retry, rather than only asserting page headings. Existing recording/token attribution and Recent templates tests remain the evidence for those unchanged paths.
