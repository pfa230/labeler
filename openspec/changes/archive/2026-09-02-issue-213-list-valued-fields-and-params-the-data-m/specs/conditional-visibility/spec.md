## ADDED Requirements

### Requirement: A `when:` map holds conditions on declared parameters, and a list is not one

*This requirement supersedes the "Evaluation against resolved parameters" bullet of `docs/SPEC.md` §5
("Conditional visibility (`when:`)"), and only that bullet, and restates its complete post-change
contract. It supersedes no other part of §5. The "Lazy missing-field evaluation" and "Enum parameter
validation" bullets are not this requirement's subject and remain authoritative as frozen: the second in
particular governs the response to a **request** supplying an `enum` value outside its `values`, which no
OpenSpec requirement has migrated and which this change does not touch. The rules below on what a `when:`
map may contain are not a supersession of anything, because §5 never stated them and no requirement in
`openspec/specs/` has; they are stated here for the first time, unchanged from what the service already
enforces.*

**Evaluation.** All of an item's conditions SHALL match the label's resolved parameter values for that
item to be **active**. If any condition is false the item SHALL be excluded from both the measurement
pre-pass and rendering, and for a `container` so SHALL everything nested inside it. A condition compares
the resolved value's string rendering against the literal the map holds. An absent parameter makes the
condition false rather than raising, under `param-resolution`, which owns that rule.

**What a `when:` map may contain.** A `when:` map that is **present** SHALL carry at least one
condition, so `when: {}` SHALL be refused at load. A condition's key or value that is empty or
whitespace-only SHALL be refused at load.

A `when:` key written with an explicit YAML **null** carries no map at all, and SHALL be treated as an
absent predicate exactly as omitting the key is: the item is unconditional and draws. That is what the
service does today and this requirement does not change it. It differs from how a `params:` entry treats
an attribute written and left empty, which `list-params` and `datetime-params` refuse, and the
difference is in what the key holds rather than in a rule about nulls: a `when:` key holds a container,
so its null is no container and there is nothing to gate on, while a parameter attribute holds a value,
so its null is a value an author wrote and left out. Unifying the two is not this change's to do.

A condition's value SHALL be a YAML string, boolean, integer or float, and SHALL be held as that
scalar's textual form, so `when: { bold: true }` and `when: { bold: "true" }` are the same condition. A
value that is a null, a sequence or a mapping SHALL fail to parse, and the template SHALL be refused on
the terms any unparseable template is.

**Every `when:` key SHALL name a parameter the template declares under `params:`.** A key naming
anything else SHALL be refused when the template loads, in a message naming the key, and the file SHALL
be quarantined under the `template-registry` rules while the service still starts. This is the rule the
service already applies and this requirement restates it unchanged rather than relaxing it, its message
included: a gate is a branch of the template, so the template must declare what it branches on, and a
`when:` over a bare `data` key would be a gate whose name nothing validates and whose control no input
list could report. Whether the same rule should hold for every name a template reads rather than for a
gate's alone is #322's question, which argues to extend this rule; nothing here relaxes it.

**A condition on an `enum` parameter SHALL name one of that parameter's declared `values`**, and SHALL
be refused at load naming the parameter and the value otherwise. This is a check on the **literal the
template wrote**, decidable from the file, and it is a different rule from frozen §5's response to a
**request** value outside the same set, which it neither restates nor supersedes.

**A `when:` key naming a parameter declared `type: list` SHALL be refused at load**, naming the key
**and the offending item's layout path**. It is a declared parameter, so the rule above admits it, and
this one refuses it.

The layout path is the one way this refusal differs from the one above, and the difference is
deliberate rather than incidental. No message raised by this stage of validation carries a layout path
today: they report a parameter name and leave the reader to find which of a template's items named it.
This requirement asks for the path on the refusal it introduces, because a gate over a list is a
mistake an author makes in one item of many and the name alone does not say which. It asks for nothing
about the messages that exist today, and changing those is a change to diagnostics this capability does
not govern.

`when` gains **no** `contains` operator, and no other operator over a list: no use case for one has been
named. Refusing the gate rather than letting it quietly never match is what keeps that door open at no
cost. A refusal is additive to relax later, so if `contains` ever earns itself, a load error becomes a
match. Never-matching would paint the corner: relaxing it then would silently flip live templates from
hidden to shown, a behaviour change nobody edited a template to get.

Because every key names a declared parameter, **a request `data` value can reach a condition only
through the parameter that declares it**, and therefore only after that parameter's coercion has
accepted it. An array supplied for a declared `list` cannot be compared, because no template carrying
such a gate loads; an array supplied for a parameter of any other type is refused during coercion before
any condition is evaluated (`list-params`). There is consequently no state in which a condition compares
against an array, and this requirement defines no behaviour for one.

**Every refusal above is a property of the template, so it holds wherever a `when:` is read**: a file
refused at load is refused for the render path, for `POST /api/templates/{id}/inputs`, for
`GET /api/templates/{id}` and for the thumbnail alike, because none of them is served a template the
registry quarantined.

This requirement makes **no** claim about branch parity between those paths, and SHALL NOT be read as
one. Which branch each path selects turns on the resolved *value*, not on the template, and
`template-inputs` owns that and deliberately limits it: the input-list path resolves leniently, absorbing
a value it cannot coerce and evaluating a gate naming it as absent where a render rejects that value
outright, and it claims parity only for defaults that resolve. Nothing here narrows or widens that.

#### Scenario: A gate on a declared list is refused when the template loads

- **WHEN** a template declares `tags: { type: list }` and a container carries `when: { tags: KIDS }`
- **THEN** the file fails validation with a message naming `tags` and that container's layout path, the
  file is quarantined, and the server still starts and serves every other template

#### Scenario: The same template arriving through a write is refused

- **WHEN** `PUT /api/templates/{id}` receives that YAML body
- **THEN** the response is `422 TemplateInvalid`, and an existing template at that id is left
  byte-for-byte unchanged

#### Scenario: A gate on a name the template does not declare is still refused

- **WHEN** a container carries `when: { tags: "KIDS" }` and the template declares no `tags` parameter
- **THEN** the file fails validation naming `tags`, in the message it produces today and with no layout
  path added to it, whatever a request would later send for `tags`

#### Scenario: A gate on an enum value outside its set is refused

- **WHEN** a template declares `size: { type: enum, values: [small, large] }` and a container carries
  `when: { size: medium }`
- **THEN** the file fails validation naming `size` and `medium`

#### Scenario: An empty map and a blank condition are refused

- **WHEN** a container carries `when: {}`, or `when: { mode: "" }`, or `when: { "  ": full }`
- **THEN** each fails validation at load, exactly as it does today, and the file is quarantined

#### Scenario: A `when:` key written and left empty is not a predicate

- **WHEN** a container carries `when:` written with no value, so it parses as an explicit YAML null
- **THEN** the template loads and that container is drawn unconditionally, exactly as one carrying no
  `when:` key at all is, and not refused as an empty map

#### Scenario: A non-string scalar condition is read as its text

- **WHEN** a template declares `bold: { type: boolean, default: true }` and a container carries
  `when: { bold: true }`
- **THEN** the container is drawn for a label resolving `bold` to true, identically to a container
  carrying `when: { bold: "true" }`

#### Scenario: An array cannot reach a condition

- **WHEN** a request sends `data: { "mode": ["a"] }` for a template declaring `mode` as a `string`
  parameter that a container gates on
- **THEN** the response is `400 InvalidRequest` with `details.reason` `request_body_invalid` naming
  `mode`, decided while parameters are resolved and before any condition is evaluated

#### Scenario: A refused template is refused on every path that reads it

- **WHEN** a template carrying `when: { tags: KIDS }` over a declared `list` is placed in the templates
  directory
- **THEN** it is quarantined, and `POST /api/templates/{id}/inputs`, `GET /api/templates/{id}`, the
  thumbnail and every render path alike report it as they report any quarantined template, rather than
  one path serving it and another refusing it
