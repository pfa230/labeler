## MODIFIED Requirements

### Requirement: A name denotes one colour on every field that takes one

The fields that take a colour are `text.color`, `stroke.color` and `background`. Every one of them
SHALL read a colour under the requirement above, from the one table stated there.

A colour name SHALL therefore denote the same value on every one of those fields. Two items in one
template that write the same name SHALL paint the same colour, whatever kind of item each is. No
field SHALL carry a vocabulary or a name table of its own, and no field SHALL read a name this table
does not hold.

A field MAY carry a **default**, the colour it paints when the key is omitted: `text.color` renders
black (`text-ink`) and `stroke.color` draws black (`shape-paint`). A default names a value from the
table above rather than adding one to it, so it cannot make a name mean two things, which is what
this requirement forbids.

This requirement supersedes the divergence
`openspec/changes/archive/2026-08-31-issue-280-shape-paint-model` recorded as intentional, under
which a text item's `red` was `#ff4136` and a shape's `red` was `#ff0000`.

#### Scenario: One name, one colour, across item kinds

- **WHEN** a template declares a `text` item with `color: red` inside a `container` with
  `background: red`
- **THEN** both paint `#ff0000`, and the paint emitted for the two items carries the same value

#### Scenario: The table is the CSS one, not the engine's

- **WHEN** a template declares `color: red`, `color: green`, `color: gray` or `color: yellow` on a
  `text` item
- **THEN** each denotes the CSS value in the table (`#ff0000`, `#008000`, `#808080`, `#ffff00`) and
  not the rendering engine's constant of the same name
