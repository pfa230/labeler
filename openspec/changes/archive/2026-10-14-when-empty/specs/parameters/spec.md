## MODIFIED Requirements

### Requirement: A value comes from the request or the declared default

The value a token reads for a declared parameter SHALL come from the label's `data`, or failing that from its default, and from nothing else. A parameter that neither supplies is absent: an `enum` is not its first value, and a `datetime` is not the render instant. A `boolean` is never absent, because it always has a default.

An absent parameter SHALL read as empty: a token naming it prints nothing, with or without a reader (`interpolation`), and an active container's `repeat:` over it draws no instance (`layout`). An absent parameter matches a `when:` condition only when that condition is `{ empty: true }` (`layout`). A parameter reference from a layout attribute is never absent, because its parameter declares a default (see "Parameter references from layout attributes").

#### Scenario: An omitted boolean is false

- **WHEN** `bold: { type: boolean }` declares no default, a container carries `when: { bold: false }`, and the render omits `bold`
- **THEN** the container renders

#### Scenario: An omitted enum with no default gates off a branch

- **WHEN** `outline: { type: enum, values: [yes] }` gates a container with `when: { outline: yes }` and the render omits `outline`
- **THEN** the label renders without that container

#### Scenario: An omitted list draws no instance

- **WHEN** `tags: { type: list }` with no default is named by an active container's `repeat:` and the render omits `tags`
- **THEN** the label renders without any instance of that container

#### Scenario: An omitted string prints nothing

- **WHEN** `caption: { type: string }` declares no default, an active `text` prints `"[{caption}]"`, and the render omits `caption`
- **THEN** the text reads `[]`
