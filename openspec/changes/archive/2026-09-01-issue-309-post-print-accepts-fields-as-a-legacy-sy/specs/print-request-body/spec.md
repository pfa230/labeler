## Purpose
Defines the JSON body `POST /api/print` accepts: which keys it names, which of them are required,
that no other key is accepted, and how a body it will not accept is rejected. It also binds the
service's own print form to that body, because a second accepted spelling is what let the form drift
onto a name nobody was maintaining.

## ADDED Requirements

### Requirement: The print request carries one parameter map, named `data`

`POST /api/print` SHALL accept exactly these keys, and no others:

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `template` | string | Yes | Template id; `404 TemplateNotFound` if no template has it. |
| `printer` | string | Yes | Printer id; `404 PrinterNotFound` if no printer has it. |
| `data` | object | Yes | The parameter map passed to the template. |
| `copies` | integer 1..100 | No | Number of label instances to print; defaults to `1`. |

`data` SHALL be **required**: it has no default, and a body omitting it SHALL be rejected rather than
treated as an empty map. An explicit `{}` SHALL remain legal and SHALL be passed to the template as
an empty map; what is removed is the default, not the empty map.

**There is no second spelling.** `fields` SHALL NOT be accepted, under any circumstance: not alone,
not alongside `data`, and not as an empty object. The service SHALL NOT read it, SHALL NOT fall back
to it, and SHALL NOT ignore it.

**No unknown key is accepted.** A body carrying any key not in the table above SHALL be rejected, and
the rejection SHALL name the offending key. This is what makes the removal of `fields` observable
rather than silent: a body carrying both `data` and `fields` fails instead of printing while
discarding one of them. The consequence is accepted deliberately — every stray key on this endpoint
now fails, not only `fields`.

**The rejection is the service's existing one.** A body that cannot be deserialized into the type
above is rejected by the request layer before the handler runs, so it SHALL take the outcome the
`request-error-envelope` capability already requires of every JSON endpoint: status `400`,
`error.code` `InvalidRequest`, `error.details.reason` `json_malformed`, and `error.details.error`
carrying the parser's message. No status, code or reason is introduced by this requirement.

Because that rejection happens before the handler, a body that both fails to deserialize and carries
an out-of-range `copies` SHALL report `json_malformed`, not `copies_invalid`. A body that does
deserialize SHALL still be subject to the `copies` range check, which is unchanged.

The published OpenAPI document SHALL describe this body completely: `data` among the required
properties of `PrintRequest`, no `fields` property, and `additionalProperties` set to `false`. The
last is not redundant with the second. An object schema that merely omits `fields` still permits it
as an additional property, so without `additionalProperties: false` the published document would
contradict the endpoint, telling a client generator that a stray key is allowed where the service
returns `400`.

**Supersession.** This requirement supersedes `docs/SPEC.md` §2.3's request field table — the four
rows listing `template`, `printer`, `data` (or `fields`) and `copies`, including the sentence
"`fields` is accepted as a legacy synonym" — and §2.3's `curl` example, which also spells `fields`.
Everything else in §2.3 remains authoritative and is unchanged by this capability: what `copies`
counts, the `BatchSummary` response and its `total` / `succeeded` / `jobs` semantics, the error
contract table, and the trusted-LAN posture. No other frozen section is superseded.

#### Scenario: A body naming `data` prints

- **WHEN** a client posts `{"template":"brother_24mm_qr","printer":"ok-printer","data":{"message":"Hello","code":"QR-1"},"copies":2}` to `POST /api/print`
- **THEN** the response status is `200`
- **AND** the body is a `BatchSummary` reporting `total` 2 and `succeeded` 2

#### Scenario: `copies` still defaults to one

- **WHEN** a client posts a body carrying `template`, `printer` and `data` and no `copies`
- **THEN** the response status is `200`
- **AND** the summary reports `total` 1

#### Scenario: An explicit empty map deserializes

- **WHEN** a client posts `{"template":"nope","printer":"ok-printer","data":{}}`, naming a template
  the registry does not have
- **THEN** the response status is `404`
- **AND** the response is not `400`, because `data: {}` deserialized and the request reached the
  handler

#### Scenario: An explicit empty map is passed to the template

- **WHEN** a client posts `{"template":"brother_24mm_qr","printer":"ok-printer","data":{}}`, naming a
  registered printer and a template whose parameters have no defaults
- **THEN** the response status is `422`
- **AND** `error.code` is `BatchInvalid`
- **AND** the reported failure names a parameter the empty map did not supply
- **AND** the response is neither `400` nor a successful print, because the empty map was carried
  into template processing rather than rejected or replaced

#### Scenario: `fields` in place of `data` is refused

- **WHEN** a client posts `{"template":"brother_24mm_qr","printer":"ok-printer","fields":{"message":"Hello","code":"QR-1"}}`
- **THEN** the response status is `400`
- **AND** `error.code` is `InvalidRequest`
- **AND** `error.details.reason` is `json_malformed`
- **AND** `error.details.error` names `fields`
- **AND** no print job is dispatched

#### Scenario: `fields` alongside `data` is refused rather than dropped

- **WHEN** a client posts a body carrying both `data` and `fields`
- **THEN** the response status is `400`
- **AND** `error.code` is `InvalidRequest`
- **AND** no print job is dispatched

#### Scenario: A body with neither key is refused

- **WHEN** a client posts `{"template":"brother_24mm_qr","printer":"ok-printer","copies":1}`
- **THEN** the response status is `400`
- **AND** `error.code` is `InvalidRequest`
- **AND** `error.details.reason` is `json_malformed`
- **AND** no label is printed from an empty map

#### Scenario: Any other unknown key is refused

- **WHEN** a client posts a body carrying `template`, `printer`, `data` and one key the table does
  not list
- **THEN** the response status is `400`
- **AND** `error.code` is `InvalidRequest`
- **AND** `error.details.error` names that key

#### Scenario: A body that cannot deserialize is refused before `copies` is checked

- **WHEN** a client posts a body that omits `data` and carries `"copies": 0`
- **THEN** the response status is `400`
- **AND** `error.details.reason` is `json_malformed`, not `copies_invalid`

#### Scenario: A deserializable body is still range-checked

- **WHEN** a client posts a body carrying `template`, `printer`, `data` and `"copies": 0`, and again
  with `"copies": 101`
- **THEN** each response status is `400`
- **AND** `error.code` is `InvalidRequest`
- **AND** `error.details.reason` is `copies_invalid`

#### Scenario: The OpenAPI document reports one map and admits no other

- **WHEN** the generated OpenAPI document's `PrintRequest` schema is read
- **THEN** `data` is among its required properties
- **AND** it declares no `fields` property
- **AND** its `additionalProperties` is `false`

### Requirement: The print form posts the body the service accepts

The service's own print screen SHALL post the body defined above, and its API client SHALL declare
exactly the keys that body has.

The client function that posts `POST /api/print` SHALL type its body as `template`, `printer`, `data`
and `copies`, with `data` required. It SHALL NOT declare `fields`, and it SHALL NOT declare any other
key the service does not accept: under the rule above such a key is a `400`, so a client type
advertising one is a defect the type system would otherwise endorse.

No source under `ui/src/` SHALL send a `fields` key to `POST /api/print`.

The print screen SHALL send the operator's entered values as `data`. When the operator has entered
nothing — a template needing no input — it SHALL send `data: {}` rather than omitting the key.

Routing is unchanged: a sheet template still prints through `POST /api/batch`, which this capability
does not touch.

#### Scenario: A tape print posts `data`

- **WHEN** an operator picks a printer for a `single` template, fills its inputs and presses Print
- **THEN** exactly one request is sent to `POST /api/print`
- **AND** its body's `data` carries the entered values
- **AND** its body has no `fields` key

#### Scenario: A template needing no input posts an empty map

- **WHEN** an operator picks a printer for a `single` template that reports no inputs, so nothing was
  entered, and presses Print
- **THEN** exactly one request is sent to `POST /api/print`
- **AND** its body carries `data` as an empty object
- **AND** its body does not omit `data`
- **AND** its body has no `fields` key

#### Scenario: The client type admits no key the service refuses

- **WHEN** the print client function's body type is read
- **THEN** its keys are exactly `template`, `printer`, `data` and `copies`
- **AND** `data` is not optional

#### Scenario: A sheet template still prints through the batch endpoint

- **WHEN** an operator presses Print for a `sheet` template
- **THEN** no request is sent to `POST /api/print`
- **AND** the request goes to `POST /api/batch` with `mode` `print`
