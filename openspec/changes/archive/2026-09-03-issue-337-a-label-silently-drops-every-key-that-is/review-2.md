Reviewed all four artifacts against `AGENTS.md`, `openspec/config.yaml`, the current tree, the canonical specs, and pinned serde. No `ANSWERS.md` at the worktree root. Findings below; the first two are blocking.

## 1. The change fails the strict validation the archive guidance mandates, and the argument dismissing it rests on a citation that does not cover it

`design.md:76` plans to ship a delta that `openspec validate` rejects, on the theory that "`archive-merge-check.sh` resolves `MODIFIED` by requirement name (AGENTS.md:425), not scenario heading."

`AGENTS.md:425-428` is about `.workflow/archive-merge-check.sh`, the git gate that judges the landing *commit*. It is not the `openspec archive` command that writes `openspec/specs/`, and it is not the validator. Both of those refuse:

```
$ openspec validate --all --strict --no-interactive
✗ change/issue-337-a-label-silently-drops-every-key-that-is
Totals: 25 passed, 1 failed (26 items)

✗ [ERROR] template-inputs/spec.md: MODIFIED "The service computes an input list for a given label"
  omits scenario(s) the current spec still has: "An option key is ignored". Copy them into the
  MODIFIED block (a MODIFIED requirement replaces the whole block, so archive refuses to drop them).
```
[verified, run in this worktree]

`openspec/config.yaml` archive guidance requires, in order, `openspec doctor`, `openspec validate --all --strict --no-interactive`, `openspec validate --archived --no-interactive`, then the three cargo gates. `openspec archive --help` shows validation is on by default and `--no-validate` is "not recommended, requires confirmation". So the archive stage (`run-change.sh:537-547`) is planned to run against a change the tool refuses, and a failure there stops the run at `run-change.sh:543`.

The underlying conflict is real and not a formatting nit: review-1 correctly refused the heading "An option key is ignored" over a `400` outcome (`review-1.md:11`), and the tool refuses to let a `MODIFIED` block drop a scenario name the current spec has (`openspec/specs/template-inputs/spec.md:188`). Renaming and keeping the name are both blocked. Resolving that needs a decision the plan has not made, which is why this is REVISE rather than a listed edit.

## 2. The `details.error` the delta publishes for a misspelled key cannot be produced on `POST /api/render/label`

`specs/template-inputs/spec.md:7` states normatively that the refusal carries "the parser's message naming the unknown field (`unknown field 'option', expected 'data'` or `unknown field 'dataa'`)", the scenario at `spec.md:122-126` asserts `error.details.error` names `dataa`, and `design.md:72` presents that diagnostic as the thing "achieved only by denying both `LabelInput` and `RenderLabelRequest`".

Built against the pinned serde 1.0.229 with exactly the two structs the plan describes:

```
render/label {"template":"t","data":{"a":1},"option":{"x":"1"}}
  -> "unknown field `option` at line 1 column 50"
render/label {"template":"t","dataa":{"a":1}}
  -> "missing field `data` at line 1 column 32"
LabelInput   {"data":{"a":1},"option":{"x":"1"}}
  -> "unknown field `option`, expected `data` at line 1 column 24"
LabelInput   {"dataa":{"a":1}}
  -> "unknown field `dataa`, expected `data` at line 1 column 8"
```
[verified empirically, standalone crate outside the repo]

Two published claims are false on the flattened path. `{"template","dataa"}` reports `missing field \`data\``, not `unknown field \`dataa\``: the generated `collected_deny_unknown_fields` block runs *after* the flattened struct is extracted (`serde_derive-1.0.229/src/de/struct_.rs:347-361`, emitted at `:405-417`), and `flat_map_take_entry` (`serde-1.0.229/src/private/de.rs:3427-3444`) never hands `dataa` to `LabelInput`, so `LabelInput` fails first on the missing field. And the `option` message on that path carries no `, expected \`data\`` clause, unlike the two `Vec<LabelInput>` endpoints, so the single message form `spec.md:7` publishes for all three endpoints is wrong there too.

This also invalidates the rejection at `design.md:41`. Replacing the flatten with an explicit `data` field on `RenderLabelRequest` was dismissed as "a larger diff for no extra guarantee"; it is in fact the option that delivers the guarantee `spec.md:7` publishes, uniformly across the three endpoints. Choosing between weakening the spec sentence and taking that alternative is a design fork.

`src/lib.rs:3508` and `:3564` show the repo's own convention for these assertions is the backticked serde form, so the single-quoted spelling used throughout the artifacts will not match what a test asserts either.

## 3. `options_not_supported` is already unreachable, so the canonical reason the delta publishes for withdrawing it is false

`specs/template-inputs/spec.md:158` publishes, as the permanent canonical justification: "A label can carry no `option` map: both `LabelInput` and `RenderLabelRequest` carry `deny_unknown_fields`, so an `option` key is refused at deserialization as `json_malformed` before it reaches `normalize_option`."

`normalize_option` (`src/render/mod.rs:1211`) raises it only when its `option` argument is `Some` (`src/render/mod.rs:1225-1229`). Every call site passes `None` today: `src/api.rs:2677` and `:2681` (render/label), `src/batch.rs:105-106` (batch), `src/api.rs:1254` and the `src/templates.rs` thumbnail calls. `src/models.rs:1237-1240` declares `LabelInput { data }` with no `option` field, so a caller's `option` key is dropped by serde and never becomes that argument, and CSV import folds `option.<name>` columns into `data` before constructing `LabelInput` (`src/api.rs:2762-2771`). `git grep` finds no other raiser. [verified]

So `deny_unknown_fields` is not what makes the slug unreachable; it is unreachable at `HEAD`. The withdrawal is still correct and deleting the variant here is still right, but three artifacts state a causal chain that does not hold: `proposal.md:3` ("the last path that can carry an `option` map ... the change that makes it unreachable"), `design.md:9` ("After the label envelope denies `option`, no caller can supply that argument"), `design.md:61` ("The variant and ... branch have exactly one reachable path"), and `spec.md:158`, which is the one that lands permanently in `openspec/specs/` and is the row `scan_canonical_withdrawals` (`src/errors.rs:732-765`) reads.

## 4. The delta declares a change to `request-data-keys` without carrying a delta for it

`spec.md:9` states "This changes the precedence `request-data-keys:70` previously published for request-level checks", and `proposal.md:22` repeats it, while the change carries no delta for that capability.

Two problems. The claim overstates what moves: `openspec/specs/request-data-keys/spec.md:70-76` is about an unknown `format` paired with an *unrecognized data key*, and that pairing is untouched, because `format_unknown` (`src/api.rs:2667`) still precedes `validate_label_data_keys` (`src/api.rs:2673`). What the change does bend is the broader sentence at `:70-71` ("Every check the service applies to the request as a whole, before any label is validated, SHALL keep reporting what it reports today"), for a body carrying an unknown envelope key. `AGENTS.md` requires a surviving exception to live next to the rule it bends, in the same contract; `archive-merge-check.sh` will sync `template-inputs` only, leaving `request-data-keys` publishing the unamended rule and `template-inputs` announcing it has changed it. Either drop the claim with the reasoning above, or carry a `MODIFIED` delta for `request-data-keys`.

## 5. The expected pre-archive registry-test failure is unstated

`design.md:61` and `proposal.md:12` justify deleting `Reason::OptionsNotSupported` here by the post-archive gate, and say deleting it "makes the canonical spec and the enum agree before the gate runs". True for `run-change.sh`, whose gates follow archive (`run-change.sh:534-573`). It is not true for the stages before it. `scan_canonical_withdrawals` scans `openspec/specs` only, never the active delta dirs, unlike the additive half at `src/errors.rs:806-812`; so between the deletion and archive, the phantom assertion fires with `SPEC §10.1 documents reasons that do not exist: ["options_not_supported"]`.

This is published, expected behavior, not a bug: `openspec/specs/layout-sizing/spec.md:1088-1096` specifies exactly it ("Before archive ... the four withdrawn slugs remain an expected registry-test failure. Archive sync removes that failure without a code edit"). But `AGENTS.md` tells the implementer to run the three gates before reporting, and `apply.sh` runs before archive, so an unwarned implementer meets a red suite whose plausible responses are reverting the deletion or editing a test whose behavior another canonical spec pins. The plan should say the failure is expected and cite that precedent.

## 6. Minor

`design.md:49` cites `src/api.rs:2654` for `format_unknown`; the `Reason::FormatUnknown` site is `src/api.rs:2667`. `design.md:9` and `proposal.md:9` cite `LabelInput` at `src/models.rs:1237-1240`; the struct is `1238-1240` with the derive at `1237`.

## Verified as correct

The `MODIFIED` block reproduces the whole requirement: diffing it paragraph by paragraph against `openspec/specs/template-inputs/spec.md:251-453` shows only the four intended replacements and no dropped prose, and all seventeen existing scenarios are carried over. `spec.md:257-260` is the correct citation for the superseded paragraph. The withdrawal table's shape parses under `scan_canonical_withdrawals` (`src/errors.rs:745-762`). The test inventory at `proposal.md:28` is complete: `src/lib.rs:2026`, `:2199`, `:2233` are the only label fixtures in `src/` or `tests/` carrying an `option` key. The UI inventory is right, and `design.md:55`'s claim that no other UI file sends `option` holds: `ResolvedLabel` is `{ data }` (`ui/src/lib/labelGrid.ts:34-36`), and `PrintForm.tsx:134`, `preview.ts:47` and `rowPreview.ts:26-33` all build from it. The extractor-ordering analysis is right, including for `color_mode` and `resolution`, which are `Option<String>` in `RenderQuery` (`src/api.rs:51-55`) and validated in the handler, so the `Query` extractor cannot preempt `Json`.

VERDICT: REVISE
