# interpolation-tokens Specification

## Purpose
Defines the interpolation token grammar every `text` and `qr` `value:` and every data-bound `image`
`src:` is written in: what a token may name, how a namespace is navigated, how a format is attached to
an instant, and which failures a template's own text decides at load rather than at render.

## Requirements

### Requirement: A token names one value and may attach one format

*This requirement supersedes, in `docs/SPEC.md` §8 ("Data binding"), the "Token types and precedence"
list, the opening `value` bullet's clause "Tokens are resolved in precedence order, then `{{` and `}}`
emit literal braces", and the closing paragraphs on `now` capture, on `422 MissingField`, and on JSON
scalar stringification. It restates their complete post-change contract. What survives untouched in §8
is the `image` binding bullet, the statement that interpolation applies to text and QR content, and the
substitution-only rule. It also supersedes the sentence in the `datetime_formats` entry of
`docs/SPEC.md`'s unnumbered `Settings` section (`docs/SPEC.md:1036`, the sentence at `:1056`) reading "Used by `{datetime.<name>}` interpolation (see §8)": the setting is
unchanged, the spelling that reaches it is `{sys.now:<name>}` and `{<datetime-param>:<name>}`.*

Interpolation stays substitution-only (ADR-0010, ADR-0055). There are no operators, no functions, no
chaining and no filter arguments. A token is:

```
token       := "{" value-path [ ":" format-name ] "}"
value-path  := bare-name | root "." key
bare-name   := ^[a-zA-Z0-9_-]+$
root        := "vars" | "sys"
format-name := ^[a-zA-Z0-9_-]+$
```

`{{` and `}}` emit literal braces and are not tokens.

This grammar governs every interpolated string a template carries, which is a `text` item's `value:`, a
`qr` item's `value:`, an `image` item's `src:`, and a parameter's `default:` in `params:`. The same
tokens, the same load-time refusals and the same render-time errors apply to all four.

One restriction is peculiar to a `default:` and holds nowhere else: its `value-path` SHALL be dotted.
A **bare** token in a `default:` SHALL be a load-time refusal naming the parameter and the token. The
sources a default may read are therefore fixed before a request arrives, so a default can never depend
on another parameter, on the request `data` map, or on a second default; there is no resolution order
among defaults and no cycle among them to detect. A `default:` that is not a string carries no token
and is used as written.

A token has exactly one interpretation, decided by its shape, so the service SHALL NOT resolve tokens
in a precedence order and SHALL NOT try one source and fall through to another when it does not match.
This replaces the four-level precedence list the frozen §8 defines.

- A **bare** `value-path` names a request value: the parameter of that name if the template declares
  one in `params:`, otherwise a field of the request `data` map. A declared parameter SHALL be resolved
  as that parameter, including its declared `default`, and the `data` map supplies its value rather
  than shadowing it.
- A **dotted** `value-path` names a value under a namespace root, per the requirement below.

A resolved value that is a JSON scalar SHALL be stringified: strings as-is, numbers and booleans via
their textual form, `null` as the empty string, and any other JSON value via its JSON text.

A value that is absent when the label renders SHALL be `422 MissingField` naming the token's
`value-path`.

A `default:` is the one exception, and it covers **every** failure raised while resolving one, not only
an absent value: an absent `{vars.<key>}`, an unknown format name, and any other error this capability
would otherwise report as the caller's. What such a failure reports is decided by the `param-resolution`
capability, because the caller supplied nothing and has nothing to correct. Where a requirement of this
capability names `422 MissingField` for one of those failures, it is superseded for a `default:` alone and
unchanged everywhere else.

Brace syntax errors in a `text` item's `value:`, a `qr` item's `value:` and an `image` item's `src:` are
unchanged by this capability: an unterminated `{` or an unmatched `}` SHALL be `400 InvalidRequest` with
`details.reason` `interpolation_syntax`, raised when the label renders.

In a `default:` the same malformed sequence SHALL be refused **when the template loads**, naming the
parameter. A literal brace in a default is still written `{{` or `}}`; what differs is when the service
says so. Three reasons, and the first is decisive: a default is not interpolated today, so this failure
is *new*, and surfacing a new failure at render would report text only a template author wrote as the
caller's `400 InvalidRequest` against a request that supplied nothing. Second, a default's braces are as
fixed at load as its tokens are, so nothing is lost by deciding them there. Third, refusing at load stops
a template being saved that can never render.

This check SHALL scan a `default:` for well-formed tokens and apply the render path's brace-balance rule
to the text **between** them, which is exactly what the render path does. It SHALL NOT be the token
scanner alone, which by design skips a malformed brace sequence rather than reporting it, and it SHALL
NOT be that brace-balance rule applied to the whole string, which treats every undoubled `{` as an error
and would refuse a legal `default: "{sys.now}"`. This capability does not extend that reasoning to the
other three sites, whose render-time contract is unchanged and remains an inconsistency it does not
resolve.

Every load-time refusal in this capability is one validation rule, reached by two paths. When a template
file is read from disk at startup or reload, the file is quarantined and the service still starts. When
the same content arrives through a template write (`POST`/`PUT`), the write SHALL be refused with
`422 TemplateInvalid` and `details.reason` `template_validation_failed`, and nothing SHALL be stored, so
an operator cannot save a template the loader would quarantine.

An `image` item's `name:` names a request `data` field directly rather than through a token. It SHALL be
a legal bare name under the same rule that binds every other bare name, so that the field an `image`
binds is always one a `{token}` could also name.

#### Scenario: A template write is refused, not quarantined

- **WHEN** a `PUT` to a template carries `{datetime.long_date}` in a `text` item's `value`
- **THEN** the response is `422 TemplateInvalid` with `details.reason` `template_validation_failed`, and
  the stored template is unchanged

#### Scenario: A bare token resolves a request field

- **WHEN** a template renders `"Asset {id}"` with `data: { "id": "A-1004" }`
- **THEN** the label reads `Asset A-1004`

#### Scenario: A bare token resolves a declared parameter's default

- **WHEN** a template declares `title: { type: string, default: "Untitled" }` and renders `"{title}"`
  with no `title` in the request
- **THEN** the label reads `Untitled`

#### Scenario: A namespaced token in a default is resolved

- **WHEN** a template declares `url: { type: string, default: "{vars.qr_base_url}" }`, the store holds
  `qr_base_url = https://ex.co/`, and the request carries no `url`
- **THEN** the label reads `https://ex.co/`

#### Scenario: A bare token in a default is refused when the template loads

- **WHEN** a template file declares `copy: { type: string, default: "{message}" }`
- **THEN** the file is quarantined with a validation message naming `copy` and `{message}`, and the same
  content arriving through a `PUT` is refused with `422 TemplateInvalid`

#### Scenario: A literal brace in a default is escaped like any other

- **WHEN** a template declares `label: { type: string, default: "{{draft}}" }` and the request omits it
- **THEN** the label reads `{draft}`

#### Scenario: An unescaped brace in a default is refused when the template loads

- **WHEN** a template file declares `label: { type: string, default: "50% {off" }`
- **THEN** the file is quarantined with a validation message naming `label`, the same content is refused
  on `PUT` with `422 TemplateInvalid`, and no request ever receives `400 InvalidRequest` for it

#### Scenario: Doubled braces are literal

- **WHEN** a template renders `"{{id}} is {id}"` with `data: { "id": "A-1004" }`
- **THEN** the label reads `{id} is A-1004`

#### Scenario: An image source is interpolated by the same rules

- **WHEN** a template's `image` item carries `src: "logos/{vars.brand}.png"` and the store holds
  `brand = acme`
- **THEN** the asset `logos/acme.png` is resolved
- **AND** the same item carrying `src: "logos/{datetime.brand}.png"` fails validation at load, naming
  `datetime` as an unknown source

#### Scenario: An absent field fails when the label renders

- **WHEN** a template renders `"{id}"` and the request carries no `id`
- **THEN** the response is `422 MissingField` naming `id`, and the template itself loaded without error

### Requirement: A dot navigates a namespace, and there are exactly two roots

A dot in a `value-path` separates a namespace root from a key inside it, and carries no other meaning.
Exactly two roots exist:

- **`vars.<key>`** resolves from the operator variables store. Everything between the first dot and the
  token's `:` or `}` is the key, verbatim, so a key may itself contain dots. Because the store is
  mutable at runtime, an absent key SHALL be `422 MissingField` at render and SHALL NOT be an error at
  template load.
- **`sys.<name>`** resolves a value the service supplies. The set of `<name>`s is closed and fixed when
  the service is built; it is not operator-configurable and not request-supplied. It has exactly one
  member, `now`, defined by the requirement below.

A dotted token whose root is neither `vars` nor `sys` names a source that does not exist. The template
SHALL fail validation at load with a message naming the token and its unknown source, and the file
SHALL be quarantined under the rules of the `template-registry` capability while the service still
starts.

A `sys.<name>` naming a value outside the closed set SHALL likewise fail validation at load, with a
message distinguishing an unknown **value** under a known root from an unknown **root**. Because a
`sys` name is the whole remainder of the path, `{sys.now.long_date}` names the value `now.long_date`,
which is not a member, and is refused for that reason rather than read as `now` with a format.

Both refusals SHALL name the offending token and SHALL state the spelling that replaces it, so the
message an operator reads carries the fix and not only the fault.

Roots are matched exactly and are lower-case. `{VARS.x}` and `{Sys.now}` name unknown sources and are
refused at load; bare names, variable keys and format names are likewise case-sensitive.

No segment may be empty. `{vars.}`, `{sys.}`, `{.x}` and a token that is only whitespace SHALL fail
validation at load.

Because the set of roots is fixed and a root is reachable only through a dot, adding a second system
value SHALL require no change to parameter-name validation and SHALL NOT invalidate any template that
does not already use that value.

#### Scenario: A variables key resolves at render

- **WHEN** a template renders `"{vars.qr_base_url}/{id}"` and the store holds
  `qr_base_url = https://box.example`
- **THEN** the label reads `https://box.example/A-1004`

#### Scenario: An absent variables key is a render error, not a load error

- **WHEN** a template references `{vars.not_set}` and the store holds no such key
- **THEN** the template loads and is served, and rendering it returns `422 MissingField` naming
  `vars.not_set`

#### Scenario: An unknown source is refused when the template loads

- **WHEN** a template file contains `{datetime.long_date}`
- **THEN** the file fails validation with a message naming the token, reporting `datetime` as an
  unknown source, and naming `{sys.now:long_date}` as the replacement; the file is quarantined, and the
  service still starts and serves every other template

#### Scenario: An unknown system value is refused when the template loads

- **WHEN** a template file contains `{sys.nwo}`
- **THEN** the file fails validation with a message reporting `nwo` as an unknown system value rather
  than as a missing field, and the file is quarantined

#### Scenario: A dotted rewrite of the system instant is refused

- **WHEN** a template file contains `{sys.now.long_date}`, the mechanical mis-rewrite of
  `{datetime.long_date}`
- **THEN** the file fails validation reporting `now.long_date` as an unknown system value and naming
  `{sys.now:long_date}` as the replacement, and the file is quarantined

#### Scenario: A root is matched exactly

- **WHEN** a template file contains `{VARS.qr_base_url}`
- **THEN** the file fails validation reporting `VARS` as an unknown source

#### Scenario: An empty segment is refused

- **WHEN** a template file contains `{vars.}` or `{sys.}`
- **THEN** the file fails validation naming the token, and the file is quarantined

#### Scenario: A variables key may contain dots

- **WHEN** a template renders `{vars.site.eu.url}` and the store holds the key `site.eu.url`
- **THEN** that key's value is substituted

### Requirement: A colon attaches a format name, and only an instant takes one

A `:` separates a value from the name of a format applied to it, and carries no other meaning. The
format name SHALL be an entry of the `datetime_formats` app setting, whose strftime pattern is applied
to the instant the `value-path` resolves to.

A format SHALL be attached only where the value is an instant. Exactly two value paths are instants:

- `sys.now`, and
- a bare token naming a parameter the template declares as `type: datetime`.

A token attaching a format to any other value path SHALL fail validation at load, with a message naming
the token and stating that a format applies to an instant only. This is decidable from the template's
own text, because `params:` is part of the file.

An instant written with no format SHALL render as ISO `%Y-%m-%d`.

Because `datetime_formats` is runtime state, a format name that the setting does not hold SHALL NOT be
an error at template load. It SHALL be `422 MissingField` when the label renders, naming the field as
the whole token text `<value-path>:<format-name>`.

A token SHALL carry at most one format name, and that name SHALL NOT be empty. A second colon is part
of no valid token. `{x:a:b}`, `{x:}` written with a trailing colon and no name, and `{:long_date}`
written with no value path SHALL each fail validation at load, naming the token. `{x:}` in particular
SHALL NOT be read as the bare value `x`: a colon that is written is a format that is claimed, and a
claim with no name is a mistake worth reporting rather than silently printing an unformatted value.

#### Scenario: A format renders the system instant

- **WHEN** a template renders `"Printed {sys.now:long_date}"` with the default `long_date` pattern
  `%B %-d, %Y` on 2026-08-23
- **THEN** the label reads `Printed August 23, 2026`

#### Scenario: A format renders a declared datetime parameter

- **WHEN** a template declaring `printed_on: { type: datetime }` renders `"{printed_on:short_date}"`
  with `printed_on` set to `2026-08-19` and the default `short_date` pattern `%m/%d/%Y`
- **THEN** the label reads `08/19/2026`

#### Scenario: An instant with no format prints an ISO date

- **WHEN** the same template renders `"{printed_on}"` and `"{sys.now}"`
- **THEN** both print their instant as `YYYY-MM-DD`

#### Scenario: A format on a string is refused when the template loads

- **WHEN** a template declaring `title: { type: string }` contains `{title:long_date}`
- **THEN** the file fails validation with a message naming the token and stating that a format applies
  to an instant only, and the file is quarantined

#### Scenario: A format on a variables key is refused when the template loads

- **WHEN** a template contains `{vars.qr_base_url:long_date}`
- **THEN** the file fails validation for the same reason and is quarantined

#### Scenario: A token carrying two colons is refused

- **WHEN** a template file contains `{sys.now:long_date:short_date}`
- **THEN** the file fails validation naming the token, and the file is quarantined

#### Scenario: An empty format name is refused rather than ignored

- **WHEN** a template file contains `{printed_on:}`
- **THEN** the file fails validation naming the token, rather than loading and printing the parameter's
  bare ISO date

#### Scenario: An unknown format name fails at render, not at load

- **WHEN** a template contains `{sys.now:no_such_format}`
- **THEN** the template loads successfully, and rendering it returns `422 MissingField` naming
  `sys.now:no_such_format`

#### Scenario: A request cannot reach a formatted token through its data

- **WHEN** a request sends `data: { "printed_on:long_date": "whatever" }` for a template declaring
  `printed_on` as a `datetime` parameter and printing `{printed_on:long_date}`
- **THEN** the label prints the parameter's instant through the `long_date` pattern, because a bare
  token cannot contain a colon and no data key is reachable under that name

### Requirement: `{sys.now}` is the request's single captured instant

Every render request SHALL capture one instant, in the server-local timezone (controlled by `TZ`), and
SHALL read the clock exactly once while doing so. This capability is the single home of that rule; the
`datetime-params` capability applies it to a declared parameter rather than restating it.

`{sys.now}` and `{sys.now:<format>}` SHALL resolve that instant, so every label in one batch, sheet or
ZIP prints the same one and a run spanning midnight cannot print two different dates.

`sys.now` SHALL NOT be reported as a request field a caller must supply, SHALL NOT appear in the field
list a template advertises, and SHALL render as a real formatted instant in a thumbnail or preview
rather than as a placeholder string.

A request SHALL have no way to supply or override `sys.now`. A `data` key spelled `sys.now` is
unreachable, because a bare token cannot contain a dot.

This requirement supersedes the `{datetime}` and `{datetime.<name>}` tokens of `docs/SPEC.md` §8 and
of ADR-0028. Those spellings resolve nothing: `{datetime.<name>}` is an unknown source and fails at
load, and bare `{datetime}` is an ordinary bare token, which is to say a request field named
`datetime`, absent from the request unless a caller sends one.

#### Scenario: One sheet prints one instant

- **WHEN** a sheet of labels prints `{sys.now:time}` on every slot and the render crosses a minute
  boundary
- **THEN** every slot prints the same time

#### Scenario: The system instant is not an advertised field

- **WHEN** a template's layout references `{sys.now}` and `{sys.now:long_date}` and nothing else that
  is data-bound
- **THEN** the field list the template advertises is empty

#### Scenario: A thumbnail prints a real date

- **WHEN** a thumbnail is rendered for a template printing `{sys.now:short_date}`
- **THEN** the thumbnail shows the current date in that format, not the literal token text

#### Scenario: The retired bare spelling becomes an ordinary field

- **WHEN** a template prints `{datetime}` and the request carries no `datetime`
- **THEN** the template loads, `datetime` is advertised as a request field, and rendering returns
  `422 MissingField` naming `datetime`

### Requirement: A bare name is a bare name, and no word is reserved

*This requirement supersedes the "Namespace rules and reserved names" list in `docs/SPEC.md` §3.0
("Parameters (`params:`)"), and nothing else in §3.0, and restates that list's complete post-change
contract. This capability is the single home of the parameter-name rule: the `datetime-params`
capability supersedes the rest of §3.0 and points here for naming rather than restating them.*

A parameter name SHALL match `^[a-zA-Z0-9_-]+$`. The reason is the grammar and nothing else: a dot
separates a namespace root from its key and a colon separates a value from its format, so a name that
is written bare may contain neither, and neither may a request `data` key a template means to read.

No word SHALL be reserved. `vars`, `sys` and `datetime` are all valid parameter names, because a root
is reachable only through a dot: `{vars}` is the parameter, `{vars.key}` is the store, and the two
cannot be confused by a reader or by the service. The restriction on parameter names is therefore
structural, over one character class, rather than lexical over an open list of words that grows with
every ambient value the service gains.

A bare token that is not a legal bare name SHALL fail validation at load, naming the token, and the
file SHALL be quarantined.

#### Scenario: A parameter may be named after a namespace root

- **WHEN** a template declares a parameter named `vars` and prints both `{vars}` and
  `{vars.qr_base_url}`
- **THEN** the template loads, `{vars}` prints the parameter's value, and `{vars.qr_base_url}` prints
  the store's value

#### Scenario: A parameter may be named `datetime`

- **WHEN** a template declares `datetime: { type: string, default: "n/a" }` and prints `{datetime}`
- **THEN** the template loads and the label prints `n/a`

#### Scenario: A parameter name carrying a separator is refused

- **WHEN** a template declares a parameter named `printed_on.long_date` or `printed_on:long_date`
- **THEN** the template fails validation naming the parameter, and the file is quarantined

#### Scenario: A malformed bare token is refused when the template loads

- **WHEN** a template contains `{}` or `{ id }`
- **THEN** the file fails validation naming the token, and is quarantined

#### Scenario: A bare token carrying a separator or a space is refused

- **WHEN** a template contains `{my field}`, `{a.b}` intended as a literal data key, or
  `{custom:Internal SKU}`
- **THEN** each fails validation naming the token: a space is not in the name's character class, a dot
  names an unknown source, and a colon claims a format on a value that is not an instant

### Requirement: A value a bare token cannot name is bound by mapping, not by spelling

A request `data` key, a CSV import header, and a connector field key are reachable from a template only
through a bare token, so each SHALL be a legal bare name to be read directly.

A source system MAY key its own fields in ways this grammar cannot name — Homebox's per-item custom
fields, keyed `custom:<name>`, are the case that exists. Such a field SHALL remain reachable: the
template declares or references a legally named field, and the operator binds the source key to it
through the field mapping the connector grid already provides, which maps a template field to any
connector key. The service SHALL NOT invent a rewriting of an illegal key into a legal one, because a
rewrite that two different keys could collide into is worse than an explicit mapping.

A convenience that pre-fills a mapping by matching a template field to a connector key of the same name
SHALL simply not match for such a key, because no template field can carry that name.

#### Scenario: A colon-keyed connector field is bound through the mapping

- **WHEN** a connection offers the field key `custom:Internal SKU` and a template prints `{internal_sku}`
- **THEN** the operator maps `internal_sku` to `custom:Internal SKU` in the connector grid, and the label
  prints that field's value

#### Scenario: A template cannot name the connector key directly

- **WHEN** a template prints `{custom:Internal SKU}`
- **THEN** the template fails validation, because `custom` is not an instant and `Internal SKU` is not a
  legal format name
