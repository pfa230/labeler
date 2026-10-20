# Tasks

## 1. Describe stock and share containment geometry

- [ ] 1.1 Add sheet `shape`/`rounded` through raw parsing, conversion, domain/OpenAPI metadata and the UI format type, and clarify the existing reason-table row as designed. Update the authoring key table and focused parsing/metadata tests: default rectangle, circle/oval dimensions, accepted physical radius and rejected radius/shape combinations with field paths. These fail when a declared outline is lost, changed or accepted outside its contract.
- [ ] 1.2 Add the shared outline containment predicate and orthogonal local-to-slot mapping in the existing geometry module. Test the spec's wide central rectangle, ellipse and rounded-corner inside/outside points and composed padding/rotation. Wire exact-geometry load checks into the current validation walk, retaining existing rectangular checks; prove a fixed outside box is refused while the content-width scenario loads. These fail on an inscribed-square restriction, wrong physical coordinates or treating an unmeasured stand-in as a real box.

## 2. Validate resolved content and clip output

- [ ] 2.1 Carry sheet-space mapping through existing render traversal and check resolved active text/QR boxes after absolute/flow placement, including repeat instances. Add render tests for the 48.9 mm centered line, measured-width success/failure, packed content, and the padded/rotated QR parameter scenario. Assert the existing reason and item/instance path through batch failure collection; these fail on missing runtime checks, wrong ancestor mapping or checking provisional geometry instead of the final box.
- [ ] 2.2 Use the shared physical-slot wrapper for thumbnails and PDF slots, native rounded box clipping and the shared compiled ellipse-group clip visitor. Add render assertions for physical-size PNG corner/center pixels and compiled PDF-slot clip geometry on populated slots across pagination; a full-slot background must be retained inside and excluded outside each outline. These fail if only thumbnails are clipped, the whole page is clipped instead of each slot, rounded corners stay square or ellipse decorations are wrongly rejected.

## 3. Supply usable examples and authoring guidance

- [ ] 3.1 Add the OL2682 circle and OL1417 rounded-rectangle templates at the catalog paths and exact positions recorded in design.md, with safe text/QR boxes and edge-reaching backgrounds; regenerate the index. Complete the guide's full-canvas sizing, crossing-error and printer-clearance examples. Produce each template's one-slot PNG and sheet PDF under `.agent-runs/` as review artifacts and establish catalog freshness by regenerating in a temporary catalog copy and comparing its index with the committed one. These checks fail on invalid examples, wrong dimensions, missing renders or a stale index.

## 4. Integration gates

- [ ] 4.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, `npm --prefix ui run lint`, `npm --prefix ui run test`, `npm --prefix ui run build`, and `npx --no-install openspec validate --all --strict --no-interactive`, retaining output under `.agent-runs/`.
