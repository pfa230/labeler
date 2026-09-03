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
is the statement that interpolation applies to text and QR content, and the substitution-only rule; the
`image` binding bullet no longer survives, because the requirement "An `image` item binds a declared
parameter or a bundled asset" supersedes it. It also supersedes the sentence in the `datetime_formats`
entry of `docs/SPEC.md`'s unnumbered `Settings` section (`docs/SPEC.md:1036`, the sentence at `:1056`)
reading "Used by `{datetime.<name>}` interpolation (see §8)": the setting is
unchanged, the spelling that reaches it is `{sys.now:<name>}` and `{<datetime-param>:<name>}`.*

Interpolation stays substitution-only (ADR-0010, ADR-0055). There are no operators, no functions and no
chaining. The one argument the grammar admits is a `join`'s separator, which selects the text placed
between the elements of a value the template already declares as a list; it composes nothing, reads
nothing else, and is defined by the reader requirement below. A token is:

```
token       := "{" value-path [ ":" reader ] "}"
value-path  := bare-name | root "." key
bare-name   := ^[a-zA-Z0-9_-]+$
root        := "vars" | "sys"
reader      := format-name | join-call
format-name := ^[a-zA-Z0-9_-]+$
join-call   := "join" "(" "'" separator "'" ")"
separator   := any run of characters other than "'", "{" and "}"
```

`{{` and `}}` emit literal braces and are not tokens. Neither is an escape **inside** a token: a doubled
brace is recognised before any token is looked for, and a token ends at the first `}` whatever precedes
it, so no brace can be written inside a `join`'s separator. What that costs, and which of two refusals a
brace there receives, is stated by the reader requirement below.

This grammar governs every interpolated string a template carries, which is a `text` item's `value:`, a
`qr` item's `value:`, an `image` item's `src:`, and a parameter's `default:` in `params:`. The same
tokens, the same load-time refusals and the same render-time errors apply to all four.

One restriction is peculiar to a `default:` and holds nowhere else: its `value-path` SHALL be dotted. A
**bare** token in a `default:` SHALL be a load-time refusal naming the parameter and the token. The
sources a default may read are therefore fixed before a request arrives, so a default can never depend
on another parameter, on the request `data` map, or on a second default; there is no resolution order
among defaults and no cycle among them to detect. A `default:` that is not a string carries no token
and is used as written, which is how a `list` parameter's sequence default reaches the model unaltered.

A token has exactly one interpretation, decided by its shape, so the service SHALL NOT resolve tokens
in a precedence order and SHALL NOT try one source and fall through to another when it does not match.
This replaces the four-level precedence list the frozen §8 defines.

- A **bare** `value-path` names the parameter of that name, which the template SHALL declare in
  `params:`. The parameter SHALL be resolved as that parameter, including its declared `default`, and
  the request `data` map supplies its value rather than shadowing it. There is no second reading: a
  bare name the template does not declare names **nothing**, and the template SHALL fail validation
  when it loads, with a message naming the token and the undeclared name. The file SHALL be
  quarantined under the `template-registry` rules while the service still starts, and the same content
  arriving through a template write SHALL be refused with `422 TemplateInvalid`. This holds wherever
  this grammar governs a bare token: a `text` item's `value:`, a `qr` item's `value:` and an `image`
  item's `src:`.
- A **dotted** `value-path` names a value under a namespace root, per the requirement below.

A resolved value that is a JSON scalar SHALL be stringified: strings as-is, numbers and booleans via
their textual form, and `null` as the empty string.

A resolved value that is a **JSON array** SHALL NOT be stringified. It is a value no scalar slot can
print, and printing its JSON text is what this change removes. Where the name is one the template
declares as `type: list`, the token is refused when the template loads, under the reader requirement
above. Where it is not, an array supplied under a `data` key the template does not declare, the
failure is only knowable at render, and it SHALL be `422 UnsupportedLayoutItem` with `details.reason`
`field_value_not_scalar`, in a message naming the field. That is the code and the shape a caller's
`data` value already fails with when an item cannot use it (`image_data_invalid`,
`image_format_unsupported`), rather than a fifth answer to the same question. That slug is an addition
to the reason registry of `docs/SPEC.md` §10.1, which is frozen and therefore does not list it; this
requirement is its published home. It adds a row to the `UnsupportedLayoutItem` set, changes no other
row, and changes no `code` string.

**A scalar slot is not only a token.** An `image` item's `name:` binds a `data` field directly rather
than through a token, and stringifies it in the measurement pre-pass and in rendering alike. An array
reaching one SHALL be refused with the same code, the same reason and the same message shape, decided
**before** the data URI is parsed, so the failure reports the value's shape rather than surfacing as
`image_data_invalid` about a data URI the caller never wrote. An `image` `name:` naming a parameter the
template declares as `type: list` is refused earlier still, when the template loads, under
`list-params`. Without this clause the one scalar slot a template can write with no `{token}` in it
would be the one place an array still printed something.

In a batch the failure SHALL be reported per label, through the machinery every other per-label failure
uses: each label reaching it SHALL appear in the `details.failures` list of the `422 BatchInvalid`
response carrying the `UnsupportedLayoutItem` code and the `field_value_not_scalar` reason, and no PDF,
ZIP or print job SHALL be produced.

Any other JSON value, meaning an object, SHALL be stringified via its JSON text, unchanged. This capability
neither defends nor extends that, and #213 is about lists; changing it would alter behaviour for inputs
the change never mentions.

A value that is absent when the label renders SHALL be `422 MissingField` naming the token's
`value-path`.

Only **existence** is checked when the template loads, and no type is. Every declared parameter
stringifies, so a bare token may name a parameter of any type; what a format may be attached to is
decided by the requirement on formats below, and nothing else about a parameter's type restricts what a
token may read.

A consequence, stated so it is not mistaken for an omission: a request `data` key naming no declared
parameter is now read by **nothing**, because no token can reach it. This requirement does not say
whether such a key may be sent, and nothing here refuses one; that question is #324's.

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

#### Scenario: A template write is refused, not quarantined

- **WHEN** a `PUT` to a template carries `{datetime.long_date}` in a `text` item's `value`
- **THEN** the response is `422 TemplateInvalid` with `details.reason` `template_validation_failed`, and
  the stored template is unchanged

#### Scenario: A bare token resolves a request field

- **WHEN** a template declaring `id: { type: string }` renders `"Asset {id}"` with
  `data: { "id": "A-1004" }`
- **THEN** the label reads `Asset A-1004`

#### Scenario: A bare token naming no declared parameter is refused when the template loads

- **WHEN** a template file declares no `sku` and a `text` item's `value:` reads `"Asset {sku}"`
- **THEN** the file fails validation with a message naming the token and `sku`, the file is quarantined,
  the service still starts and serves every other template, and the same content arriving through a
  `PUT` is refused with `422 TemplateInvalid`

#### Scenario: The refusal applies wherever a bare token is written

- **WHEN** one template file's `qr` item reads `{sku}` and another's `image` `src:` reads
  `"logos/{sku}.png"`, and neither declares `sku`
- **THEN** each file fails validation naming `sku` and is quarantined

#### Scenario: A bare token may name a parameter of any type

- **WHEN** a template declares `copies: { type: integer }`, `bold: { type: boolean }` and
  `width: { type: length }`, and a `text` item reads `"{copies} {bold} {width}"`
- **THEN** the template loads, and each value is printed through the stringification rule above

#### Scenario: A bare token resolves a declared parameter's default

- **WHEN** a template declares `title: { type: string, default: "Untitled" }` and renders `"{title}"`
  with no `title` in the request
- **THEN** the label reads `Untitled`

#### Scenario: An undeclared array in a scalar slot is refused when the label renders

- **WHEN** a template renders `"{tags}"`, declares no `tags` parameter, and the request sends
  `data: { "tags": ["A", "B"] }`
- **THEN** the response is `422 UnsupportedLayoutItem` with `details.reason` `field_value_not_scalar`,
  its message names `tags`, and the label does not print `["A","B"]`

#### Scenario: An undeclared array bound by an image name is refused the same way

- **WHEN** an active `image` item carries `name: "logo"`, the template declares no `logo` parameter, and
  the request sends `data: { "logo": ["a", "b"] }`
- **THEN** the response is `422 UnsupportedLayoutItem` with `details.reason` `field_value_not_scalar`
  naming `logo`, rather than `image_data_invalid` about a data URI the caller never wrote

#### Scenario: An undeclared array nobody reads is not an error

- **WHEN** the same request is sent for a template whose active layout reads no `tags`
- **THEN** the label renders, because the value is judged where it is used and nowhere else

#### Scenario: A batch names every label that reached an unusable array

- **WHEN** a batch of two labels both send an array for a `data` key an active item prints
- **THEN** the response is `422 BatchInvalid`, no artifact is produced, and `details.failures` carries
  one entry per label with the `UnsupportedLayoutItem` code and the `field_value_not_scalar` reason

#### Scenario: An object still prints its JSON text

- **WHEN** a request sends `data: { "meta": { "a": 1 } }` for a template printing `{meta}`
- **THEN** the label prints that value's JSON text, exactly as it does today

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

- **WHEN** a template declaring `id` renders `"{{id}} is {id}"` with `data: { "id": "A-1004" }`
- **THEN** the label reads `{id} is A-1004`

#### Scenario: An image source is interpolated by the same rules

- **WHEN** a template's `image` item carries `src: "logos/{vars.brand}.png"` and the store holds
  `brand = acme`
- **THEN** the asset `logos/acme.png` is resolved
- **AND** the same item carrying `src: "logos/{datetime.brand}.png"` fails validation at load, naming
  `datetime` as an unknown source

#### Scenario: An absent field fails when the label renders

- **WHEN** a template declaring `id: { type: string }` with no `default:` renders `"{id}"` and the
  request carries no `id`
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
load, and bare `{datetime}` is an ordinary bare token, which is to say the parameter named `datetime`
where the template declares one, and a load-time refusal where it does not.

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

- **WHEN** a template prints `{datetime}` and declares `datetime: { type: string }` with no `default:`,
  and the request carries no `datetime`
- **THEN** the template loads, `datetime` is advertised as a request field, and rendering returns
  `422 MissingField` naming `datetime`
- **AND** a template printing `{datetime}` while declaring no parameter of that name is quarantined,
  naming `datetime`
### Requirement: A bare name is a bare name, and no word is reserved

*This requirement supersedes the "Namespace rules and reserved names" list in `docs/SPEC.md` §3.0
("Parameters (`params:`)"), and nothing else in §3.0, and restates that list's complete post-change
contract. This capability is the single home of the parameter-name rule: the `datetime-params`
capability supersedes the rest of §3.0 and points here for naming rather than restating them.*

A parameter name SHALL match `^[a-zA-Z0-9_-]+$`. The reason is the grammar and nothing else: a dot
separates a namespace root from its key and a colon separates a value from its reader, so a name that
is written bare may contain neither, and neither may a request `data` key a template means to read.

No word SHALL be reserved, in the name position or in the reader position. `vars`, `sys`, `datetime` and
`join` are all valid parameter names, because a root is reachable only through a dot and a reader only
through a colon: `{vars}` is the parameter, `{vars.key}` is the store, and `{join}` is the parameter
named `join`, which no `{x:join(...)}` can be confused with by a reader or by the service. `join` is not
reserved as a format name either: a reader written bare is a format name whatever it spells, and only a
parenthesized argument makes one a join. The restriction on parameter names is therefore structural, over
one character class, rather than lexical over an open list of words that grows with every ambient value
the service gains.

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

#### Scenario: A parameter may be named `join`

- **WHEN** a template declares `join: { type: string, default: "n/a" }` and prints `{join}`
- **THEN** the template loads and the label prints `n/a`, because a parameter name and a reader name are
  different positions and neither reserves a word

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
  names an unknown source, and a colon claims a reader that `Internal SKU` is neither a legal format
  name nor a `join('<separator>')` call for

### Requirement: A value a bare token cannot name is bound by mapping, not by spelling

A name a template reads **directly** is reachable only through a bare token, so it SHALL be a legal
bare name **and** SHALL name a parameter the template declares. A request `data` key and a CSV import
header, which becomes one, are read that way: a key that is not a legal bare name, or that the template
does not declare, is read by nothing.

A **connector field key is not read by a template at all**, and this rule does not reach it. It is the
source system's own key, and it reaches a label only through the field mapping, which binds it to a
template field. The mapping's template side SHALL be a legal bare name and SHALL be declared, because
that is the name a token reads. Its connector side is subject to neither rule, because no token ever
reads it: it MAY be spelled in any way the source system spells it.

A source system MAY key its own fields in ways this grammar cannot name — Homebox's per-item custom
fields, keyed `custom:<name>`, are the case that exists. Such a field SHALL remain reachable: the
template declares a legally named parameter, and the operator binds the source key to it through the
field mapping the connector grid already provides, which maps a template field to any connector key.
The service SHALL NOT invent a rewriting of an illegal key into a legal one, because a
rewrite that two different keys could collide into is worse than an explicit mapping.

A convenience that pre-fills a mapping by matching a template field to a connector key of the same name
SHALL simply not match for such a key, because no template field can carry that name.

#### Scenario: A colon-keyed connector field is bound through the mapping

- **WHEN** a connection offers the field key `custom:Internal SKU` and a template declaring
  `internal_sku: { type: string }` prints `{internal_sku}`
- **THEN** the operator maps `internal_sku` to `custom:Internal SKU` in the connector grid, and the label
  prints that field's value

#### Scenario: A template cannot name the connector key directly

- **WHEN** a template prints `{custom:Internal SKU}`
- **THEN** the template fails validation, because `custom` is not an instant and `Internal SKU` is not a
  legal format name

### Requirement: A colon attaches a reader: a format to an instant, or a join to a list

A `:` separates a value from a **reader** applied to it, and carries no other meaning. There are exactly
two readers, and a token carries at most one:

- a **format name**, written bare, an entry of the `datetime_formats` app setting, whose strftime pattern
  is applied to the instant the `value-path` resolves to;
- a **join**, written `join('<separator>')`, which renders the elements of a value the template declares
  as `type: list`, separated by the literal.

**The parenthesized argument is what distinguishes them, not the word.** A reader written bare is a
format name, whatever it spells; a reader written with an argument is a join, and `join` is the only
spelling an argument may follow. No word is reserved in the reader position, so `{sys.now:join}` still
names a `datetime_formats` entry called `join` and resolves exactly as it does today. This matters
because `datetime_formats` is stored operator configuration (`docs/SPEC.md:1054-1058`) whose entry names
are the operator's to choose: a rule that made one of them unreachable would strand stored user data,
which is the one thing this project does not break without a migration.

*This requirement replaces "A colon attaches a format name, and only an instant takes one", which this
change removes. It restates that requirement's complete contract and adds the `join` reader.*

**A format SHALL be attached only where the value is an instant.** Exactly two value paths are instants:

- `sys.now`, and
- a bare token naming a parameter the template declares as `type: datetime`.

A token attaching a format to any other value path SHALL fail validation at load, with a message naming
the token and stating that a format applies to an instant only. This is decidable from the template's own
text, because `params:` is part of the file.

An instant written with no reader SHALL render as ISO `%Y-%m-%d`.

Because `datetime_formats` is runtime state, a format name that the setting does not hold SHALL NOT be an
error at template load. It SHALL be `422 MissingField` when the label renders, naming the field as the
whole token text `<value-path>:<format-name>`.

**A join SHALL be attached only to a bare token naming a parameter the template declares as
`type: list`.** A join on any other value path (an undeclared name, a parameter of any other type,
`sys.now`, or a `vars.<key>`) SHALL fail validation at load, naming the token. An undeclared name is
refused with the rest, because the template says nothing about it and a rule that waited for a request to
find out would make the same template load and fail per caller. The consequence is exact and intended:
**an array is printable only through a parameter declared `type: list`.**

**A parameter declared `type: list` SHALL be read only through a join.** A bare `{tags}` naming one, and
`{tags:<name>}` naming one for any bare reader name, SHALL each fail validation at load, naming the
token: the first because a list is not a value a scalar slot can print, the second because a bare reader
is a format name and a list is not an instant. That second rule is what refuses `{tags:join}` written
with no argument, and its message SHALL say that a list is read through `join('<separator>')` rather than
only that a format applies to an instant, because the token an author meant to write is one character
class away. This is what replaces printing a JSON array as its JSON text.

**An argument may follow only the word `join`.** A bare reader name other than `join` carrying a
parenthesized argument SHALL fail validation at load, naming the token, whatever value path it is
attached to. `{sys.now:long_date(', ')}` and `{sys.now:join(', ')}` are both refused there: the first
because `long_date` takes no argument, the second because a join reads a declared list and `sys.now` is
not one.

**The separator is a single-quoted literal**, because the token's own string is double-quoted in YAML. It
runs from the first `'` after the `(` to the next `'`, and:

- it MAY be empty, which concatenates the elements;
- it MAY contain a `:`, because the token is parsed by structure rather than by counting colons;
- it MAY NOT contain a `'`. There is no escape and no doubling: a further `'` before the `)` SHALL fail
  validation at load, naming the token. Admitting an escape later turns a refusal into an accepted value,
  which is additive; guessing one now would fix a spelling nothing asks for;
- it MAY NOT contain `{` or `}`, and neither is refused by a rule of its own: both are already decided
  before this grammar is reached, because a token ends at the first `}` and is abandoned at a `{`.
  `{{` and `}}` are not escapes inside a token, because no token has yet been recognised where they
  appear.

  **A separator carrying a brace never renders, and the refusal it gets is one of two.** A brace
  re-pairs the braces around it, and which refusal fires follows from that and from nothing else:

  - where the re-pairing still yields a token, that token SHALL fail validation at load, naming it. A
    `}` closes the token early, so `{tags:join('}')}` is refused as the token `{tags:join('}`; a single
    `{` makes the scanner abandon and restart, so `{tags:join('{')}` is refused as the token `{')}`.
  - where it yields no token, the string is left carrying an unmatched brace, and it SHALL be refused by
    the brace-balance rule that already governs the site it is written at: at load, naming the
    parameter, in a `default:`, and at render as `400 InvalidRequest` with `details.reason`
    `interpolation_syntax` in a `text` or `qr` `value:` or an `image` `src:`. `{tags:join('{{')}` is that
    case, because the doubled brace is skipped before any token is looked for.

  A load message here names a token the author did not write, and this capability states that rather than
  implying otherwise: the scanner reports the token it found, and a brace is what moved the boundary.
  Making every one of these a load refusal would mean applying the brace-balance rule to a `text`, `qr`
  or `image src` value at load, which the requirement below explicitly declines to do; changing that is a
  separate decision about three sites this change does not otherwise touch.

The call SHALL be written exactly `join('<separator>')`: no whitespace between `join`, the parentheses and
the quotes, and nothing after the closing `)`. Anything else SHALL fail validation at load, naming the
token.

A token SHALL carry at most one reader, and a format name SHALL NOT be empty. A second colon outside a
`join`'s separator is part of no valid token. `{x:a:b}`, `{x:}` written with a trailing colon and no name, and `{:long_date}` written with
no value path SHALL each fail validation at load, naming the token. `{x:}` in particular SHALL NOT be read
as the bare value `x`: a colon that is written is a reader that is claimed, and a claim with no name is a
mistake worth reporting rather than silently printing an unformatted value.

**What a `join` renders.** The elements of the resolved list SHALL be concatenated in order with the
separator between consecutive elements: no separator before the first or after the last. A list of one
element renders that element, and a list of zero elements renders the empty string. The result is then
ordinary interpolated text and is escaped for the renderer exactly as any other resolved value is. A
`list` parameter that is absent when an active item joins it SHALL be `422 MissingField` naming the
parameter, under `param-resolution`, on the same terms as every other absent parameter.

Because a `default:`'s `value-path` SHALL be dotted, and `join` attaches only to a bare name, no
parameter default can carry a `join`.

#### Scenario: A format renders the system instant

- **WHEN** a template renders `"Printed {sys.now:long_date}"` with the default `long_date` pattern
  `%B %-d, %Y` on 2026-08-23
- **THEN** the label reads `Printed August 23, 2026`

#### Scenario: A format renders a declared datetime parameter

- **WHEN** a template declaring `printed_on: { type: datetime }` renders `"{printed_on:short_date}"`
  with `printed_on` set to `2026-08-19` and the default `short_date` pattern `%m/%d/%Y`
- **THEN** the label reads `08/19/2026`

#### Scenario: An instant with no reader prints an ISO date

- **WHEN** the same template renders `"{printed_on}"` and `"{sys.now}"`
- **THEN** both print their instant as `YYYY-MM-DD`

#### Scenario: A join renders a declared list

- **WHEN** a template declaring `tags: { type: list, default: [CONSUMABLE, KIDS] }` renders
  `"{tags:join(', ')}"` with no `tags` in the request
- **THEN** the label reads `CONSUMABLE, KIDS`

#### Scenario: An empty separator concatenates

- **WHEN** the same template renders `"{tags:join('')}"`
- **THEN** the label reads `CONSUMABLEKIDS`

#### Scenario: A separator may contain a colon

- **WHEN** the same template renders `"{tags:join(' : ')}"`
- **THEN** the label reads `CONSUMABLE : KIDS`, because the token is parsed by structure and the second
  colon is inside the literal

#### Scenario: A one-element and a zero-element list

- **WHEN** requests send `tags: ["ONE"]` and `tags: []` for a template printing `{tags:join(', ')}`
- **THEN** the first prints `ONE` and the second prints nothing, and neither is an error

#### Scenario: A bare token on a declared list is refused when the template loads

- **WHEN** a template declaring `tags: { type: list }` contains `{tags}`
- **THEN** the file fails validation naming the token, and is quarantined, rather than loading and
  printing the value's JSON text

#### Scenario: A format on a declared list is refused

- **WHEN** the same template contains `{tags:long_date}`
- **THEN** the file fails validation with a message naming the token and stating that a format applies to
  an instant only

#### Scenario: A join on a value that is not a declared list is refused

- **WHEN** a template contains `{title:join(', ')}` for a `string` parameter, `{sys.now:join(', ')}`, or
  `{items:join(', ')}` for a name the template does not declare
- **THEN** each fails validation at load naming the token, and the file is quarantined

#### Scenario: A join with no argument is refused

- **WHEN** a template declaring `tags: { type: list }` contains `{tags:join}`
- **THEN** the file fails validation naming the token, in a message saying a list is read through
  `join('<separator>')`, because a bare reader is a format name and a list is not an instant

#### Scenario: A bare `join` on an instant is still a format name

- **WHEN** an operator's `datetime_formats` holds an entry named `join` and a template contains
  `{sys.now:join}`
- **THEN** the template loads and the label prints that entry's pattern applied to the instant, exactly
  as it does today, because no word is reserved in the reader position

#### Scenario: An argument on a bare reader name that is not join is refused

- **WHEN** a template contains `{sys.now:long_date(', ')}`
- **THEN** the file fails validation naming the token

#### Scenario: A join on the system instant is refused

- **WHEN** a template contains `{sys.now:join(', ')}`
- **THEN** the file fails validation naming the token, because a join reads a declared list

#### Scenario: A quote inside the separator is refused

- **WHEN** a template declaring `tags: { type: list }` contains `{tags:join(''')}` or
  `{tags:join('it''s')}`
- **THEN** each fails validation naming the token, because the literal admits no escape and no doubling

#### Scenario: A brace inside the separator is refused when the template loads

- **WHEN** a template declaring `tags: { type: list }` contains `{tags:join('}')}` or
  `{tags:join('{')}` in a `text` item's `value`
- **THEN** the file fails validation naming the token the scanner produced, `{tags:join('}` and `{')}`
  respectively, and the file is quarantined

#### Scenario: A doubled brace inside the separator is refused when the label renders

- **WHEN** the same template instead contains `{tags:join('{{')}`
- **THEN** the template loads, because the doubled brace is skipped and no token is recognised, and
  rendering it returns `400 InvalidRequest` with `details.reason` `interpolation_syntax` for the
  unmatched `{`
- **AND** no label prints the separator or any part of the token text

#### Scenario: A malformed call is refused

- **WHEN** a template contains `{tags:join(', ')x}`, `{tags:join( ', ' )}` or `{tags:join(a)}`
- **THEN** each fails validation at load naming the token

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

#### Scenario: A default cannot carry a join

- **WHEN** a template declares `caption: { type: string, default: "{tags:join(', ')}" }`
- **THEN** the file fails validation naming `caption` and the token, because a default's value path must
  be dotted

### Requirement: An `image` item binds a declared parameter or a bundled asset

*This requirement supersedes the `image` binding bullet of `docs/SPEC.md` §8 ("Data binding") — "`image`
items bind either a bundled asset via `src` or a multipart upload / data URI via `name` (a single data
key resolved against the request `data` map)" — and restates its complete post-change contract. It
supersedes nothing else in §8, and it does not restate the rest of that section: the `{datetime}` and
`{datetime.<name>}` tokens of §8 are already retired by this capability's other requirements, and
copying them back would resurrect them.*

An `image` item takes its bytes from one of two places:

- **`src:`** names a file under the assets root. It is an interpolated string, governed by the token
  grammar this capability defines, and every token in it is subject to that grammar's load-time
  refusals.
- **`name:`** names one request value carrying the image's bytes as a base64 data URI.

**There is no multipart upload, and this requirement drops the phrase in words rather than by
omission.** The superseded bullet reads "a multipart upload / data URI via `name`", and ADR-0055
repeats it, but no endpoint accepts `multipart/form-data` and none ever has: `name:` resolves against
the request `data` map and the value is parsed as a data URI. Naming only the data URI here corrects a
description of something that was never built; no request that works today stops working, and no
working spelling is withdrawn. Frozen §4.1 already describes `name:` as "a data key whose value is a
base64 data URI", so the two frozen sections stop disagreeing.

How many of the two an item may write is `docs/SPEC.md` §4.1's rule, decided when the template loads.
This requirement neither restates nor changes it, and decides only what a `name:` may name; an item
carrying both, or neither, is outside its scope.

`name:` SHALL name a parameter the template declares as `type: string`. It is not merely a legal bare
name. A `name:` that names no declared parameter, or that names a parameter of another type, SHALL fail
validation when the template loads, with a message naming the offending name and, for the second case,
its declared type. The file SHALL be quarantined under the `template-registry` rules while the service
still starts, and the same content arriving through a template write SHALL be refused with
`422 TemplateInvalid` and `details.reason` `template_validation_failed`.

The rule is the one every other reference site already follows — a `when:` key, a `font_weight`, a
`color`, a `stroke.color`, a `background`, a dynamic extent and a bare token in a `value:` each name a
declared parameter or fail at load — so an `image` binds a name out of the template's own vocabulary or
it binds nothing. `string` is the type because the value is a data URI, which is what a `string`
parameter carries and what the `image` control publishes for it (`template-inputs`).

A `name:` that is not a legal bare name SHALL fail validation as it does today, with its own message,
and that check SHALL precede the declaration check, so an illegal name is never reported as an
undeclared one.

A `name:` parameter that resolves to no value when the label renders SHALL be `422 MissingField` naming
the parameter, on the same terms as any other absent parameter, and its declared `default:` applies
first exactly as it does everywhere else (`param-resolution`). This is §4.1's "Missing data key →
`MissingField`" read through the parameter the name now is, and the outcome is unchanged.

#### Scenario: An undeclared image name is refused when the template loads

- **WHEN** a template file carries an `image` item with `name: "logo"` and declares no `logo` in
  `params:`
- **THEN** the file fails validation with a message naming `logo`, the file is quarantined, the service
  still starts and serves every other template, and the same content arriving through a `PUT` is
  refused with `422 TemplateInvalid`

#### Scenario: An image name of the wrong type is refused when the template loads

- **WHEN** a template declares `logo: { type: integer }` and an `image` item carries `name: "logo"`
- **THEN** the file fails validation with a message naming `logo` and its declared type, and the file
  is quarantined

#### Scenario: An illegal image name is still reported as illegal

- **WHEN** an `image` item carries `name: "my logo"`
- **THEN** the file fails validation reporting the name's character class, not an undeclared parameter

#### Scenario: A declared image name binds the request's data URI

- **WHEN** a template declares `logo: { type: string }`, an `image` item carries `name: "logo"`, and
  the request sends `logo` as a PNG data URI
- **THEN** the label renders that image

#### Scenario: A declared image name with no value fails when the label renders

- **WHEN** the same template renders with no `logo` in the request and no `default:` declared for it
- **THEN** the response is `422 MissingField` naming `logo`, and the template itself loaded without
  error
