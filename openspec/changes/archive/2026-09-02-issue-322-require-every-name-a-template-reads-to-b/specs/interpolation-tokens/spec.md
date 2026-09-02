## ADDED Requirements

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

## MODIFIED Requirements

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
their textual form, `null` as the empty string, and any other JSON value via its JSON text.

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
