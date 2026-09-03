TREE_SHA256: db5b1732375bc8839cd876b51a660718de5c267796d8190b3b68330c76ccfa43
SPECS_SHA256: 73f475b073e71565a1bbd354e97f3cacc317310bf6d6fba0e2227032e13b8013

# Diff review: issue-338-rename-invalidoptionvalue-to-invalidenum

No `ANSWERS.md` at the worktree root. I edited nothing; `git status` is unchanged from when I started.

## Gates, run here

`cargo fmt --check` exit 0, `cargo clippy --all-targets --all-features` exit 0, `cargo test` 858 passed / 0 failed / 2 ignored [verified]. `openspec validate --changes --strict` passes (CLI 1.9.0 at `/home/pfa/.local/bin/openspec`) [verified].

## Acceptance criteria, against `.agent-runs/issue-338.md`

1. Render request outside `values` returns `InvalidEnumValue` with the same status, message and `details` keys/values, pinned by a test: `src/lib.rs:2643` `render_enum_out_of_range_is_422_invalid_enum_value`, run individually and passing [verified].
2. Batch row reports it per-row, pinned by a test: `src/lib.rs:2693` `batch_enum_out_of_range_reports_invalid_enum_value_per_row`, passing [verified].
3. `InvalidOptionValue` appears nowhere in `src/` or `ui/src/`: `grep -rn` over both returns nothing [verified]. Remaining repo hits are `docs/SPEC.md`, `docs/adr/0052`, archived change folders, and `openspec/specs/template-inputs/spec.md`, all frozen, historical, or archive's to write.

## Checked and clean

- Both call sites renamed and nothing else: `src/render/mod.rs:356` (strict enum branch) and `src/render/mod.rs:1219` (`normalize_option`). `src/errors.rs:203-214` keeps `422`, `"Invalid option selection"`, and `details` `{selection, allowed}` byte-identical, still through unreasoned `new()`, so no `reason` key [verified].
- The previous round's blocking finding is fixed. `specs/enum-validation/spec.md:11` now names the `docs/SPEC.md:1069` CSV import clause and bounds its supersession there, including the `:1068` default clause of the same sentence. All four quotations in that note match `docs/SPEC.md` byte for byte under whitespace normalisation [verified by script].
- All eight `APPROVE_WITH_CHANGES` items in `review.md` are applied, including the corrected `:356`/`:1219` call-site references in `proposal.md`, `design.md` and `tasks.md` task 1.2, and `CHANGES_APPLIED: yes` is set.
- The `template-inputs` MODIFIED delta reproduces the published requirement with exactly two changed lines, both intended, heading identical so archive resolves it by name [verified by diff against `openspec/specs/template-inputs/spec.md`].
- I sandbox-archived the change in a `/tmp` copy of the `openspec` tree (the repo was not touched): `enum-validation` syncs to a single `# enum-validation Specification` H1, so the delta's own H1 does not duplicate, and `template-inputs` gains only the two intended line changes plus three blank lines from archive's own normalisation [verified].
- `normalize_option` is genuinely unreachable: `src/api.rs:2677,2681` and `src/batch.rs:105-106` pass `None`, and `/api/import/csv` folds `option.<name>` columns into `data` at `src/api.rs:2765-2772` rather than into an option map [verified]. Renaming rather than deleting matches `design.md` and leaves #214 its work.
- The requirement's claim that the lenient path never raises this holds: `src/render/mod.rs:338-347` routes `ResolveMode::Lenient` into `resolve_and_coerce_default`, which returns only `param_default_unresolvable` or `internal` (`src/render/mod.rs:639-640`) [verified].
- `enum-validation` does not exist under `openspec/specs/`, so `ADDED` is the right first-touch operation. `src/openapi.rs` never enumerated the code. The UI never matched on it (`ui/src` matches only `BatchInvalid`). Per-row `index` attribution stays covered by `src/batch.rs:320`.
- Branch is level with `origin/main` (`git rev-list --count HEAD..origin/main` is 0), so no pre-review rebase is owed. `openspec/specs/` is untouched in the working tree, which is correct: apply ends at implementation.

## Non-blocking

**1. A tautological assertion.** `src/lib.rs:2739` asserts `body["error"]["details"].get("failures").is_some()` after `failures` was already extracted with `.expect("failures")` at `src/lib.rs:2731-2734`. It cannot fail, and the comment above it claims to check the top-level shape was not reshaped, which it does not do.

**2. The behaviour is now pinned four times.** `src/render/mod.rs:10618-10637` (strengthened coercion-matrix case), `src/render/mod.rs:10691-10756` (new 57-line dedicated unit test), `src/templates.rs:5779-5790` (strengthened) and `src/lib.rs:2643-2691` (HTTP, which is what task 3.1 asked for) all assert the same five facts. The dedicated unit test at `:10691` is the surplus one: it calls the same `resolve_parameters_mode` at the same layer as `:10618`. `diff-review-1.md` raised this as its finding 2 and it was neither applied nor justified with file:line evidence, which `AGENTS.md` asks for.

**3. Redundant assertion inside that test.** `src/render/mod.rs:10749-10755` collects the `details` keys into a `BTreeSet` and compares to `["allowed", "selection"]`, which restates the `len() == 2` check plus the two key lookups three lines above.

## Limitation

`.workflow/` in this checkout holds only `loop`; `specs-digest.sh`, `review-gate-check.sh`, `archive-merge-check.sh` and the suites are absent. I could not recompute the `SPECS_SHA256: 73f475b0…` recorded in `review.md` against the current `specs/`, nor run the gate scripts. Everything above is verified by other means. Separately, `diff-review-1.md` carries a `TREE_SHA256:` but no `SPECS_SHA256:`; per `AGENTS.md` (#362) a round artifact without a contract digest simply does not trigger the identical-bytes refusal, so that is benign, and it is driver output rather than anything in this diff.

Nothing here must be fixed before this lands.

VERDICT: APPROVE
