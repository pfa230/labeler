## Context

See `proposal.md` for the issue. `parseCsv` assembles each row with `data[col] = value`. `LabelGrid` reads the same data directly in `DataCell` and its column getter. `labelInputs.ts` already defines `setOwnKey` and `getOwnKey`, and `pruneDataForSubmit` uses them.

The JavaScript distinction is concrete: assignment to an inherited `__proto__` setter does not create a string-valued own field ([MDN](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Object/proto)); a plain object inherits `constructor` ([MDN](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Object/constructor)). The existing helpers already implement the needed property ownership semantics. This is a source review; no browser reproduction is claimed.

## Goals / Non-Goals

**Goals:** Carry valid parameter names through CSV ingestion and grid display/editing into the existing submission path.

**Non-Goals:** See the proposal. No contract delta: `skip_specs: true` records that this implements the current parameter-name and UI requirements. No change to the helpers, CSV header rules, grid row shape, submission pruning or backend.

## Decisions

### 1. Reuse the existing own-key operations

Routine default: import `setOwnKey` from `labelInputs.ts` into `csv.ts` and use it for the existing cell assignment. Keep its current empty-cell value. Add `getOwnKey` to `LabelGrid`'s existing import and use it at both data reads, retaining current blank-value handling.

Changing only the writer leaves inherited values visible in template columns absent from the CSV. Changing only the grid reads cannot recover a value already discarded during ingestion. A blacklist would reject valid names. Null-prototype dictionaries or a new wrapper would introduce a second object convention when the existing helpers solve all three sites.

Keep spread copies and computed-key object updates: they already create own properties, and the existing submission helper already ignores inherited values. Replacing those operations adds changes without correcting a remaining failure.

### 2. Add regressions at the failing boundaries

Routine default: extend the existing suites rather than add another harness.

- In `csv.test.ts`, parse a row with `__proto__`, `constructor` and an ordinary header. Assert own-property presence, exact values and JSON serialization. This fails if CSV reverts to direct assignment; accessing a value alone is not sufficient evidence of an own field.
- In `LabelGrid.test.tsx`, show supplied special-name values and keep a missing `constructor` cell blank. Edit a special-name cell and assert the returned row holds its own edited value. The blank-cell assertion fails if a grid data read consults the prototype; edit assertions also guard the existing computed-key update path.
- In `Import.test.tsx`, load those headers against matching template parameters, edit a cell and inspect actual outgoing Print and Download JSON using the existing fetch fixture. Both special-name values and the ordinary field must survive. Reuse a shared case for the two actions rather than duplicate the workflow. This fails if values are lost between parsing, grid state and submission even when isolated tests pass.

Create special-name fixture properties with CSV, JSON parsing, computed keys or the existing writer; a bare object-literal `__proto__:` entry does not represent the intended own data field. Reuse existing ordinary CSV, blank/default and submission coverage.

## Risks / Trade-offs

- A test can accidentally encode the same object-literal mistake as the implementation. Assert ownership and serialized request data explicitly.
- There are two grid reads. Change both in the same patch; fixing only the editor leaves the grid getter inconsistent.
- No new contract or configuration decision is pending. The accepted contract already permits both names.

## Validation

During implementation, demonstrate failing regressions against the original paths before accepting their fixes. Run the existing UI lint, full test and build gates, plus strict OpenSpec validation. This scoping turn validates artifacts only; UI behavior remains to be verified during implementation.

## Cut pass

| Kept item | Existing requirement or explicit request | Simpler version and concrete failure |
|---|---|---|
| CSV writer reuse and parser regression (task 1.1) | `ui`: CSV Import screen, Grid cells keep their values; `parameters`: Parameters are declared as a sequence | Direct assignment loses `__proto__`. Reuse one helper instead of changing the dictionary model. |
| Both grid readers and component regression (task 1.2) | `ui`: Grid cells keep their values, Parameter controls; #422's missing-value acceptance | Leaving either direct read permits inherited data in the grid. No new accessor is needed. |
| Import-through-submission regression for both actions (task 2.1) | `ui`: What a screen submits; #422's ingestion-through-submission acceptance | Parser-only assertions cannot establish the displayed, edited row reaches outgoing batch JSON. Share the Print/Download case and existing fixture. |
| UI gates (task 3.1) | Repository gates for the planned UI diff | A targeted case alone misses shared-grid regressions; use the existing lint, test and build commands. |
| Strict artifact validation (task 3.2) | Repository gate for `openspec/`; native skip-specs workflow | Use the pinned validator; no new artifact-content checker or behavior delta is needed. |

Cut: connector mapping/materialization, new helpers, dictionary wrappers, null-prototype conversion, reserved-name filters, backend changes, helper-only duplicate tests and edits to already-correct submission/spread/computed-key paths. Keep the current data fields, blank rules and ordinary-name behavior without new configuration.
