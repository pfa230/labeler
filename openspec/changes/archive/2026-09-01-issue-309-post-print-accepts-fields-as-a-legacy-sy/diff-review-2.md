TREE_SHA256: 9cbbfdf1449beae511f3207f191bd4935bc509e10661fb305828ca3676587e99

## Diff review: issue-309 (POST /print drops `fields`)

**Scope checked:** the full working-tree diff (`src/models.rs`, `src/api.rs`, `src/lib.rs`, `ui/src/api/client.ts`, `ui/src/pages/print/PrintForm.tsx`, `ui/src/pages/print/PrintForm.test.tsx`) against `proposal.md`, `specs/print-request-body/spec.md`, `design.md`, `tasks.md`, the issue body at `.agent-runs/issue-309.md`, and AGENTS.md.

**Gates re-run by me, all green [verified]:** `cargo fmt --check` (0), `cargo clippy --all-targets --all-features` (0), `cargo test` (763 passed / 0 failed / 2 ignored), `npm run lint` (0), `npm run test` (49 files, 435 tests passed), `npm run build` (0). `openspec validate <change> --strict` reports valid. `.workflow/specs-digest.sh` recomputes `de4f3d03…`, matching `review.md`'s `SPECS_SHA256`, and `.workflow/review-gate-check.sh . --plan-only` passes.

**Contract check.** `PrintRequest` is now `template`, `printer`, required `data: HashMap<String, Value>`, defaulted `copies`, with `#[serde(deny_unknown_fields)]` (`src/models.rs:1231-1239`); the handler is `LabelInput { data: req.data }` with no fallback (`src/api.rs:2555`); the UI type and form carry exactly the four keys and send `data` (`ui/src/api/client.ts:86-93`, `ui/src/pages/print/PrintForm.tsx:180`). No source outside frozen `docs/` and `docs/adr/` sends `fields` to `/print` [verified by rg across the tree]. Every scenario in `specs/print-request-body/spec.md` has a test behind it, including the two the round-1 reviewer required: `additionalProperties: false` (`src/lib.rs:6712-6715`) and the no-input UI case (`ui/src/pages/print/PrintForm.test.tsx:764-825`).

**Round-1 findings all discharged.** The three "no print job is dispatched" assertions now read a real witness, `GET /api/recent-templates` (`src/lib.rs:6497-6510`, `:6538-6551`, `:6567-6580`); that witness is load-bearing, because a successful `/print` under `build_app()`'s in-memory store and single injected principal populates recents, as `recents_are_recorded_with_local_actor` demonstrates (`src/lib.rs:8992-9017`). Task 2.9 is narrowed and now states truthfully that 2.7 was green before and after (`tasks.md:44-47`). Task 1.2 no longer claims the pre-existing `copies: 101` case (`tasks.md:12-14`). The `types.ts:101` misattribution is corrected in both `design.md:39-42` and `tasks.md:82-83`.

**Redness of the new tests, checked against the pre-change struct by reading:** 2.2 (old code returns `200`), 2.3 (`data` won, `200`), 2.4 (`unwrap_or_default()` gave `{}`, so `422`, not `400`), 2.5 (unknown key ignored, `200`), 2.6 (`copies_invalid`, not `json_malformed`), 2.8 (`data` not required, `fields` present, no `additionalProperties`), UI 4.1/4.2 (`body.data` was `undefined`). Each is red before and green after.

### Findings

**1. `api_print_empty_data_is_passed_to_template`'s failure assertion is looser than the evidence available. (minor, non-blocking)**

`src/lib.rs:6597-6601` asserts `msg.contains("message") || msg.contains("code")` on `failures[0].message`. Both substrings are generic English, and `"message"` is also the JSON key name, so an unrelated render failure whose text happens to contain either word would satisfy it. The failure actually produced is `AppError::missing_field` (`src/errors.rs:215-221`), which sets `code` to `MissingField` and formats `Missing required field '<name>'`, and `BatchFailure` carries that `code` verbatim (`src/batch.rs:107-112`). Asserting `failures[0]["code"] == "MissingField"` alongside the substring would pin the same property without relying on word overlap. Not blocking: the `422` / `BatchInvalid` pair already proves the empty map reached `run_batch`, which is what the scenario at `specs/print-request-body/spec.md:78-86` is for, so the loose clause is redundant reinforcement rather than the load-bearing assertion.

**2. An edit outside the recorded impact list, correct but unlisted. (minor, non-blocking)**

`src/lib.rs:7398` gained `"data":{}` in the `application/problem+json` content-type case. The edit is required (that body previously relied on the removed default and would now `400` instead of reaching the handler for its `404` assertion) and it preserves what the test checks. It is simply absent from `proposal.md:62-64`'s enumeration of test call sites and from task 1.1's list, so the impact record is one line short of the diff. Worth a line in `proposal.md` if the author is touching it anyway; `proposal.md` is outside the digest, so correcting it is free.

### Not raised, having checked

`docs/SPEC.md:1336` still spells `fields`, but it sits under `## Changelog` as a dated historical entry, so the supersession clause's "no other frozen section is superseded" holds. §2.3's error contract table row (`docs/SPEC.md:261`) reads "Malformed JSON or `copies` outside `[1, 100]`", which still describes the endpoint accurately under the widened `json_malformed` definition in `openspec/specs/request-error-envelope/spec.md:73-74`, so the delta's claim that the table is unchanged is right. No scenario in `request-error-envelope` conflicts with the new rejections. `print_label`'s `#[utoipa::path]` still omits `422`; that predates this change and the proposal scopes it out. The comment removed from `ui/src/api/client.ts:87` disappeared with the type it annotated, which is ordinary.

Neither finding must be fixed before this lands.

VERDICT: APPROVE
