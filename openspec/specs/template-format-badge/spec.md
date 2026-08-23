# template-format-badge Specification

## Purpose
Defines how a template's format is presented in the UI: what the badge states for a `single` and for a
`sheet` template, which cues separate the two, the legibility each must hold to on every background it
appears over, and which surfaces carry the badge as against naming the format in prose. Frozen
`docs/SPEC.md` defines format semantics at §3.1 but describes no format badge, so this capability
supersedes no section of it.

## Requirements

### Requirement: The format badge separates the two formats without colour

The format badge SHALL let a user tell a `single` template from a `sheet` template without reading the
badge's text and without relying on its colour.

The badge SHALL carry three cues, each of which distinguishes the two formats on its own:

- an **icon** that depicts the format. The icon for `single` SHALL depict exactly one cell. The icon
  for `sheet` SHALL depict four or more cells laid out in at least two rows and at least two columns.
  The exact cell count and arrangement are not fixed by this requirement.
- a **colour** treatment. The colour a `single` badge resolves its text to SHALL differ from the colour
  a `sheet` badge resolves its text to, and the two chip fill colours SHALL likewise differ. Two
  different references to one colour do not satisfy this: the resolved values must differ.
- the **text**, which for a sheet also states its position count and so differs in length from a
  single's text (see the next requirement).

Removing any one cue SHALL leave the two formats distinguishable by the remaining two. In particular,
rendered in greyscale, or by a user who cannot discriminate the two hues, the badge SHALL still
separate the formats by icon and by text.

The icon SHALL be presentational: it SHALL be hidden from assistive technology and SHALL contribute
nothing to the badge's text. What assistive technology conveys for the badge SHALL be exactly the text
a sighted user reads, so a screen reader user hears the format named and never a shape.

#### Scenario: The two icons differ in cell count

- **WHEN** a `single` template and a `sheet` template are shown side by side
- **THEN** the `single` badge's icon depicts exactly one cell
- **AND** the `sheet` badge's icon depicts four or more cells in at least two rows and two columns

#### Scenario: The two badges differ in resolved colour

- **WHEN** the colours a `single` badge and a `sheet` badge resolve to are compared
- **THEN** their text colours are different colour values
- **AND** their chip fill colours are different colour values

#### Scenario: Colour is not the only cue

- **WHEN** the badges are rendered with colour removed
- **THEN** a `single` badge and a `sheet` badge remain distinguishable by icon cell count and by text

#### Scenario: The icon is not conveyed

- **WHEN** the badge is read by assistive technology
- **THEN** the text conveyed is exactly the badge's visible text
- **AND** the icon is marked hidden from assistive technology

### Requirement: A sheet badge states its position count

A badge for a `sheet` template SHALL state the number of label positions on the sheet alongside the
word `sheet`. The count SHALL be the number of positions the template declares.

A badge for a `single` template SHALL state the word `single` alone. `single` has no positions, so
there is no count to state.

The count SHALL appear wherever the badge appears, so that the badge reads the same on the template
grid as on the template detail page.

#### Scenario: A sheet badge names its position count

- **WHEN** a `sheet` template declaring 30 positions is shown
- **THEN** its badge states `sheet` and the count 30

#### Scenario: A single badge states the word alone

- **WHEN** a `single` template is shown
- **THEN** its badge states `single` and no count

#### Scenario: A one-position sheet is still a sheet

- **WHEN** a `sheet` template declaring exactly one position is shown
- **THEN** its badge states `sheet` and the count 1
- **AND** it is not presented as a `single` template

### Requirement: The badge-bearing surfaces render one badge

The badge SHALL appear on exactly two surfaces, and SHALL be identical on both in icon, colour, text
and count:

- the card for an installed template on the template grid;
- the `Format` row of an installed template's detail page.

Surfaces that name a template's format in running prose rather than as a badge are outside this
requirement and SHALL be left as they are. Three exist:

- the template catalog listing, which states the format of a template not yet installed as a plain
  word, and which has no position count available to it;
- the detail page's `Dimensions` row, whose sentence for a sheet template ends in the word `sheet`;
- the preview pane's fallback link text for a sheet preview.

None is a badge, and none gains an icon, a colour treatment or a count.

#### Scenario: Grid and detail agree

- **WHEN** the same `sheet` template is viewed on the template grid and on its detail page
- **THEN** both render the same icon, the same colours, and the same text including the position count

#### Scenario: Prose mentions of a format are untouched

- **WHEN** the template catalog lists a template whose format is `sheet`
- **THEN** it states the format as it does today, with no badge, no icon and no position count

#### Scenario: The Dimensions row keeps its sentence

- **WHEN** a `sheet` template's detail page is shown
- **THEN** its `Dimensions` row reads as it does today, with the badge appearing only in the `Format`
  row

### Requirement: The badge is legible on every background it appears over

The badge sits over more than one background: an unselected template card, a selected template card,
and the detail page. Selection tints a card, so a background the badge sits over may be the same colour
as the badge's own chip fill.

In light mode and in dark mode, over every one of those backgrounds:

- the contrast between a badge's text colour and the colour immediately behind that text SHALL be at
  least 4.5:1, computed by the WCAG 2.x relative-luminance formula. This SHALL hold for the `single`
  badge and the `sheet` badge alike, so the change does not leave one format less legible than the
  other.
- the chip SHALL be delineated from what is behind it, so that it reads as a chip and not as bare
  text. A fill alone SHALL NOT be what delineates it, because the background behind it may be that
  same fill.

The colour that marks a `sheet` SHALL be distinct from every colour the UI already assigns a meaning:
the accent, the success colour and the error colour. A format is neither a success nor an error, and
must not be coloured as one.

#### Scenario: Both badges meet the ratio over every background, light mode

- **WHEN** each badge's text contrast is computed against every background the badge appears over, in
  the light palette
- **THEN** every computed ratio is at least 4.5:1

#### Scenario: Both badges meet the ratio over every background, dark mode

- **WHEN** each badge's text contrast is computed against every background the badge appears over, in
  the dark palette
- **THEN** every computed ratio is at least 4.5:1

#### Scenario: A chip over its own fill colour is still a chip

- **WHEN** a badge is rendered over a background whose colour equals the badge's own chip fill
- **THEN** the chip is still delineated from that background

#### Scenario: A format is not coloured as a status

- **WHEN** a `sheet` badge is shown
- **THEN** its colour is not the colour the UI uses for the accent
- **AND** it is not the colour the UI uses for success
- **AND** it is not the colour the UI uses for an error
