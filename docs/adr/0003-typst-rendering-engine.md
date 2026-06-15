# 3. Typst as the rendering engine

**Status:** Accepted

## Context

The service needs to produce both rasterized single labels (PNG) and vector label sheets (PDF) with
text layout, font shaping, alignment, clipping, rotation, and embedded images (QR codes). Options
considered conceptually: a direct 2D drawing library (e.g. raw `tiny-skia`/PDF writers), an HTML→PDF
pipeline (headless browser), or a typesetting engine.

## Decision

Generate [Typst](https://typst.app/) markup per request and compile it in-process with `typst-as-lib`,
using `typst-render` for PNG and `typst-pdf` for PDF. Layout items map to Typst primitives
(`#place`, `#box`, `#text`, `#image`, `#line`, `#rect`). Fonts are provided via `typst-kit` plus the
bundled `fonts/InterVariable.ttf`.

## Consequences

- One engine covers text shaping, both output formats, and vector quality for free; no browser
  dependency.
- The renderer's job is reduced to string generation, so Typst-string escaping and length formatting
  (`render/helpers.rs`) become correctness-critical.
- Typst's coordinate origin (top-left) differs from the template's, requiring a conversion (see
  ADR-0004).
- Typst's API changes between releases; pin versions in `Cargo.toml` and verify upgrades against the
  render tests.
- Text auto-fit (`font_size: {min, max}`) is computed independently with `fontdue` before emitting
  markup, since Typst does not expose fit-to-box sizing directly.
