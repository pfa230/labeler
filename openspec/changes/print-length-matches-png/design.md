## Context

`render_single_label_as` (`src/render/mod.rs`) returns the label's bytes and `width_mm`, the Typst page width. `batch.rs` copies `width_mm` into each `PrintUnit`, and `api.rs` turns it into `y-dimension` with `driver::hundredths_mm` (round to nearest). `width_mm` has no other consumer.

`typst-render` sizes the pixmap as `(pixel_per_pt * size).round()`, so a PNG's width is the label's length in dots rounded to nearest. The page's long side is its width, so the PNG's width is its length along the feed. ptouch-print-server converts `y-dimension` back to dots with `floor(y * dpi / 2540)` and refuses a PNG longer than that. `hundredths_mm(px * 25.4 / dpi)` would still fail: rounding to the nearest hundredth can land below the pixel boundary, and at 180 DPI about 4/9 of pixel lengths would come back one dot short.

## Goals / Non-Goals

**Goals:** a PNG job's `y-dimension` truncates back to exactly the PNG's pixel width at the DPI it was rasterized at.

**Non-Goals:** changing the PNG's size, PDF jobs' `y-dimension`, or `x-dimension`.

## Decisions

**`RenderedSingle` reports `y_dimension: i32` in place of `width_mm: f64`.** For `SingleKind::Png` it is `(pixmap.width() * 2540).div_ceil(dpi)`, with `dpi` the same value passed to `typst_render::render`; for `SingleKind::Pdf` it is `driver::hundredths_mm(width_mm)`. `PrintUnit.width_mm: Option<f64>` becomes `y_dimension: Option<i32>`, and `api.rs` puts it into `MediaSize` unchanged. The render function is the only place that knows both the pixel width and the DPI it rasterized at, so computing the value there needs no second source of either.

For any DPI below 2540, `ceil(px * 2540 / dpi)` truncates back to `px`: it is at least `px * 2540 / dpi`, and less than that plus one hundredth, which is under one dot. Integer `div_ceil` is exact. A float ceiling of `px * 25.4 / dpi * 100` lands one hundredth high on representation error (130 px at 200 DPI gives 1651.0000000000002, so it declares 1652), which contradicts the spec's value.

Rejected alternatives:

- *Decode the PNG header in `api.rs` to read its width.* Needs the DPI the render chose passed alongside anyway, and adds a parse of bytes labeler just wrote. The render function already holds both numbers.
- *Keep `width_mm` and add `y_dimension`.* `width_mm` would have no consumer left.
- *Round `hundredths_mm` up for every length.* Changes PDF and `x-dimension` values, which the issue leaves alone, and is the float ceiling above.
- *Crop or size the PNG to the declared length.* Cuts the last dot of content and changes the render endpoint's output.

## Risks / Trade-offs

- A PNG job now declares up to half a dot plus one hundredth more than the label's template length (about 0.08 mm at 180 DPI). That is the length of the image actually sent, which is what the printer prints.

