# Spec Delta

## ADDED Requirements

### Requirement: Sheet text and QR stay inside the stock outline

Every active sheet `text` and `qr` box with positive area SHALL have its four corners inside the stock outline (`templates`), after all ancestor placement, padding and rotation, using the coordinate tolerance. A crossing SHALL fail with `422 UnsupportedLayoutItem`, reason `item_out_of_frame`. The check SHALL NOT change the box's size or position.

#### Scenario: A centered line uses the wide middle

- **WHEN** a 50-by-50 ellipse sheet places a text box at `[0.55, 20]` with size `[48.9, 10]`
- **THEN** the box is accepted without reducing the layout to an inscribed square

#### Scenario: Nested rotation carries a QR across the curve

- **WHEN** a 50-by-50 ellipse sheet has a full-slot container rotated 90 degrees with padding 1, containing a QR at `[20, 20]` whose square size comes from a parameter defaulting to 5, and a request supplies 25
- **THEN** the template loads with its default, but the request fails with `item_out_of_frame` naming the QR's path, even though its 25-by-25 box fits the container's inner frame

## MODIFIED Requirements

### Requirement: Load-time validation runs the render-time sizing rules

Load SHALL validate geometry with the same sizing rules render uses, against a frame built from the template alone: `width.max` for a dynamic width, and each geometry parameter reference instantiated from its `default`, or, when that default is tokened, from its `min`, else `0`. Load SHALL NOT measure text, encode QR codes, decode images or run a flow arrangement; a content extent SHALL stand in at its available extent. A refusal at load therefore depends only on the template.

Structural validation SHALL traverse every branch whether or not its `when:` gate could match. A packed child SHALL be checked at load as if it were its container's only child, so an accumulation of siblings past the inner box is a render-time failure only.

Sheet curved-boundary checks SHALL run at load only when the box and its complete placement in the sheet slot resolve without content measurement or flow arrangement; otherwise those checks SHALL be deferred until render.

#### Scenario: An invalid inactive branch is refused

- **WHEN** an item behind `when: { debug: true }`, with `debug` defaulting to `false`, declares `size: [0, 10]`
- **THEN** the template is refused at load

#### Scenario: A geometry parameter with a tokened default

- **WHEN** `size: ["{box_w}", 10]` names a parameter with `min: 12` and `default: "{vars.box_w}"`
- **THEN** load validates that axis as 12

#### Scenario: An accumulation is a render failure

- **WHEN** a flow container with inner width 20 and `gap: 2` holds two `content`-width texts
- **THEN** the template loads; values measuring 5 and 6 render; values measuring 14 and 6 fail with `item_out_of_frame`

#### Scenario: Content width is checked after measurement

- **WHEN** a 50-by-50 ellipse sheet places a text at `[5, 20]` with size `[content, 10]`
- **THEN** the template loads without using an available-width stand-in for curved containment; a resolved width of 40 renders, and a resolved width of 45 fails with `item_out_of_frame`
