## 1. History read contract

- [x] 1.1 Add the store read, job response model, `GET /api/jobs` handler and OpenAPI registration. Verify with the existing API/store test setup that two users' successful/failed jobs and `local` appear with the specified fields, UTC timestamps, full error and deterministic same-second ordering, plus `[]` for no jobs. The assertions must fail for a caller-only filter, inner join or wrong projection/order. Run existing `token_prints_as_its_owner` and Recent templates coverage to check the untouched attribution/query paths.

## 2. History screen

- [x] 2.1 Add the API type/query hook and Print history page with recorded facts, textual outcomes, readable full failure detail and loading/empty/error/Retry states. Verify through Testing Library that a two-actor success/failure fixture is shown in API order, usernames/local ids and times are visible, failure detail is readable, and Retry performs another read after an error; hiding a field, losing the full detail or treating failure as empty must fail these assertions.
- [x] 2.2 Add the guarded `/print-history` route and navigation link between Import and Settings. Update the existing Shell navigation-order test and exercise the real application route by activating that link; a missing route, wrong href or misplaced item must fail.

## 3. Gates

- [x] 3.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, all UI lint/test/build gates and `npx --no-install openspec validate --all --strict --no-interactive`; retain results in `.agent-runs/`.
