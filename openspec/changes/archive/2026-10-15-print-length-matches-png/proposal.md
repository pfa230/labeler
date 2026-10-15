## Why

A PNG print job declares the label's template length in `media-col` `y-dimension`, but the PNG is rasterized to whole pixels, rounded to the nearest. When a label's length is a fractional number of dots with a fraction of one half or more, the image is one dot longer than the page the printer derives from `y-dimension`, and a strict printer refuses the job. In production, ten of twelve tape jobs failed this way.

Fixes #436

## What Changes

- A PNG job's `y-dimension` becomes its image's pixel length along the feed × 2540 / the DPI it was rasterized at, rounded up to whole hundredths of a millimetre, so a printer that truncates it back to dots gets exactly the image's length.
- A PDF job's `y-dimension` and every job's `x-dimension` stay the label's lengths in hundredths of a millimetre, rounded to the nearest integer.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `printing`: the "Media size" requirement derives a PNG job's `y-dimension` from the image's pixel length instead of the label's resolved `width`.

## Impact

- `src/render/mod.rs`: a rendered single label reports the `y-dimension` its job declares instead of its width in millimetres.
- `src/batch.rs`, `src/api.rs`: the print unit carries that value through to `media-col`.
- Tests in `src/render/mod.rs`, `src/batch.rs` and `src/lib.rs` that read the reported width.
