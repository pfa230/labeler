## 1. Report the declared length

- [ ] 1.1 In `src/render/mod.rs`, replace `RenderedSingle.width_mm` with `y_dimension: i32`: `(pixmap.width() * 2540).div_ceil(dpi)` for a PNG, using the DPI passed to `typst_render::render`, and `driver::hundredths_mm` of the page width for a PDF. Verify with task 2.1.
- [ ] 1.2 In `src/batch.rs`, replace `PrintUnit.width_mm` with `y_dimension: Option<i32>`, and in `src/api.rs` pass it into `MediaSize.y_dimension` as is. Verify with `cargo build`.

## 2. Tests

- [ ] 2.1 In `src/render/mod.rs`, replace the ±1 px tolerance check in `a_rendered_single_label_reports_its_resolved_width` with: a 20 mm `width_template` rendered as PNG with `resolution_dpi: Some(180)` is 142 px wide and reports `y_dimension` `2004`, and as PDF reports `2000`. Keep the 62.5 mm (`6250`) and 2 in (`5080`) checks for PDF, and the content-sized check that the longer message reports the larger `y_dimension`. Verify the test fails with the PNG branch temporarily set to `hundredths_mm` of the page width (2000) and with both rasterization and the length calculation at the template's 200 DPI instead of the rasterized 180 DPI (157 px, 1994), then passes with task 1.1's value.
- [ ] 2.2 Update the `src/batch.rs` and `src/lib.rs` tests that read `width_mm` to read `y_dimension`, keeping their comparisons; in `every_job_carries_the_template_media_size` render the expected value with `SingleKind::Pdf` instead of `SingleKind::Png`, preserving its assertions, and verify `cargo test` passes.

## 3. Gates

- [ ] 3.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test` and `npx --no-install openspec validate --all --strict --no-interactive`, and verify all pass.
