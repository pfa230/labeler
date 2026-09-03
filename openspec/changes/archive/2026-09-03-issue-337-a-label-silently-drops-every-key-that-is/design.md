## Context

See proposal.md — Why.

Current state:

- `LabelInput` (`src/models.rs:1255-1257`, derive `1254`) is `pub struct LabelInput { pub data: HashMap<String, Value> }` with no `deny_unknown_fields`. It is used as `Vec<LabelInput>` in `BatchRequest.labels` and `TemplateInputsRequest.labels`, and as `#[serde(flatten)] pub label: LabelInput` in `RenderLabelRequest` (`src/models.rs:1220`, fields `1222-1224`). A caller sending `{"data":{…},"option":{…}}` or `{"dataa":{…}}` gets the extra key dropped and a label rendered from the remaining `data` (or from empty data).
- `src/extract.rs` wraps `axum::Json<T>` and maps `JsonRejection` into `AppError` via `From<JsonRejection>`. `src/errors.rs:483-501` maps `JsonDataError` and `JsonSyntaxError` to `AppError::malformed_json` (400 `InvalidRequest` / `json_malformed` with `details.error` carrying the parser message) and maps `MissingJsonContentType` to 415. This is the `request-error-envelope` mapping.
- `normalize_option` (`src/render/mod.rs:1211-1235`) is the only site raising `Reason::OptionsNotSupported` (`src/reason.rs:69`). It fires when `option.is_some()` and `template.options()` is `None`, but at HEAD every production call site passes `None` (`src/api.rs:2677,2681`, `src/batch.rs:105-106`, `src/api.rs:1254`), so the branch is already unreachable before this change; CSV `option.<name>` columns raise `csv_option_column_unknown` (`docs/SPEC.md:758`), not this slug.
- `ui/src/lib/livePreview.ts:44,46` builds a `label` and a `body` that spread `{ option: input.option }` when `hasOpt(input.option)` is true. `ui/src/api/types.ts:101-102` types `TemplateInputsRequest.labels[]` as `{ data?: Record<string, unknown>; option?: Record<string,string> }` and thus explicitly permits the same key on `POST /api/templates/{id}/inputs`. No caller populates `PreviewInput.option` today, but both must be gone before the server starts rejecting it.
- Existing `src/lib.rs` batch tests at `2022`, `2194`, `2228` send `{"option":{…},"data":{…}}` and expect `200`; they will become invalid once the envelope is strict.

Constraints: `docs/SPEC.md` is frozen (ADR-0057) — the `options_not_supported` row at §10.1:739 stays readable and is superseded only by the delta. `docs/adr/` is frozen at ADR-0091. Pre-1.0 breaking-change rule applies: a dropped spelling becomes a parse error naming the file and the key, which `deny_unknown_fields` gives. No migration note.

## Goals / Non-Goals

**Goals:**

- Make any key other than `data` on a label a deserialization failure that the existing `Json` extractor surfaces as `400 InvalidRequest / json_malformed`, on all three endpoints.
- Ensure the failure reports the unknown field name in serde's backticked form (``unknown field `option`, expected `template` or `data``` on `RenderLabelRequest`, ``unknown field `option`, expected `data``` on `Vec<LabelInput>` paths) and that status, `code`, `reason` and the named key hold uniformly across all three endpoints.
- Remove every UI typed surface that can construct an `option` label so no client sends a request the server now rejects.
- Retire `options_not_supported` from the published contract and delete the dead enum variant/branch in the same change that closes its last path, so the mandatory post-archive `cargo test` gate passes.

**Non-Goals:**

- Changing what `request-data-keys` refuses *inside* `data` (keys naming no declared parameter). That stays `data_key_unknown`.
- Deleting the remaining option plumbing (`Options` struct, `TemplateContent::options()`, remaining `normalize_option` logic, UI grid `option` columns) — that is #214, `skip_specs: true`.
- Adding a second, handler-level validation for the label envelope beyond serde.
- Changing `BatchRequest`/`TemplateInputsRequest` top-level shape beyond the label envelope.

## Decisions

**Make `RenderLabelRequest` explicit (`template` + `data`) with `deny_unknown_fields`, keep `LabelInput` for the `Vec<LabelInput>` paths.**

`RenderLabelRequest` at HEAD is `{ template, #[serde(flatten)] label: LabelInput }` (`src/models.rs:1220`, struct `1221`, fields `1222-1224`). In serde 1.0.229 the outer flattened struct collects unmatched fields while `FlatStructAccess` gives the flattened struct only keys matching its declared fields; leftover keys are rejected only when the outer has `deny_unknown_fields` (`serde_derive` `struct_.rs:276,347`, `private/de.rs:3403`). As review-2/3 verified, that still mis-reports: `{"template":"t","option":{…}}` is dropped with only `LabelInput` denied, `{"template":"t","dataa":{…}}` reports `missing field `data`` rather than `unknown field `dataa``, and the `option` message on that path lacks the `, expected `data`` clause. The uniform `unknown field `dataa`` / `unknown field `option`` the spec requires is not achievable with `flatten` at this serde.

The fix is to remove the `flatten` and make `RenderLabelRequest` explicit:

```rust
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderLabelRequest {
    pub template: String,
    pub data: HashMap<String, Value>,
}
```

Wire shape is unchanged (`{ "template": "...", "data": {…} }`), but deserialization is now direct, so each type reports its own `expected` clause while status, `code`, `reason` and the named key are uniform (verified against pinned serde 1.0.229 / serde_json 1.0.151: `{"template":"t","data":{"a":1},"option":{"x":"1"}}` gives ``unknown field `option`, expected `template` or `data` at line 1 column 39``, `{"template":"t","dataa":{"a":1}}` gives ``unknown field `dataa`, expected `template` or `data` at line 1 column 23``). This also fixes the OpenAPI break: `RenderLabelRequest` as `request_body` for `/render/label` (`src/api.rs:2578`, registered at `src/openapi.rs:141,146`) is currently modeled as `allOf: [{ $ref: LabelInput }, { template }]` with `LabelInput` carrying `additionalProperties: false` inside one branch, so `{"template":"shelf","data":{…}}` fails that branch on the unmatched `template` key (review-3 verified against utoipa 5.5.0; `src/lib.rs:7808` (`openapi_print_request_is_strict`) pins this strictness for `PrintRequest`). An explicit object emits a single schema `{ properties: { template, data }, required, additionalProperties: false }` that validates correctly; utoipa's silently dropping outer `deny_unknown_fields` on an `allOf` no longer matters.

`LabelInput` (`src/models.rs:1255-1257`, derive at `1254`) keeps `#[serde(deny_unknown_fields)]` for `BatchRequest.labels: Vec<LabelInput>` and `TemplateInputsRequest.labels: Vec<LabelInput>` (no flatten, already uniform). The two types now share validation but not a Rust type — the handlers change from `req.label.data` to `req.data` on the render/label path, which aligns with `PrintRequest`'s explicit `data` field.

*Alternative rejected — deny both `LabelInput` and `RenderLabelRequest` but keep `flatten`.* It narrows the misspelled case to `missing field `data`` and the `option` message to the short form, so the spec must weaken its `details.error` claim; and it leaves the `allOf`/`additionalProperties: false` OpenAPI break for every valid body.

*Alternative rejected — only `LabelInput`.* Satisfies batch/inputs but not `POST /api/render/label` as above.

**Rely on the existing `Json` → `AppError` mapping; do not add a new reason.**

`src/errors.rs:493-495` maps `JsonRejection::JsonDataError` to `malformed_json` (400 / `InvalidRequest` / `json_malformed`), the `request-error-envelope` contract for "body is valid JSON but does not deserialize into the endpoint's type". A label with `option` is that shape. Introducing a new `reason` like `label_key_unknown` would split one deserialization failure into two reasons by which type it hit, which `request-error-envelope` deliberately does not do. The parser's `details.error` (``unknown field `option`, expected `template` or `data``` on `RenderLabelRequest`, ``unknown field `option`, expected `data``` on the `Vec<LabelInput>` paths, ``unknown field `dataa`, expected `template` or `data``` / ``unknown field `dataa`, expected `data``` respectively) is the only per-field diagnostic; `details.reason` stays uniform. Tests assert the backticked serde form (cf. `src/lib.rs:3613-3614,3669-3670`), not single-quoted.

**Extractor runs before handler, so `json_malformed` wins over `format_unknown` for an envelope key — but not for a data key.**

`Json<RenderLabelRequest>` is an extractor (`src/api.rs:2591`); axum completes it before calling the handler, while `FormatUnknown` is produced inside the handler at `src/api.rs:2667` (not `2654`). A request carrying both a bad `format` query and an unknown *envelope* key therefore reports `json_malformed`, not `format_unknown`. This does **not** change `request-data-keys:70-76` (`format_unknown` vs. an unrecognized key *inside* `data`): `FormatUnknown` at `src/api.rs:2667` still precedes `validate_label_data_keys` at `src/api.rs:2673`, so that pairing is untouched. The envelope ordering is a deserialization fact, and no separate `request-data-keys` delta is needed — the surviving exception rule (AGENTS.md) is not bent for the case that spec scopes to.

*Alternative rejected — preserve `format_unknown` priority by manually extracting the body inside the handler after validating `format`.* It would require abandoning the `Json` extractor for this endpoint, custom `Bytes` parsing, and reimplementing the `request-error-envelope` mapping inside the handler, for a precedence edge that has no caller relying on it and that the uniform `json_malformed` contract intentionally treats as one reason.

**UI: delete `option` from both `livePreview.ts` and `types.ts`.**

`livePreview.ts` carries `PreviewInput.option?: Record<string,string>` and `hasOpt` + `sortObj` helpers, building `const label = { data: input.data, ...(hasOpt(input.option) ? { option: input.option } : {}) }` and the same spread into the single-label `body`. `types.ts:101` types `TemplateInputsRequest.labels[]` as `{ data?: …; option?: … }` and thus keeps a typed client surface that can construct requests the server now rejects. The fix is to delete the `option` field from `PreviewInput`, the `hasOpt` guard, both spreads (leaving `label = { data: input.data }` and `body = { template: id, data: input.data }`), the `option` sort branch in `previewKey`, and the `option` field from `TemplateInputsRequest`. No other UI file sends `option` on a label — the batch grids prune to the input list before submit.

*Alternative considered — keep `PreviewInput.option` but strip it before send.* It leaves a typed field that no server accepts; deleting it makes the contract visible in the type.

**Delete `Reason::OptionsNotSupported` and its branch here, not in #214.**

The variant and `normalize_option`'s `if option.is_some() { Err(OptionsNotSupported) }` branch (`src/render/mod.rs:1224-1229`, `src/reason.rs:69`) were already unreachable at HEAD — every production call site passes `None` (`src/api.rs:2677,2681`, `src/batch.rs:105-106`, `src/api.rs:1254` and thumbnail paths) and `LabelInput` (`src/models.rs:1255-1257`, derive `1254`) has no `option` field so a carried key is dropped rather than forwarded — and `LabelInput`/explicit `RenderLabelRequest` now both carry `deny_unknown_fields` so a future `option` is refused as `json_malformed` in any case. `POST /api/import/csv` still accepts `option.<name>` columns, but those are judged under `csv_option_column_unknown` (`docs/SPEC.md:758`), not this slug, so the withdrawn table's "No caller can supply an option selection" is scoped to label envelopes. Deleting the variant here is still required for the mandatory post-archive gate: `spec_documents_every_reason_and_invents_none` (`src/errors.rs:665`, `scan_canonical_withdrawals` `src/errors.rs:732-765`) scans canonical `openspec/specs/**/spec.md` for `withdrawn` tables and fails when a canonically withdrawn slug remains in `Reason::ALL`, and `AGENTS.md` requires `archive` before `cargo test`. #214 retains the remaining plumbing deletions (`Options` struct, `TemplateContent::options()`, the rest of `normalize_option` when fully dead, UI grid columns) and lands with `skip_specs: true`. Between the code deletion and `archive`, the *phantom* half (`SPEC §10.1 documents reasons that do not exist: ["options_not_supported"]`) will be red — `scan_canonical_withdrawals` scans only `openspec/specs` (not active deltas) while the additive half scans deltas (`src/errors.rs:771-773`). This is expected and specified at `openspec/specs/layout-sizing/spec.md:1088-1096` (pre-archive withdrawn slugs remain an expected failure; archive sync removes it without a code edit). The implementer SHALL NOT revert or edit `src/errors.rs` on that red; `run-change.sh` gates after archive, and `apply.sh` runs before it.

*Alternative rejected — keep variant until #214 and establish landing dependency #214 → #337.* #214 is blocked by #337 and its siblings, so inverting the order would deadlock the split, and a landed change depending on an unlanded change still leaves the withdrawn slug in the interim canonical set.

**Update tests that will break.**

`src/lib.rs:2022` (`batch_sheet_download_returns_pdf`), `2194` (`batch_sheet_print_failure_marks_all`), `2228` (`batch_sheet_print_success_one_job`) currently send `{"option":{…},"data":{…}}` and expect `200`. They will be updated to `{"data":{…}}` (or the `option` variant removed where it was the only subject) so the gate stays green. New HTTP tests will be added to pin the `400 json_malformed` rejection and its `details.error` (backticked serde form, cf. `src/lib.rs:3613-3614,3669-3670`) on all three endpoints. `src/lib.rs:7740` (2.1 MiB body rejected at `DefaultBodyLimit` before deserialization) stays green and needs no change.

## Risks / Trade-offs

- **A caller sending `option` breaks at once, with `json_malformed` rather than `options_not_supported`.** → Intended, pre-1.0, no window. `details.error` still names `option` (``unknown field `option`, expected `template` or `data``` on `RenderLabelRequest`, ``unknown field `option`, expected `data``` on `Vec<LabelInput>` paths); the old slug was reachable only via a path this change deletes.
- **A misspelled `data` key (`dataa`) becomes `json_malformed` (``unknown field `dataa`, expected `template` or `data``` on `RenderLabelRequest`, ``unknown field `dataa`, expected `data``` on `Vec<LabelInput>` paths) rather than `missing field `data``.** → Achieved by the explicit `RenderLabelRequest` and strict `LabelInput`; denying a flattened type would still report the less precise missing-field diagnostic.
- **Batch with one malformed label element fails as `400`, not `422 BatchInvalid`.** → Correct: the request cannot be parsed into `BatchRequest` at all, so no label index exists. This matches how a batch body missing `template` already fails.
- **Combined bad `format` + unknown label key now reports `json_malformed`.** → Axum evaluates extractors before the handler; preserving `format_unknown` priority would require abandoning the `Json` extractor. The spec now documents the new ordering.
- **UI deletion is pure.** → No caller populates `option`, so wire bytes are unchanged until some future caller would have, at which point they now correctly send only `data`.
- **Validator now passes.** → The `MODIFIED` requirement retains the historical heading `An option key is ignored` (updated body to `400`) so `openspec validate --strict` sees the name as present. The previous draft renamed it to `A label carrying an option key is refused`, which the validator reported as missing while `archive-merge-check.sh` resolves `MODIFIED` by requirement name (AGENTS.md:425) — the mismatch taught the plan to keep the historical heading.

## Migration Plan

None. No stored data or template content changes. The API contract change is `400 InvalidRequest / json_malformed` with `details.error` backticked ``unknown field `option`, expected `template` or `data``` (`RenderLabelRequest`) / ``unknown field `option`, expected `data``` (`Vec<LabelInput>` paths) and ``unknown field `dataa`, expected `template` or `data``` / ``unknown field `dataa`, expected `data```. The `options_not_supported` slug is withdrawn and the variant deleted here; #214 follows to delete remaining dead plumbing with `skip_specs: true`.

Before archive, `cargo test` is expected red with `SPEC §10.1 documents reasons that do not exist: ["options_not_supported"]` — canonical `docs/SPEC.md:739` still lists it while `Reason::ALL` no longer does. This is the published `layout-sizing:1088-1096` pre-archive withdrawal shape; `openspec archive` (which runs `openspec validate --all --strict` at `run-change.sh:537-547`) syncs the withdrawn table and the next `cargo test` after archive is green. `run-change.sh` gates after archive; `apply.sh`-time red is not a revert signal.

## Open Questions

None. Extractor precedence and serde flatten semantics were confirmed against `src/api.rs:2591,2667` (FormatUnknown), OpenAPI `allOf`/`additionalProperties: false` against pinned utoipa 5.5.0 (`src/openapi.rs:141,146`, `src/lib.rs:7703-7730`) via standalone measurement (`{"template":"t","data":{"a":1},"option":{"x":"1"}}` at column 39, `{"template":"t","dataa":{"a":1}}` at column 23); status/code mapping against `src/extract.rs` and `src/errors.rs:483-501`; UI surfaces against `ui/src/lib/livePreview.ts:44,46` and `ui/src/api/types.ts:101-102`; pre-archive phantom against `src/errors.rs:732-765,665,771-773` and `openspec/specs/layout-sizing/spec.md:1088-1096`.
