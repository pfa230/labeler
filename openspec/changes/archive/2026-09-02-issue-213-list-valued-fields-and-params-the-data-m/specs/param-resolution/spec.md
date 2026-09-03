## MODIFIED Requirements

### Requirement: A declared default is resolved against one request-scoped snapshot

A `default:` that is a string SHALL be interpolated by the `interpolation-tokens` grammar before it is
used, restricted there to namespaced tokens. A `default:` that is not a string carries no token and
SHALL be used as written.

"Used as written" governs interpolation and nothing else: it says a non-string default reaches
resolution unaltered, not that any non-string shape is admissible. A **sequence** `default:` is
admissible on a `list` parameter and on no other type, and SHALL be refused at load elsewhere, naming
the parameter. That refusal belongs to this requirement rather than to `list-params` because it is a
statement about defaults in general, and it follows from the sentence below that a default may not carry
a value the request could not have carried: a request supplying a JSON array for a `string`, `enum`,
`boolean`, `integer`, `number` or `datetime` parameter is refused, so a declaration supplying a sequence
for one must be too. Without it the model would hold that default as the debug text of the sequence it
could not represent, and a label would print `Sequence [String("a")]`.

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
reject an `enum` default outside `values` and a default that overflows the frame it sizes. Two checks
join that set with the `list` type, and both are decidable from the declaration alone: a sequence
`default:` on any type but `list`, above, and an element of a `list` default that is not a YAML string
scalar (`list-params`). A `list` default is never a string, so it can carry no interpolation syntax and
this paragraph's exemption can never apply to one. A default
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

#### Scenario: A sequence default on a type that is not a list is refused

- **WHEN** a template declares `title: { type: string, default: [A, B] }`
- **THEN** the template fails validation naming `title`, and the file is quarantined, rather than
  loading with a default holding the sequence's debug text

#### Scenario: A sequence default on a list is used as written

- **WHEN** a template declares `tags: { type: list, default: [CONSUMABLE, KIDS] }` and a request omits
  `tags`
- **THEN** the default resolves to `["CONSUMABLE", "KIDS"]` with no interpolation applied to any element,
  and `{tags:join(', ')}` prints `CONSUMABLE, KIDS`
