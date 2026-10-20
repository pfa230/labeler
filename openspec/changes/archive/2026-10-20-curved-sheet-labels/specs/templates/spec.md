# Spec Delta

## MODIFIED Requirements

### Requirement: Label format

`format` SHALL be tagged by `type`: `single` (one label) or `sheet` (a grid of identical slots on a page). Lengths are in the template `unit`.

| `type` | Key | Type | Required | Constraint |
|---|---|---|---|---|
| `single` | `width` | Dimension or range | yes | |
| `single` | `height` | Dimension | yes | |
| `single` | `media_width` | number | no | > 0; nominal media width, read only by `printing`, no effect on rendering |
| `sheet` | `paper_width`, `paper_height` | number | yes | > 0 |
| `sheet` | `label_width`, `label_height` | number | yes | > 0; the size of every slot |
| `sheet` | `shape` | `rect` or `ellipse` | no | defaults to `rect`; the stock outline within the slot |
| `sheet` | `rounded` | number | no | `rect` only; finite, > 0 and at most half the shorter label side; absent means square corners |
| `sheet` | `positions` | sequence of `[x, y]` | yes | not empty; `x`, `y` ≥ 0; each is a slot's bottom-left corner, page origin bottom-left |

An `ellipse` SHALL be centered in the slot with diameters `label_width` and `label_height`. A `rect` SHALL use those dimensions with the declared `rounded` corner radius.

A Dimension SHALL be a number or a parameter reference `"{name}"` resolved from that parameter's value (see `parameters`). `width` MAY instead be a range `{ min, max }` whose bounds are each a Dimension; a range needs both bounds, checked after instantiating parameter defaults, with the message `a dynamic-width single template must specify both width.min and width.max`. A literal fixed value or literal bound SHALL be > 0, and literal `min` SHALL be ≤ literal `max`. At load, parameter references in `format` and `layout` geometry SHALL be instantiated with their defaults and the result validated.

#### Scenario: A sheet with no positions is refused

- **WHEN** a `sheet` format declares `positions: []`
- **THEN** the template is invalid with the message `positions must not be empty`

#### Scenario: A range on height is refused

- **WHEN** a `single` format declares `height: { min: 10, max: 20 }`
- **THEN** the template is invalid, with an error naming `format.height`

#### Scenario: Circle and oval dimensions

- **WHEN** an ellipse sheet declares label dimensions 50 by 50, or 80 by 50
- **THEN** its stock outline is respectively a circle of diameter 50, or an ellipse with horizontal diameter 80 and vertical diameter 50
