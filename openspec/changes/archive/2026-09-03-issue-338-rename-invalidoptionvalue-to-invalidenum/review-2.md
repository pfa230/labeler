## Plan review: issue-338-rename-invalidoptionvalue-to-invalidenum

Context: `review.md` records `SPECS_SHA256: 8bee5d4e…`; the delta now digests to `4e8cba50…` [verified via `.workflow/specs-digest.sh`], because the author added the § CSV import supersession to `specs/enum-validation/spec.md:11` after `diff-review-1.md` raised it. That edit voided the prior verdict, which is why this review runs. `openspec validate --strict` passes [verified].

### Checked and clean

- The three superseded frozen sites are the only ones. `grep -n InvalidOptionValue docs/SPEC.md` returns exactly `567`, `683`, `1069` [verified], and the quoted text at `specs/enum-validation/spec.md:11` matches all three byte for byte. The CSV clause is quoted narrowly enough to leave the sibling "declared `default`" sentence at `docs/SPEC.md:1068` authoritative, which is correct.
- The `BatchInvalid` half of `docs/SPEC.md:1069` is restated at `specs/enum-validation/spec.md:34` rather than dropped, so superseding the whole sentence loses nothing.
- The §10.1 claim holds: no row exists for this code [verified], and `src/errors.rs:203-214` builds `details` with no `reason`.
- The `MODIFIED` on `template-inputs` is a faithful complete restatement. `diff` against `openspec/specs/template-inputs/spec.md:251-453` shows exactly the two intended edits and nothing else [verified]. The requirement heading matches the published one at `:251`, so archive resolves it by name.
- The contract matches the code. `details.selection` is the offending name to the supplied string and `details.allowed` is `values.clone()` in declared order (`src/render/mod.rs:350-358`) [verified]. `ParamType::List` is a separate type with no enum-member coercion (`src/render/mod.rs:68-78`), so the requirement's trigger ("a parameter declared `type: enum`") is complete, not merely the common case.
- `batch-validation` already governs all three endpoints the requirement names (`openspec/specs/batch-validation/spec.md:12`) and already specifies `{ index, code, reason?, message }` in ascending index order (`:41-42`), so delegating the entry shape is sound and scenario 2's `failures[0]` is unambiguous once that is read.
- All three issue acceptance criteria map to scenarios and tasks.

### Findings

**1. `proposal.md` and `design.md` still describe a two-site supersession; the delta now names three.**

`specs/enum-validation/spec.md:11` supersedes `docs/SPEC.md:566-567`, `:683` **and** `:1069`. `proposal.md:11` names only the first two. `design.md:5` describes only the first two as the frozen documentation, `design.md:19` says "it supersedes the frozen §5 sentence and §10 row", and `design.md:20` says "the `ADDED` requirement names `docs/SPEC.md:566-567` and `docs/SPEC.md:683`" and then "Every other row of those tables remains authoritative". `grep -n "1069\|CSV\|csv"` over both files returns nothing [verified].

This is not cosmetic. `AGENTS.md` keeps `proposal.md` and `design.md` permanently under `openspec/changes/archive/` as the only account of why a change was made, now that `docs/adr/` is frozen. A later reader asking why `docs/SPEC.md` § CSV import stopped being authoritative finds a design that says it never was touched. `design.md:20`'s framing is also structurally wrong for the third site: the CSV clause is prose, not a table row.

**2. `design.md:28` tells the implementer to sweep `openspec/specs`, which apply is forbidden to write.**

> Mitigation: implementation task explicitly covers `src/`, `ui/src/`, `openspec/specs/template-inputs`, and `docs/AUTHORING.md:753,766`

`openspec/config.yaml` (`operations.apply.guidance`) says "do not sync deltas into `openspec/specs/`", and `AGENTS.md` records that `archive-merge-check.sh:141` refuses a commit changing a published spec with no delta behind it. Following `design.md:28` literally produces a hand-edit of `openspec/specs/template-inputs/spec.md:339,423` that the landing gate refuses. `tasks.md` does not carry that target and the implementer did not do it (`git status` shows `openspec/specs/` untouched [verified]), so the misdirection cost nothing this round, but the design is the record and it currently points at the wrong tree.

**3. Both call-site line references are wrong, and `design.md` contradicts itself about one of them.**

At the base commit the two sites are `src/render/mod.rs:356` (`invalid_option_value` in the strict enum branch) and `src/render/mod.rs:1219` (inside `normalize_option`, declared at `:1211`) [verified via `git show HEAD:src/render/mod.rs`]. The rename shifted no lines, so those numbers hold before and after.

`proposal.md:5`, `proposal.md:28`, `design.md:5` and `tasks.md` task 1.2 all cite `:315` and `:1169`, inherited from the issue body. At the base commit, line 315 sits in the *list*-parameter lenient fallback and line 1169 sits in the sheet-composition `writeln!` error map. Neither raises this error. Meanwhile the `design.md` Decisions bullet "Leave the dead `normalize_option` path renamed, not deleted" cites `src/render/mod.rs:1219`, which is correct, so the design gives two different lines for the same call site. `AGENTS.md` requires findings and evidence to carry a real `file:line`; the plan's own evidence does not resolve.

### Not raised

`specs/enum-validation/spec.md:23,25,30,58` state parts of the contract by reference to the removed name ("stays byte-identical to the value the service returned under the previous code", "the two keys the previous code carried", "proving the rename did not reshape the object"). I considered this against the first-touch rule's "complete post-change contract, not the difference" (`AGENTS.md:23-24`) and dropped it: the table at `:19-21` and the bullets at `:27-28` already state the whole contract independently, nothing is underspecified, and the same register is normal across published specs here (`request-data-keys/spec.md:138,216,221,226`, `conditional-visibility/spec.md:81`). Redundancy dressed up as a rule violation is not a finding.

### Required changes

None of these touch `specs/`, so `SPECS_SHA256` stays `4e8cba50…` and no implementation or test changes follow.

1. In `proposal.md:11`, add the third superseded site to the sentence, so it reads that the `ADDED` requirement supersedes the frozen `docs/SPEC.md` §5 enum-validation sentence (`docs/SPEC.md:566-567`), the `InvalidOptionValue` row of the error-code table in `docs/SPEC.md` §10 (`docs/SPEC.md:683`), and the CSV import clause in `docs/SPEC.md` § CSV import (`docs/SPEC.md:1069`) reading "and a disallowed enum value fails the row (`BatchInvalid` / `InvalidOptionValue`)".
2. In `design.md:5`, add `docs/SPEC.md:1069` to the list of frozen sites documenting this code, quoting that clause.
3. In `design.md:19`, change "it supersedes the frozen §5 sentence and §10 row" to "it supersedes the frozen §5 sentence, the §10 row and the § CSV import clause".
4. In `design.md:20`, replace the sentence with one naming all three sites: the `ADDED` requirement names `docs/SPEC.md:566-567`, `docs/SPEC.md:683` and `docs/SPEC.md:1069`, notes that §10.1 has no row for this code so no row is superseded there, and leaves every other row of §10, every row of §10.1 and the rest of § CSV import authoritative. Drop the "rows" framing, since the CSV site is prose.
5. In `design.md:28`, replace `openspec/specs/template-inputs` with `openspec/changes/issue-338-rename-invalidoptionvalue-to-invalidenum/specs/template-inputs/spec.md`, and add that `openspec/specs/` is written by archive and must not be edited by the implementer.
6. Correct the call-site line numbers to `src/render/mod.rs:356` (strict enum coercion) and `src/render/mod.rs:1219` (`normalize_option`, declared at `src/render/mod.rs:1211`) in all four places that carry the stale pair: `proposal.md:5` (both refs), `proposal.md:28` ("the two call sites at `:315` and `:1169`"), `design.md:5` (both refs), and `tasks.md` task 1.2 (both refs). Leave the already-correct `src/render/mod.rs:1219` in the `design.md` Decisions bullet as is.

VERDICT: APPROVE_WITH_CHANGES
