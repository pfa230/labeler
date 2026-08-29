## MODIFIED Requirements

### Requirement: Load-time validation and render-time resolution are one algorithm

Size resolution SHALL have exactly one implementation. Load-time validation SHALL run it, rather than
a second copy of the same rules, against a frame built from the template alone: parameter defaults
instantiated per the frozen `docs/SPEC.md` §3.1 rule "At load time, parameter defaults are
instantiated to validate default geometry bounds", which this requirement does **not** supersede, and
`format.width.max` on the horizontal axis of a dynamic-width `single`.

A geometry parameter reference is permitted without an explicit `default`, so instantiation SHALL
retain the existing fallback chain unchanged: the declared `default` when present and parsing as a
number, otherwise the parameter's `min`, otherwise `0`.

A refusal at load SHALL therefore depend only on the template's structure and its declared parameter
defaults, never on the data of any request. It is not a claim that no request could render the
template.

Load-time validation SHALL NOT measure text, encode a QR, or decode an image, and SHALL NOT run a
container's arrangement. At that stage a content source SHALL be taken to yield its available extent.
That is a true upper bound on every claim, because a content or frame extent is clamped by the
available extent, so **no single item's own extent** accepted at load can overflow its frame at render
for want of a measurement.

The guarantee is exactly that wide, and this requirement says so rather than leaving the wider reading
available. It covers one item against its own frame and cannot cover an **accumulation** of siblings,
because load has nothing measured to accumulate: inside a flow container every content-source child
stands in at the whole padded inner extent, so their sum says nothing about the room they will take.
Packed children can therefore accumulate past the padded inner box at render, and the first child the
arrangement positions past that box fails the ordinary bounds check with `UnsupportedLayoutItem` and
`item_out_of_frame`, which is the refusal an author-placed item out of its frame already gets. No
reason is added for it, and load refuses nothing on its account. Load instead checks each packed child
against the padded inner box as if it were the only child, which is a true necessary condition and
catches an oversized authored extent where it is written.

Structural validation SHALL traverse every branch, active or not: a written zero, an impossible
padding, a malformed placement, a `qr` asking for a content or frame extent without `module_size`, or
a shrinking `to` on an unresolved axis is refused wherever it is written, including behind a gate no
default parameter satisfies. Only intrinsic evaluation and frame requirements are skipped for an
inactive branch, and an inactive item's value is not resolved.

This requirement supersedes the frozen `docs/SPEC.md` §7 note "Sizing/bounds logic is intentionally
duplicated between validation (compile time) and rendering (request time); the two must stay in
sync."

#### Scenario: An invalid inactive branch is still refused at load

- **WHEN** a template declares an item behind `when: { debug: true }` whose `size` is `[0, 10]`, and
  `debug` declares `default: false`
- **THEN** the template fails validation and is quarantined

#### Scenario: An inactive item imposes no requirement

- **WHEN** an item's `when` gate does not match the resolved parameters
- **THEN** it imposes no frame requirement on any ancestor and is never asked for an intrinsic size

#### Scenario: An inactive branch's data is still lazy

- **WHEN** a template declares an otherwise valid `text` behind an inactive `when` gate whose value
  references a data field no request supplies
- **THEN** the template loads and renders without `MissingField`

#### Scenario: A geometry parameter with no default falls back to its minimum

- **WHEN** a template declares `size: ["{box_w}", 10]` and `box_w` declares `min: 12` and no `default`
- **THEN** load-time validation resolves that axis as 12
- **AND** a parameter declaring neither resolves as 0, which the written-zero rule then judges

#### Scenario: An intrinsic size is never consulted at load

- **WHEN** a template declares a `content`-width `text` whose placeholder content would overflow
- **THEN** it loads, because no text is measured at load, and whether it overflows is per request

#### Scenario: An accumulation is a render failure, not a load refusal

- **WHEN** a flow container with an authored inner width of 20 and `gap: 2` holds two `content`-width
  text children whose values are supplied per request
- **THEN** the template loads, because at load each child stands in at the whole 20-wide inner box and
  no arrangement is run
- **AND** a request whose values measure 5 and 6 renders both on one line
- **AND** a request whose values measure 14 and 6 fails at render with `UnsupportedLayoutItem` and
  `details.reason` of `item_out_of_frame`, because the second child is positioned at 16 and its far
  edge is 22

#### Scenario: A data-dependent zero is not a load-time refusal

- **WHEN** a template's box collapses to zero width only for requests supplying an empty value
- **THEN** it loads, and renders an empty box for those requests and a normal box for others
