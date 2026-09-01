TREE_SHA256: a95268211ce568bbe6d1432883a7b099ed5c82023cde6bbae21549400570540c

Reviewed the full diff, the four artifacts, and re-ran the gates myself: `cargo test` 763 passed / 0 failed, `npm run lint`, `npm run test` (435 passed), `npm run build` all green [verified]. `specs-digest.sh` recomputes `de4f3d03…`, matching `review.md`'s `SPECS_SHA256`, and `review-gate-check.sh . --plan-only` passes [verified].

The contract change itself is correct and minimal. `PrintRequest` now carries one required `data` plus `deny_unknown_fields` (`src/models.rs:1229-1237`), `print_label` drops the fallback (`src/api.rs:2555`), the UI type and form move to `data` (`ui/src/api/client.ts:86-93`, `ui/src/pages/print/PrintForm.tsx:180`), and no source outside frozen `docs/` and `docs/adr/` still sends `fields` to `/print` [verified by grep]. I checked each new server test for redness against the pre-change struct: `api_print_fields_is_rejected`, `..._alongside_data_...`, `..._neither_...`, `..._unknown_key_...`, `..._missing_data_reports_json_malformed_...` and `openapi_print_request_is_strict` all fail against HEAD's `data.or(fields).unwrap_or_default()` [verified by code reading]. That is the substance of the change, and it is done.

The findings are about claims the artifacts make that the code does not carry.

**1. Three spec scenarios' final assertion is missing, and the tasks claiming it are checked. (blocking)**

`specs/print-request-body/spec.md:95` and `:102` require "AND no print job is dispatched", and `:110` requires "AND no label is printed from an empty map". Tasks 2.2, 2.3 and 2.4 (`tasks.md:25-32`) each restate it ("and that no print job was dispatched" / "and that no label was printed from an empty map") and all three boxes are checked. The tests assert status, `error.code` and `error.details.reason` and nothing else: `src/lib.rs:6492-6501`, `:6518-6522`, `:6534-6538`. No dispatch witness is read.

The behavior is safe (`json_malformed` is produced only by `crate::extract::Json`'s rejection, which runs before `print_label` is entered, so dispatch is entailed), but that is a justification, not an assertion, and CLAUDE.md is explicit that a checked box is a claim the next reader trusts instead of redoing the work. A witness exists and is cheap: `GET /api/recent-templates`, which `recents_are_recorded_with_local_actor` (`src/lib.rs:8955-8960`) already uses to assert emptiness before a print. Either add it to the three tests, or record the entailment against the boxes so the record matches the code.

**2. Task 2.9 records a red run that could not have happened for 2.7. (blocking)**

`tasks.md:44-45` is checked and reads "record that 2.2 through 2.8 fail against the unmodified `PrintRequest`. A test in this group that already passes is not testing the change." `api_print_empty_data_is_passed_to_template` (`src/lib.rs:6583-6606`) posts `data: {}`; against the old struct that deserialized to `Some({})` and `req.data.or(req.fields).unwrap_or_default()` yielded the same `{}`, producing the identical `422` / `BatchInvalid` / `Missing required field 'code'` [verified: `src/errors.rs:215-222`, `src/batch.rs:105-120`, `tests/fixtures/templates/brother_24mm_qr.yaml` has no defaults]. So 2.7 was green before and after. The test still has value as a forward guard against a future "treat empty as something else" shortcut, and `design.md:110-119` argues for it on those terms, but the task's blanket claim is false and no record of the run exists anywhere in the repo. Narrow 2.9 to the tests that were actually red and say why 2.7 is not one of them.

**3. Task 1.2 claims work that was already present. (minor)**

`tasks.md:12-14` says to "extend it to cover `copies: 101` as well as `copies: 0`". `git show HEAD:src/lib.rs` line 6404 already read `for bad in [0u32, 101]`, so only the `copies_invalid` reason assertion was new. The box is checked over a half that needed nothing.

**4. `design.md:39` and `tasks.md:80-81` misname a file location. (minor)**

Both call `ui/src/api/types.ts:101` "the **batch** label type". That line is `TemplateInputsRequest`, the body of `POST /api/templates/{id}/inputs` [verified: `ui/src/api/types.ts:100-102`]. The scoping decision is right either way; the citation is wrong, and `design.md` is freely correctable without touching the digest.

Two things I checked and am not raising: `docs/SPEC.md:1336` still spells `fields` for `/print`, but it sits under `## Changelog` (`docs/SPEC.md:1097`), a dated historical entry rather than a normative section, so the supersession clause's "no other frozen section is superseded" holds; and `print_label`'s `#[utoipa::path]` still omits a `422` response (`src/api.rs:2530-2539`), which predates this change and which the proposal explicitly scopes out.

VERDICT: REVISE
