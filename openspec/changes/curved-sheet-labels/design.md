# Design

## Context

See proposal.md for motivation. Sheet formats currently carry only page dimensions, label dimensions and positions (`src/raw.rs`, `src/models.rs`, `src/convert.rs`). Load validation in `src/templates.rs` and both render paths use the full rectangular slot as their root frame. `render_sheet_pages` compiles the sheet once and clips each slot with a Typst box; thumbnails compile a physical-size single-slot document. Ellipse containers paint an ellipse but deliberately clip their children to a rectangle, so they cannot supply the physical stock boundary.

The renderer has resolved `PlacedBox` values after normal placement or flow packing. `RenderContext` recurses through container padding and orthogonal rotations. Load validation uses the shared resolver without measuring text or running flow, with available extents standing in for content. These existing limits determine when a physical text/QR box is known.

## Goals / Non-Goals

**Goals:** Use the full physical canvas, validate text/QR boxes against its actual outline, and apply the same outline to thumbnail and PDF ink.

**Non-Goals:** No curved text wrapping, automatic placement or boundary-driven font sizing, rectangle-limited canvas, separate safe-area/margin field, geometry framework or dependency update. Existing root padding or authored positions express any additional registration clearance. Container paint/clipping rules remain local to containers.

## Decisions

### Vendor-backed shape set

The initial set is plain/rounded rectangles and ellipses, with equal ellipse diameters representing circles. Existing width and height supply those dimensions; a separate diameter field or circle enum would duplicate geometry. The survey found concrete sheet stock for each shape:

| Vendor stock | Physical label | Sheet evidence |
| --- | --- | --- |
| [OnlineLabels OL2682](https://www.onlinelabels.com/products/ol2682) | 2-inch circle | 12 per US Letter sheet; left/right margins 0.625, top/bottom 0.6 inches; horizontal pitch 2.625, vertical pitch 2.6 inches |
| [Avery 22820](https://www.avery.com/templates/22820) | 2 by 3-1/3-inch oval | 8 labels per sheet |
| [OnlineLabels OL1417](https://www.onlinelabels.com/products/ol1417) | 6 by 5-inch rounded rectangle, 0.125-inch corner radius | 2 per US Letter sheet; margins left/right 1.25, top/bottom 0.4375 inches; vertical gap 0.125 inches |

Checked 2026-10-20. These sources support the selected set without custom paths or cutouts. OnlineLabels also [describes a 0.125-inch safe margin](https://www.onlinelabels.com/support/faq/305?categoryId=1) for important content against printer shifting; that is authoring guidance, not a universal new format default.

Routine schema default: add `shape` using the existing `rect`/`ellipse` vocabulary, default `rect`, and optional `rounded` for rectangular sheets only. A physical radius is finite, positive and at most half the shorter label side; reject an impossible radius instead of changing the stock geometry at render. Keep the two-stage raw/conversion/domain path and path-bearing validation. Expose the effective shape and any authored radius in existing format metadata/OpenAPI, with matching UI types. No catalog-index shape field is needed: the index still describes a sheet and installs its YAML.

### Full canvas and explicit box validation

The user rejected an inscribed square because a centered line would lose usable horizontal space, and accepted full-canvas checks with an error for crossings. For a 50 mm circle, a centered 10 mm-high rectangle can be 48.9898 mm wide; the automatic inscribed square would be only 35.3553 mm wide. The spec uses a 48.9 mm-wide box to remain inside the exact boundary. Coordinates, edge-relative anchors, `fill` and existing text fitting keep their current meanings.

Define the safe region as the physical outline's interior. Validate the whole resolved positive-area text or QR box, including a QR's authored quiet-zone allocation, not individual glyphs or dark QR modules. Zero-area boxes need no curved check, matching the existing empty-extent rule. Four-corner containment suffices for the selected convex outlines and existing orthogonal rotations. Decorative container paint, lines and images may cover the full slot; the physical clip limits their ink. No decoration flag or second background layer is required.

Put the small point/four-corner predicate and orthogonal coordinate mapping in the existing resolver geometry module. Ellipses use normalized distance from the slot center; rounded rectangles use their straight interior and corner-circle distance. Reuse the existing coordinate tolerance, with no configurable precision. Both load and render call this rule and use the existing `item_out_of_frame` classification for a crossing; no new reason slug is needed. Curved failures do not have an offending x/y axis, so do not manufacture an axis-specific resolver violation.

Update the existing `item_out_of_frame` row in the errors spec to explicitly include a box crossing the sheet stock outline. The full rectangular layout frame and the curved physical boundary are distinct, so leaving only 'beyond the frame' would omit the newly accepted raise-site. This is a clarification of the chosen existing classification, not a new reason slug or a change to the accepted failure policy. The errors table owns that reason's meaning; the layout delta owns when containment fails.

At render, carry the accumulated mapping from the current local frame into sheet coordinates through the existing item/container traversal. Check active text/QR boxes in `render_single_item`, after absolute placement or flow returns a resolved box. Compose container position, author-space padding and the existing 0/90/180/270 rotation. Check per repeat instance using its existing path and let batch collection handle failures normally. The check does not alter measurement or run a second fitting pass.

### Check only genuinely known geometry at load

Keep existing structural and rectangular validation unchanged. Extend its current traversal with an optional exact sheet-space context: the root is exact; content-sized or packed placements make the affected context unknown. Resolve an absolute box with the current resolver when its axes and all ancestor placements are exact without measurement or flow. `fill` is known in a known frame; a content-size stand-in is not an actual resolved box. Propagate the container mapping only while exact, otherwise defer the curved check for that subtree until render.

This needs no symbolic dependency solver, early text measurement or load-time flow execution. `AxisSpec.fixed_by_template` and intrinsic-demand flags do not establish a known physical box, so they must not be used as the gate. Instantiated geometry defaults remain the existing load inputs. Every resolved active box is checked at render, including data-dependent and packed boxes that load could not determine.

### Clip ink through the pinned Typst frame representation

The installed Typst 0.15.1 exposes `GroupItem.clip: Option<Curve>`, `Curve::ellipse` and `Frame::clip`. Both its raster renderer and PDF exporter consume those curves; no rasterized PDF or package bump is needed. Keep one compilation per sheet. Emit the same small physical-slot wrapper around each PDF label and the thumbnail's one-slot body: the existing native box clip handles plain rectangles, and its radius handles rounded rectangles. Give ellipse slot boxes an internal label and set their compiled groups' clip to `Curve::ellipse` through one recursive frame visitor shared by both paths. Rebuild the document from adjusted pages and retain the existing exporters. This avoids a separate rounded-curve builder and thumbnail-only clipping implementation.

Slot identification is internal and carries no request data. Existing page sizes, positions and pagination remain intact. Keep today's thumbnail page fill; clipping colored ink does not imply a new transparent-background contract.

### Authoring examples and verification

Add two catalog examples, not a catalog for every surveyed SKU: an OL2682 circle and an OL1417 rounded rectangle under `catalog/sheet/onlinelabels/`. In inches, OL2682 has x positions 0.625, 3.25 and 5.875, each at y 8.4, 5.8, 3.2 and 0.6; OL1417 has x 1.25 at y 5.5625 and 0.4375. Declare positions in top-to-bottom row order, left-to-right within a row. Exercise ovals in the focused shape checks without another catalog entry. Each example uses explicit text/QR boxes inside its outline and a background that demonstrates physical clipping. Update the authoring guide with shape keys, full-canvas coordinates, the wide-centered-line example, box-crossing errors and authored printer clearance. Regenerate the ordinary catalog index.

Implementation must render the examples' one-slot PNG thumbnails and sheet PDFs, rasterize the PDFs for inspection, and open the images to check stock clipping, intact text/QR and alignment against the vendor geometry. These are required visual checks, not checkboxes claiming an image was inspected. Save output under `.agent-runs/`; automated assertions cover the geometry, clipping and error behavior described in tasks.md.

## Risks / Trade-offs

- Whole-box containment can reject a large text box even when its visible glyphs occupy a safe middle strip. This is the user's accepted validation policy; the author sizes the box to the desired strip.
- Ancestor transforms are the main integration risk. Focused nested-padding/rotation and packed-content cases must prove physical slot coordinates are used.
- Load cannot decide measured or packed geometry. It retains its existing checks and defers only the new curved check until the real box exists.

## Migration Plan

An omitted shape remains a square-cornered rectangle with the existing physical size. There is no stored-data migration, template rewrite or legacy spelling to support. The new catalog examples depend on this format support being implemented.
