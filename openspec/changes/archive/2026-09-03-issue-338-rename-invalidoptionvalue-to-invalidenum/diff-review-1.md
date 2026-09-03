TREE_SHA256: c7dbccf321540c9a363251da0fd9cbb2ce31d22163d88784e9c0b651674532fc

Reviewed the diff (5 files), the four change artifacts, the two spec deltas, and the frozen spec. Gates: `cargo fmt --check` exit 0, `cargo clippy --all-targets --all-features` exit 0, `cargo test` 835 passed / 0 failed [verified]. `openspec validate --strict` passes, and `.workflow/specs-digest.sh` still returns `8bee5d4e...`, matching `review.md`'s `SPECS_SHA256`, so the plan verdict is intact [verified].

## Blocking

**1. A third frozen-spec statement of the old code is left authoritative, and the requirement contradicts it.**

`docs/SPEC.md:1069`, in the `## CSV import` section (heading at `docs/SPEC.md:1063`), reads:

> and a disallowed enum value fails the row (`BatchInvalid` / `InvalidOptionValue`).

The `ADDED` requirement scopes its supersession to exactly two sites and closes the door on more: `specs/enum-validation/spec.md:11` names `docs/SPEC.md:566-567` and `docs/SPEC.md:683`, then says "It supersedes no other row of that table and no other part of §10, and it supersedes no row of `docs/SPEC.md` §10.1". The CSV import section is named nowhere, so under the precedence rule (`AGENTS.md`: a frozen section stays authoritative "until an OpenSpec requirement explicitly names and supersedes it, and then only for that section") it keeps asserting `InvalidOptionValue` for the per-row code.

That is not a silence the new requirement leaves alone. `specs/enum-validation/spec.md:34` legislates that exact endpoint: "Inside `POST /api/batch`, `POST /api/print`, and `POST /api/import/csv`, the per-label failure SHALL be reported ... carrying `code` `InvalidEnumValue`". So after this lands, the documented lookup procedure returns two different codes for one request, and the requirement's own claim to restate "that code's complete post-change contract" (`spec.md:11`) is false.

Nor is `batch-validation` covering it: it supersedes `docs/SPEC.md:1069`'s sibling concerns but only for the `BatchInvalid` half (`openspec/specs/batch-validation/spec.md:72-86`), and it never names this code. That same requirement is the precedent for the fix: a first-touch requirement there enumerated all three frozen sites it displaced, including a prose paragraph rather than only table rows. This one enumerates two of three.

Failure scenario: a client author asks what `POST /api/import/csv` returns for a disallowed enum value, reads `docs/SPEC.md` § CSV import, checks `openspec/specs/` for a superseding requirement, finds one that explicitly declines to supersede anything beyond §5 and §10, and ships a matcher on `InvalidOptionValue`. The service returns `InvalidEnumValue` (`src/lib.rs:2634`, passing test [verified]).

Fix: add the § CSV import clause to the supersession paragraph at `specs/enum-validation/spec.md:11`. No code change; `docs/SPEC.md` is frozen and must not be edited. Cost to name up front: this edits `specs/`, which voids the plan verdict per `AGENTS.md`, so the plan review re-runs. I judge that cost lower than publishing a contract that contradicts an authoritative frozen sentence about the endpoint it names, because no gate reads frozen prose and nothing would ever catch this later.

## Non-blocking

**2. One behaviour is pinned three times.** `src/render/mod.rs:10542-10608` (`invalid_enum_value_pins_spec_selection_and_allowed`, 57 lines) asserts status, code, message, `reason == None`, and `details` keys for the scalar out-of-range case. The strengthened assertions at `src/templates.rs:5708-5719` assert the same five things at the same layer, and the new HTTP test `src/lib.rs:2643-2688` asserts them a layer up, which is what task 3.1 actually asked for. The unit test is surplus to both. Not blocking: it is correct and cheap to run.

**3. Free rides if the delta is reopened for finding 1.** `specs/enum-validation/spec.md`'s third scenario is titled "An undeclared-enum edge still carries the old details shape", but nothing in it is undeclared and its assertion is scenario 1's `details` check restated. If that file is edited anyway, retitling it costs nothing.

## Checked and clean

- `InvalidOptionValue` appears nowhere in `src/` or `ui/` [verified]; remaining hits are `docs/SPEC.md`, `docs/adr/0052`, and archived change folders, all frozen or historical. `openspec/specs/template-inputs/spec.md:339,423` still carry the old code, which is correct: archive syncs the delta, and apply ends at implementation.
- `src/openapi.rs` does not enumerate the code, so the proposal's conditional schema edit is correctly a no-op.
- Status `422`, message `Invalid option selection`, and `details` keys `selection`/`allowed` are byte-identical (`src/errors.rs:203-214`); no reshape, which is what the first attempt was rejected for.
- The requirement's claim that the lenient path never raises this holds: `src/render/mod.rs:337-347` routes `ResolveMode::Lenient` to `resolve_and_coerce_default` before the enum branch.
- `normalize_option` (`src/render/mod.rs:1211-1224`) is genuinely unreachable from the API: every handler passes `None` (`src/api.rs:1254,2677,2681`), `src/batch.rs` threads no option, and `/api/import/csv` folds `option.<name>` columns into `data` (`src/api.rs:2765-2772`) rather than into an option map. Renaming rather than deleting it matches the design's decision and leaves #214 its work.
- The `reason`-slug consistency test (`src/errors.rs:663-700`) reads only §10.1, so the rename does not touch it, and no test pins the §10 code table.
- Worktree is clean apart from the five modified files and the untracked change folder; `.agent-runs/` is gitignored (`.gitignore:37`).

VERDICT: REVISE
