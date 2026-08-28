## Purpose
Defines the `wrap` field on a `text` layout item: its name, its default, and what happens to a template
still using its former spelling. This capability owns the schema and the migration, and nothing else:
the layout consequences of the flag belong to `layout-sizing`, and the input list's controls to
`template-inputs`.

## Requirements

### Requirement: A text item's soft-wrapping flag is named `wrap`

A `text` layout item SHALL accept an optional boolean field `wrap`, defaulting to `false`. It decides
soft wrapping and nothing else; what it does to the layout is specified by the `layout-sizing`
requirement "Text is laid out against the box it will get, and what does not fit is authored".

`multiline` SHALL NOT be an accepted field on a `text` layout item. The rule turns on whether the key
is **written**, not on what it holds: `multiline: true`, `multiline: false`, `multiline: "yes"` and
`multiline:` written and left empty (an explicit YAML null) SHALL all be refused alike. There is no
alias and no deprecation window.

A template declaring it SHALL fail to load: it is excluded from the served set and reported as broken
through the same channel as every other content fault, under the existing rules of the
`template-registry` capability, and SHALL NOT abort startup. The reported error SHALL name the file,
the layout path of the offending item, and the rename, so the reader can fix the template without
consulting a changelog. The same refusal SHALL apply to a template submitted through the write
endpoint, which validates before writing.

The parameter attribute `params[].multiline` is a different field with a different meaning and is
unchanged by this requirement.

This requirement supersedes the frozen `docs/SPEC.md` §4.1 clause naming `multiline` (default `false`)
in the `text` item's field list, and the §4.1 sentence "Single-line text collapses spaces to
non-breaking and renders only the first line." The rest of that field list — `value`, placement,
`font_size`, `font_weight`, `alignment`, `when` — is unchanged and remains authoritative.

#### Scenario: The flag defaults to off

- **WHEN** a `text` item declares no `wrap` field
- **THEN** it does not soft-wrap, and a line wider than its box is resolved by its `overflow` policy

#### Scenario: An unmigrated template is quarantined, not served

- **WHEN** the templates tree holds one template whose `text` item declares `multiline: true` and one
  valid template
- **THEN** the service starts and the valid template is served
- **AND** the unmigrated template is not served, and is reported as broken with an error naming its
  file, the item's layout path, and the `multiline` → `wrap` rename

#### Scenario: An explicitly null old key is refused, not ignored

- **WHEN** a `text` item declares `multiline:` with no value
- **THEN** the template is refused with the same error, rather than loading as though the key were
  absent

#### Scenario: A non-boolean old key gets the rename error, not a type error

- **WHEN** a `text` item declares `multiline: "yes"`
- **THEN** the template is refused with the error naming the rename

#### Scenario: Writing an unmigrated template through the API is refused

- **WHEN** a template whose `text` item declares `multiline` is submitted to the template write
  endpoint
- **THEN** the write is refused with the validation error naming the field and the rename, and no file
  is written

#### Scenario: A parameter may still declare `multiline`

- **WHEN** a template declares a `string` parameter with `multiline: true` and a `text` item with
  `wrap: false`
- **THEN** the template loads and is served
