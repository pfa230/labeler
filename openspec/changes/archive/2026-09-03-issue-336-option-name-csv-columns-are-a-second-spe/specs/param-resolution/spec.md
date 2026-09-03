## REMOVED Requirements

### Requirement: A parameter is required unless the template declares a default
**Reason**: The `option.<name>` column category is withdrawn; every CSV column is a plain data
column whose empty cells are `""` rather than an omission.
**Migration**: Rename any `option.<name>` header to the declared parameter name; no template change
is required.

## ADDED Requirements

### Requirement: A parameter is required unless the template declares a default: CSV data cells are plain values

For one request, the value a `{token}` reads for a declared parameter SHALL come from exactly two
places, tried in this order:

1. the request's `data` map;
2. the parameter's declared `default:`, resolved per the declared-default requirement.

There is no third place. The service SHALL NOT derive the value a token reads from the parameter's
type, from its `values` list, from its `min` or `max`, or from the clock. A parameter that neither
source supplies is **absent**, and absent is a state the render carries rather than an error in itself.

An absent parameter that an **active** layout item reads through a token SHALL be `422 MissingField`
naming the parameter, on the same terms and with the same payload as an absent request field. Whether
an item is active is decided by its `when:` predicate, and an item under an unmatched predicate is
neither measured nor rendered, so a parameter that only an inactive branch reads SHALL NOT be required.

**A `repeat:` key is a read of its parameter and gets the same answer.** An active `container` carrying
`repeat: tags` reads the list it names in order to know how many instances to draw (`repetition`), so an
absent `tags` SHALL be `422 MissingField` naming it, and one only an inactive container repeats SHALL
NOT be required. An absent list SHALL NOT be read as a list of zero elements: `list-params` keeps `[]`
distinct from an omission on every other path, and folding them together here would undo that
distinction where it is most visible, in the count of things a label draws. This clause is written
because the sentence above names a token and a `repeat:` is not one; nothing else about it differs.

**A repeat binds one name inside one subtree, and that is not a third source.** Within the subtree a
`repeat: tags` creates, the name `tags` denotes one element of the value the two sources above already
resolved, for what that subtree reads as text (`repetition`). No new place is consulted: the list still
comes from the request's `data` map or from the declared `default:`, and the binding only says which
part of it a token or a `when:` key inside that subtree sees. Outside every such subtree the two-source
rule reads exactly as it does above.

An absent parameter named by a `when:` predicate SHALL make that predicate false. It SHALL NOT be an
error, because a predicate asks what a value is and absence is an answer. A template whose every branch
is gated on an absent parameter therefore renders none of them rather than failing.

This rule holds for every parameter type. A `boolean` with no declared `default:` is not `false`, an
`enum` with no declared `default:` is not its first value, and a `datetime` with no declared `default:`
is not the render instant.

**Two things that look like a third source and are not.** A CSV data column is placed into the row's
`data` map before the label is built, even when the cell is empty, so it reaches this rule as
`"<name>": ""` rather than as an omission. And the renderer's internal option-selection argument is
populated by nothing at all: no request model carries it, so no caller can reach it, and the preview
requirement supplies none either. No token takes a value through it.

**What this rule does not reach, stated here rather than in a footnote.** A numeric parameter named by a
container's `width`/`height` `ref:` is resolved by *different* mechanisms, which do derive a value when
the parameter has no usable default, and which do not even agree with each other: at load
`load_geometry_values` falls back `min` → `max` → `0.0` (`src/templates.rs:1514-1529`) while
`resolve_f32_default` falls back `min` → `0.0` (`:1531-1544`) and `resolve_u16_default` falls back to
`400` (`:1546-1556`); at render `render_geometry_values` falls back `min` → `0.0` and never consults
`max` (`src/render/mod.rs:927-946`). They carry the same defect this requirement removes, in another
place, and this capability neither governs nor changes them; they are tracked as **#261**. The absolute
sentence above is about the value a token reads.

#### Scenario: An omitted boolean with no default fails

- **WHEN** a template declares `bold: { type: boolean }`, an active `text` item renders `{bold}`, and
  the request omits `bold`
- **THEN** the response is `422 MissingField` naming `bold`

#### Scenario: An omitted enum with no default fails

- **WHEN** a template declares `size: { type: enum, values: [small, large] }`, an active item renders
  `{size}`, and the request omits `size`
- **THEN** the response is `422 MissingField` naming `size`, rather than the label printing `small`

#### Scenario: An omitted enum gates a branch off rather than failing

- **WHEN** a template declares `outline: { type: enum, values: [yes] }`, a container carries
  `when: { outline: yes }`, and the request omits `outline`
- **THEN** the label renders with that container absent, and the response is not an error

#### Scenario: An omitted boolean gates a branch off rather than selecting one

- **WHEN** a container carries `when: { bold: "false" }`, `bold` declares no `default:`, and the
  request omits `bold`
- **THEN** that container is absent, rather than rendered because `bold` was taken as `false`

#### Scenario: An omitted list a container repeats fails

- **WHEN** a template declares `tags: { type: list }` with no default, an active packed container
  carries `repeat: tags`, and the request omits `tags`
- **THEN** the response is `422 MissingField` naming `tags`, rather than the strip rendering with no
  instances

#### Scenario: A list only an inactive repeat names is not required

- **WHEN** that container also carries `when: { show_tags: "true" }` and the request omits both
  `show_tags` and `tags`
- **THEN** the label renders with no instances and no error

#### Scenario: A parameter only an inactive branch reads is not required

- **WHEN** an inactive container's `text` item renders `{caption}` and the request omits `caption`
- **THEN** the label renders, and no `MissingField` is raised for `caption`

#### Scenario: A declared default is used

- **WHEN** a template declares `bold: { type: boolean, default: false }` and the request omits `bold`
- **THEN** the label renders with `bold` resolved to `false`

#### Scenario: A filled CSV data cell is an ordinary value

- **WHEN** a CSV import carries a `data` column `orientation` whose cell reads `horizontal`
- **THEN** that row's label carries `orientation: horizontal` in its `data`, and the declared
  default is not reached

#### Scenario: A blank CSV data cell is an empty string

- **WHEN** a CSV import carries a `data` column `title` whose cell is empty for a row, and the
  named parameter declares no `default:`
- **THEN** that row's label carries `title: ""` in its `data` and does not fail with
  `422 MissingField`; if an active item reads it the label renders with the empty string, and
  if the parameter were an `enum` the row would instead fail, contributing a `details.failures`
  entry whose `code` is `InvalidEnumValue` under `422 BatchInvalid`
