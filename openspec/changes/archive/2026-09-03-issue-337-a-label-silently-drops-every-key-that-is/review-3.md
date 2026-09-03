I have verified every finding against the artifacts, the tree, pinned serde 1.0.229 and utoipa 5.5.0. No `ANSWERS.md` at the worktree root.

**Process note (fact, not a finding):** `design.md` was last written 10:47 and `specs/template-inputs/spec.md` 10:45, while `review-2.md` landed 11:51 — no author round ran after review-2, so the artifacts I am judging are the ones review-2 judged, unchanged [verified via mtimes]. Findings 1, 2, 4, 5 and 6 below therefore restate review-2's, independently re-verified. Finding 3 is new.

## 1. The plan ships a change that fails a mandated gate, and its dismissal cites the wrong tool (blocking)

`design.md:76` accepts a known `openspec validate` failure on the theory that "`archive-merge-check.sh` resolves `MODIFIED` by requirement name (AGENTS.md:425), not scenario heading."

That citation is about `.workflow/archive-merge-check.sh`, the git gate on the landing commit. It is not the validator, and it is not `openspec archive`. `openspec/config.yaml:148-149` mandates `openspec validate --all --strict --no-interactive` before the cargo gates. Run here:

```
✗ [ERROR] template-inputs/spec.md: MODIFIED "The service computes an input list for a given label"
  omits scenario(s) the current spec still has: "An option key is ignored".
Totals: 25 passed, 1 failed (26 items)
```
[verified, run in this worktree]

The author's own round-1 log records this as known: "`openspec validate` now reports missing old scenario [verified: `openspec validate --strict` fails on that name]" (`.agent-runs/propose-opencode.log`). So the plan knowingly plans to fail a required gate.

The underlying conflict is real, not cosmetic. `openspec/specs/template-inputs/spec.md:438` publishes the scenario "An option key is ignored", and the tool refuses to let a `MODIFIED` block drop it; review-1 correctly refused keeping that heading over a `400` outcome (`review-1.md:11`). Keeping it and renaming it are both blocked, and the plan makes no decision resolving that. That is a design fork, not an edit I can state.

## 2. The `details.error` the delta publishes cannot be produced on `POST /api/render/label` (blocking)

`spec.md:7` states normatively that the refusal carries "the parser's message naming the unknown field (`unknown field 'option', expected 'data'` or `unknown field 'dataa'`)" for all three paths; `spec.md:124-126` asserts `error.details.error` names `dataa`; `design.md:72` presents that diagnostic as "Achieved only by denying both `LabelInput` and `RenderLabelRequest`"; `proposal.md:9` says denying both "makes all three endpoints reject any key other than `data` on a label and report the unknown field name."

Built against pinned serde 1.0.229 with exactly the two structs the plan specifies (both carrying `deny_unknown_fields`):

```
RenderLabelRequest {"template":"t","data":{"a":1},"option":{"x":"1"}} -> unknown field `option` at line 1 column 50
RenderLabelRequest {"template":"t","dataa":{"a":1}}                   -> missing field `data` at line 1 column 32
LabelInput         {"data":{"a":1},"option":{"x":"1"}}                -> unknown field `option`, expected `data`
BatchRequest       {"labels":[{"dataa":{"a":1}}]}                     -> unknown field `dataa`, expected `data`
```
[verified empirically, standalone crate outside the repo]

Two published claims are false on the flattened path: `{"template","dataa"}` reports `missing field \`data\``, never `unknown field \`dataa\``; and the `option` message there carries no `, expected \`data\`` clause, so the single message form `spec.md:7` publishes for all three endpoints is wrong there too. The scenario at `spec.md:122-126` names no endpoint, unlike its three siblings at `:116`, `:128` and `:133`, so an implementer writing it against `/api/render/label` will find it unimplementable.

Note the acceptance criterion in the issue ("A label carrying a misspelled key is refused the same way") *is* met at status and `code` — a misspelled key still yields `400 InvalidRequest / json_malformed`. It is the delta that over-publishes, and it is the delta that lands permanently.

This also invalidates the rejection at `design.md:41`: replacing the flatten with an explicit `data` field on `RenderLabelRequest` was dismissed as "a larger diff for no extra guarantee", when it is in fact the only option delivering the guarantee `spec.md:7` publishes. Choosing between weakening that sentence and taking the alternative is a fork the plan has not made.

Minor consequence: `src/lib.rs:3508` and `:3564` show the repo asserts these messages in serde's backticked form; the single-quoted spelling used throughout the artifacts will not match a test.

## 3. `deny_unknown_fields` on `RenderLabelRequest` publishes an OpenAPI schema that rejects every valid body (blocking, new)

`RenderLabelRequest` is `request_body` for `/render/label` (`src/api.rs:2578`) and both it and `LabelInput` are registered components (`src/openapi.rs:141,146`). Against pinned utoipa 5.5.0, with exactly the attributes the plan specifies:

```
LabelInput          -> { properties: { data }, required: [data], additionalProperties: false }
RenderLabelRequest  -> { allOf: [ {$ref: LabelInput}, { properties: { template }, required: [template] } ] }
```
[verified empirically, standalone crate outside the repo]

Two things follow. utoipa silently drops the outer `deny_unknown_fields` — the emitted `RenderLabelRequest` schema is byte-identical with and without it, so the document never publishes the strictness the change is about. Worse, `LabelInput` now carries `additionalProperties: false` *inside* an `allOf` branch, and `additionalProperties` only sees properties declared in its own schema object. `{"template":"shelf","data":{...}}` — the canonical valid body — therefore fails the `$ref: LabelInput` branch on the unmatched `template` key. Today that body validates, because `LabelInput` has no `additionalProperties: false`; this change introduces the break, for every consumer that validates against the published document.

The repo treats the emitted schema as contract: `src/lib.rs:7703-7730` (`openapi_print_request_is_strict`) pins `additionalProperties == false` for `PrintRequest`. Nothing in `proposal.md:24-29` (Impact), the delta or `design.md` mentions the OpenAPI surface at all. `BatchRequest`/`TemplateInputsRequest` are unaffected, having no flatten.

This is a second, independent reason the alternative at `design.md:41` (explicit `data` field, no flatten) may be the right call, which is why it belongs in a re-decision rather than a listed edit.

## 4. The delta announces a change to `request-data-keys` while carrying no delta for it

`spec.md:9` states "This changes the precedence `request-data-keys:70` previously published for request-level checks", and `proposal.md:22` repeats it. The change carries no `specs/request-data-keys/` delta [verified: the only delta file is `specs/template-inputs/spec.md`].

The claim also overstates what moves. `openspec/specs/request-data-keys/spec.md:70-76` publishes the pairing "unknown `format` + unrecognized *data* key → `format_unknown`", and that pairing is untouched: an unrecognized key inside `data` still deserializes, so `format_unknown` (`src/api.rs:2667`) still precedes `validate_label_data_keys` (`src/api.rs:2673`). What the change actually bends is the broader sentence at `:70-71` ("Every check the service applies to the request as a whole, before any label is validated, SHALL keep reporting what it reports today"), for a body carrying an unknown *envelope* key. AGENTS.md requires a surviving exception to live next to the rule it bends, in the same contract; after archive, `request-data-keys` will publish the unamended rule while `template-inputs` announces it has amended it. Either drop the claim with that reasoning, or carry a `MODIFIED` delta for `request-data-keys`.

## 5. The canonical justification for withdrawing `options_not_supported` is false in two ways

`spec.md:158` publishes, permanently, as the row `scan_canonical_withdrawals` reads (`src/errors.rs:732-765`): "A label can carry no `option` map: both `LabelInput` and `RenderLabelRequest` carry `deny_unknown_fields`, so an `option` key is refused at deserialization as `json_malformed` **before it reaches `normalize_option`**. **No caller can supply an option selection** for a template that declares none."

Neither half holds.

- It never reached `normalize_option`. That function raises the slug only when its `option` argument is `Some` (`src/render/mod.rs:1224-1229`), and every production call site passes `None`: `src/api.rs:2677`, `:2681` (render/label), `src/batch.rs:105-106` (batch), `src/api.rs:1256` (thumbnail). `src/models.rs:1238-1240` has no `option` field, so a caller's `option` key is dropped by serde and never becomes that argument. `git grep` finds exactly one raiser [verified]. The slug is already unreachable at `HEAD`; `deny_unknown_fields` is not what makes it so. `proposal.md:3`, `design.md:9` and `design.md:61` state the same false chain.
- A caller *can* still supply an option selection. `POST /api/import/csv` accepts `option.<name>` columns, validates each against `template.params`, raises `csv_option_column_unknown` for an undeclared one, and folds the rest into `data` (`src/api.rs:2723-2772`). That reason is live and published at `docs/SPEC.md:758`.

The withdrawal itself is still correct and deleting the variant here is still right. But AGENTS.md requires an exception's proof to be the concrete one, and this row is the permanent record. The true reason is narrow and checkable: `normalize_option`'s only raising branch is reachable only from an `option` argument no call site ever passes `Some` to, and the variant and branch are deleted here.

## 6. The expected pre-archive registry-test failure is unstated

`design.md:61` and `proposal.md:12` justify deleting `Reason::OptionsNotSupported` here by the post-archive gate, and claim it "makes the canonical spec and the enum agree before the gate runs". True for `run-change.sh`, whose gates follow archive. Not true for the stages before it: `scan_canonical_withdrawals` scans `openspec/specs` only and never the active delta dirs, unlike the additive `scan_specs` at `src/errors.rs:770-772` [verified, `src/errors.rs:732-734,785-787`]. Between the deletion and archive the phantom assertion fires with `SPEC §10.1 documents reasons that do not exist: ["options_not_supported"]`, because `docs/SPEC.md:739` is frozen and still lists it.

This is published, expected behavior — `openspec/specs/layout-sizing/spec.md:1091-1096` specifies exactly it ("Before archive … the four withdrawn slugs remain an expected registry-test failure. Archive sync removes that failure without a code edit"). But AGENTS.md tells the implementer to run the three gates before reporting, and `apply.sh` runs before archive, so an unwarned implementer meets a red suite whose plausible responses are reverting the deletion or editing a test another canonical spec pins. The plan should say the failure is expected and cite that precedent.

## 7. Minor

`design.md:49` cites `src/api.rs:2654` for `format_unknown`; the `Reason::FormatUnknown` site is `src/api.rs:2667` [verified]. `design.md:9`/`proposal.md:9` cite `LabelInput` at `src/models.rs:1237-1240`; the struct is `1238-1240`, with the derive at `1237`.

## Verified as correct

The `MODIFIED` block reproduces the whole requirement, with only the intended replacements and no dropped prose; `spec.md:257-260` is the right citation for the superseded paragraph (`openspec/specs/template-inputs/spec.md:256-260`). The extractor-ordering analysis is right: `Json(req): Json<RenderLabelRequest>` is at `src/api.rs:2591` and `format`/`color_mode`/`resolution` are validated inside the handler, so `json_malformed` does win. The `deny_unknown_fields` + `flatten` combination compiles under serde 1.0.229 and does reject an extra key on the flattened path. The test inventory is complete: `src/lib.rs:2026`, `:2199`, `:2233` are the only label fixtures carrying `option`, and the one other label element with a sibling key (`src/lib.rs:7740`) is a 2.1 MiB body rejected at the `DefaultBodyLimit` before deserialization, so it stays green. The UI inventory is right and `design.md:55`'s claim that no other UI file sends `option` holds: `resolveLabels` builds `{ data }` only (`ui/src/lib/labelGrid.ts:34-48`), so the grid `option` maps (`ui/src/lib/labelGrid.ts:17`, `ui/src/lib/connectorRows.ts:101`) never reach the wire. `POST /api/print` is already strict (`src/models.rs:1246-1247`), so "three endpoints" is the right scope. The batch `400`-not-`422` trade-off at `design.md:73` is correctly argued: `openspec/specs/batch-validation/spec.md:12` scopes itself to labels of a request that parsed, and an unparseable body has none.

VERDICT: REVISE
