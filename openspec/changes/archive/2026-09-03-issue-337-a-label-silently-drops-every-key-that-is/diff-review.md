# Diff review

AUTHORS: agy
REVIEWER: opencode
VERDICT: APPROVE
ROUNDS: 2
TREE_SHA256: 204f821a2247486a1e58005df38193a3439b10d2efa97f4b528e018828942809
SPECS_SHA256: 599c912f398563ee3926c0ac29e59592ce3512a7581eaa32e2e2b7abeff2464a

Findings against `proposal.md`, `design.md`, `tasks.md`, `specs/template-inputs/spec.md` and `AGENTS.md` [verified]:

**Implementation matches contract [verified]:**
- `src/models.rs:1220-1225` is `#[serde(deny_unknown_fields)] pub struct RenderLabelRequest { template, data }` with no `flatten`; `src/models.rs:1254-1258` is `#[serde(deny_unknown_fields)] pub struct LabelInput { data }` — the explicit struct fixes the `flatten` `missing field`/`allOf` defects documented at `proposal.md:9` and `design.md:35`.
- `src/api.rs:2591` keeps `Json<RenderLabelRequest>` extractor ordering; `src/api.rs:2667` `FormatUnknown` still precedes `src/api.rs:2673` `validate_label_data_keys`, and `src/api.rs:2598,2673,2677` correctly read `req.data` (not `req.label.data`). Envelope `json_malformed` vs `format_unknown` precedence now matches `specs/template-inputs/spec.md:9`.
- `src/reason.rs:66-69` deletion and `src/render/mod.rs:1224` collapsed to `None => Ok(None)` delete the only raising branch, already unreachable (`src/api.rs:2677,2681` all pass `None`) — required by `specs/template-inputs/spec.md:161` `options_not_supported` withdrawal.
- `ui/src/lib/livePreview.ts:4-15` deletes `PreviewInput.option`, `hasOpt`, and the `option` spreads/`previewKey` branch; `ui/src/api/types.ts:102` is `{ data?: Record<string,unknown> }[]` only — no UI path can construct a rejected label.
- `src/lib.rs:2023,2195,2233` fixtures now send only `{"data":…}`; new tests at `src/lib.rs:7836` (`openapi_render_label_request_is_strict`), `11564`, `11592`, `11645`, `11730` pin `400 InvalidRequest/json_malformed` with backticked `unknown field `option`` / `dataa` per-type (`expected `template` or `data`` vs `expected `data``) and the envelope-vs-format ordering, matching `specs/template-inputs/spec.md:110-147`.

**Gates [verified]:** `cargo fmt --check` pass, `cargo clippy --all-targets --all-features` pass, `ui` `npm run lint` pass, `ui` `npm test` 474/474 pass. `cargo test` 862 passed, 1 failed: `errors::tests::spec_documents_every_reason_and_invents_none` phantom `["options_not_supported"]` at `src/errors.rs:802`. This is the documented pre-archive shape (`proposal.md:12`, `design.md:94`, `openspec/specs/layout-sizing/spec.md:1088-1096`): `scan_canonical_withdrawals` (`src/errors.rs:732`) scans only `openspec/specs` while the additive half (`src/errors.rs:771`) scans deltas, so the withdrawal is invisible until `openspec archive` syncs the delta to `openspec/specs/`. Not a code defect; `run-change.sh` archives before gating.

**Earlier blocking stale test resolved [verified]:** `diff-review-1.md` blocked on `ui/src/lib/livePreview.test.ts` still referencing deleted `PreviewInput.option`. Current `ui/src/lib/livePreview.test.ts:5,44` removes `option` from `base` and the `omits empty option` cases and asserts `body.labels[0]` equals `{ data: {x:"1"} }`.

No blocking defect remains.

