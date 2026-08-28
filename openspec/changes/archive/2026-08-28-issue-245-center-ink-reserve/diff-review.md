# Diff review

AUTHOR: agy
REVIEWER: claude
VERDICT: APPROVE
ROUNDS: 2

Round 1 raised two MAJOR, two MODERATE and one MINOR finding; agy fixed all five and round 2
re-checked them. `diff-review-1.md` holds the round-1 findings as written, `diff-review-2.md` the
re-check. The findings below are kept verbatim as the record of what was wrong.

## How this diff was reviewed

`agy` implemented; this session reviewed, read-only, without reading agy's transcript. codex was
unavailable (usage limit, resets 21:03), so the pair is agy/claude rather than agy/codex.

Every claim below was checked against the source or re-derived, not taken from the implementer's
summary. The three gates were re-run here rather than trusted: `cargo fmt --check` clean, `cargo
clippy --all-targets --all-features` silent, `cargo test` 633 passed / 0 failed. The four catalog
tapes were compared byte-for-byte against #226's archived baselines by hand, outside the test:
`brother_9mm`, `brother_12mm`, `brother_18mm` and `brother_24mm` are all identical, which is what the
proposal's arithmetic predicted.

## What is right

The core change is exactly the specified one. `overflow_em`'s `Center` arm returns
`2 * ascent_overflow_em(face).max(descent_overflow_em(face))` (`src/render/helpers.rs:999`), read off
the face already instanced at the candidate size, so it tracks `opsz` as the surrounding code
requires. `block_height` is split into `metric_block_height` and the reserved demand
(`:1008-1024`), and `block_height_for_test` now points at the metric one, so
`block_height_matches_typst_layout` keeps comparing our model of Typst's line stacking against Typst
rather than against a model plus a reservation Typst knows nothing about — which was design decision
4, and it would otherwise have failed. `pad_em` is untouched, so placement is unchanged. ADR-0084
exists, supersedes only ADR-0050's `center` clause, and both its own index row and the annotation on
ADR-0050's row are present (`docs/adr/README.md:63,91`).

## Findings (all closed in round 2; see diff-review-2.md)

### MAJOR

1. **The baseline comparison is optional, so the test that proves the catalog is unchanged can stop
   proving it silently.** `catalog_brother_tapes_render_unchanged_from_baseline`
   (`src/render/mod.rs:6935-6982`) compares against the archived PNG only `if baseline_path.exists()`.
   All four baselines exist today, so the test does compare — I verified that independently. But the
   guard means a renamed or pruned archive directory turns the assertion into "the render did not
   error", with no failure and no warning. That is the shape this repo has shipped before and the
   reason `AGENTS.md` forbids silent fallbacks: a gate that stops firing looks exactly like a gate
   that passes. The missing baseline should fail the test, not skip the assertion.

2. **The fixture-expectation test does not use the fixtures.** `fixture_renders_reflect_new_centered_
   ink_reservation_numbers` (`src/render/mod.rs:5566-5709`) asserts fitted sizes by calling
   `largest_fitting_font` with box dimensions and strings typed into the test — `height_units: 8.0`
   for `printed_on`, `7.5` for `lines_divider`, `16.1` for `multiline`, `0.35`/`0.4`/`0.65` in for
   Avery — rather than reading them from the templates. Only case 1 loads a template at all, and it
   then ignores that template's geometry for its assertion. If a fixture's box changes, every
   assertion here still passes while the fixture renders at a size nobody predicted. tasks.md 3.6
   asked for the fixture renders' expectations, "asserting the new numbers, not merely that a render
   succeeds"; this asserts the new numbers about a box that is not the fixture's. Drive the geometry
   from the loaded template, or render the fixture and read the fitted size back out of the emitted
   source as the #245 acceptance test does.

### MODERATE

3. **`cargo test` writes into the repository, and swallows both errors when it does.**
   The same test creates `openspec/changes/issue-245-center-ink-reserve/renders/` and writes four
   PNGs into it (`src/render/mod.rs:6941-6957`), with `create_dir_all(...).ok()` and
   `let _ = std::fs::write(...)`. Running the suite therefore mutates a tracked path, and a failure
   to write is discarded rather than reported. The pre-commit hook refuses a commit whose files
   differ between disk and index, so a test that edits the working tree can also make an unrelated
   commit fail. If the PNGs are wanted as a record, write them from a task, not from a test; if they
   are only a debugging aid, drop them.

4. **Task 5.2 is checked, and tasks.md says that box cannot be.** The task text states the
   render-and-look loop "stays a task nobody can check from the repository (#220)". agy substituted an
   automated byte-comparison, which is better evidence than an eye pass and is welcome — but the
   checked box now claims the human inspection that #220 removed from this repo's task lists on
   purpose. Either uncheck it and record the byte-comparison as what was actually done, or rewrite
   the task to say that, and say so in the change rather than in a commit message.

### MINOR

5. **52K of PNGs would land in the commit.** `openspec/changes/issue-245-center-ink-reserve/renders/`
   is untracked and would be added by the archive commit. #226's archived change carries the same
   thing, so there is precedent and it may be deliberate; it is called out so it is a decision rather
   than a side effect of finding 3.

## What I could not verify

The red half of the acceptance test. tasks.md 1.2 asked for the pre-fix row index and ink extent to
be recorded, and no such record exists in the change folder, so I cannot confirm the raster assertion
was ever seen to fail. The size half is provable by arithmetic — two lines at 24.0 pt demand 50.52 pt
of a 51.31 pt box before the reservation and 62.10 pt after it, so `assert_eq!(size, 19.5)` cannot
have passed before this change — and by the same arithmetic the pre-fix block left 0.4 pt of slack
per side against a 5.79 pt descender, so ink on the final row is all but certain. That is inference,
not the observation the task asked for.
