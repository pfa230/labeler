# repetition Specification

## Purpose
Defines `repeat:`, the key that renders one authored container once per element of a list parameter, so
that the number of things on a label comes from the data rather than from a ceiling its author wrote.
It covers where the key may be written, what it may name, how the instances are produced and ordered,
and what the repeated name means inside the subtree it creates.

## Requirements

### Requirement: `repeat:` names a declared list parameter, on a container packed by a flow parent

*This requirement adds the `repeat` key to the `container` field list of the frozen `docs/SPEC.md` §4.1
("Item types"), and supersedes that bullet to the extent of that list, exactly as `flow-layout` did for
`flow`. It states the complete contract of the key it adds: what a `container` may write there, what
that value may name, where such a container may sit, and what each refusal reports. Every other field
§4.1's `container` bullet names, every other bullet in §4.1, and every other statement in it remain
authoritative and are not restated here.*

A `container` MAY carry an optional `repeat:` key. Its value SHALL be the **bare name** of a parameter
the template declares as `type: list`, written as it is written in `params:`. It is a name and not a
token: `repeat: tags` is the spelling, and no `{` or `}` appears in it.

Six refusals decide, from the template's own text, whether a `repeat:` may be written where it is:

1. **`repeat:` is a key on `container` and on no other item type.** A `text`, `qr`, `image` or `line`
   carrying it SHALL be refused at load as an unknown field, naming that item's layout path, exactly as
   any other key those items do not declare is refused. A repeating text is written as a container
   holding one text. One rule and one place to look, at the cost of a wrapper on the plainest case.
2. **A `repeat:` written and left empty**, so that it parses as an explicit YAML null, SHALL be refused
   at load, naming the key and the container's layout path. A `repeat:` holds a value the author wrote,
   as a parameter attribute does and unlike a `when:`, which holds a container of conditions and whose
   null is therefore no predicate at all (`conditional-visibility`). A key read and dropped is a
   declaration that silently did nothing.
3. **A `repeat:` naming a parameter the template does not declare** SHALL be refused at load, naming
   the key, the name and the container's layout path. This is the rule every name a template reads
   already lives under (`interpolation-tokens`).
4. **A `repeat:` naming a declared parameter of any type but `list`** SHALL be refused at load, naming
   the key, the parameter, its declared type and the container's layout path. Only a list has elements.
5. **A repeating container whose parent does not arrange by flow** SHALL be refused at load, naming the
   key and the container's layout path. The layout root arranges absolutely, so a `repeat:` on a
   root-level item is refused by this rule. N copies at one coordinate is overprinting and not an
   arrangement; requiring a flow parent is what makes every question about where the instances go one
   `flow-layout` has already answered.
6. **A `repeat:` naming a parameter that an enclosing `repeat:` already repeats** SHALL be refused at
   load, naming the parameter and the inner container's layout path. It would rebind an element to an
   element. A nested `repeat:` over a **different** declared list parameter SHALL be accepted, because
   nothing about it is ambiguous, and the two bindings compose over the subtree they nest in.

A repeating container is a **packed child**, so `flow-layout`'s refusal of `at` and `to` on one already
applies to it and is not restated here. So does its extent rule, and the consequence is worth stating
because a repeat is the first thing that reaches it from the data: a packed container giving neither
`size` nor `to` resolves to `[fill, fill]` and takes its parent's whole padded inner box, so a repeating
container written without an extent draws for a one-element list and fails with `item_out_of_frame` on
the second instance of any longer one. That is `layout-sizing` and `flow-layout` unchanged. A pill that
hugs its own element says `size: [content, content]`, and one that takes a fixed slot says a number.

Every refusal above is a template-content fault. The file SHALL be quarantined under the
`template-registry` rules while the service still starts and still serves every other template, and the
same content arriving through a template write SHALL be refused with `422 TemplateInvalid` and
`details.reason` **`template_parse_failed`**, on the terms `list-params` sets for its own refusals: each
is decided from the file's own shape, or from `params:` once that map exists, before any validation pass
over the loaded template is reached.

**Every refusal a `repeat:` brings into existence is reported that way**, which is the six above and the
two the scope requirement below defines, a join and a bare reader on the repeated name inside the scope.
All eight are decided from the file's text before validation, all eight quarantine the file, and all
eight report `template_parse_failed` on a write, so an author never has to know which of them a template
tripped in order to know what the response will say.

One refusal a repeat can trip is **not** one it brings into existence, and it is unchanged: a `when:`
key naming a parameter declared `type: list`, which `conditional-visibility` refuses today and goes on
refusing on the repeating container itself and on every item outside a repeat scope. This change does
not move it, does not restate what it reports, and #319 says of it in as many words that it "stands
there unchanged".

`GET /api/templates/{id}` SHALL return the container as it was authored, carrying its `repeat:` key,
one item and not one per element, so a template read back and resubmitted unchanged is accepted.

#### Scenario: A repeating pill in a row loads

- **WHEN** a template declares `tags: { type: list }` and a `row` flow container with an authored size
  holds one `container` carrying `repeat: tags`, `size: [content, content]`, `padding: 0.8` and a
  `text` reading `{tags}` with `size: [content, content]`
- **THEN** the template loads

#### Scenario: A repeating container without an extent fills its strip

- **WHEN** that same pill omits `size` and `to`, so it resolves to `[fill, fill]`, and a request sends
  `tags: ["A"]`
- **THEN** the label renders with one pill filling the strip
- **AND** a request sending `tags: ["A", "B"]` fails with `UnsupportedLayoutItem` and `details.reason`
  of `item_out_of_frame` naming the second instance, because two fill children cannot share a box

#### Scenario: `repeat:` on a text item is refused

- **WHEN** a `text` item inside a flow container carries `repeat: tags`
- **THEN** the file fails validation with an unknown-field error naming that item's layout path, the
  file is quarantined, and the service still starts

#### Scenario: A `repeat:` written and left empty is refused

- **WHEN** a packed container carries `repeat:` written with no value, so it parses as an explicit YAML
  null
- **THEN** the file fails validation naming `repeat` and that container's layout path, rather than
  loading as a container that repeats nothing

#### Scenario: An undeclared name and a wrongly typed one are both refused

- **WHEN** a packed container carries `repeat: items` and the template declares no `items`, or carries
  `repeat: title` for a parameter declared `type: string`
- **THEN** each fails validation naming the key, the name and that container's layout path, and the
  file is quarantined

#### Scenario: A repeat outside a flow container is refused

- **WHEN** a `container` carrying `repeat: tags` is a direct child of the layout root, or of a
  `container` carrying no `flow` block
- **THEN** each fails validation naming `repeat` and that container's layout path

#### Scenario: A nested repeat over the same parameter is refused

- **WHEN** a container repeating `tags` holds a flow container whose own child carries `repeat: tags`
- **THEN** the file fails validation naming `tags` and the inner container's layout path

#### Scenario: A nested repeat over a different list is accepted

- **WHEN** a template declares `tags` and `codes` as `list` parameters and a container repeating `tags`
  holds a flow container whose child carries `repeat: codes`
- **THEN** the template loads, and each `tags` instance holds one instance per `codes` element

#### Scenario: Every refusal a repeat brings into existence reports the same reason

- **WHEN** `PUT /api/templates/{id}` receives a body carrying, in turn, each of the six refusals this
  requirement defines
  and each of the two the scope requirement defines, `{tags:join(', ')}` and `{tags:long_date}` inside a
  repeating container
- **THEN** all eight responses are `422` with `error.code` `TemplateInvalid` and `error.details.reason`
  `template_parse_failed`
- **AND** an existing template at that id is left byte-for-byte unchanged, and no file is created when
  the write was create-only

#### Scenario: The unchanged gate refusal is not moved

- **WHEN** `PUT /api/templates/{id}` receives a body whose repeating container carries
  `when: { tags: KIDS }` over the declared list
- **THEN** the response is `422 TemplateInvalid`, reporting exactly what `conditional-visibility`'s
  refusal of a `when:` key over a declared list reports for a template carrying no `repeat:` at all

#### Scenario: A repeating container round-trips

- **WHEN** a template carrying a repeating container is read through `GET /api/templates/{id}`
- **THEN** the response holds that container once, carrying `repeat:` and neither `at` nor `to`
- **AND** submitting the returned document unchanged is accepted

### Requirement: A repeat produces one instance of its container per element, in element order

For one label, an **active** repeating container SHALL produce one **instance** of itself per element of
the list its `repeat:` names, in the order the elements are held, each taking the place its parent's
child order gave the authored container. A child written before the repeating container is packed
before every instance, and one written after it is packed after every instance.

An instance SHALL be an ordinary packed child of its parent in every respect, and this requirement adds
no arrangement rule of its own. Order, `gap`, `wrap`, `line_gap`, the secondary-axis alignment, the
sizing of a packed child, the two overflow checks and the `overflow` policy that decides the second one
are `flow-layout`'s and apply to an instance unchanged.

Three consequences follow from that sentence rather than from rules of this capability, and are stated
so they are not looked for elsewhere:

- **A list of zero elements produces no instances.** The repeating container contributes no child, and a
  flow container assembles what it has, so a strip of no pills is the padding-sized container
  `flow-layout` already describes and is not an error. `[]` and a `default: []` reach this identically.
- **There is no cap on the instance count.** Too many pills for the box is the overflow policy, which
  already answers it: `fail` fails the render naming the instance that did not fit, and `trim` leaves it
  and every instance after it undrawn.
- **Each instance is sized on its own.** Two instances of one authored `size: [content, content]`
  container resolve to different extents when their elements differ in length, because each is measured
  from what it holds.

**The container's own `when:` is evaluated once, in the enclosing scope, before any element is bound.**
It gates the whole repetition: a predicate that does not match produces no instances at all, and one
that matches produces every instance. It cannot name the repeated parameter, because the repeat scope
starts below the `repeat:` key, so `conditional-visibility`'s refusal of a `when:` key naming a declared
list stands there unchanged.

**A repeat is a read of its parameter.** An active repeating container whose named parameter is absent,
meaning the request omitted it and the declaration supplies no usable default, SHALL be
`422 MissingField` naming that parameter. This is the answer `param-resolution` gives a token that reads
an absent parameter, and it is stated here because a `repeat:` is not a token. An absent list SHALL NOT
be read as zero elements: `list-params` distinguishes `[]` from an omission everywhere else, and folding
them together here would undo that. A repeating container whose `when:` gate does not match reads
nothing, so a parameter only such a container repeats is not required.

**The measurement pre-pass and the rendering SHALL expand a repeat identically**, so the count a
container assembles from and the count it draws can never differ.

**A load-time check that reads the repeated subtree reads it once**, as a single instance, because every
instance carries the same authored geometry and the count is request data. `flow-layout`'s load-time
check that a packed child's authored extent fits its parent's padded inner box therefore applies to the
authored container, and its render-time check on the accumulation applies to the instances.

**A render-time failure inside an instance SHALL name that instance**, as the authored item's layout
path with the element's zero-based index appended after a `#`: the fourth pill of a repeat at
`layout[0].items[0]` is `layout[0].items[0]#3`, and an item nested inside it extends that path as it
otherwise would. A `#` appears in no JSON path segment, so an instance path is never mistaken for one.
A load-time refusal names the authored path with no index, because no instance exists at load.

#### Scenario: Three tags render three pills in order

- **WHEN** a request sends `tags: ["A", "B", "C"]` for a template whose `row` flow container with an
  authored size and `gap: 1` holds one `size: [content, content]` container repeating `tags` and
  printing `{tags}` from a `size: [content, content]` text
- **THEN** three pills are drawn left to right reading `A`, `B` and `C`, each hugging its own element
  and one `gap` from the previous one's trailing edge

#### Scenario: A declared default supplies the elements

- **WHEN** the same template declares `tags: { type: list, default: [CONSUMABLE, KIDS] }` and a request
  omits `tags`
- **THEN** two pills are drawn, reading `CONSUMABLE` and `KIDS`

#### Scenario: Siblings keep their places around the instances

- **WHEN** the strip holds a `content`-width text, then a container repeating `tags` over two elements,
  then another text
- **THEN** the four drawn children are that first text, the two instances in element order, and the last
  text, packed in that order

#### Scenario: An empty list draws the strip and no pills

- **WHEN** a request sends `tags: []`, and when the parameter instead declares `default: []` and the
  request omits it
- **THEN** each renders the enclosing strip with no pills and no error

#### Scenario: An absent list is a missing field

- **WHEN** a template declares `tags: { type: list }` with no default, an active container repeats it,
  and the request omits `tags`
- **THEN** the response is `422 MissingField` naming `tags`, rather than a strip with no pills

#### Scenario: A gated-off repeat requires nothing

- **WHEN** that same container carries `when: { show_tags: "true" }`, the request omits both `show_tags`
  and `tags`
- **THEN** the label renders with no pills and no error, because the container is inactive and reads
  nothing

#### Scenario: The gate is evaluated once, not per element

- **WHEN** the repeating container carries `when: { show_tags: "true" }` and the request sends
  `show_tags: "true"` with `tags: ["A", "B"]`
- **THEN** both instances are drawn
- **AND** a request sending `show_tags: "false"` draws neither

#### Scenario: A gate on the repeating container naming the repeated list is refused

- **WHEN** the repeating container itself carries `when: { tags: KIDS }`
- **THEN** the file fails validation naming `tags` and that container's layout path, under
  `conditional-visibility`, because the repeat scope starts below the `repeat:` key

#### Scenario: More pills than the strip holds fail where they land

- **WHEN** a `row` flow container with an authored padded inner width of 20, `gap: 2` and no `wrap`
  packs instances resolving to 8 wide from a four-element list
- **THEN** the render fails with `UnsupportedLayoutItem` and `details.reason` of `item_out_of_frame`,
  naming `layout[0].items[0]#2` for the third instance
- **AND** the same container declaring `overflow: trim` draws the first two instances and succeeds

#### Scenario: Instances wrap like any other packed children

- **WHEN** that container declares `wrap: true`, `line_gap: 1` and an authored padded inner width of 20
- **THEN** the first two instances sit on the first line and the third begins a second line, one
  `line_gap` below it

### Requirement: Inside a repeated subtree the repeated name is one element

The **repeat scope** of a `repeat: p` is the container carrying the key and everything nested inside it,
beginning below the `repeat:` key itself. Within that scope, and within no other part of the template,
the name `p` SHALL denote the **one element** the instance is being drawn for, which is a string.

The binding reaches every place the scope reads that name **as text**, and exactly two do:

- **An interpolation token.** A bare `{p}` in a `text` or `qr` `value:` or an `image` `src:` within the
  scope SHALL render the bound element. `interpolation-tokens` writes its two list rules for a name that
  denotes a list, which inside the scope it does not, so neither reaches there: the bare token is the
  spelling, and this requirement states what a reader attached to that name does instead.

  **Two refusals, and they are this capability's.** Within the scope, `{p:join('<sep>')}` SHALL fail at
  load, because a join reads a list and there is none there to read; and `{p:<name>}` written with a
  bare reader SHALL fail at load as a format attached to a value that is not an instant, which is what a
  bare reader on any other string parameter already fails as, and not with the message naming
  `join('<separator>')`, which would name a spelling that is refused in that scope. Each message SHALL
  name **the token and the offending item's layout path**: a scope holds many items reading one name, so
  the token alone does not say which, and this is the reason `conditional-visibility` and `list-params`
  each gave for the path on the refusals they introduced.

  Both are decided from the repeat structure and the declared type together, alongside the six refusals
  above and in the same place, so both are reported as those six are: the file is quarantined, and a
  template write is `422 TemplateInvalid` with `details.reason` `template_parse_failed`. They belong
  here rather than to `interpolation-tokens` for the same reason: without a `repeat:` neither exists,
  and that capability's own refusals, which are decided from `params:` alone, keep the reason they
  publish.
- **A `when:` key.** A `when:` key naming `p` on any item **within** the scope SHALL compare the bound
  element against the literal, by the ordinary rule every other condition is compared by. This is the
  exception `conditional-visibility` states to its refusal of a `when:` key naming a declared list, and
  it states the same reason.

**The binding reaches nothing else, and every other rule keyed on the parameter's declared type holds
inside the scope unchanged.** A `size`, `max_w` or `max_h` reference, a `font_weight`, a `color` or
`background` reference, and an `image` item's `name:` each read a parameter as a **typed value** rather
than as text, and `list-params` refuses a list at every one of them. Those six refusals SHALL apply
inside a repeat scope exactly as they apply outside it, naming the parameter and the context. The reason
is a failure the wider rule would cause and this one does not: a template's geometry and colours are
validated at load against one instantiated value per parameter, and a repeated name has no single value
there, so a per-instance extent or colour would be one no load could check and one the load-time
refusal of an oversized packed child could not see. Permitting them later is additive; withdrawing them
would not be.

**Outside every repeat scope nothing changes.** `{p:join(', ')}` still prints the joined list, a bare
`{p}` is still a load refusal, and a `when:` key naming `p` is still a load refusal. A template may do
both: one strip repeating `tags` and one caption joining it are the same parameter read two ways.

**A nested repeat over a different list nests the scopes.** Inside a container repeating `codes` that is
itself inside a container repeating `tags`, `tags` denotes the outer element and `codes` the inner one,
and both are strings.

**A parameter `default:` never sees a binding.** Defaults are resolved once for the request before any
layout is walked, and a default's value path must be dotted (`interpolation-tokens`), so no default can
name a repeated parameter bare and none is resolved per instance.

**Both the `repeat:` key and a token in the scope reading `p` are reads of the parameter `p`** for
every purpose that counts reads: the input list holds an entry for `p`, its `interpolated` flag is
true, and `truncated_elsewhere` is computed for a token exactly as for any other name a `text` item
reads (`template-inputs`). The key alone is enough, because an absent `p` is `422 MissingField` whether
or not the subtree prints it, and `interpolated` is what a preview fills from: a repeat reported as a
structural read like a `when:` key would leave a strip of fixed-content instances with no value for the
one name that decides how many there are.

#### Scenario: A bare token inside the scope prints one element

- **WHEN** a container repeating `tags` holds a `text` reading `"{tags}"` and the request sends
  `tags: ["A", "B"]`
- **THEN** two pills are drawn reading `A` and `B`, rather than either printing a joined list

#### Scenario: A join inside the scope is refused

- **WHEN** a `text` inside a container repeating `tags` reads `"{tags:join(', ')}"`
- **THEN** the file fails validation with a message naming the token and that text item's layout path,
  and is quarantined

#### Scenario: A bare reader inside the scope is a format on a string

- **WHEN** a `text` inside that container reads `"{tags:long_date}"`
- **THEN** the file fails validation with a message naming the token, that text item's layout path, and
  stating that a format applies to an instant only

#### Scenario: A gate inside the scope compares the element

- **WHEN** a `text` inside a container repeating `tags` carries `when: { tags: KIDS }` and the request
  sends `tags: ["KIDS", "SPARES"]`
- **THEN** the template loads, that text is drawn in the first instance and not in the second

#### Scenario: The same parameter is joined outside the strip

- **WHEN** the same template also holds a `text` outside every repeating container reading
  `"{tags:join(', ')}"`
- **THEN** it prints `KIDS, SPARES`, unchanged by the repeat

#### Scenario: A typed slot inside the scope still refuses the list

- **WHEN** an item inside a container repeating `tags` declares `size: ["{tags}", 4]`, or
  `color: "{tags}"`, or is an `image` carrying `name: "tags"`
- **THEN** each fails validation at load naming `tags` and the context, exactly as the same spelling
  outside a repeat does

#### Scenario: Nested scopes bind two names

- **WHEN** a container repeating `tags` holds a flow container whose child repeats `codes`, and that
  child prints `"{tags}-{codes}"`, with `tags: ["A", "B"]` and `codes: ["1", "2"]`
- **THEN** four texts are drawn reading `A-1`, `A-2`, `B-1` and `B-2`, in that order

#### Scenario: The operator is offered the control that fills the strip

- **WHEN** an input list is computed for a label of a template whose only read of `tags` is a repeating
  container printing `{tags}`
- **THEN** it holds an entry for `tags` with control `list` and `interpolated` true
