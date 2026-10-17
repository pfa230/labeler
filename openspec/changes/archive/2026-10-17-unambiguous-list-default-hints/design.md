## Context

See `proposal.md` for the problem. `FieldForm.tsx` builds the print-form hint with `String(param.default)`. The list control itself starts empty, and existing PrintForm coverage distinguishes an untouched list, which is omitted, from one explicitly emptied by the operator, which sends `[]`. This design records the presentation choice before implementation.

## Goals / Non-Goals

**Goals:** Change only the print-form list-default representation so element boundaries are visible.

**Non-Goals:** See the proposal. The `parameters` contract already supplies arrays of strings; no new parsing, API field or server validation is needed.

## Decisions

### 1. Use compact JSON for list defaults

Routine default for review: render list defaults with `JSON.stringify` and no replacer or indentation, retaining the existing `default: ` prefix. The four required hints are `default: ["A","B"]`, `default: ["A,B"]`, `default: [""]` and `default: []`. Reverse-order defaults remain in that order. JSON serialization supplies brackets and string quoting without a hand-written escaping rule ([JSON.stringify documentation](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/JSON/stringify)).

Apply this at the existing hint expression using the array branch of the published default value. Keep `String` for scalar defaults and the existing absent-default/checkbox visibility condition. There is one affected caller, so no exported formatter or shared formatting module is needed.

A plain join, even with spaces, cannot reliably distinguish an element containing the chosen separator. Hand-quoting elements duplicates the built-in serializer. A setting or alternative presentation mode adds no capability the issue requests.

### 2. Keep presentation independent of field values

Do not assign the displayed default to form state or change `ParamInput`, `pruneDataForSubmit` or the list editor. A hint is explanatory text, and the existing blank/default submission contract continues to own omission and explicit empty arrays. Applying JSON formatting to scalar values would add visible quotes outside #351's scope.

### 3. Verify rendered text and reuse submission coverage

Routine default: add a table-driven FieldForm test for the four required arrays plus `["B", "A"]`, asserting literal expected hint text. It must fail with the original `String` conversion; do not compute its expected strings using the serializer under test.

Update the existing PrintForm list-default test's hint expectation and retain its assertions for untouched omission and explicit `[]` after editing/removal. Reuse the existing scalar-default and ordinary ordered-list tests. There is no need for a second submission suite or tests of JavaScript's serializer itself.

## Risks / Trade-offs

- Quoted JSON is more explicit than prose, with brackets and quotes visible to the operator. That is the single proposed representation for this issue, without a presentation setting.
- Accidentally prepopulating a list would change submission. The existing PrintForm regression must continue to distinguish omission from an explicit empty list.

## Validation

Scoping validates the artifacts only. Implementation runs the UI lint, full test and build gates and strict OpenSpec validation. Existing list and scalar submission tests remain the evidence that the hint-only change leaves request semantics intact.

## Cut pass

| Kept item | Requirement or explicit request | Simpler version and concrete failure |
|---|---|---|
| Inline array-only JSON formatting (task 1.1) | `ui`: Parameter controls as amended | `String` collapses distinct arrays. Use the built-in serializer at the sole affected expression instead of a new formatter. |
| Rendered hint cases (task 1.2) | `ui`: Parameter controls; #351's four distinctions and preserved order | One ordinary array cannot establish the comma/empty distinctions. Five literal cases cover those requirements without testing the serializer in isolation. |
| Existing PrintForm expectation and submission assertions (task 1.3) | #351's unchanged blank-field submission; `ui`: What a screen submits | Display-only assertions miss accidental state prepopulation. Reuse the current omission/explicit-empty test instead of duplicating submission coverage. |
| UI gates (task 2.1) | Repository gates for the planned UI diff | A targeted hint test alone does not cover existing form behavior or the type/build checks. Use current commands. |
| Strict delta validation (task 2.2) | Repository gate for the `ui` delta | Use the pinned validator to check the complete modified requirement and retained scenario, without another content checker. |

Cut: formatting options, custom escaping, a shared formatter module, template-detail and scalar reformatting, changes to form state/submission, backend work and duplicate list-submission tests. No field, dependency or configuration option is added.
