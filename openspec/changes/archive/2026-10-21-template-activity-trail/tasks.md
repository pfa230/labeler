# Tasks

## 1. Persist and read retained activity

- [x] 1.1 Add the additive activity migration, entry/page models, and Store insertion and descending-id cursor reads from design.md. Add Store tests that reopen a file-backed database after 205 inserts and assert exact 100/100/5 pages, exclusive cursors and the null final cursor; these fail on lost history, pruning, wrong order, overlap or unbounded pages.
- [x] 1.2 Add the authenticated `GET /api/activity` route, cursor error mapping, `activity_cursor_invalid` reason and OpenAPI operation/models. Exercise shared reads through the router, and assert the error envelope for invalid cursors; these fail if the read contract or reason mapping is absent.

## 2. Record selected template operations

- [x] 2.1 Wire Principal attribution and the small result-recording helper into create, replace, delete and explicit reload. Extend existing template HTTP tests to assert one entry per selected operation, no entry for internal reloads, user/token-owner attribution and no-auth `actor_id`/`actor_name` both equal to `local`, and classified failed operations including YAML rejection. Use distinctive input text to assert it appears in neither persisted nor returned activity; these assertions fail on missing, duplicate, misattributed or content-bearing entries.
- [x] 2.2 Make activity insertion failure return the specified Internal error. In a file-backed fixture, induce an actual activity-table write failure without a production fault flag, perform a template create, and assert both the 500 message and the created file; this fails if recording errors are ignored or the response incorrectly implies rollback.

## 3. Browse Activity

- [x] 3.1 Add the client types/query, authenticated `/activity` page and Shell navigation entry using the existing UI stack and palette. Add UI tests for a displayed failed action, Older's supplied cursor and Newest's reset, correct final-page controls, and loading/empty/error states; update navigation expectations and add `activity` to the domain list in `AGENTS.md` (`CLAUDE.md` is its symlink). These fail if operators cannot identify the recorded action, reach older history or distinguish a failed read from empty history.

## 4. Integration gates

- [x] 4.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, `npm --prefix ui run lint`, `npm --prefix ui run test`, `npm --prefix ui run build`, and `npx --no-install openspec validate --all --strict --no-interactive`; retain output under `.agent-runs/`.
