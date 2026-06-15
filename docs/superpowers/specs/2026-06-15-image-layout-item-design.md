# Design: Image layout item (issue #3 / P1-11)

**Date:** 2026-06-15
**Issue:** #3 — Image layout item (Phase 1, milestone M1)
**Status:** Approved design, pre-implementation.

## Context

The template layout model (`src/models.rs`) currently supports `text`, `qr`, `line`, and `container`
items, rendered by generating Typst source and compiling it with `typst-as-lib` (PNG via
`typst-render`, PDF via `typst-pdf`). Real labels routinely need a static logo and, for data-driven
runs, a per-item picture. This adds an `image` layout item covering both.

QR images are already embedded inline as `#image(bytes("<svg text>"), format: "svg")`, which works only
because SVG is text. Raw PNG/JPEG bytes cannot be safely embedded in a Typst source string, so binary
images need a different delivery path.

## Goal

Add an `image` layout item that embeds a raster (PNG/JPEG) or SVG into a rendered label, from either a
static bundled asset or per-label base64 data, rendering correctly in both PNG and PDF output.

## Decisions (approved)

1. **Sources:** both static (bundled asset) and data-bound (per-label).
2. **Data-bound form:** base64 data URI inline. No server-side URL fetching in this issue (avoids the
   SSRF/network/caching design); URL fetching can be added later as a third source.
3. **Static form:** a path resolved under a configured assets root, with a path-traversal guard.
4. **Schema shape:** flat fields, mutually exclusive `src` (static) or `name` (data-bound), consistent
   with the existing flat items (`text`/`qr` key off `name`).
5. **Typst delivery:** decode to bytes in Rust, register as an in-memory virtual file via
   `TypstEngine::builder().with_static_file_resolver(...)`, and reference it by path in the source.
   Verified available in typst-as-lib 0.15.0: `with_static_file_resolver<IB, F, B>(binaries)` where
   items are `(F: IntoFileId, B: IntoBytes)` (`StaticFileResolver`, in-memory binaries keyed by
   `FileId`).
6. **Errors:** reuse existing codes — `MissingField` (422) for an absent data key,
   `UnsupportedLayoutItem` (422) for bad base64 / unsupported MIME / decode failure / asset path
   problems. No new error code, keeping the error contract stable.

## Schema

New `image` item carrying the shared placement (`at`, `size`, `max_w`, `max_h`, `rotate`) plus:

- exactly one of:
  - `src: String` — static asset path, resolved under the assets root, or
  - `name: String` — data key whose value is a base64 data URI (`data:image/png;base64,...`);
- optional `fit: contain | cover | stretch`, default `contain`.

Supported formats: PNG, JPEG, SVG. The data-bound value must be a data URI so the MIME carries the
format; bare base64 is not accepted.

```yaml
- type: image            # static asset
  src: logo.png
  at: [0.0, 0.0]
  size: [10.0, 10.0]
- type: image            # data-bound
  name: photo
  at: [0.0, 0.0]
  size: [auto, auto]
  max_w: 20.0
  max_h: 20.0
  fit: contain
```

Implementation touches the three schema layers per ADR-0002:
- `src/raw.rs`: `ImageRaw` (`deny_unknown_fields`), flattened `Placement`, optional `src`, `name`,
  `fit`.
- `src/models.rs`: `LayoutItem::Image { placement, source, fit }` where `source` is an enum
  (`Asset(String)` | `Field(String)`); a `Fit` enum with `Default = Contain`.
- `src/convert.rs`: `TryFrom<ImageRaw>` enforcing exactly-one-of `src`/`name`.

## Rendering

A new `render_image_item` in `src/render/mod.rs`, mirroring `render_qr_item`:

1. Resolve to `(bytes, format)`:
   - **static `src`:** join under the assets root, canonicalize, verify the result stays under the
     root (reject traversal), read the file; format from extension.
   - **data-bound `name`:** look up `data[name]` (absent → `MissingField`); parse the data URI;
     base64-decode; format from the MIME.
2. Register the bytes as a virtual Typst file with a unique path (e.g. `/labeler-img-{n}.{ext}`).
3. Emit `#image("{vpath}", width: {w}, height: {h}, fit: "{fit}")` inside the existing
   `#place(...)[#box(...clip:true)[...]]` + `wrap_rotation` pattern.

**Flow change:** `with_static_file_resolver` is a builder-time call, so all images must be collected
before the engine is built. A single image collector (the `Vec<(vpath, bytes)>` plus a monotonic path
counter) is **shared across the entire render**, not owned per `RenderContext`. This matters because
the sheet path creates a fresh `RenderContext` per label and containers recurse into nested
`RenderContext`s; a per-context counter would produce colliding virtual paths within one compile. The
collector is threaded by reference (e.g. `&RefCell<ImageCollector>` or an `&mut` passed through
`render_items`) so every item, container, and label registers into the one list with globally unique
paths. Both `render_single_label` and `render_sheet_labels` build the collector first, then pass it to
`TypstEngine::builder()`.

Format mapping: PNG → `"png"`, JPEG → `"jpg"`, SVG → `"svg"` (Typst `image` format strings).

## Configuration

One new setting: the **assets root directory**, env-overridable (default `assets/`), documented now and
folded into the formal env-config work later (issue #9 / P1-64). The traversal guard is canonicalize +
`starts_with(root)`.

## Validation and errors

- **At template load** (`TemplateDefinition::validate`): structural checks only — exactly-one-of
  `src`/`name` (enforced in `convert.rs`), placement bounds fit the layout, `fit` valid.
- **At render:** both sources are resolved and validated then, mirroring how a data-bound value is only
  knowable at render. Static `src`: format from extension, path canonicalized and confined to the
  assets root (traversal rejected), file read. Data-bound: missing data key → `MissingField` (422);
  bad base64 / unsupported MIME / asset path or format problems → `UnsupportedLayoutItem` (422).
- **Deferred:** load-time fail-fast for a bad static `src` (consistent with the duplicate-id check)
  would be nicer but needs the assets resolver exposed outside the render module; left as a follow-up.

## Testing

- **Parse/convert:** `src` form; `name` form; exactly-one-of rejection (neither / both); unsupported
  `fit`; traversal path rejection; bounds rejection.
- **Render:** a single-format template with a static image renders a valid PNG and a valid PDF; a
  data-bound base64 image renders; negatives: missing data key → `MissingField`, malformed base64 →
  `UnsupportedLayoutItem`.
- Tests use a tiny 1×1 PNG fixture committed under the test assets dir.

## Documentation and ADR

- Update `docs/SPEC.md` §4 to document the `image` item (required; this is a behavior change), and its
  changelog.
- Write **ADR-0009** recording the image source model decision: static-asset-under-root plus
  data-URI-inline, and explicitly no server-side URL fetching in Phase 1 (the security rationale).
  ADR-0007 and ADR-0008 are reserved for the printer-architecture and UI-delivery prerequisite ADRs.
- Add the `image` item to the relevant OpenAPI schemas (`src/openapi.rs`) so it appears in the API doc.

## Out of scope (this issue)

Server-side URL fetching of images; image cropping/filters; non-PNG/JPEG/SVG formats; per-label image
upload UI. These are later items.

## Acceptance criteria (from issue #3)

- A template with an `image` item (static asset and/or data-bound) renders in both PNG and PDF.
- Schema added across `raw.rs` / `models.rs` / `convert.rs` (ADR-0002).
- Bounds validated like other items.
- Positive and negative tests pass; `cargo fmt`, `cargo clippy --all-targets --all-features`, and
  `cargo test` are clean.
