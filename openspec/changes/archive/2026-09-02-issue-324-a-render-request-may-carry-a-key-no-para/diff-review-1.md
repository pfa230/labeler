TREE_SHA256: d184ce601aa58cdc92b2800d483f73178d6f66c1c9cd6c6d397122ca1f7a31a8

## Diff review — issue-324

**What I verified as sound.** The four call sites match the plan: `validate_label_data_keys` runs after query validation and before the render on `POST /api/render/label` (`src/api.rs:2670`), at the top of `render_single_batch`'s loop (`src/batch.rs:94-104`) and of `render_sheet_pages`' loop (`src/render/mod.rs:998-1008`), each pushing a `BatchFailure` and continuing exactly as the existing render-failure arms do, so every failing label is still listed. The CSV check sits after the `option.` check and before labels are built (`src/api.rs:2737-2761`); reading only `parsed_rows.first()` is safe because the `csv` reader is non-flexible, so an unequal-length row is already `csv_row_invalid` and row 0's keys are the header's non-`option.` columns. `unknown_param_names` sorts `String`s, whose byte order is code-point order, so the "ascending" contract holds. `SPECS_SHA256` recomputes to `f813d445…`, matching the approving `review.md`, so `specs/` was not edited after the verdict. All three gates pass: `cargo fmt --check`, `cargo clippy --all-targets --all-features` clean, `cargo test` 805 passed / 0 failed [verified]. Callers of the widened `render_sheet_pages` are all updated; no server-built label path (thumbnail, template preview via `sampleData`) and no UI submit path reaches the check unpruned.

### Findings

**1. `tasks.md:46` — 6.2 is checked, its recording clause is unperformed, and its coverage claim is false. (blocking)**
The task reads "Record which existing test covers each." No such record exists anywhere in the change folder or the diff. Worse, the premise is wrong: 6.2 asserts the three restated scenarios are "already covered by existing tests", but the label-cap `413 BatchTooLarge` had no existing test. `batch_oversized_body_is_413` (`src/lib.rs:7107`) tests the ~2 MiB `DefaultBodyLimit`, not `max_labels`; the only label-cap test is the one this change added, `issue_324_4_9_batch_admission_cap_precedes_data_key_validation` (`src/lib.rs:8527`). `tasks.md` lands permanently under `openspec/changes/archive/`, and AGENTS.md is explicit: a checked box is a claim the next reader trusts instead of redoing the work, so check one only after performing it.

**2. `tasks.md:52` — 7.2 is checked, but three added test functions cannot fail pre-change. (blocking)**
7.2 claims "every test added in groups 4, 5 and 6 fails against the pre-change behaviour". `issue_324_4_3_…` (`src/lib.rs:8252`, asserts a render succeeds), `issue_324_4_8_…` (`8502`, asserts `format_unknown`) and `issue_324_4_9_…` (`8527`, asserts `413`) all assert behaviour this change leaves unchanged, so each is green before the check exists. They are correct guard tests, demanded by tasks 4.3, 4.8 and 4.9; the task list contradicts itself and the box as checked states something untrue. Fix by scoping 7.2's claim to the tests that assert new behaviour and naming the guard tests as the exception.

**3. `src/lib.rs:8498` — the negative half of 4.7 asserts too little. (should fix)**
`assert_ne!(body["error"]["details"]["reason"], "data_key_unknown")` passes if `reason` is absent altogether, or if the resolution failure regressed to any other slug. The spec scenario requires the same label without the stale key to "still report the resolution failure unchanged", and the concrete value is available: an uncoercible integer returns `Reason::RequestBodyInvalid` (`src/render/mod.rs:325-330`). Assert `request_body_invalid` positively.

**4. A named `batch-validation` scenario has no test. (should fix)**
"A label failing two ways contributes one entry ... a label both carries an unrecognized key and omits a required parameter" is untested. `issue_324_6_1_…` (`src/lib.rs:8688`) puts the two defects on *different* labels, and 4.7 uses an uncoercible value rather than an omission. The precedence between the key check and `MissingField` on one label is therefore asserted nowhere.

**5. `src/api.rs:2681` — `_ => unreachable!()` trades a compile-time guarantee for a runtime panic. (minor)**
The handler previously matched `query.format` once, exhaustively, with the render in each arm. It now matches a `&str` the same function produced four lines earlier, so a third format added at `src/api.rs:2661` without a matching arm below becomes a 500 panic rather than a compile error. Binding a small enum instead of a `&str` keeps the two matches coupled and removes the `unreachable!`.

VERDICT: REVISE
