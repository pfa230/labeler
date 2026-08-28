# Diff review, round 2 (re-check)

AUTHOR: agy
REVIEWER: claude
VERDICT: APPROVE

Round 1's five findings, re-checked against the source. The scope of this round is those five items
and the code that changed to address them; nothing else was re-reviewed.

1. **MAJOR 1, baseline comparison was optional — fixed.** `std::fs::read(&baseline_path)` now panics
   with the path when a baseline is missing (`src/render/mod.rs:6959-6961`), so a pruned archive
   directory fails the test instead of silently deleting the assertion.

2. **MAJOR 2, fixture test did not use the fixtures — fixed, and properly.** agy split
   `compile_label_doc` into `compile_label_source` returning the emitted Typst plus its image files,
   and a thin wrapper that compiles it (`src/render/mod.rs:460-500`). The fixture test now loads
   `brother_24mm_printed_on`, `brother_24mm_lines_divider` and `brother_24mm_multiline` from the
   registry, renders each with real data, and reads the fitted size back out of the emitted source.
   The box geometry is the template's own, so changing a fixture now moves the assertion.
   The refactor is behavior-neutral: same body, same order, split at the `compile_paged` call.

3. **MODERATE 3, tests wrote into the repository — fixed.** Both `create_dir_all(...).ok()` and
   `let _ = std::fs::write(...)` are gone; the test only reads.

4. **MODERATE 4, task 5.2 claimed an unverifiable eye pass — resolved, by the plan's author rather
   than by agy.** agy unchecked the box, which was the right response to the finding but left an
   unchecked task, and archive forbids those. The contradiction was mine: the task text demanded a
   loop #220 says cannot be claimed. It has been rewritten to state what is actually checkable — byte
   equality against the archived baselines, enforced by a test that fails on a missing one — and
   checked. This is a change to `tasks.md`, not to `specs/`, so the plan review's digest still holds.

5. **MINOR 5, PNGs would land in the commit — fixed.** `renders/` is gone from the change folder.

Gates re-run here, not taken from the implementer: `cargo fmt --check` clean,
`cargo clippy --all-targets --all-features` 0 warnings, `cargo test` 633 passed / 0 failed.
The four catalog tapes were also compared by hand against #226's baselines: all identical.

No new findings. The one thing round 1 could not verify is unchanged and stays on the record there:
no observation of the acceptance test failing before the fix was kept, only the arithmetic that says
it must have.
