# Design: M1 rendering completeness — single-label PDF (#4) and line geometry (#6)

**Date:** 2026-06-15
**Issues:** #4 (single-label PDF), #6 (fix line size-semantics). Milestone M1.
**Status:** Approved design, pre-implementation.

## Context

M1 ("Rendering completeness") had four issues; #3 (image layout item) is done. This design covers the
two remaining implementable ones: #4 and #6. **#5 (copies) is deferred to #28** ("[ADR] Per-label
configuration and batch composition"): single-label physical copies are already handled by the
printer/CUPS/browser, and sheet-level copies are one facet of per-label batch composition that needs a
design decision first. M1 fully closes once #5 lands against #28.

## #4 — Single-label PDF output

Today `render_single_label` only emits PNG (`typst-render`); sheets already emit PDF (`typst-pdf`).
Office printing of a single label wants PDF too.

- **Format selection:** a `?format=png|pdf` query param on `render_label`, default `png`. Unknown
  values → `400 InvalidRequest`. PNG keeps `Content-Type: image/png`; PDF returns `application/pdf`.
- **Render module:** extract `compile_single_doc(template, data, option) -> PagedDocument` from the
  current `render_single_label` (the source-build + `compile_paged` steps). Keep `render_single_label`
  (PNG, unchanged signature, so existing tests/callers are unaffected) and add `render_single_label_pdf`
  that reuses `compile_single_doc` and encodes via `typst_pdf::pdf`. DPI affects only the PNG raster.
- **API:** `render_label` reads the format param and calls the matching renderer, setting the response
  content type accordingly.
- **Tests:** a single-format template renders a valid `%PDF`; `?format=bogus` → 400; the existing PNG
  path is unchanged.
- **Implementation note (confirm during coding):** the exact axum `Query` extractor wiring for the
  optional `format` (a small `#[derive(Deserialize)]` query struct with a defaulted enum).

## #6 — Line geometry (`at` + `to`)

For most items `size` is a box `[w, h]`; `line` abused it as a delta `[dx, dy]` from `at` (CAPABILITIES
§3.1). Fix by giving `line` its own start/end geometry and removing its dependence on the box
`Placement`.

- **Schema:** `line { at: [x, y] (start, default [0,0]), to: [x, y] (end), thickness }`. Both endpoints
  are absolute in frame coordinates, the same space as every other item's `at`. Lines drop
  `size`/`max_w`/`max_h`/`auto`/`rotate` (unused by any template). Changes span `raw.rs` (`LineRaw`,
  `deny_unknown_fields`), `models.rs` (`LayoutItem::Line { at, to, thickness }`), and `convert.rs`.
- **Render:** `render_line_item` maps `at` and `to` through the existing y-flip (`to_page_coords`) and
  draws between them. Delete the now-dead `resolve_line_delta` / `resolve_line_value`.
- **Validation:** both endpoints within layout bounds, `at != to`, `thickness > 0`. Remove the
  line-specific delta/auto resolution.
- **Migration (breaking schema change; no external users yet):** rewrite the three `line` items —
  `avery5163.yaml` (`at:[1.45,0] size:[0,1.8]` → `at:[1.45,0] to:[1.45,1.8]`; `at:[0,1.35]
  size:[2.35,0]` → `at:[0,1.35] to:[2.35,1.35]`) and `brother12mm.yaml` (`at:[0.5,1.0] size:[23,0]` →
  `at:[0.5,1.0] to:[23.5,1.0]`) — and the `Line` in the `render_single_label_with_qr` test.
- **Docs:** update SPEC §4 (line) and its changelog. No new ADR; this is a schema detail, rationale
  lives here.
- **Tests:** parse a line with `at`/`to`; render it; validation rejects `at == to` and out-of-bounds
  endpoints.

## Delivery and testing

- One branch `feat/m1-rendering`, two commits: `Fixes #4`, then `Fixes #6` (independent; #4 first).
  Each marks its plan entry DONE (commit hash) in `docs/PLAN-phase-1.md`.
- After implementation: one adversarial review pass over the diff; address findings; then
  `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test` all
  clean, plus a PNG + PDF render smoke. Merge to `main`, push.

## Out of scope

#5 copies and all per-label batch composition (→ #28); multi-template batches; line rotation/auto-fill.

## Acceptance criteria

- **#4:** a single-format template renders both PNG and PDF; format chosen by `?format=`; unknown
  rejected; tests cover both formats and the error.
- **#6:** `line` uses `at`/`to`; the two sample templates and the affected test are migrated; validation
  rejects degenerate/out-of-bounds lines; SPEC updated.
- Full verification suite (fmt/clippy `-D warnings`/test) green.
