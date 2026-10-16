## Context

See `proposal.md` for the problem. `ParamInput.tsx` already writes `min`, `max` and `step` on numeric inputs and stores a parsed number or `""`. `PrintForm.tsx` uses ordinary button click handlers, with no native form submission or constraint-validation call. `labelInputs.ts` already owns control-value helpers and blank omission. `ParamInput` is used only by the print form, through `FieldForm`.

[verified] At base `5680c296`, a local Chromium 153 harness mounted the unchanged `PrintForm` with real React/query/toast providers and mocked API responses. Trusted browser text input entered `0` and `11` for `amount` with `min: 1, max: 10`. Native `rangeUnderflow`/`rangeOverflow` recognized the respective violation; neither field had an accessible error and both buttons remained enabled. Browser mouse clicks recorded `/api/render` and `/api/print` bodies carrying each invalid number. The ignored `.agent-runs/issue-238/` contains the harness, browser driver and `reproduction.json`; no printer or Labeler API server was contacted. This establishes the defect, not a fix.

The HTML [constraint-validation documentation](https://developer.mozilla.org/en-US/docs/Web/HTML/Guides/Constraint_validation) confirms that attributes supply constraints while scripts must evaluate them or invoke native submission. A whole-form validity gate would also evaluate unrelated constraints; only declared numeric bounds belong here.

## Goals / Non-Goals

Implement the `ui` delta with derived values in the existing components. The `parameters` contract remains the authority for server coercion and defaults. Keep existing control parsing, native bounds and blank omission; do not copy server validation into the browser. Other exclusions are in the proposal.

## Decisions

### One small bound-message function, no validation state

Add one pure helper in `labelInputs.ts` taking the parameter and its held value and returning a bound error or no error. It applies only to `number`/`integer` parameters with a held number; blank controls have no error. Compare against each present bound independently, including a bound of zero, using strict `<` and `>` comparisons. Do not coerce blanks to zero, resolve defaults, round or clamp values.

Use the same helper in the numeric branch of `ParamInput` for feedback and in `PrintForm` while walking `detail.params` to derive whether any field is invalid. Read form entries with the existing `getOwnKey`. Two callers share the rule without an error-map type, extra form state, effect or registration protocol. Derivation follows the current value and template automatically. Separate comparisons in each component are slightly shorter at the call sites but can disagree about zero bounds or blanks.

### Immediate field feedback and button availability

Routine presentation defaults: display `Must be at least <min>.` or `Must be at most <max>.` beside the numeric input, mark it with `aria-invalid`, and connect the message through `aria-describedby` using React's existing `useId` primitive. Exact copy is not contractual. Text identifies the bound without relying on colour. Remove the error and invalid marker as soon as the value becomes valid or blank.

Include the derived invalid flag in both existing button `disabled` expressions, alongside busy/stale/printer conditions. Rely on native disabled-button behavior for action clicks; no second handler guard or DOM validity scan is needed. Disabling only one button leaves the other reproduced submission path open; merely styling an input or setting its attributes leaves both open. An enabled action that does nothing gives less immediate feedback than the selected disabled state and adjacent explanation.

### Preview and blank submission retain their own rules

Do not feed this flag to `useLivePreview`; it continues previewing held data. A failed preview alone cannot disable the actions, and a successful preview cannot override an out-of-range field. Sheet previews and Downloads share `/api/render`, so endpoint-only counts cannot establish that no Download occurred. For negative action tests, close the preview at a mobile viewport; separately prove that preview failure does not block a corrected value. No live-preview implementation change is needed.

A cleared value continues through `pruneDataForSubmit` as an omitted key, with the service selecting a default or absence. A bounded field does not become required. The delta qualifies the broad existing screen/preview wording and defines the new bounds rule in one requirement; it does not edit `Parameter controls`, which the separate list-hint proposal amends.

### Verify interactions at the request boundary

Extend `PrintForm.test.tsx` using its current fetch stub and real controls. A compact table covers both numeric types with lower-only, upper-only and two-sided bounds. Include zero bounds, exact endpoints, correction and a valid value on an unbounded side. Assert accessible field feedback, both actions disabled, and no action request after attempted clicks while invalid. Positive checks then enable and invoke both actions and inspect their values, so a permanently disabled button cannot pass. Clearing defaulted and non-defaulted fields must remove the error and submit an omitted key. Include two invalid fields to show that correcting only one cannot enable the run.

Before accepting the regression tests, run them against the old behavior: missing errors, enabled buttons or recorded invalid action requests must fail them. Repeat typed-input and mouse-click checks in a current browser after implementation, recording the requests and availability under `.agent-runs/`. Reuse existing preview-error, blank/default and stale/printer tests rather than adding a second harness to the repository.

## Risks / Trade-offs

- Preview requests can still carry invalid data and show a server refusal. This is the existing independent preview behavior; action assertions isolate previews rather than changing that contract.
- Numeric parsing remains as implemented, including integer conversion. This change promises bound feedback for held numeric values, not a new input grammar.
- A disabled action explains its reason through the field message, so the message must be textual and associated with the input; a colour-only border does not meet the delta.

## Cut pass

| Kept mechanism or task | Requirement served | Simpler alternative and concrete failure |
| --- | --- | --- |
| Shared bound-message helper; task 1.1 | Print form numeric bounds | Attributes alone reproduced four invalid submissions; separate comparison rules can disagree between feedback and availability. |
| Numeric input message; task 1.2 | Print form numeric bounds; Accessible palettes | A coloured border does not identify the violated bound in text. |
| Invalid marker; task 1.2 | Print form numeric bounds | Text alone does not expose the field's invalid state to assistive tools. |
| Message id and association; task 1.2 | Print form numeric bounds | Unassociated text does not give the input an accessible error description. |
| Derived flag in both disabled expressions; task 1.3 | Print form numeric bounds | Disabling one action still permits the other; visual feedback alone still submits. |
| Integrated numeric/action regressions; task 2.1 | All bounds-delta scenarios and #238's no-submission acceptance | Attribute-only tests pass on the reproduced broken implementation; negative-only tests accept permanently disabled actions. |
| UI gates; task 3.1 | Repository required checks for `ui/src/` | Omitting them leaves the changed controls unchecked. |
| OpenSpec gate; task 3.2 | Repository required check for `openspec/` | Omitting it leaves the amended contract unchecked. |

Removed from consideration: a general validation framework, stored errors/error map, extra field-error props, whole-form native validity gating, duplicate handler guards, clamping, server work, grid work, preview suppression, new dependencies and a permanent browser harness. None is needed for the requested declared-bound feedback and action clicks. Standalone helper unit tests would mirror comparisons; request-boundary tests cover the actual failure.
