# 39. Per-field render override (color and resolution negotiated independently)

Date: 2026-07-01

## Status

Accepted

Refines [ADR-0033](0033-capability-aware-rendering.md) (capability-aware rendering, slice 3
negotiation) and uses the per-printer render config from [ADR-0032](0032-ipp-auth-custom-ca.md).

## Context

ADR-0033 slice 3 added IPP `Get-Printer-Attributes` negotiation on the print path: a printer with no
`config.render` gets its color mode and resolution auto-detected. But the implementation treated the
configured render profile as all-or-nothing. In `src/api.rs` the print path did
`configured = driver.configured_render_options()` (an `Option<ImageRenderOptions>`) and, if it was
`Some`, used it wholesale and skipped the `Get-Printer-Attributes` query entirely (except when a
media-width check forced it).

`ImageRenderOptions` has non-optional fields (`color_mode: ColorMode`, `resolution_dpi: Option<u32>`),
and `CupsDriver` collapsed a missing `render.color_mode` into `Color` as soon as a `render` block
existed. So a user who set `render.resolution` to pin DPI silently lost color auto-detection: the
printer's advertised bilevel capability was ignored and the label went out in color/PDF. The knob that
looked like "just set the resolution" quietly disabled negotiation of an unrelated field. This is the
UX trap surfaced while redesigning the printer setup form (#117).

## Decision

1. **Render overrides are per-field.** Introduce `RenderOverride { color_mode: Option<ColorMode>,
   resolution_dpi: Option<u32> }`. Each `None` field means "not overridden, negotiate it." The driver
   trait exposes `configured_render_override() -> RenderOverride` (replacing the all-or-nothing
   `configured_render_options() -> Option<ImageRenderOptions>`).

2. **Effective value is resolved independently per field:** override, else negotiated from
   capabilities, else default. `effective_render(&RenderOverride, Option<&PrinterCapabilities>)`
   computes both fields; the print path fetches capabilities whenever *either* field is unset (or a
   media-width check is pending).

3. **Negotiated color preserves the artifact-format guard.** Negotiated `color_mode` is `BiLevel` only
   when the printer is bilevel AND advertises `image/png`, because a bilevel single/tape job is sent as
   `image/png` (`print_artifact_format`): a bilevel-but-non-PNG printer must not be auto-switched to a
   format it cannot accept. An *explicit* `color_mode` override is the user's choice and is honored as
   given. This replaces the `negotiated_profile` helper, whose bilevel-AND-png guard also discarded the
   negotiated resolution when it failed; resolution now negotiates independently of the color guard.

4. **Capabilities gain reporting-only fields.** `PrinterCapabilities` carries `model`
   (`printer-make-and-model`) and `color_known` (whether the printer advertised any color-mode or
   raster-type attribute), so a probe/UI can distinguish "color-capable" from "said nothing" rather
   than presenting unknown as color. These do not change the print decision.

## Consequences

- Overriding one render field no longer disables negotiation of the other. A printer configured with
  only `resolution` still auto-detects bilevel; one configured with only `color_mode` still
  auto-detects DPI.
- Output for printers with no `config.render` is unchanged (full negotiation as before). Output for a
  printer that previously relied on the all-or-nothing skip only changes if it had a *partial*
  `config.render`, which now negotiates the unset field, the intended fix.
- `negotiated_profile` is removed in favor of `effective_render`; the `FakeDriver` and print-path tests
  mirror the per-field precedence.
- Supersedes the ADR-0033 note that "an explicit `config.render` (even `color_mode: color`) suppresses
  negotiation entirely."
