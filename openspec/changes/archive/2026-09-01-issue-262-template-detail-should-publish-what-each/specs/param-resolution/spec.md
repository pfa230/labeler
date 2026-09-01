## MODIFIED Requirements

### Requirement: A declared default is resolved against one request-scoped snapshot

A `default:` that is a string SHALL be interpolated by the `interpolation-tokens` grammar before it is
used, restricted there to namespaced tokens. A `default:` that is not a string carries no token and
SHALL be used as written.

A request SHALL capture one instant and read the variables store once, and every default it resolves
SHALL be resolved against that one snapshot. Resolution itself happens per label, so a batch, sheet or
ZIP resolves a given default once per label and SHALL get the identical value every time. The
observable rule is that no two labels in one request may see a given default resolve differently.

A default's resolved value SHALL be validated and coerced by exactly the rule a value the caller supplied
for that parameter is validated and coerced by. A default is not privileged: it may not carry a value the
request could not have carried. What the *rule* shares is what counts as invalid; what it does not share
is who is told. A value the caller sent that fails is the caller's error and keeps its existing response,
and a resolved default that fails the same rule is the template's, reported as
`422 TemplateInvalid` with `details.reason` `param_default_unresolvable` under the requirement below.

**This is a breaking change for a literal default carrying a value a request would be rejected for** — a
`boolean` declaring `default: "yes"` loads and renders today, because a default was inserted without
validation, and SHALL be rejected from this change onward on the terms the request value `"yes"` is
rejected.

Coercion applies to the value as the model holds it, and this capability does not reshape that value
first. One consequence is worth stating because it looks like an inconsistency and is not worth machinery
to remove: a template declaring `default: 1` on a `boolean` is held as a float and is rejected, while a
request sending `bold: 1` is accepted, because the conversion that reads a template collapses a YAML
integer to a float for every non-`integer` type. An author writes `default: true`. Making the two agree
means preserving the authored scalar's kind through that conversion, which is a defect of its own, tracked as #270.

Where validation is *lenient* — the input-list path, which absorbs a value it cannot coerce rather than
rejecting it — a declared default that fails validation SHALL be absorbed the same way for gate
evaluation: the parameter is absent, and a gate naming it is false. What the list *publishes* for it is
not absorbed but reported, under `template-inputs`.


A default carrying **no interpolation syntax at all** SHALL keep the load-time checks it has today, which
reject an `enum` default outside `values` and a default that overflows the frame it sizes. A default
carrying any — a token, or an escape — SHALL NOT have its *value* checked at load.

The test is syntax and not tokens, because an escape changes the value without being one: `{{draft}}`
carries no token yet resolves to `{draft}`, so an `enum` declaring `values: ["{draft}"]` would be refused
at load for a default that resolves to an allowed value, and one declaring `values: ["{{draft}}"]` would
pass load and resolve outside its own set. Both directions are wrong, and both disappear if the check
skips anything a brace could change.
Load-time validation SHALL therefore treat a parameter whose default carries a token exactly as it
treats a parameter with no default at all, and the checks that default would have faced SHALL be
applied to its resolved value instead.

A client cannot resolve a default, because the tokens in one read the variables store and the request's
instant, and it cannot safely pass one through either: seeding `{vars.base}` into a control submits a
data value that prints verbatim, since interpolation is substitution-only; seeding `{{draft}}` submits
text the server would have unescaped to `{draft}`; and seeding `price }} net` submits text that never
reaches the `interpolation_syntax` check an unmatched brace is meant to fail.

The service SHALL therefore resolve it rather than each client, and SHALL publish the **resolved**
default: every path that publishes a default to a client — the input list on either endpoint, and the
template detail's report — SHALL publish the value this requirement's resolution produces, after
coercion, and SHALL publish none where that resolution fails (`template-inputs`). A client SHALL seed a
control only from what those paths publish, and SHALL NOT read a default out of the raw parameter
declaration to seed with. `{vars.base}` therefore reaches a control as the store's value, `{{draft}}` as
`{draft}`, and neither reaches it as text the server would have to unescape a second time.

Publishing it is a read of the same two sources a render reads, taken on paths that took neither before.
Those paths pay one variables read and one settings read per request for it, and a failure of either is
a failure of the request rather than an unresolvable default.

A client SHALL NOT supply the first entry of an `enum`'s `values` for a parameter that declares no
`default:`, in a form control, in a grid column, or in any reconciliation of a row against a template.
That is the same inference this capability removes from the service, and moving it into a client does
not make it a declaration.

#### Scenario: A literal default is checked when the template loads

- **WHEN** a template declares `size: { type: enum, values: [small, large], default: medium }`
- **THEN** the template fails validation naming `size` and `medium`, and the file is quarantined

#### Scenario: A tokened default is checked when it resolves

- **WHEN** a template declares `size: { type: enum, values: [small, large], default: "{vars.size}" }`
  and the store holds `size = medium`
- **THEN** the template loads without error, and a request omitting `size` fails when it renders

#### Scenario: A literal default a request could not have sent is rejected

- **WHEN** a template declares `bold: { type: boolean, default: "yes" }`, an active item reads `{bold}`,
  and a request omits `bold`
- **THEN** the response is `422 TemplateInvalid` with `details.reason` `param_default_unresolvable`,
  naming `bold` and `yes`
- **AND** it is judged invalid by the same coercion rule that rejects a request sending `bold: "yes"`,
  but is not reported as that request's error

#### Scenario: A client preview supplies a legal enum value

- **WHEN** a client renders its live preview of a template printing `{size}` where `size` declares
  `values: [small, large]` and no `default:`
- **THEN** the request it posts carries `size: small`, and the preview renders rather than being rejected
  for a value outside the parameter's `values`

#### Scenario: A grid does not select an undefaulted enum for the operator

- **WHEN** the CSV import grid loads a template declaring `size: { type: enum, values: [small, large] }`
  with no `default:`
- **THEN** no row is pre-set to `small`, and a row left unset is reported as needing a value rather than
  submitted as `small`

#### Scenario: A grid selection reaches the request

- **WHEN** an operator selects `large` for a row in that grid and submits
- **THEN** that row's label carries `size: large` where the service reads it, rather than in a sibling
  object no request model accepts

#### Scenario: A tokened default is seeded from its resolved value

- **WHEN** the print form loads a template declaring `url: { type: string, default: "{vars.base}" }` and
  the store holds `base = https://example.test`
- **THEN** the control holds `https://example.test` rather than the text `{vars.base}`, the form does not
  demand a value for `url`, and submitting it unchanged omits `url` so the service resolves the default

#### Scenario: A tokened default that cannot resolve seeds nothing and is required

- **WHEN** the same template loads with no `base` in the store
- **THEN** the control is empty, the form demands a value for `url`, and the reason it is empty is
  surfaced against it

#### Scenario: An escaped brace is seeded as the value it resolves to

- **WHEN** the print form loads a template declaring `label: { type: string, default: "{{draft}}" }`
- **THEN** the control holds `{draft}`, and submitting it unchanged omits `label`, so the label prints
  `{draft}` rather than the four-brace text a control seeded from the declared text would have submitted

#### Scenario: A plain default is seeded

- **WHEN** the print form loads a template declaring `title: { type: string, default: "Untitled" }`
- **THEN** the control holds `Untitled`

#### Scenario: Every label in one batch sees one resolved default

- **WHEN** a batch of labels omits a parameter declaring `default: "{sys.now}"` and the run crosses
  midnight
- **THEN** every label resolves it to the same instant, and no two labels print different dates

#### Scenario: A variable edited mid-request does not split a batch

- **WHEN** the variables store changes while a batch is rendering, and its labels omit a parameter
  declaring `default: "{vars.base}"`
- **THEN** every label resolves the value the store held when the request began

### Requirement: A default that cannot be resolved is the template's fault, not the caller's

Resolving a default fails in exactly two ways: a token in it names a value that is absent, or the
resolved value is one the parameter's declaration forbids. Both SHALL be reported as
`422 TemplateInvalid` with `details.reason` `param_default_unresolvable`, and the message SHALL name the
parameter, the token that failed where one did, and the resolved value where there is one.

One failure, one payload. It SHALL carry four facts: the parameter it is about, `reason`
(`param_default_unresolvable`), `message`, and then `token` where a token could not be resolved and
`value` where a resolved value the declaration forbids exists. A failure carries at most one of `token`
and `value`, and absent facts are absent keys. The message stays what it is today and stays the
human-readable form; the other facts are carried structurally because a read-only report of this failure
publishes them as fields (`template-inputs`), and a report that had to parse the message would be a
second, weaker statement of the same failure.

That one payload has two serializations, and they put the same facts in different structural places. The
wire shapes SHALL be exactly these.

**On a render path's `422`**, in the error envelope, where `message` and `details` are siblings under
`error`:

```
{ "error": {
    "code": "TemplateInvalid",
    "message": "<the human-readable message>",
    "details": { "reason": "param_default_unresolvable",
                 "param": "<parameter name>",
                 "token": "<token>",     // present only when a token failed
                 "value": "<value>" }    // present only when a resolved value was forbidden
} }
```

The message lives in `error.message` and SHALL NOT be duplicated inside `details`. `details` names the
parameter under `param`, because nothing else in the envelope does.

**On a read-only report**, as the value of a `param_defaults` key or as an entry's `default_error`:

```
{ "reason": "param_default_unresolvable",
  "message": "<the human-readable message>",
  "token": "<token>",     // present only when a token failed
  "value": "<value>" }    // present only when a resolved value was forbidden
```

Here `message` is a field of the object, because there is no envelope to carry it, and there is no
`param` field, because the key this object hangs under already names the parameter; a `param` field there
would restate its own key and could be made to contradict it.

The two are projections of one payload, not two payloads. For a given failure the `reason`, `message`,
`token` and `value` strings SHALL be identical between them; only the parameter's location and the
message's location differ.

**A read-only endpoint reports this failure rather than raising it.** `GET /api/templates/{id}` and
`POST /api/templates/{id}/inputs` resolve declared defaults without rendering anything, and a failure
there SHALL be `200` carrying the read-only projection in the response body — as the entry's `error` in
`param_defaults`, and as the entry's `default_error` in an input list — not a `422`. Nothing is being
printed, the template is not quarantined, and it still renders for any caller who supplies the
parameter. `422 TemplateInvalid` stays the answer on the paths that render: a single label, a batch, a
sheet and a print job.

These are the two ways *resolution* fails at render. A default's brace syntax fails earlier and is not
one of them: `interpolation-tokens` requires an unterminated `{` or an unmatched `}` in a `default:` to be
refused when the template loads, naming the parameter. That is not a pre-existing failure — a default is
not interpolated today, so `default: "50% {off"` loads and prints verbatim — and it is new here, which is
why it is decided at load rather than left to surface at render as the caller's `400 InvalidRequest`. Text
only a template author wrote must not be reported against a request that supplied nothing, which is the
same argument this requirement makes about `MissingField`.

It SHALL NOT be reported as `422 MissingField`. `MissingField` tells a caller which field to add, and a
caller who omitted a parameter that has a default has nothing to add: the fault is in the template, and
naming the caller's request for it sends the wrong person to look. Interpolation raises `MissingField`
for an absent variable and for an unknown format name; inside a default, that error SHALL be remapped to
this one rather than surfacing as the caller's.

*This requirement supersedes the `TemplateInvalid` row of the error-code table in `docs/SPEC.md` §10
(`docs/SPEC.md:686`), which reads "Template fails structural validation (e.g. a dynamic `format.width`
missing one bound)", and restates that code's complete post-change contract. It supersedes no other row
of that table and no other part of §10, and it adds one row to the reason registry in §10.1 while
changing none of the rows already there. The `request-error-envelope` capability supersedes the same
table for the addition of `Internal` (500), and this change narrows its "every other row remains
authoritative" sentence so the published set says so outright rather than relying on a reading. The two
supersessions are disjoint.*

`TemplateInvalid` (422) SHALL mean that the **template**, rather than the request, is at fault. Its
complete set of reasons after this change is:

| Reason | When |
| --- | --- |
| `template_parse_failed` | The YAML did not parse. |
| `template_validation_failed` | The template parsed but failed structural validation. |
| `template_duplicate_id` | Two templates on disk declare the same id. |
| `template_group_invalid` | A template's group is not a legal group. |
| `template_group_case_conflict` | A group differs from an existing one only by case. |
| `template_group_unsafe_path` | A group resolves outside the templates directory. |
| `param_default_unresolvable` | A declared default cannot be resolved for this request. |

Only the last row is new; the other six are the code's existing reasons, restated unchanged so that this
requirement is a complete contract rather than a redefinition that silently drops what it omits. Six of
the seven are decided without a request. The seventh is not: it is request-time and depends on the
variables store, so `TemplateInvalid` is no longer raised only while validating a template. That is
deliberate, and it is what distinguishes this code from `MissingField` — the template is what must
change to fix it in all seven cases, and that is what the code tells a caller. No `code` string changes.

In a batch the failure SHALL be reported per label, through the same machinery every other per-label
failure uses: every label that reaches the unresolvable default SHALL appear in the `details.failures`
list of the `422 BatchInvalid` response carrying the `TemplateInvalid` code and the
`param_default_unresolvable` reason. Because the failure depends on the template and the request's
snapshot and on no label's data, every label that omits the parameter SHALL fail identically, and the
batch stays all-or-nothing: no PDF, no ZIP and no print job SHALL be produced.

#### Scenario: A default naming an absent variable

- **WHEN** a template declares `url: { type: string, default: "{vars.base}" }`, the store holds no
  `base`, and a request omits `url`
- **THEN** the response is `422 TemplateInvalid` with `details.reason` `param_default_unresolvable`,
  `details.param` `url` and `details.token` `vars.base`, its message names both, and it is not a
  `MissingField` naming `vars.base`

#### Scenario: A default resolving outside an enum's values

- **WHEN** a template declares `size: { type: enum, values: [small, large], default: "{vars.size}" }`,
  the store holds `size = medium`, and a request omits `size`
- **THEN** the response is `422 TemplateInvalid` with `details.reason` `param_default_unresolvable`,
  `details.param` `size` and `details.value` `medium`, and its message names both

#### Scenario: The same failure is a 200 on a read-only endpoint

- **WHEN** `GET /api/templates/{id}` is read for that template
- **THEN** the response is `200`, and `param_defaults.size` carries an `error` with the same `reason`,
  `message` and `value` strings the render path reports
- **AND** that `error` carries no `param` key, because the `param_defaults` key it hangs under is the
  parameter's name

#### Scenario: A datetime default resolving to text the parser rejects

- **WHEN** a template declares `printed_on: { type: datetime, default: "{sys.now:long_date}" }` and a
  request omits `printed_on`
- **THEN** the response is `422 TemplateInvalid` with `details.reason` `param_default_unresolvable`,
  naming `printed_on`

#### Scenario: Structural validation keeps its own reason

- **WHEN** a template is submitted whose dynamic `format.width` is missing one bound
- **THEN** the response is `422 TemplateInvalid` with `details.reason` `template_validation_failed`,
  exactly as before

#### Scenario: A batch names every label that reached the broken default

- **WHEN** a batch of three labels all omit a parameter whose default cannot be resolved
- **THEN** the response is `422 BatchInvalid`, no artifact is produced, and `details.failures` carries
  one entry per label, each with the `TemplateInvalid` code and the `param_default_unresolvable` reason

#### Scenario: A caller who supplies the value is unaffected

- **WHEN** the same template with an unresolvable default receives a request that supplies the
  parameter
- **THEN** the label renders, because the default is never reached

