# Plan review

AUTHOR: opencode
REVIEWER: claude
VERDICT: APPROVE_WITH_CHANGES
ROUNDS: 3

## Plan review: issue-338-rename-invalidoptionvalue-to-invalidenum

**State.** `review.md` currently holds round 2's `APPROVE_WITH_CHANGES` (written 11:50:15) with six required changes. None have been applied: `proposal.md` and `design.md` both carry mtime 10:42:24, `CHANGES_APPLIED:` is absent from `review.md:1-6`, and every defect that verdict named is still present [verified]. Since this round overwrites `review.md`, I have re-verified those six independently and restate the still-valid ones below so they are not lost, plus two of my own against `specs/`.

`openspec validate --changes --strict` passes [verified]. `.workflow/specs-digest.sh` returns `4e8cba50…` [verified]; `review.md` carries no `SPECS_SHA256:` field at all, which the driver writes after the verdict, so I treat that as driver state and not a plan defect.

### Checked and clean

- The frozen sites are exactly three. `grep -n InvalidOptionValue docs/SPEC.md` returns `567`, `683`, `1069` [verified], and each quotation at `specs/enum-validation/spec.md:11` matches its source byte for byte, including the trailing period on the CSV clause.
- `enum-validation` does not exist under `openspec/specs/` [verified: `ls openspec/specs/`], so `ADDED` is the correct first-touch operation, and the `MODIFIED` on `template-inputs` resolves against a requirement that does exist.
- The `MODIFIED` restatement is complete and faithful. Diffed against `openspec/specs/template-inputs/spec.md:251-453`: 203 lines each, exactly two changed lines, both intended, heading identical so archive resolves it by name [verified].
- The contract matches the code. `src/render/mod.rs:351-358` builds single-entry `selection` (name to supplied string) and `allowed` (name to `values.clone()`, declared order); `src/errors.rs:203-214` emits status `422`, message `Invalid option selection`, and `details` with no `reason` [verified]. `docs/SPEC.md` §10.1 has no row for this code [verified].
- The per-row delegation is sound. `openspec/specs/batch-validation/spec.md:11-13` already governs `POST /api/batch`, `POST /api/print` and `POST /api/import/csv`, and `:41-45` already fixes the `{ index, code, reason?, message }` shape and the "`reason` present exactly when the code carries one" rule [verified].
- The endpoint spellings are right: `src/api.rs:275,278,312` nest `/print` and `/import/csv` under `/api` [verified], so the delta's `/api/import/csv` is correct even though `docs/SPEC.md:1064` writes it without the prefix.
- All three issue acceptance criteria map onto scenarios and tasks.

### Findings

**1. `specs/enum-validation/spec.md:11` bounds its supersession for §10 and §10.1 but leaves § CSV import unbounded.**

The note closes with "It supersedes no other row of that table and no other part of §10 beyond the named sites, and it supersedes no row of `docs/SPEC.md` §10.1 … Every other row of §10 and every row of §10.1 remains authoritative under the frozen spec." Nothing says the same for § CSV import.

That silence is not neutral, because `AGENTS.md` states precedence at section granularity: a frozen section "stays authoritative until an OpenSpec requirement explicitly names and supersedes it, and then only for that section." The requirement names `docs/SPEC.md` § CSV import. A reader applying that sentence literally, and noticing that the author bothered to disclaim scope for the two other sections and not this one, can conclude the whole section is displaced. The `ADDED` requirement restates none of what else that section carries: BOM stripping, `csv`-crate quoting, output composition through the shared `/batch` path, and, in the very same sentence at `docs/SPEC.md:1068`, "Any declared parameter the CSV omits defaults to its declared `default` value." Calling the target "the CSV import sentence" makes that last one worse, since the clause being superseded is the second half of the sentence whose first half states the default rule.

Failure scenario: a reader asks what `POST /api/import/csv` does with a column the CSV omits, reads `docs/SPEC.md` § CSV import, checks `openspec/specs/enum-validation`, finds a requirement that names that section as superseded and disclaims nothing about it, and concludes the default rule has no authority anywhere.

**2. `specs/enum-validation/spec.md:32` states the lenient path's fallback in a way `template-inputs` contradicts.**

> The lenient `POST /api/templates/{id}/inputs` path SHALL NOT raise it; it absorbs the uncoercible value and falls back to the declared default per `template-inputs` and `param-resolution`.

`template-inputs` says the opposite for the no-default case. The delta's own `specs/template-inputs/spec.md:26-30` reads "SHALL be treated as though the label did not carry that name at all … so the parameter takes its declared `default` if it has one and is otherwise absent", and its scenario at `:110-115` pins exactly that: `orientation: ""` on an enum "declaring `values: [horizontal, vertical]` and no default" returns `200` with `required: true`, no `default`, and the parameter absent for gate evaluation.

So one capability says the lenient path falls back to the declared default, and another says it does so only when there is one. This is the "spec duplication drift" `design.md:29` names as a risk, landing in the same change that names it.

Failure scenario: an implementer or test author reads `enum-validation` for the lenient contract, sends an out-of-range enum for a parameter with no `default:`, and expects a fallback value where `template-inputs:110-115` requires the parameter to be absent.

**3. `proposal.md` and `design.md` describe a two-site supersession; the delta names three.**

`specs/enum-validation/spec.md:11` supersedes `docs/SPEC.md:566-567`, `:683` **and** `:1069`. `proposal.md:11` names only the first two. `design.md:5` lists only the first two as the frozen documentation; `design.md:19` says "it supersedes the frozen §5 sentence and §10 row"; `design.md:20` says the requirement "names `docs/SPEC.md:566-567` and `docs/SPEC.md:683`" and then "Every other row of those tables remains authoritative". `grep -n "1069\|CSV\|csv"` over both files returns nothing [verified].

This is not cosmetic. With `docs/adr/` frozen, `AGENTS.md` makes `proposal.md` and `design.md` the permanent account of why, kept under `openspec/changes/archive/`. A later reader asking why § CSV import stopped being authoritative finds a design asserting it was never touched. `design.md:20`'s "rows" framing is also structurally wrong for the third site, which is prose and not a table row.

**4. `design.md:28` directs the implementer at `openspec/specs/`, which apply is forbidden to write.**

> Mitigation: implementation task explicitly covers `src/`, `ui/src/`, `openspec/specs/template-inputs`, and `docs/AUTHORING.md:753,766`

`openspec/config.yaml` (`operations.apply.guidance`) forbids syncing deltas into `openspec/specs/`, and `AGENTS.md` records that `archive-merge-check.sh:141` refuses a commit changing a published spec with no delta behind it. Followed literally, this produces a hand-edit of `openspec/specs/template-inputs/spec.md` that the landing gate refuses. `tasks.md` does not carry that target and the implementer did not do it (`git status` shows `openspec/specs/` untouched [verified]), so it cost nothing this round, but the archived design points at the wrong tree.

**5. Both call-site line references are wrong, and `design.md` contradicts itself about one of them.**

At the base commit `2603e1f` the two sites are `src/render/mod.rs:356` (`invalid_option_value` in the strict enum branch) and `src/render/mod.rs:1219` (inside `normalize_option`, declared at `:1211`) [verified via `git show HEAD:src/render/mod.rs`]. The rename shifted no lines.

`proposal.md:5`, `proposal.md:27`, `design.md:5` and `tasks.md` task 1.2 all cite `:315` and `:1169`, inherited verbatim from the issue body (`.agent-runs/issue-338.md:4-6`). At the base commit, line 315 sits inside a `position ` prefix branch of the lenient fallback and line 1169 sits in the sheet-composition `writeln!` error map [verified]; neither raises this error. Meanwhile `design.md:22` cites `src/render/mod.rs:1219` for the same call site and is correct, so the design gives two different lines for one site. `src/errors.rs:18,203` is correct throughout.

### Not raised

- `specs/enum-validation/spec.md:23,25,30,58` state parts of the contract by reference to the removed name. The table at `:19-21` and the bullets at `:27-28` already state the whole contract standalone, so nothing is underspecified, and the register is normal in published specs here.
- Scenario 2 (`:46-53`) leaves the passing first label's `data` unspecified, so `failures[0]` is index 1 only if that label passes. `batch-validation:41-42` fixes ascending-index ordering and the scenario says the second label is the one carrying the bad value; a reader cannot reasonably read the first as failing too.
- Task 3.2 pins `POST /api/batch` only and argues `/api/print` and `/api/import/csv` follow "by the same constructor". Both go through the shared dispatch at `src/api.rs:2311` [verified], and the acceptance criterion asks for a batch row. Stating the reasoning in the task is the honest form of that.
- `specs/template-inputs/spec.md:90-91` still says "the code that path already returns". That is a general framing over five codes, not a historical claim about this one, and it is published text the delta is not otherwise touching.

### Required changes

Items 1 and 2 edit `specs/`, which is permitted here and moves `SPECS_SHA256`; the digest is written after these edits. Neither changes behaviour, so no implementation or test change follows from any item below.

1. In `specs/enum-validation/spec.md:11`, after the sentence "Every other row of §10 and every row of §10.1 remains authoritative under the frozen spec.", add: "It supersedes nothing else in § CSV import. The rest of that section, including the clause of the same sentence at `docs/SPEC.md:1068` reading \"Any declared parameter the CSV omits defaults to its declared `default` value\", remains authoritative under the frozen spec." Also change "the CSV import sentence in `docs/SPEC.md` § CSV import (`docs/SPEC.md:1069`)" earlier in the same note to "the CSV import clause in `docs/SPEC.md` § CSV import (`docs/SPEC.md:1069`)", leaving the quotation that follows it unchanged.

2. In `specs/enum-validation/spec.md:32`, replace the second sentence with: "The lenient `POST /api/templates/{id}/inputs` path SHALL NOT raise it; it treats the out-of-range value as though the label did not carry that name at all, so the parameter takes its declared `default` if it has one and is otherwise absent, per `template-inputs` and `param-resolution`." Leave the first sentence of that paragraph as is.

3. In `proposal.md:11`, add the third superseded site so the sentence reads that the `ADDED` requirement supersedes the frozen `docs/SPEC.md` §5 enum-validation sentence (`docs/SPEC.md:566-567`), the `InvalidOptionValue` row of the error-code table in `docs/SPEC.md` §10 (`docs/SPEC.md:683`), and the CSV import clause in `docs/SPEC.md` § CSV import (`docs/SPEC.md:1069`) reading "and a disallowed enum value fails the row (`BatchInvalid` / `InvalidOptionValue`)".

4. In `design.md:5`, add `docs/SPEC.md:1069` to the list of frozen sites documenting this code, quoting that clause, alongside the existing `:566-567` and `:683` quotations.

5. In `design.md:19`, change "it supersedes the frozen §5 sentence and §10 row" to "it supersedes the frozen §5 sentence, the §10 row and the § CSV import clause".

6. In `design.md:20`, replace the bullet with one naming all three sites: the `ADDED` requirement names `docs/SPEC.md:566-567`, `docs/SPEC.md:683` and `docs/SPEC.md:1069`, notes that §10.1 has no row for this code so no row is superseded there, and leaves every other row of §10, every row of §10.1 and the rest of § CSV import authoritative. Drop the "rows" framing, since the CSV site is prose.

7. In `design.md:28`, replace `openspec/specs/template-inputs` with `openspec/changes/issue-338-rename-invalidoptionvalue-to-invalidenum/specs/template-inputs/spec.md`, and add that `openspec/specs/` is written by archive and must not be edited by the implementer.

8. Correct the call-site line numbers to `src/render/mod.rs:356` (strict enum coercion) and `src/render/mod.rs:1219` (`normalize_option`, declared at `src/render/mod.rs:1211`) in all four places carrying the stale pair: `proposal.md:5` (both refs), `proposal.md:27` ("the two call sites at `:315` and `:1169`"), `design.md:5` (both refs), and `tasks.md` task 1.2 (both refs). Leave the already-correct `src/render/mod.rs:1219` in the `design.md:22` Decisions bullet as is, and leave `src/errors.rs:18,203` as is.

The author applies these changes, sets `CHANGES_APPLIED: yes`, and NO further review follows.

CHANGES_APPLIED: yes
SPECS_SHA256: 73f475b073e71565a1bbd354e97f3cacc317310bf6d6fba0e2227032e13b8013
