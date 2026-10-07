# Interpolation

## Purpose

Defines the `{...}` token grammar used in interpolated template strings: escaping, the `params`, `vars` and `sys` namespaces, the per-request snapshot, datetime formats, the `join` reader, and how tokens bind into `text`, `qr` and `image` items.

## Requirements

### Requirement: Interpolated strings and token grammar

Interpolation SHALL be substitution only: no operators, functions or chaining. It applies to a `text` item's `value:`, a `qr` item's `value:`, an `image` item's `src:`, and a string `default:` in `params:`, with the same grammar at all four sites. A token is:

```
token       := "{" value-path [ ":" reader ] "}"
value-path  := bare-name | root "." key
bare-name   := [a-zA-Z0-9_-]+
root        := "vars" | "sys"
reader      := format-name | "join('" separator "')"
format-name := [a-zA-Z0-9_-]+
separator   := any characters except "'", "{" and "}"
```

`{{` and `}}` SHALL emit literal braces. They are recognised before any token, and a token ends at the first `}`. Roots, names, keys and format names are case-sensitive. A token SHALL carry at most one reader. No word is reserved: `vars`, `sys`, `datetime` and `join` are legal parameter names, because a root is reached only through a dot and a reader only through a colon.

#### Scenario: Doubled braces are literal

- **WHEN** a template declaring `id` renders `"{{id}} is {id}"` with `id` = `A-1004`
- **THEN** the label reads `{id} is A-1004`

#### Scenario: A parameter may be named after a root

- **WHEN** a template declares a parameter `vars` and prints `{vars}` and `{vars.qr_base_url}`
- **THEN** `{vars}` prints the parameter and `{vars.qr_base_url}` prints the store value

### Requirement: Interpolation syntax is checked when the template loads

Every interpolated string SHALL be parsed when the template loads; a malformed token or an unbalanced brace SHALL fail validation with a message naming the token (`template contains '<token>': ...`) and the key's path, under the invalid-template rule owned by `templates`. Refused at load:

| Case | Examples |
|---|---|
| Empty token or segment | `{}`, `{ }`, `{vars.}`, `{sys.}`, `{.x}`, `{:long_date}` |
| Bare name outside the character class | `{ id }`, `{my field}` |
| Unknown root (message: `unknown source '<root>'`) | `{datetime.long_date}`, `{a.b}`, `{VARS.x}`, `{Sys.now}` |
| Unknown `sys` value (message: `unknown system value '<v>'`) | `{sys.nwo}`, `{sys.now.long_date}` |
| Empty, doubled or malformed reader | `{x:}`, `{x:a:b}`, `{sys.now:long_date(', ')}`, `{tags:join( ', ' )}`, `{tags:join(a)}`, `{tags:join(', ')x}`, `{tags:join(''')}` |
| An undoubled `{` with no token, or an unmatched `}` | `50% {off`, `a } b` |

#### Scenario: An unknown root is refused

- **WHEN** a template file contains `{datetime.long_date}`
- **THEN** it fails validation reporting `datetime` as an unknown source

#### Scenario: An unknown system value is not a missing field

- **WHEN** a template file contains `{sys.now.long_date}`
- **THEN** it fails validation reporting `now.long_date` as an unknown system value

#### Scenario: An unbalanced brace is refused at load

- **WHEN** a `text` item's `value:` is `"50% {off"`
- **THEN** the template fails validation naming that item's `value` path

### Requirement: A bare token names a declared parameter

A bare token SHALL name a parameter the template declares in `params:`, or the template SHALL fail validation at load (`template contains '{sku}': undeclared parameter 'sku'`). It resolves to that parameter's resolved value (request value or default, owned by `parameters`); there is no fallback to another source. A parameter of any type may be named. A value absent at render SHALL be `422 UnsupportedLayoutItem` with reason `missing_field` naming the parameter.

Inside a container repeating a list parameter, the bare repeated name is one element; that scope is owned by `layout`.

#### Scenario: A bare token resolves a request value or default

- **WHEN** a template declaring `title: { type: string, default: "Untitled" }` renders `"{title}"` without `title`, and again with `title` = `Box 3`
- **THEN** the labels read `Untitled` and `Box 3`

#### Scenario: An undeclared bare token is refused at load

- **WHEN** a `text` `value:`, a `qr` `value:` or an `image` `src:` reads `{sku}` and the template declares no `sku`
- **THEN** the template fails validation naming `sku`

#### Scenario: An absent value fails at render

- **WHEN** a template declaring `id: { type: string }` with no default renders `"{id}"` and the request carries no `id`
- **THEN** the response is `422 UnsupportedLayoutItem` with reason `missing_field` naming `id`

### Requirement: Value stringification

A resolved value SHALL print as: a string as-is, a number or boolean in its JSON text form. A resolved value SHALL print literally: no character in it is interpreted as renderer markup.

#### Scenario: Scalars print their text

- **WHEN** a template prints `"{copies} {bold}"` with integer `copies` = 3 and boolean `bold` = true
- **THEN** the label reads `3 true`

### Requirement: The `vars` namespace

`{vars.<key>}` SHALL resolve from the variables store (owned by `settings`). The key is everything between the first dot and the reader or closing brace, so it may contain dots. An absent key SHALL NOT fail at load; it SHALL be `422 UnsupportedLayoutItem` with reason `missing_field` naming `vars.<key>` at render.

#### Scenario: A dotted key resolves

- **WHEN** a template renders `{vars.site.eu.url}` and the store holds the key `site.eu.url`
- **THEN** that key's value is printed

#### Scenario: An absent key fails at render

- **WHEN** a template references `{vars.not_set}` and the store has no such key
- **THEN** the template loads, and rendering returns `422 UnsupportedLayoutItem` with reason `missing_field` naming `vars.not_set`

### Requirement: One snapshot per request

`sys` has exactly one value, `now`. Each render request SHALL read the clock once, in the server-local timezone (`TZ`), and read the variables store and the `datetime_formats` setting once. Every token on every label of that request, including a token in a parameter default, SHALL resolve against that snapshot, so every `{sys.now}` prints the same instant. A request cannot supply or override `{sys.now}`, and it is never a request input. Thumbnails and previews SHALL print a real instant, not the token text.

#### Scenario: One sheet prints one instant

- **WHEN** every slot of a sheet prints `{sys.now:time}` and rendering crosses a minute boundary
- **THEN** every slot prints the same time

#### Scenario: One batch resolves a default once

- **WHEN** every label of a batch omits a parameter declaring `default: "{sys.now}"` and the run crosses midnight
- **THEN** every label prints the same date

### Requirement: Format readers apply to instants

A format reader SHALL be attached only to an instant: `sys.now`, or a bare token naming a parameter declared `type: datetime`. Any other value path with a format SHALL fail validation at load, stating that a format applies to an instant only. An instant with no reader SHALL print as `%Y-%m-%d`. A format name SHALL name an entry of the `datetime_formats` setting (owned by `settings`), whose strftime pattern prints the instant; a name the setting lacks SHALL NOT fail at load and SHALL fail the render as `422 UnsupportedLayoutItem` with reason `missing_field` naming the token's `<value-path>:<format-name>`.

#### Scenario: A format renders an instant

- **WHEN** a template renders `"Printed {sys.now:long_date}"` with `long_date` = `%B %-d, %Y` on 2026-08-23
- **THEN** the label reads `Printed August 23, 2026`

#### Scenario: A declared datetime parameter takes a format

- **WHEN** `printed_on: { type: datetime }` is `2026-08-19` and the template prints `{printed_on:short_date}` and `{printed_on}` with `short_date` = `%m/%d/%Y`
- **THEN** the label prints `08/19/2026` and `2026-08-19`

#### Scenario: A format on a non-instant is refused

- **WHEN** a template contains `{title:long_date}` for a `string` parameter, or `{vars.qr_base_url:long_date}`
- **THEN** it fails validation stating that a format applies to an instant only

#### Scenario: An unknown format name fails at render

- **WHEN** a template contains `{sys.now:no_such_format}`
- **THEN** it loads, and rendering returns `422 UnsupportedLayoutItem` with reason `missing_field` naming `sys.now:no_such_format`

### Requirement: The `join` reader reads a list parameter

`join('<separator>')` SHALL be attached only to a bare token naming a parameter declared `type: list`; on anything else (another type, an undeclared name, `sys.now`, `vars`) it SHALL fail validation at load. A list parameter SHALL be read only through `join` outside a repeat scope: a bare `{tags}` or `{tags:<format>}` SHALL fail at load with a message saying a list is read through `join('<separator>')`. A join SHALL print the elements in order with the separator between consecutive elements; one element prints itself, zero elements print the empty string. The separator may be empty or contain `:`, and may not contain `'`, `{` or `}`.

#### Scenario: A join renders a list

- **WHEN** a template declaring `tags: { type: list, default: [CONSUMABLE, KIDS] }` prints `{tags:join(', ')}`, `{tags:join('')}` and `{tags:join(' : ')}`
- **THEN** they read `CONSUMABLE, KIDS`, `CONSUMABLEKIDS` and `CONSUMABLE : KIDS`

#### Scenario: A list read without join is refused

- **WHEN** a template declaring `tags: { type: list }` contains `{tags}` or `{tags:join}` outside a repeat scope
- **THEN** it fails validation naming the token and saying a list is read through `join('<separator>')`

#### Scenario: A join on a non-list is refused

- **WHEN** a template contains `{title:join(', ')}` for a `string` parameter, or `{sys.now:join(', ')}`
- **THEN** it fails validation naming the token

### Requirement: Tokens in a parameter default

A string `default:` SHALL be interpolated with this grammar when the default is used, and only dotted tokens (`vars`, `sys`) are allowed: a bare token, including one carrying `join`, SHALL fail validation at load naming the parameter and the token. A non-string default carries no tokens. When a default is used and how its failures are reported is owned by `parameters`.

#### Scenario: A namespaced default resolves

- **WHEN** `url: { type: string, default: "{vars.qr_base_url}" }`, the store holds `qr_base_url = https://ex.co/`, and the request omits `url`
- **THEN** the label prints `https://ex.co/`

#### Scenario: A bare token in a default is refused

- **WHEN** a template declares `copy: { type: string, default: "{message}" }`
- **THEN** it fails validation naming `copy` and `{message}`

### Requirement: Image binding

An `image` item SHALL take its bytes from `src:`, interpolated. How the resolved string is read, as a data URI or as a path under the assets root, is owned by `layout`.

#### Scenario: An image source is interpolated

- **WHEN** an `image` carries `src: "logos/{vars.brand}.png"` and the store holds `brand = acme`
- **THEN** the asset `logos/acme.png` is used

#### Scenario: A parameter supplies a per-label image

- **WHEN** a template declares `photo: { type: string }`, an `image` carries `src: "{photo}"`, and the request sends `photo` as a PNG data URI
- **THEN** the label renders that image
