## 1. Move the incidental `/print` tests onto `data`

These tests post `fields` while testing something else. They pass before and after the contract
change, because `data` is already accepted today; moving them first keeps the later red-then-green
steps about the removal alone.

- [x] 1.1 In `src/lib.rs`, change the `fields` key to `data` in the `/print` payloads of
      `print_webhook_ok_single_template_jobs_equal_copies`, `print_webhook_defaults_to_one_copy`,
      `print_webhook_copies_out_of_range_is_400`, `print_webhook_unknown_template_is_404`,
      `print_webhook_oversized_body_is_413`, `print_webhook_requires_auth`, and
      `recents_are_recorded_with_local_actor`, leaving every assertion otherwise unchanged.
- [x] 1.2 Add `error.details.reason` is `copies_invalid` to
       `print_webhook_copies_out_of_range_is_400`, which today asserts only the status and
       `error.code` (the `copies: 101` case was already covered).
- [x] 1.3 Run `cargo test` and confirm 1.1 and 1.2 pass against the unmodified `PrintRequest`.

## 2. Write the server tests for the new contract, red first

Every test in this group must fail against the unmodified `PrintRequest`; that is what proves it is
testing the removal rather than passing for an unrelated reason. Each refusal asserts the status and
`error.details.reason`, never the status alone.

- [x] 2.1 Delete `api_print_accepts_data_or_fields` and `api_print_data_precedes_fields`. They exist
      only to pin the synonym, and the tests below replace their coverage.
- [x] 2.2 Add a test posting `fields` in place of `data` to `/api/print`, asserting `400`,
      `error.code` `InvalidRequest`, `error.details.reason` `json_malformed`, that
      `error.details.error` names `fields`, and that no print job was dispatched.
- [x] 2.3 Add a test posting a body carrying both `data` and `fields`, asserting `400`,
      `error.code` `InvalidRequest`, and that no print job was dispatched.
- [x] 2.4 Add a test posting `{"template":…,"printer":…,"copies":1}` with neither key, asserting
      `400`, `error.code` `InvalidRequest` and `error.details.reason` `json_malformed`, and that no
      label was printed from an empty map.
- [x] 2.5 Add a test posting `template`, `printer`, `data` and one key the contract does not list,
      asserting `400`, `error.code` `InvalidRequest`, and that `error.details.error` names that key.
- [x] 2.6 Add a test posting a body that omits `data` and carries `"copies": 0`, asserting `400` with
      `error.details.reason` `json_malformed` rather than `copies_invalid`.
- [x] 2.7 Add a test posting `{"template":"brother_24mm_qr","printer":"ok-printer","data":{}}` against
      a registered fake printer, asserting `422`, `error.code` `BatchInvalid`, and that the reported
      failure names a parameter the empty map did not supply — the outcome only reachable if the
      empty map was carried into template processing.
- [x] 2.8 Add an OpenAPI test over the generated `PrintRequest` schema asserting that `data` is among
      its required properties, that it declares no `fields` property, and that its
      `additionalProperties` is `false`, following the `openapi_schema_contains_param_types` pattern.
- [x] 2.9 Run `cargo test` and record that 2.2, 2.3, 2.4, 2.5, 2.6 and 2.8 fail against the unmodified
       `PrintRequest`; 2.7 (`data: {}` → `422 BatchInvalid`) was already green because the old
       `PrintRequest` deserialized `data: {}` to `Some({})` and `unwrap_or_default()` yielded the same
       empty map, so its outcome did not change.

## 3. Change the print request contract

- [x] 3.1 In `src/models.rs`, replace `PrintRequest`'s `data: Option<HashMap<..>>` and
      `fields: Option<HashMap<..>>` with a single required `data: HashMap<String, serde_json::Value>`
      carrying neither `Option` nor `#[serde(default)]`, and add `#[serde(deny_unknown_fields)]` to
      the struct. `template`, `printer` and `copies`, including `default_print_copies`, are unchanged.
- [x] 3.2 In `src/api.rs`, replace `req.data.or(req.fields).unwrap_or_default()` with `req.data`. Add
      no handler-side check for `fields` or for unknown keys: the deserializer holds that rule, and
      `crate::extract::Json` already maps its rejection to `400` / `InvalidRequest` /
      `json_malformed`.
- [x] 3.3 Run `cargo test` and confirm every test from groups 1 and 2 now passes.
- [x] 3.4 Confirm `src/openapi.rs` needs no edit: `PrintRequest` is already registered, and 2.8 is
      what proves the generated document followed the struct.

## 4. Move the UI onto `data`

- [x] 4.1 In `ui/src/pages/print/PrintForm.test.tsx`, change the three assertions reading
      `body.fields` (the tape-print body, the second body assertion, and the helper that returns the
      captured `/api/print` body) to read `body.data`, and add to each an assertion that the body has
      no `fields` key.
- [x] 4.2 Add a `PrintForm.test.tsx` case for a `single` template reporting no inputs: stub its
      `/api/templates/{id}/inputs` response as an empty input list, select a printer, press Print, and
      assert exactly one `/api/print` call whose body carries `data` deep-equal to `{}`, does not omit
      `data`, and has no `fields` key.
- [x] 4.3 Run `npm run test` in `ui/` and record that 4.1 and 4.2 fail against the unmodified form,
      which sends `fields` and no `data`.
- [x] 4.4 In `ui/src/api/client.ts`, change `printLabel`'s body type to exactly `template`, `printer`,
      `data` and `copies`, with `data` required. Remove `fields` and remove `option`, which no caller
      passes and which `deny_unknown_fields` now makes a `400`.
- [x] 4.5 In `ui/src/pages/print/PrintForm.tsx`, send `data: submittedData` instead of
      `fields: submittedData`. Leave the sheet path on `POST /api/batch` untouched.
- [x] 4.6 Run `npm run test` in `ui/` and confirm 4.1 and 4.2 now pass, and that the existing case
      asserting a `sheet` template makes no `/api/print` call still passes.
- [x] 4.7 Confirm no source under `ui/src/` sends a `fields` key to `/api/print`. `ui/src/api/types.ts:101`'s
       `option?` on `TemplateInputsRequest.labels[].option` (body of `POST /api/templates/{id}/inputs`) is out of scope and stays.

## 5. Gates

- [x] 5.1 Run `cargo fmt`.
- [x] 5.2 Run `cargo clippy --all-targets --all-features` and fix any finding at its root cause; never
      silence one with `#[allow(clippy::...)]`.
- [x] 5.3 Run `cargo test`.
- [x] 5.4 Run `npm run lint`, `npm run test` and `npm run build` in `ui/`. `npm run build` runs
      `tsc -b`, which is what enforces that `printLabel`'s body type admits no key the service refuses.
