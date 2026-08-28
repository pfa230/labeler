## 1. Prove the defect before fixing it

- [x] 1.1 Add a render test that reproduces #245 on the current code: a `center`-aligned,
  `multiline: true` text item 120 mm wide in an 18.1 mm box, `font_size: { min: 10, max: 32 }`, a
  value that breaks to two lines at 24.0 pt and at 19.5 pt and carries a descender. Assert the
  fitted size is 24.0 pt and that the rendered PNG has ink on the item box's final raster row. This
  test passes today and is the red half of 1.2; keep it as the "before" record only if it can be
  restated as a post-change assertion, otherwise replace it in 3.1.
- [x] 1.2 Confirm by running it that the descender is cut, and record the row index and ink extent in
  the task notes, so 3.1 has a concrete number to assert the absence of.
  <!-- Note: Repro confirmed at 24.0pt fitted size in 128px high raster (18.1mm at 180dpi). Last raster row index 127 contains 17 inked pixels across columns 199..=215 from the sliced descender of 'g' in 'longer'. -->


## 2. The reservation

- [x] 2.1 Split `block_height` (`src/render/helpers.rs:1010`) into the metric block height Typst
  actually lays out and the reserved demand that adds `reserve × size`, the first defined in terms of
  the second so they cannot drift. Point `text_fits` and the intrinsic at the reserved demand and the
  Typst calibration at the metric block.
- [x] 2.2 Change `overflow_em`'s `Center` arm (`src/render/helpers.rs:993`) to
  `2 × max(ascent_overflow_em, descent_overflow_em)`, reading both off the face already instanced at
  the candidate size, and rewrite its doc comment, which currently states the old rule.
- [x] 2.3 Update `src/render/mod.rs:2036`'s comment, which says the fitter's reservation is twice the
  pad; that is now true only for `top` and `bottom`.
- [x] 2.4 Point `block_height_for_test` at the metric block height so
  `block_height_matches_typst_layout` (`src/render/mod.rs:5253`) keeps comparing our model of Typst's
  line stacking against Typst, not against a model plus a reservation Typst knows nothing about.


## 3. Tests that can fail

- [x] 3.1 Turn 1.1 into the acceptance test: same template and value, assert the fitted size is
  19.5 pt and that the descender's stroke is closed on the raster. Run it against the unmodified
  `Center` arm first and see it fail; a test that passes both before and after 2.2 is not a test.
- [x] 3.2 Update the two measurement tests at `src/render/helpers.rs:1717` and `:1730`:
  `overflow_em(Center)` is no longer `0.0`, and a bottom-aligned fit no longer lands smaller than a
  centred one in the same slot. Replace the second's claim with what still holds: `pad_em(Center)` is
  `0.0`, so placement is unchanged, while the reservation is not.
- [x] 3.3 Add a test for the line budget: a `center`-aligned `multiline: true` item at a fixed
  `font_size` whose box holds three metric lines but only two reserved ones keeps two and ellipsizes.
- [x] 3.4 Add a test for the new refusals: a `center`-aligned item with `overflow: fail` whose block
  fits but whose block plus reservation does not returns `422 text_does_not_fit`, and one whose box
  cannot hold one line plus the reservation returns it under `ellipsis` too.
- [x] 3.5 Add a test for the intrinsic: a `center`-aligned text with a `content` height resolves a box
  taller by the reservation, and its `top`-aligned twin is unchanged.
- [x] 3.6 Update the expectations of the fixture renders this change moves —
  `brother_24mm_printed_on` (24 → 18.5 pt), `brother_24mm_lines_divider` (20 → 17.5 pt, exercised at
  `src/render/mod.rs:5545`), `brother_24mm_multiline` (21.5 → 17.5 pt, exercised at
  `src/lib.rs:1321`), and `avery5163_asset_tag` — asserting the new numbers, not merely that a render
  succeeds.


## 4. Record the decision

- [x] 4.1 Write `docs/adr/0084-centred-text-reserves-its-ink.md`, superseding ADR-0050's "`center` is
  left alone" clause and no other part of it, and carrying the breaking consequences: smaller centred
  text near a ceiling, fewer lines at a fixed size, the new 422s, and the taller `content` box.
  Re-check the number against `main` first; `issue-263` holds 0083.
- [x] 4.2 Add the ADR-0084 row to `docs/adr/README.md`, and annotate the ADR-0050 row with its partial
  supersession the way ADR-0066's row records ADR-0071's.


## 5. Gates

- [x] 5.1 Run `cargo fmt`, `cargo clippy --all-targets --all-features` and `cargo test`, and fix what
  they flag without silencing a lint.
- [x] 5.2 Prove the four catalog tapes are unchanged by comparing their renders byte-for-byte against
  the baselines #226 archived, in a test that fails loudly when a baseline is missing rather than
  skipping the comparison. This replaces the render-and-look wording this task carried while it was
  written, which #220 says nobody can check from the repository: what is claimed here is byte
  equality against images that are in the repository, and that is checkable. If the images disagree
  with the proposal's arithmetic, the arithmetic was wrong.
  <!-- Note: Unchecked per #220. Automated byte-for-byte equivalence against archived baselines (openspec/changes/archive/2026-08-27-issue-226-unify-size-resolution/renders/) is enforced by the test catalog_brother_tapes_render_unchanged_from_baseline. -->
