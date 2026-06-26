# 33. Capability-aware rendering (printer-driven bi-level/resolution; fail-open media gate)

Date: 2026-06-26

## Status

Accepted

## Context

Rendering today is template-driven and printer-agnostic: Typst rasterizes at `template.dpi` and labeler
emits a PDF (or an anti-aliased PNG for previews), and the CUPS driver sends it via IPP `Print-Job` with
`document-format = application/pdf` and no capability query (`src/driver.rs`, `src/render/mod.rs`). The same
artifact goes to every printer.

For bi-level thermal label printers (Brother PT-2730 / QL etc.) this is wrong: we hand the printer a
grayscale/vector PDF and its own CUPS filter hard-thresholds it to 1-bit with whatever threshold it uses,
so anti-aliased text edges become jagged, small fonts blur, and QR quiet zones risk stray dots. There is
also no guard that the loaded media matches the template (a 12mm-wide template sent to a printer with 18mm
tape loaded just prints wrong).

Research into the standard approach (PWG IPP Everywhere / PWG 5100.14, RFC 8011, PWG Raster / PWG 5102.4,
OpenPrinting CUPS, plus the companion project `pfa230/ptouch-print-server`) established: treat printing as a
capability-driven protocol, query `Get-Printer-Attributes`, render to the device's advertised
format/resolution/color, and when targeting bi-level OWN the halftoning by emitting PWG Raster `black_1`,
split by element type (threshold vector, error-diffuse images). The companion PAPPL app for the PT-2730
already encodes the boundary: its raster core does only device packing and explicitly delegates gray->B/W
to the renderer ("the renderer (labeler / PAPPL bi-level) owns gray->B/W"), and it reports the loaded tape
width live in the Brother status block (`status.c` `tape_mm`).

labeler has two render paths that must both be served: download/preview (no printer) and print (through the
driver).

## Decision

1. **Rendering is a pure function `render(template, profile)`.** The `profile` carries the static
   render-quality parameters `{ color_mode, resolution, pixel_type, dither_policy }`. The renderer never
   knows whether a printer exists; only the SOURCE of the profile differs per path.

2. **Profile resolution precedence (print path):**
   `per-render override > per-printer configured profile (in the printer config, see ADR-0032/#39)
    > negotiated Get-Printer-Attributes > template default > built-in default (today's anti-aliased
    full-fidelity render)`.
   **Download/preview path:** `per-render override > template default > built-in` (no printer; a request
   MAY optionally name a printer or saved profile to simulate print output).

3. **Bi-level halftoning is owned by labeler and split by element type, by POST-PROCESSING the Typst
   raster to 1-bit.** Typst always anti-aliases (`typst_render` has no AA-disable), so the mechanism is:
   rasterize at (or above) the target/device resolution, then convert the resulting grayscale to `black_1`
   per element kind, hard-threshold for text / lines / barcodes / QR (which collapses the anti-aliased gray
   to hard edges at the device grid), Floyd-Steinberg error diffusion for raster `Image` items. (The ADR
   ratifies the post-process-to-1-bit model and the threshold-vs-diffuse split; the exact resolution
   strategy, render-at-DPI vs supersample-then-downsample and threshold tuning, is a slice-spec detail.)
   For bi-level targets labeler emits PWG Raster `black_1`; for PDF-capable / office printers it keeps PDF.

4. **The render profile and the wire FORMAT are resolved separately.** The render profile (decision 1/2)
   decides HOW to rasterize; the wire format (PDF vs `image/pwg-raster`) is independently constrained to the
   printer's `document-format-supported`. A configured profile may outrank negotiation for render intent,
   but it can never force a format the printer does not accept, a profile/printer format conflict (e.g.
   config wants `black_1` raster but the printer advertises only PDF) is a clean PREFLIGHT rejection, not a
   silent wrong send (with an optional configured fallback to PDF).

5. **Capability negotiation is additive with graceful degradation.** Absent or insufficient
   `Get-Printer-Attributes` falls back to the template-default profile and PDF (today's behavior). The
   self-rendered raster path is opt-in by sufficiency (taken only when the printer advertises enough: a
   usable resolution, `black_1`, and an acceptable format). A per-printer config can also pin the legacy PDF
   path explicitly (opt-out) so a printer that prints acceptably today is never switched without consent.
   Proprietary / PPD-backed printers (Brother QL-800 P-touch, Zebra ZPL, Dymo) supply a manual per-printer
   profile and may require a non-raster target, so the driver render target is pluggable
   (PDF / PWG-Raster `black_1` / vendor language).

6. **Media / tape width is NOT a render input; it is a fail-open preflight GATE.** The template owns
   geometry, the renderer always renders to the template's declared tape width. The loaded tape
   (`media-ready` / live device status, e.g. PT-2730 `tape_mm`) is consulted only to reject a CONFIRMED
   template/loaded mismatch before sending the job. Missing or unreported media data is IGNORED and never
   blocks printing. Tape width is live state, resolved `per-print override > live media-ready/status
   > template width`, and is never a stored setting (a tape swap must not require a settings change). It
   has a much shorter cache life than the static quality profile (effectively re-read per print).

## Consequences

- The download path and printers with no capabilities / no profile fall through to template defaults and
  PDF, which is exactly today's behavior. Printers that negotiate sufficient capabilities or carry a
  bi-level profile INTENTIONALLY change from PDF to PWG Raster `black_1`, that is the improvement, not a
  regression, and a per-printer config can pin the legacy PDF path to opt out. So existing output is
  preserved unless a printer opts in (explicitly or via negotiation).
- New machinery: a 1-bit render path (threshold + error diffusion at a target resolution); a PWG Raster
  `black_1` emitter and format selection; an IPP `Get-Printer-Attributes` client with a cache; a per-printer
  render-profile config; a fail-open media gate reading live status.
- Delivered in independently shippable slices: (1) bi-level render output on the download path; (2) the
  render-profile struct + precedence plumbing; (3) IPP capability negotiation; (4) PWG-Raster wire format +
  format selection; (5) the media preflight gate. Each gets its own spec/plan.
- Relationships: extends [ADR-0007](0007-printer-architecture-and-transport-model.md) (driver abstraction,
  now a pluggable render target); orthogonal to [ADR-0026](0026-auto-length-dynamic-width.md) and #78
  (which size the dynamic LENGTH from content, the media gate instead validates the template's fixed
  tape/media WIDTH dimension against the loaded media); uses the per-printer config from
  [ADR-0032](0032-ipp-auth-custom-ca.md) / #39 to hold the manual profile; and pairs with
  `ptouch-print-server` (PAPPL), which owns device packing/status, not tone.

## Open questions (deferred to the per-slice specs, not this ADR)

- Exact threshold level and whether to render at device DPI vs supersample-then-downsample for vector.
- PWG Raster encoding details (line compression, header fields, `black_1` bit order) and whether labeler
  emits `black_1` directly to IPP/CUPS or hands grayscale to PAPPL's own bi-level path.
- `Get-Printer-Attributes` cache TTL and refresh; the `media-col` size to template-width (mm) match
  tolerance; continuous vs die-cut handling in the gate.
