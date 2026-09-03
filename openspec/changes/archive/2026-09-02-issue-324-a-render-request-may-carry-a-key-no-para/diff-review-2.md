TREE_SHA256: 16c6c4579fbfeccc33bc8da4e87d05de21bdf7453f585f8ae5f65d72fbc52847

## Diff review round 2 — issue-324

**Round 1 findings, re-checked.** All five are addressed: `tasks.md:46` (6.2) now records a test per restated scenario and corrects the false "already covered" premise [verified against `src/lib.rs:1822,1840,1870,8527,8688`]; `tasks.md:52` (7.2) now scopes the fail-before/pass-after claim and names 4.3/4.8/4.9/6.3 as preserved-behavior guards; `src/lib.rs:8498` now asserts `request_body_invalid` positively (`src/lib.rs:8501`); the missing precedence scenario has a test (`issue_324_6_4_…`, `src/lib.rs:8778`); `_ => unreachable!()` is gone, replaced by a local `RenderFormat` enum matched exhaustively (`src/api.rs:2652-2688`).

**What I verified as sound.** `unknown_param_names` (`src/render/mod.rs:150-160`) reads only `params` and sorts `String`s, which is code-point order. The CSV shortcut of reading `parsed_rows.first()` (`src/api.rs:2739`) is safe: the `csv` reader is non-flexible, so a record whose length differs from the header is already `csv_row_invalid` at `src/api.rs:2256`, and `parse_csv_rows` refuses an empty file at `2270`, so row 0's non-`option.` keys are exactly the header's data columns. Ordering holds on every path: query validation precedes the key check on `/render/label` (`src/api.rs:2656-2688`), the label cap precedes the loop (`src/batch.rs:56`), and the file's own refusals precede the column check (`src/api.rs:2726-2761`). No unchecked render path takes caller data: `thumbnail` (`src/api.rs:1234`) takes no body, and `useTemplatePreview` keys its body off `sampleData(detail.inputs.all)` (`ui/src/lib/preview.ts:14`), so every key is a declared param. Every UI submit and preview path funnels through `pruneDataForSubmit`, which is a whitelist against the input list (`ui/src/lib/labelInputs.ts:246-249`), so no screen can send an unrecognized key. `specs-digest.sh` recomputes `f813d445…`, matching `review.md`, so `specs/` is unedited since the plan verdict. `avery5163_asset_tag` has 10 slots per page, so `issue_324_4_5`'s index-10 label really is on page 2. Gates: `cargo fmt --check` clean, `cargo clippy --all-targets --all-features` clean, `cargo test` 806 passed / 0 failed [verified].

### Findings

**1. `tasks.md:29` and `tasks.md:37` are checked over an assertion neither test makes. (blocking)**

Task 4.6 reads "…carrying the same code and reason, **and dispatches no print job**". `issue_324_4_6_print_reports_per_copy` (`src/lib.rs:8384`) stops at the response: it asserts the 422, the three indices and their code/reason (`src/lib.rs:8404-8413`) and never checks that nothing reached the printer. Task 5.2 reads "…is the same `400`, **and no print job is dispatched**"; the 5.2 block (`src/lib.rs:8578-8592`) asserts only status, code and reason.

Both spec scenarios name that clause as the requirement, not as commentary: `request-data-keys` — "A print request dispatches nothing … no print job is dispatched"; `batch-validation` — "AND no print job is dispatched for any copy". A 422 body proves no `BatchSummary` was returned; it does not prove no job was sent, because a job sent before the failure would still return the same 422. That distinction is exactly what the scenario was written to pin.

It is observable and the repo already has the idiom. `record_job` fires for both `ok` and `failed` transports (`src/api.rs:2459,2466`) and feeds `GET /api/recent-templates` (`src/api.rs:2853`); `recents_are_recorded_with_local_actor` uses "recents empty before any print" as an assertion at `src/lib.rs:8019`-style form (`src/lib.rs:10019`). Adding that read after the 422 in 4.6 and after the 400 in 5.2 gives a test that fails if a job is ever dispatched.

This is the round-1 defect class, unfixed in a second place: AGENTS.md says a checked box is a claim the next reader trusts instead of redoing the work, and "a task saying to add an HTTP test is not satisfied by a unit test one layer below the status code". `tasks.md` lands permanently under `openspec/changes/archive/`.

**2. `tasks.md` group 6 does not record `issue_324_6_4_…`. (minor)**

The test added for round-1 finding 4 (`src/lib.rs:8778`) is the only coverage of `batch-validation`'s "A label failing two ways contributes one entry" scenario, and no task names it. Group 6 already carries a coverage record at 6.2, so the archived list reads as though that scenario is untested.

**3. `issue_324_4_3` uses a weaker template than its scenario. (minor)**

The scenario is "`subtitle` is read only inside a container gated on `orientation: vertical`, and a label selecting `orientation: horizontal` carries both". The test's template (`src/lib.rs:8252`) declares `subtitle` and no item reads it anywhere. Both cases are "declared but absent from this label's input list" (`derive_inputs_internal` collects only referenced names, `src/templates.rs:367-374`), and the check is a plain `params` lookup, so behaviour is identical and nothing is hidden. It is a naming mismatch between the scenario and the test, not a coverage hole.

VERDICT: REVISE
