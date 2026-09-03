# Plan review

AUTHOR: opencode
REVIEWER: claude
VERDICT: APPROVE_WITH_CHANGES
ROUNDS: 5

# Plan review — `issue-337-a-label-silently-drops-every-key-that-is`

No `ANSWERS.md` at the worktree root. Reviewed `proposal.md`, `specs/template-inputs/spec.md` and `design.md` against `AGENTS.md`, `openspec/config.yaml`, the canonical specs and the tree at HEAD (`4c387ec`). `openspec validate --all --strict --no-interactive` passes, 28/28 [verified, run here].

The substance is sound and I re-verified the load-bearing parts rather than trusting rounds 1-4. The `details.error` strings the delta publishes are correct — I built the two structs the plan specifies against the pinned `serde 1.0.229` / `serde_json 1.0.151` in a standalone crate outside the repo [verified]:

```
RenderLabelRequest {"template":"shelf","data":{"title":"Bolts"},"option":{"x":"1"}}
  -> unknown field `option`, expected `template` or `data` at line 1 column 53
RenderLabelRequest {"template":"shelf","dataa":{"title":"Bolts"}}
  -> unknown field `dataa`, expected `template` or `data` at line 1 column 27
LabelInput         {"data":{"title":"Bolts"},"option":{"x":"1"}}
  -> unknown field `option`, expected `data` at line 1 column 34
BatchRequest       {"template":"t","labels":[{"data":{…},"option":{"x":"1"}}]}
  -> unknown field `option`, expected `data` at line 1 column 60
```

Also verified: the extractor ordering (`Json` at `src/api.rs:2591`, `FormatUnknown` at `:2667`, `validate_label_data_keys` at `:2673`); every `normalize_option` call site passing `None` (`src/api.rs:2677,2681`, `src/batch.rs:105-106`, `src/api.rs:1254`); `Reason::OptionsNotSupported` at `src/reason.rs:69` and its sole raiser at `src/render/mod.rs:1225-1229`; `src/errors.rs:665` / `732-765` / `771-773` and that the ADDED requirement's table shape is what `scan_canonical_withdrawals` actually parses; the `layout-sizing:1088-1096` pre-archive-phantom precedent; `docs/SPEC.md:739` and `:758`; `src/openapi.rs:141,146`; the seventeen canonical scenarios reproduced in order; and the UI claim that `option` never reaches the wire — `ResolvedLabel` is `{ data }` alone (`ui/src/lib/labelGrid.ts:34-36,44`), so `labelGrid.ts:17` and `connectorRows.ts:101` are grid-local [verified].

What survives is a cluster of citations that are wrong at HEAD (three of them landing permanently in `openspec/specs/`), one task that instructs a failing assertion, and one published contract clause with nothing pinning it.

## 1. `spec.md:257-260` names the wrong paragraph, and the number is published canonically

Delta `specs/template-inputs/spec.md:7` ends: "This requirement supersedes `spec.md:257-260` of this capability, which stated that a key other than `data` is ignored". `proposal.md:22` repeats it.

`openspec/specs/template-inputs/spec.md:257-260` is the scenario *A repeat is an interpolated read even when nothing prints the element*. The superseded paragraph is at `:284-287` [verified]. It is not drift — `git show HEAD:openspec/specs/template-inputs/spec.md` gives the same lines, and the file's last touching commit predates this change.

This one matters more than the rest because it lands in the permanent canonical spec: a future reader following the supersession pointer arrives at a `repeat:` scenario. Round 4's own required change 1 said "Publish no line or column number anywhere in the delta"; this number survived it and is wrong.

## 2. Every `src/models.rs` citation is off by seventeen lines, including one in the delta

At HEAD, `RenderLabelRequest` is derive `1220`, struct declaration `1221`, fields `1222-1224`; `LabelInput` is derive `1254`, struct `1255-1257` [verified against `git show HEAD:src/models.rs`, and unchanged at `4c387ec^`]. The artifacts cite `1203` / `1204` / `1205-1207` and `1237` / `1238-1240` throughout: `proposal.md:3,9,26`, `design.md:7,35,50,74`, `tasks.md` 1.1 and 1.2.

Review-4 recorded `design.md:35`'s "derive `1203`, struct `1204`, fields `1205-1207`" as `[verified]`. It is not; those lines are inside `TemplateFormat`. `1203` is `#[serde(tag = "type", rename_all = "snake_case")]`.

`specs/template-inputs/spec.md:162` carries the same wrong number into the canonical contract: "`LabelInput` at `src/models.rs:1238-1240` has no `option` field".

## 3. `src/lib.rs:3508,3564` is cited as evidence for a claim it does not support

`proposal.md:10` ("carrying the parser's backticked message in `details.error` … cf. `src/lib.rs:3508,3564`"), `design.md:58` ("Tests assert the backticked serde form (cf. `src/lib.rs:3508,3564`)") and `design.md:80` all point at the same pair.

At HEAD both lines are `assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "case: {id}")` inside template-validation loops [verified]. They assert a status and say nothing about message form. The tests that actually assert the backticked serde spelling are `src/lib.rs:3613-3614` (`msg.contains("unknown field \`options\`")`) and `:3669-3670` (`unknown field \`option\``).

## 4. `tasks.md` 4.2 and 4.4 instruct assertions on columns the named bodies do not produce

Task 4.2: "`{ "template":"shelf","data":{…},"option":{…} }` → … ``unknown field `option`, expected `template` or `data``` **at line 1 column 39**". Task 4.4: "`{ "template":"shelf","dataa":{…} }` on render/label → … **at column 23**".

Measured above, those bodies land at column **53** and **27**. The 39/23 pair is `design.md:48`'s measurement, taken with `"template":"t"` — a different body. An implementer following 4.2 literally writes an assertion that fails.

The column belongs in neither place. It is a function of the body, not of the contract, which is exactly why round 4 removed it from the delta; leaving it in the task list re-introduces the same brittleness one layer down.

## 5. The delta publishes an OpenAPI contract clause with no scenario and no test task

`specs/template-inputs/spec.md:7` states: "The OpenAPI schema for this endpoint is a single object with `additionalProperties: false` rather than an `allOf` that rejects its own valid body." Nothing pins it. The four new scenarios (`:110-140`) all cover the `400`; none reads the document. `tasks.md` 1.4 says only "**Verify** OpenAPI at `src/openapi.rs:141,146` now emits a single object" — a checkbox whose only evidence is that someone looked.

The repo's own precedent does both: `openspec/specs/print-request-body/spec.md:45-50` states the clause, `:136-140` is a scenario reading the generated document, and `src/lib.rs:7808` (`openapi_print_request_is_strict`) is the test. The plan cites `src/lib.rs:7703-7730` as "pins this strictness for `PrintRequest`" (`design.md:48`, `tasks.md` 1.4) — that range is `api_print_unknown_key_is_rejected`, an HTTP rejection test, not the schema test.

## What I looked at and am not requiring changed

**Where the contract lives.** The delta puts a normative body-shape rule for `POST /api/render/label` and `POST /api/batch` — including that endpoint's OpenAPI schema and extractor precedence — inside `template-inputs`, whose Purpose (`spec.md:3-8`) is "the set of controls an operator must be offered for one label". `print-request-body` is the closer model: a capability of its own, defining one endpoint's accepted body, naming `docs/SPEC.md` §2.3 as superseded. Nothing here names §2.1 or §2.2.

I checked whether that is fixable and concluded the plan's choice is forced. The canonical sentence being replaced (`:284-287`) itself spans all three endpoints ("it has none on `POST /api/render/label` or `POST /api/batch` either"), so it must be superseded by a `MODIFIED` against this requirement, and openspec resolves `MODIFIED` by reproducing the whole requirement. Splitting the envelope rule into a new capability would leave the same rule stated in two places or require a second delta against a requirement that does not exist yet. Neither §2.1 nor §2.2 states an ignore-extra-keys rule, so there is no frozen sentence left contradicted. I raise it so the choice is on the record, not as a required edit.

**`400` rather than `422 BatchInvalid` for a batch label with a bad envelope key.** Correct and correctly argued (`design.md:86`). `batch-validation:12,40-44` scopes its per-label reporting to labels the request could be parsed into; a body that fails `BatchRequest` deserialization has no index to report. `request-data-keys:65-68`'s per-label guarantee is about keys inside `data` and is untouched.

**Keeping the `An option key is ignored` heading with a `400` body.** Defensible for the reason `spec.md:116` gives, and round 4 already settled the wording.

## Required changes

The author applies these and no further review follows.

1. In `specs/template-inputs/spec.md:7` and `proposal.md:22`, replace `spec.md:257-260` with `spec.md:284-287`.
2. Replace every `src/models.rs` line citation across `proposal.md`, `design.md`, `tasks.md` and `specs/template-inputs/spec.md` by this mapping, which is the HEAD (`4c387ec`) layout: `1203` → `1220` (the `RenderLabelRequest` derive); `1204` → `1221` (its struct declaration); `1205-1207` and `1204-1207` → `1222-1224` (its fields); `1203-1207` → `1220-1225` (the whole item); `1237` → `1254` (the `LabelInput` derive); `1238-1240` → `1255-1257` (its struct). Concretely this touches `proposal.md:3,9,26`, `design.md:7,35,50,74`, `tasks.md` 1.1 and 1.2, and `specs/template-inputs/spec.md:162`.
3. In `proposal.md:10`, `design.md:58` and `design.md:80`, replace `src/lib.rs:3508,3564` with `src/lib.rs:3613-3614,3669-3670`.
4. In `tasks.md` 4.2, delete " at line 1 column 39"; in `tasks.md` 4.4, delete " at column 23". Name no line or column number in any task.
5. In `specs/template-inputs/spec.md`, insert this scenario immediately after the `#### Scenario: A single label carrying an unknown envelope key is refused` block (currently `:136-140`) and immediately before `#### Scenario: No labels is not an error`:

   ```
   #### Scenario: The published schema for the single-label body is a strict object

   - **WHEN** the generated OpenAPI document's `RenderLabelRequest` schema is read
   - **THEN** it is a single object schema carrying no `allOf`
   - **AND** `template` and `data` are among its required properties
   - **AND** its `additionalProperties` is `false`
   ```

6. Replace `tasks.md` task 1.4 with: "- [ ] 1.4 Add a test asserting the generated OpenAPI `RenderLabelRequest` schema (registered at `src/openapi.rs:141`) carries no `allOf`, lists `template` and `data` among its required properties, and sets `additionalProperties: false`, alongside `openapi_print_request_is_strict` at `src/lib.rs:7808`".
7. In `design.md:48` and `tasks.md` 1.4, replace the citation `src/lib.rs:7703-7730` with `src/lib.rs:7808` where it is offered as the test pinning `PrintRequest`'s OpenAPI strictness; `7703-7730` is the HTTP unknown-key rejection test `api_print_unknown_key_is_rejected` and may stay cited as that.

CHANGES_APPLIED: yes
SPECS_SHA256: 599c912f398563ee3926c0ac29e59592ce3512a7581eaa32e2e2b7abeff2464a
