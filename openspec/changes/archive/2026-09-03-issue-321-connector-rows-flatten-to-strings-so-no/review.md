# Plan review

AUTHOR: claude
REVIEWER: opencode
VERDICT: APPROVE_WITH_CHANGES
ROUNDS: 1

Plan review for issue-321: checking your proposal and design against the spec.Findings against proposal.md, specs/connector-multi-valued-fields/spec.md, specs/connector-browser/spec.md, specs/connector-field-transforms/spec.md and design.md:

**MAJOR - Row-grid read-only scope underspecified, drives shared component (`LabelGrid.tsx`):** `specs/connector-multi-valued-fields/spec.md:244-254` states "The row grid SHALL show a multi-valued cell read-only, rendering the display text defined above" without qualifying which grid. The deliverable shares one `LabelGrid` across Connect (connector rows, can hold `string[]` via `ui/src/lib/connectorRows.ts:26-38` `rowsFromMaterialized`) and CSV/manual/batch grids which never hold an array (proposal.md:53-55, design.md:153-157). Design correctly distinguishes `array presence` vs `spec.control === "list"` (`design.md:146-162`), but the spec as written lets an implementer follow the spec literally and make `control === "list"` read-only everywhere, or make every grid joint-display on `Array.isArray(value)`. The contract must state the producer exclusivity: only a row produced by `POST /connections/{id}/materialize` may hold an array, and only there does the read-only display-text branch apply; the CSV/batch grids remain `spec.control === "list"` => em-dash as decided in `ui/src/components/LabelGrid.tsx:151-196`.

**MAJOR - Requirement title misnames browse wire:** `specs/connector-multi-valued-fields/spec.md:10` "A connector row value is a string or a list of strings" omits the browse number variant the body then allows (`spec.md:25-26` "JSON string, JSON number, or JSON array of strings"). The title is the identifier archive uses to locate requirement text. It should name the three shapes or be split per endpoint to avoid a drifted name rewriting the wrong requirement (`.workflow/archive-merge-check.sh` resolves `MODIFIED` by name).

**MINOR - Overreaching explanatory invariant in transforms:** `specs/connector-field-transforms/spec.md:37-39` adds "Such a prefix declares single-valued text, so the multi-valued refusal above cannot reach it." This is not a contract but a future-schema assertion. A future multi-valued `custom:` column would be silently exempt without a delta. Delete the sentence or qualify as current fact: "Today that prefix declares single-valued text; the refusal is keyed on `multi_valued` regardless of prefix."

**MINOR - Empty-list display duplicated, not owned:** `specs/connector-multi-valued-fields/spec.md:158` and `specs/connector-browser/spec.md:33-34` both define empty-list display/ordering. The single definition should be `connector-multi-valued-fields` ("joined with ', '", empty => "") with `connector-browser` referencing it; currently both state it independently.

**MINOR - Homebox `tags` column no supersede line:** `specs/connector-multi-valued-fields/spec.md:107` adds `tags` (`ty text, tier cheap, multi_valued true`) as `ADDED` with no `supersedes` clause. It is additive on a new column, so no frozen section is displaced, but proposal.md:99 says "docs/SPEC.md §12 is superseded in the three places the delta names." The delta names two superseded sections explicitly and a third via the Connect mapping (`spec.md:209`). State explicitly that the `tags` requirement is additive and supersedes nothing to avoid archival doubt.

### Required changes
1. In `specs/connector-multi-valued-fields/spec.md` Requirement "A mapped multi-valued column reaches the label as a list", scope the grid sentence to connector-origin rows: "A row produced by `POST /connections/{id}/materialize` that carries a multi-valued cell SHALL be shown in the row grid read-only, rendering the display text; rows produced by CSV parsing or manual entry carry no array and keep the em-dash." Keep the scenario.

2. Rename `specs/connector-multi-valued-fields/spec.md:10` to include the number shape, e.g., "A connector row value is a string, a number (browse only), or a list of strings" and adjust the browse/materialize subsections accordingly; or split into two requirements per endpoint.

3. In `specs/connector-field-transforms/spec.md:37-39`, delete "so the multi-valued refusal above cannot reach it" or replace with "Today that prefix declares single-valued text."

The author applies these edits and sets `CHANGES_APPLIED: yes`; NO further review follows.

CHANGES_APPLIED: yes
SPECS_SHA256: c00cf24c42edf79915cb126c0df2ed34925eb689edd0a10e9f89fa5d9fb59990
