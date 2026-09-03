Reviewed `proposal.md`, `specs/template-inputs/spec.md` and `design.md` against `AGENTS.md`, `openspec/config.yaml`, the canonical specs and the tree. No `ANSWERS.md` at the worktree root. `openspec validate --all --strict --no-interactive` passes, 26/26 [verified, run here].

Most of what reviews 1 to 3 raised is now genuinely fixed and I re-verified it: the explicit `RenderLabelRequest` decision, the extractor-ordering analysis (`Json` at `src/api.rs:2591`, `FormatUnknown` at `:2667`, `validate_label_data_keys` at `:2673`), the corrected withdrawal justification, the pre-archive phantom red with its `layout-sizing:1088-1096` precedent (`run-change.sh:534-545` archives before the gates at `:556-573`), the call-site inventory (`src/api.rs:2677,2681`, `src/batch.rs:105-106`, `src/api.rs:1254`, all `None`), the test inventory (`src/lib.rs:2026,2199,2233` are the only `option` label fixtures; `:7740` stays green behind `DefaultBodyLimit`) and the UI inventory (`livePreview.ts:8,12-14,19,44,46`, `types.ts:102`; `fetchTemplateInputs` at `labelInputs.ts:38` already takes `{ data? }[]` only). The `request-data-keys` question is resolved correctly: a deserialization failure has always preceded the `format` check, so enlarging the set of failing bodies bends nothing at `request-data-keys:69-76`.

What is left is one factual error published as verified, plus draft narration landing in a permanent contract.

## 1. The `details.error` strings the delta publishes for `POST /api/render/label` are wrong, and `design.md:48` presents them as measured

Built against the pinned `serde 1.0.229` / `serde_json 1.0.151` with exactly the two structs the plan specifies:

```
RenderLabelRequest {"template":"t","data":{"a":1},"option":{"x":"1"}}
  -> unknown field `option`, expected `template` or `data` at line 1 column 39
RenderLabelRequest {"template":"t","dataa":{"a":1}}
  -> unknown field `dataa`, expected `template` or `data` at line 1 column 23
LabelInput         {"data":{"a":1},"option":{"x":"1"}}
  -> unknown field `option`, expected `data` at line 1 column 24
BatchRequest       {"template":"t","labels":[{"dataa":{"a":1}}]}
  -> unknown field `dataa`, expected `data` at line 1 column 34
```
[verified empirically, standalone crate in `/tmp`, outside the repo]

`spec.md:7` publishes "rejected uniformly as `unknown field \`option\`` (at line 1 column 50) and `unknown field \`dataa\`` respectively"; `spec.md:114` and `spec.md:140` repeat "at line 1 column 50". The column is 39, not 50, and the message carries a `, expected \`template\` or \`data\`` clause the spec says it does not. An implementer writing `spec.md:140`'s scenario as an assertion on `details.error` asserts a string the server never produces.

The 50 is not a slip. `unknown field \`option\` at line 1 column 50` is verbatim what `review-3.md` measured for the **flatten** shape, which this draft rejected. `design.md:48` presents it as "verified standalone crate" for the explicit struct, and `design.md:99` lists the diagnostics under confirmed. That is an unmeasured figure carried over from the discarded alternative.

The uniformity claim is also false as written: the `expected` clause lists each type's own fields, so render/label and the `Vec<LabelInput>` paths differ. What is uniform is the status, `code`, `details.reason`, and that `details.error` names the key. Same claim at `proposal.md:9,22,27`, `design.md:20,58,85,93`.

## 2. Draft and review narration lands in the permanent canonical contract

`spec.md:116` is an italic note reading in part "previous review-1 heading was rejected as misleading; validator requires the name be kept". `spec.md:9` ends "The previous draft's claim that `request-data-keys:70` was amended is withdrawn." `spec.md:7` says "(no `flatten`; see design)".

All three land in `openspec/specs/template-inputs/spec.md` at archive. `AGENTS.md` puts rationale in `proposal.md`/`design.md` under `openspec/changes/archive/` and the contract in `openspec/specs/`; a canonical requirement that cites which review round rejected which heading, and what a superseded draft claimed, is the wrong document for both. "see design" is worse than redundant: once `design.md` moves under `openspec/changes/archive/<date>-issue-337-.../`, the reference resolves to nothing.

Keeping the historical heading itself is defensible. `openspec validate --strict` does refuse a `MODIFIED` block that drops a scenario name the current spec carries (`openspec/specs/template-inputs/spec.md:438`), and review-1 was right that renaming it publishes a heading the tool then reports as missing. The constraint is real; the explanation just has to be written for a future reader rather than for the previous reviewer.

One side effect worth noting: `scan_canonical_withdrawals` (`src/errors.rs:745-750`) latches `in_withdrawn_section` on any line containing "withdrawn" and clears it only on a line starting with `#`. `spec.md:9`'s trailing sentence latches it for the rest of that requirement. Harmless today, since no `|` line follows before the next heading [verified], and gone once the sentence is.

## 3. The misspelled-key scenario names no endpoint

`spec.md:124-128` reads "**WHEN** a label is sent carrying `{ "dataa": ... }`" with no endpoint, unlike its three siblings at `:118`, `:130` and `:136`. Since the message differs by endpoint (`expected \`data\`` versus `expected \`template\` or \`data\``), the scenario cannot be implemented as written. Review-3 raised this; it is unaddressed.

## 4. Citations

- `design.md:35` "struct `1203`, fields `1204-1207`": derive is `1203`, the struct declaration `1204`, fields `1205-1207` [verified].
- `proposal.md:12` and `design.md:74` cite `spec_documents_every_reason_and_invents_none` as `src/errors.rs:783`. The test is at `src/errors.rs:665`; `783` is a line inside it.
- The same two lines cite "the additive half scans deltas (`src/errors.rs:785-787`)". `785-787` is the canonical-withdrawals scan; the additive delta scan is `src/errors.rs:771-773` [verified].
- `design.md:20` and `:85` use `'option'` / `'dataa'`, contradicting `design.md:58`'s own rule that tests assert the backticked form.

Everything else checks out, including the `400`-not-`422` batch trade-off, the OpenAPI `allOf` break the explicit struct removes, and the `MODIFIED` block reproducing all seventeen canonical scenarios in order.

## Required changes

The author applies these and no further review follows.

1. `specs/template-inputs/spec.md:7`, replace from "On `POST /api/render/label` the wire shape is unchanged" through "an `allOf` that breaks validation." with: "On `POST /api/render/label` the wire shape is unchanged but the server type becomes an explicit `RenderLabelRequest { template: String, data: HashMap<String, Value> }` carrying `#[serde(deny_unknown_fields)]` and no `flatten`, so the same bodies are rejected as ``unknown field `option`, expected `template` or `data``` and ``unknown field `dataa`, expected `template` or `data```. The `expected` clause names the fields of the type the body failed against, so it differs between this endpoint and the `Vec<LabelInput>` paths; what holds uniformly across all three is the status, the `code`, the `details.reason` and that `details.error` names the offending key in serde's backticked form. The OpenAPI schema for this endpoint is a single object with `additionalProperties: false` rather than an `allOf` that rejects its own valid body." Publish no line or column number anywhere in the delta.
2. `specs/template-inputs/spec.md:114`, replace the parenthetical with: "(``unknown field `option`, expected `template` or `data``` on `POST /api/render/label`, ``unknown field `option`, expected `data``` on the `Vec<LabelInput>` paths)".
3. `specs/template-inputs/spec.md:140`, replace the parenthetical with: "(``unknown field `option`, expected `template` or `data```)".
4. `specs/template-inputs/spec.md:126`, name the endpoint: "**WHEN** `POST /api/render/label` is sent `{ "template": "shelf", "dataa": { "title": "Bolts" } }`"; and at `:128` replace the parenthetical with "(``unknown field `dataa`, expected `template` or `data```)".
5. `specs/template-inputs/spec.md:116`, replace the whole italic note with: "*The heading is historical. `openspec validate --strict` refuses a `MODIFIED` block that drops a scenario name the current spec carries, so the name outlives the rule it described; the normative outcome is the `400` above.*"
6. `specs/template-inputs/spec.md:9`, delete the final sentence "The previous draft's claim that `request-data-keys:70` was amended is withdrawn."
7. `specs/template-inputs/spec.md:7`, delete "; see design" from "(no `flatten`; see design)".
8. `design.md:48`, replace the parenthetical "(verified standalone crate: ... `unknown field \`dataa\``)" with the measured pair: "(verified against pinned serde 1.0.229 / serde_json 1.0.151: `{"template":"t","data":{"a":1},"option":{"x":"1"}}` gives ``unknown field `option`, expected `template` or `data` at line 1 column 39``, `{"template":"t","dataa":{"a":1}}` gives ``unknown field `dataa`, expected `template` or `data` at line 1 column 23``)", and change "reported uniformly as ... on all three endpoints" to state that each type reports its own `expected` clause while status, `code`, `reason` and the named key are uniform.
9. Apply the same correction to every other place the artifacts claim the render/label message is the bare form, is identical to the `Vec<LabelInput>` form, or sits at column 50: `proposal.md:9,22,27` and `design.md:20,58,85,93`. Use backticks, not single quotes, at `design.md:20` and `:85`. Delete "render/label at column 50" from `design.md:93`. At `design.md:99`, drop the diagnostics from the list of things "confirmed" against `serde_derive`/`private/de.rs` and cite the standalone measurement instead.
10. Fix the four citations in finding 4: `design.md:35` to derive `1203`, struct `1204`, fields `1205-1207`; `proposal.md:12` and `design.md:74` to `src/errors.rs:665` for the test and `src/errors.rs:771-773` for the additive delta scan; leave `src/errors.rs:732-765` and `785-787` where they correctly name `scan_canonical_withdrawals` and its call.

VERDICT: APPROVE_WITH_CHANGES
