# ui-colour-palette Specification

## Purpose

Defines what the web UI's accent colour must hold to wherever it carries meaning: as text over the
grounds it is painted on, as the fill of a control whose label sits on it, and as a non-text indicator
that marks a thing as selected. Frozen `docs/SPEC.md` describes the REST service, the template schema
and the layout model, and says nothing about the web UI's palette, so this capability supersedes no
section of it.

## Requirements

### Requirement: Accent-coloured text meets AA over every ground it is painted on

The UI paints text in the accent colour, and that text sits over three grounds: the accent tint that
marks a selected or active item, the raised surface of a card or panel, and the page ground behind
them.

In light mode and in dark mode, over each of those three grounds, the contrast between the accent
colour and the ground SHALL be at least 4.5:1, computed by the WCAG 2.x relative-luminance formula.

This SHALL hold for every kind of accent-coloured text alike, and no kind is exempt for being short,
being a single glyph, or being decorative. A favourite star, a one-word chip, a link and a heading are
all text.

A theme SHALL NOT satisfy this by leaving a ground undefined. Every ground the accent is painted over
SHALL have a defined colour in both palettes, so the ratio is computable from the palette rather than
inherited from whatever happens to be behind.

#### Scenario: Accent text over the selection tint, both palettes

- **WHEN** the accent colour's contrast against the accent tint is computed in the light palette, and
  again in the dark palette
- **THEN** both ratios are at least 4.5:1

#### Scenario: Accent text over a card surface and the page ground, both palettes

- **WHEN** the accent colour's contrast against the card surface, and against the page ground, is
  computed in each palette
- **THEN** every one of the four ratios is at least 4.5:1

#### Scenario: A single-glyph accent mark is held to the text ratio

- **WHEN** a favourite marker is drawn in the accent colour over a card
- **THEN** its contrast against that card is at least 4.5:1, the same ratio a sentence would be held to

### Requirement: The label on an accent-filled control meets AA in both palettes

The UI fills its primary controls with the accent colour and paints their label on that fill.

In each of the two palettes, the contrast between that label's colour and the accent fill SHALL be at
least 4.5:1.

The label's colour SHALL be a property of the palette in force, so that changing palette changes the
label. A control whose label colour is decided independently of the palette cannot meet the ratio in
both palettes at once, because the two accents it must read against differ.

The two palettes SHALL be free to resolve the label to different colours. A light palette's accent may
be dark enough that white reads on it while a dark palette's accent is light enough that white does
not. What binds is the ratio in each palette, not a value shared between them.

#### Scenario: Button label over the accent fill, light palette

- **WHEN** the accent ink's contrast against the accent fill is computed in the light palette
- **THEN** the ratio is at least 4.5:1

#### Scenario: Button label over the accent fill, dark palette

- **WHEN** the accent ink's contrast against the accent fill is computed in the dark palette
- **THEN** the ratio is at least 4.5:1

#### Scenario: Changing palette changes the label

- **WHEN** the palette in force changes from light to dark
- **THEN** the label colour of every accent-filled control changes with it
- **AND** the label clears 4.5:1 against the accent fill in the palette it lands in

### Requirement: One primary accent serves both the text role and the fill role

Within a palette, the colour the UI paints accent text in and the colour it fills accent controls with
SHALL be the same colour, and the palette SHALL offer no second shade of the accent that an author
could choose between for either of those two roles.

Two shades of one hue placed next to each other, a button beside a link beside a chip, read as an
inconsistency rather than as a system, and a palette that carries both leaves every later author to
guess which one their case wants.

The accent tint is a different role and is not bound by this. It is a ground the accent is painted
over, never a colour the accent is painted in, so it is not a shade an author choosing a text or fill
colour can reach for. The previous requirement is what holds it to its ratio.

Two palettes MAY resolve the accent to different colours: that is what a light and a dark theme are.
The requirement binds the roles within one palette, not the palettes to each other.

#### Scenario: Accent text and accent fill are one colour

- **WHEN** the colour of accent-painted text and the colour of the accent control fill are compared
  within a palette
- **THEN** they are the same colour value
- **AND** this holds in the light palette and in the dark palette

#### Scenario: No second accent shade exists to choose between

- **WHEN** an author picks the colour for accent text, or for an accent control's fill
- **THEN** the palette offers exactly one colour for that role
- **AND** it offers no second shade of the accent that would also serve

### Requirement: The accent as a non-text mark stays distinguishable

The accent also marks things without carrying text. The border of a selected template card is such a
mark.

In both palettes, the contrast between the accent and the surface immediately behind such a mark SHALL
be at least 3:1, the WCAG 2.x ratio for a non-text user-interface component.

A selection SHALL NOT be conveyed by its tint alone. The tint that fills a selected item is a wash of
the accent and is deliberately close to the surface behind it, so a mark that meets the 3:1 above
SHALL accompany it. A user who cannot separate the tint from the surface still sees the selection.

#### Scenario: The selected card's border carries the 3:1

- **WHEN** the accent's contrast against the surface behind a selected card's border is computed in
  each palette
- **THEN** both ratios are at least 3:1

#### Scenario: Selection survives an indistinguishable tint

- **WHEN** a selected card and an unselected card are compared with the tint rendered
  indistinguishable from the surface
- **THEN** the selected one is still marked, by a border or stripe that meets the 3:1

### Requirement: The palette proves these ratios from its own values

The ratios above SHALL be asserted from the colour values the shipped palette actually carries, not
from a copy of them kept beside the assertions.

A palette edit that drops any pairing named in this capability below its ratio SHALL fail the project's
test suite. An assertion that reads a duplicate of the palette cannot do that: the duplicate goes
stale, the suite keeps passing, and the shipped colours drift out from under it.

A colour named by an assertion but absent from the palette SHALL fail loudly rather than be skipped, so
that renaming a colour out of the palette cannot quietly delete the assertion that covered it.

#### Scenario: A regression in the palette fails the suite

- **WHEN** an accent colour in the shipped palette is changed to a value that drops one of the pairings
  above below its ratio
- **THEN** the test suite fails

#### Scenario: A colour the assertions need but the palette lacks fails loudly

- **WHEN** a colour an assertion reaches for is absent from the palette
- **THEN** the suite fails naming it, rather than passing with that comparison skipped
