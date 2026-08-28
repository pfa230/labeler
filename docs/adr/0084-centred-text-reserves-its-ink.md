# 84. Centred text reserves its ink

Date: 2026-08-28

## Status

Accepted. Issue [#245](https://github.com/pfa230/labeler/issues/245). Supersedes the "`center` is left alone" clause of [ADR-0050](0050-ink-reservation-at-slot-edges.md) and no other part of it. Builds on [ADR-0080](0080-unify-size-resolution.md) and [ADR-0082](0082-text-overflow-policy.md).

## Context

[ADR-0050](0050-ink-reservation-at-slot-edges.md) established ink reservation at slot edges for `Top` and `Bottom` alignments so that ink falling outside the cap-height-to-baseline line box (accents above, descenders below) remains inside the item's `clip: true` bounding box. However, ADR-0050 left `Center` alone under the assumption that centring already splits the slack and that reserving both sides would cost a full em on top of a 0.7275em line box.

That assumption fails when the auto-shrink fitter operates: the fitter's purpose is to consume the slack. When a centred multiline or single-line item fits tightly into its box, no slack remains, and descenders or accents are sliced off by the clip box (#245). Furthermore, the reserve is `2 × max(u, d)` = 0.4824em in Inter, not a full 1.0em, because the cap-height box already covers the metric body.

## Decision

1. **`Center` reserves ink in the fitter**:
   `overflow_em` returns `2 × max(ascent_overflow_em, descent_overflow_em)` for `VerticalAlign::Center`. Twice the larger overflow is reserved because a centred block is centred on its metric box, so the slack `(H − metric_block) / 2` on each side must absorb the overflow on that side. For bundled symmetric Inter (`u = d = 0.2412em`), this equals `u + d = 0.4824em`.

2. **Placement is unchanged**:
   `pad_em` remains `0.0` for `Center`. Centring the metric box already splits the slack evenly, so no `#pad` inset is emitted in Typst source.

3. **Unified reservation across fit and line budget**:
   The reservation is applied consistently wherever fit is judged: in `text_fits` (size search), in the one-line minimum floor check, and in `max_lines` (multiline line budget).

4. **Separation of metric block height from reserved demand**:
   The model separates `metric_block_height` (what Typst actually lays out and what layout calibration compares against Typst compiled frames) from `block_height` (the reserved demand including `reserve × size` used by fitting and intrinsic height resolution).

5. **Intrinsic height includes the reservation**:
   A `center`-aligned text item with `content` height resolves its box height to `metric_block_height + reserve × size`, matching the behavior of `Top` and `Bottom`.

## Consequences

- **BREAKING (visible output)**: Height-bound centred text near its ceiling renders smaller:
  - `brother_24mm_printed_on.yaml` line 1 (8.0 mm box, max 24 pt) drops 24.0 pt → 18.5 pt.
  - `brother_24mm_lines_divider.yaml` line 1 (7.5 mm box, max 20 pt) drops 20.0 pt → 17.5 pt.
  - `brother_24mm_multiline.yaml` 2-line wrapped text (16.1 mm box, max 32 pt) drops 21.5 pt → 17.5 pt.
  - `avery5163_asset_tag.yaml` horizontal `{id}` (0.35 in box, max 22 pt) drops 22.0 pt → 20.5 pt, and `{name}` drops 24.0 pt → 23.5 pt.
- **Fewer lines at fixed font size**: A centred multiline item at a fixed `font_size` whose box cannot hold all lines plus the reservation drops lines to ellipsis (e.g. `avery5163_asset_tag` `{tags}`/`{description}` in 0.65 in box holds 2 lines instead of 3).
- **New 422 errors under overflow policy**:
  - Centred items declaring `overflow: fail` whose metric block fits but whose ink reservation does not return `422 text_does_not_fit`.
  - Centred items whose box is shorter than one line plus reservation (`1.2099 × size` in Inter) return `422 text_does_not_fit` under both `ellipsis` and `fail`.
- **Taller content box**: A centred item asking for `content` height resolves a taller box (by `2 × max(u, d) × size`).
- **Catalog templates unaffected**: All four bundled continuous tape templates (`catalog/tape/brother/*.yaml`) remain unchanged at `font_size.max` because their height is not the binding constraint.
