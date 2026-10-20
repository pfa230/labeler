# Proposal

## Why

Sheet slots currently have rectangular bounds and clipping, so their layout cannot represent round, oval or rounded-corner stock. Keeping the full canvas and checking resolved content boxes preserves useful space across a circle's middle without cutting text or QR codes at its edge.

Fixes #142

Source issue: https://github.com/pfa230/labeler/issues/142

## What Changes

- Add sheet stock shape metadata: `shape: rect` by default or `ellipse`, with an optional physical corner radius `rounded` for rectangles; existing label width and height define the outline.
- Keep full-slot coordinates and current layout sizing. Check text and QR boxes against the physical outline after placement, including nested padding and rotations; reject crossings instead of adapting the layout.
- Use the same containment rule for geometry known at load and for resolved geometry at render. Decorative paint, lines and images may reach the edge and are clipped to the stock outline.
- Clip both sheet PDFs and physical-size one-slot PNG thumbnails to that outline.
- Document authoring and add round and rounded-rectangle catalog examples grounded in vendor dimensions.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `templates`: Sheet stock shape declarations.
- `layout`: Curved-boundary checks for resolved text and QR boxes, with load-time handling of geometry that needs measurement or packing.
- `rendering`: Physical stock clipping for sheet slots.

## Impact

Touches raw format parsing, conversion, domain/OpenAPI models, load geometry validation, resolved-item rendering and compiled-frame clipping. The UI's matching format type needs the new metadata; no new UI control or runtime dependency is required. Catalog examples, generated index and `docs/AUTHORING.md` change alongside focused checks.

Non-goals: automatic movement, curved wrapping or shape-driven font shrinking; an inscribed or authored safe rectangle; new margin settings; arbitrary paths, cutouts or rotated stock outlines; changes to single-label formats, container paint semantics or print dispatch. Existing text fitting continues within authored boxes.
