## Purpose

Names the one key that gates whether a layout item is drawn, so that a template author has exactly
one spelling for a condition and a template written against any earlier one fails loudly instead of
rendering as though it had been written correctly.

## ADDED Requirements

### Requirement: `when:` is the only conditional-visibility key

Any layout item — `container`, `text`, `qr`, `image`, `line` — MAY carry an optional `when:` map,
the predicate deciding whether that item is active. That key SHALL be spelled `when`, and it SHALL be
the only key with that meaning on any item.

`option` SHALL NOT be accepted on a `container`, and SHALL NOT be accepted on any other layout item
either. A `container` declaring `option` SHALL be refused at load exactly as it is refused on every
other item type: as an unknown field, in an error naming that item's layout path. There is no alias,
no desugaring into `when`, no deprecation window and no warning path, so a template written against
the earlier spelling fails to load rather than rendering a condition its author can no longer see in
the file.

A `container` declaring both `when` and `option` SHALL be refused on the same terms, because the
refusal is of the key and not of a conflict between two keys. A refusal SHALL NOT depend on what the
`option` map contains: an empty map, a map naming an undeclared parameter and a map that would have
been a valid `when` are all refused identically.

The refusal is the ordinary template-content fault. The file is quarantined, its error carries the
layout path of the offending item, the fault is reported through the paths the `template-registry`
capability specifies for a refused template, and the server still starts and still serves every other
template.

The same refusal SHALL apply to a template submitted over HTTP, because the body of a write is parsed
on the same terms a file on disk is. A `PUT /api/templates/{id}` whose YAML body declares `option` on
any layout item SHALL be rejected with `422`, `error.code` `TemplateInvalid` and
`error.details.reason` `template_parse_failed`, in a message naming `option` and that item's layout
path. The rejection SHALL be decided before anything is written, as the `template-registry`
capability's requirement that a `422` from a template write means nothing was written already
demands: replacing an existing template leaves its stored file byte-for-byte unchanged, and a
create-only write (`If-None-Match: *`) creates no file.

This requirement supersedes the frozen `docs/SPEC.md` §5 insofar as that section names which key
spells a condition, and only insofar as it does. How a `when:` predicate is evaluated — that all of
its conditions must match the resolved parameter values, that an inactive item and its children are
excluded from both the measurement pre-pass and rendering, when an absent parameter makes a predicate
false, and when a bad value rejects the render — is not this requirement's subject and is unchanged
by it. Those rules live in `docs/SPEC.md` §5 and, where later requirements have superseded it, in the
`param-resolution` and `template-inputs` capabilities.

#### Scenario: A container declaring `option` is refused

- **WHEN** a template declares a `container` carrying `option: { orientation: vertical }`
- **THEN** the template fails to load and is quarantined with an unknown-field error naming `option`
  and that container's layout path
- **AND** the server still starts and still serves every other template

#### Scenario: A `PUT` body carrying `option` is rejected before the write

- **WHEN** `PUT /api/templates/{id}` receives a YAML body whose `container` carries
  `option: { orientation: vertical }`
- **THEN** the response is `422` with `error.code` `TemplateInvalid`, `error.details.reason`
  `template_parse_failed`, and a message naming `option` and that container's layout path
- **AND** an existing template at that id is left byte-for-byte unchanged, and no file is created
  when the write was create-only

#### Scenario: The same condition under `when` loads

- **WHEN** the same template is rewritten with `when: { orientation: vertical }` in place of
  `option:`
- **THEN** the template loads, and the container is drawn exactly when the predicate matches

#### Scenario: Both spellings together are still refused

- **WHEN** a `container` declares both `when: { mode: a }` and `option: { mode: a }`
- **THEN** the template is refused with the same unknown-field error naming `option`, and neither
  predicate is applied

#### Scenario: `option` on a non-container item is refused

- **WHEN** a template declares `option` on a `text`, `qr`, `image` or `line` item
- **THEN** the template is refused with an unknown-field error naming that item's layout path,
  unchanged from today

#### Scenario: An empty `option` map is refused

- **WHEN** a `container` declares `option: {}`
- **THEN** the template is refused with the same unknown-field error, because the key is refused
  before its contents are read
