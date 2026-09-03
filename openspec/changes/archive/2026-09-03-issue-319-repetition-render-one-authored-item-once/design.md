## Context

See `proposal.md` for motivation. What shapes the approach is where the layout tree is walked and what
each walk has in its hand.

**Three walks, one tree.** Load-time validation walks the authored items twice, once for parameter
references (`validate_item_references`, `src/templates.rs:1021` at the root and `:1629` for a
container's children) and once to instantiate declared defaults and check geometry
(`instantiate_item_defaults`, `:1740`, which resolves each `DynamicValue::Ref` to one literal per
parameter). Rendering walks them twice more: a measurement pre-pass (`measure_items`,
`src/render/mod.rs:1509`) that produces one `Measured` per **active** child, and the render walk
(`:1928`) that rebuilds the same active list and zips it against those nodes positionally
(`:1931-1936`, `:1971-1976`). The two render walks agreeing is a standing invariant of this file, and a
repeat is the first construct that can make one child into several.

**Values reach an item through one map.** `RenderContext` (`:1349`) holds
`data: &HashMap<String, JsonValue>`, and everything that reads a parameter as text goes through it:
`resolve_item_text` for tokens and `is_item_active` for `when:`. Geometry does not: sizes, `font_weight`
and colours are resolved from the label-level `geometry_values` map and from `resolved_data` before the
walk begins, and load validates them against one instantiated value per parameter.

**Where a new layout key goes.** `raw.rs` (`ContainerRaw`, `:285`), `models.rs`
(`LayoutItem::Container`, `:1105`) and `convert.rs` move together, and `deserialize_present_typed`
already gives `ContainerRaw` the ability to tell a key written and left empty from a key absent, which
is how `shape`, `stroke`, `background`, `rounded` and `flow` are read.

**Where a refusal is decided is what a caller sees.** `parse_and_validate` reports everything
`parse_template` raises as `template_parse_failed` and everything `validate()` raises as
`template_validation_failed` (`src/api.rs:640-646`), and `parse_template` is serde plus
`TemplateContent::try_from` (`src/parse.rs:25-34`), so a check inside the conversion is a parse-stage
refusal. That conversion already carries what a repeat needs to judge itself: `try_from_raw` threads
`is_packed`, which is "my parent arranges by flow" (`src/convert.rs:351`), and
`TryFrom<TemplateDefinitionRaw>` ends with both the converted layout and the converted `params:` in
hand (`:714-729`).

**What fills a preview.** `placeholder_data` invents a value for an entry only when it is both
`interpolated` and `required` (`src/templates.rs:163-165`), and the client's live preview mirrors that
rule in `sampleData` (`ui/src/lib/preview.ts:14-33`). Whether the derivation calls a read
"interpolated" therefore decides whether a template previews at all.

## Goals / Non-Goals

**Goals:**

- One authored container drawn once per element of a list parameter, with the count coming from the
  request and the placement coming from `flow-layout` unchanged.
- Every refusal decidable from the template's own text at load, which is where this codebase puts them.
- The measurement pre-pass and the render walk unable to disagree about the instance count.
- No new token syntax, no new namespace root, and no new `details.reason` slug.

**Non-Goals:**

- A per-instance index, a first/last predicate, a separator item between instances, or any other
  construct a template engine's `for` loop usually carries. None is in #319 and each is a separate
  contract.
- A list of anything but strings, which is `list-params`' settled shape.
- A UI editor for a `list` parameter, which is #318.
- Reaching a per-element value from a size, a colour or an `image` `name:`; see decision 3.
- A limit on how many instances one request may ask the renderer for. #319 settles that there is none,
  and the risk section says plainly what that leaves unbounded rather than capping it under another
  name.

## Decisions

### 1. `repeat:` multiplies the container carrying it, and rebinds the name in scope

Taken from #319's design section, which settled it against four alternatives, and restated here because
`docs/adr/` is frozen and this is where the reasoning now lives.

- **A third namespace root** (`{item.value}`) contradicts `interpolation-tokens`' "exactly two roots",
  whose argument is that a fixed root set is what keeps parameter-name validation stable.
- **A binding name declared on the block** (`as: tag`, then `{tag}`) is an undeclared bare name, which
  #322 refuses at load. It would need an exception carved into the rule #322 exists to establish.
- **Index addressing** (`{tags.0}`) addresses rather than iterates, so it never solved the problem.
- **A reader** (`{tags:item}`) needs no exception and works, but it widens the colon position from "a
  reader over this value" to "a reader over this value in this context". `join` and a datetime format
  mean the same thing wherever they are written; `item` would not.

Rebinding in scope leaves `{tags}` a pure function of the parameter named `tags` and makes the repeat
change what that parameter *is* inside the subtree, which is ordinary lexical scoping and needs no
grammar at all.

The cost is the one #319 accepts: `repeat: tags` printing one tag through `{tags}` reads slightly oddly.
An alias would drag #322 back in for a cosmetic gain.

### 2. The binding is a per-instance data map, not an overlay threaded through every reader

Each instance is walked under a `RenderContext` whose `data` is a clone of the enclosing map with the
repeated name overwritten by the element as a JSON string. Everything downstream then works untouched:
`resolve_item_text`, `is_item_active`, and a nested repeat that clones again and adds its own name.

The alternative is an overlay, a `&[(name, value)]` slice on the context consulted ahead of `data`. It
avoids a clone per instance, and it costs a signature change to `interpolate` in `render/helpers.rs`
and to every other reader, including the ones resolving parameter defaults, which have no bindings and
never will. The clone is bounded by a label's parameter map, which is a handful of small JSON values,
and it is paid once per instance rather than per token. Correctness first, and this is the version with
fewer places to get it wrong.

Rejected outright: substituting the element into the item's text at expansion time. A token is not text
until it is resolved, and pre-substituting one would defeat brace escaping, the missing-field error and
the load-time refusals that read the token as written.

### 3. The binding reaches what reads the name as text, and stops there

Inside a repeat scope the name is bound for an interpolation token and a `when:` key. A `size`, `max_w`
or `max_h` `ref:`, a `font_weight`, a `color` or `background` `ref:`, and an `image` `name:` keep
reading the declared type, which is `list`, so `list-params`' six refusals stand inside the scope
exactly as outside it, and none of them needs new code.

The uniform-sounding alternative is that the name simply *is* a string inside the scope, everywhere. It
is attractive, and #319's own phrase "ordinary lexical scoping" points at it, so it needs a real reason
to be refused rather than a preference. The reason is a failure at load. Geometry and colour references
are validated against one instantiated value per parameter (`src/templates.rs:1740`,
`resolve_f32_default`), and a repeated name has no single value there: a `default: ["#f00", "#0f0"]`
offers two and a parameter with no default offers none. So a per-instance extent would be one load could
not check, which is the check that refuses a packed child too large for its parent's padded inner box
before any request arrives (`flow-layout`), and a per-instance colour would be one load could not parse.
Choosing an element to validate against would be a guess, and validating every element is a rule nobody
asked for.

The second reason is that the refusals already exist and are already tested: `check_param_ref` refuses a
`list` at each of those slots today (`src/templates.rs:1322-1347`, called at `:1511-1631`). Keeping them
means the scope is implemented in exactly the two places it is claimed to reach, and a reviewer can see
that from the diff. Admitting a slot later is additive; withdrawing one would not be.

`list-params`' requirement carries this boundary rather than `repetition` alone, because that is the
requirement an author reads to find out what a list may not do.

### 4. One expansion function, called by every walk

Expansion is a pure function of (the authored children, the resolving context): it returns the sequence
of children to walk, each as an authored index, an optional element index, and the binding to walk it
under. `measure_items` and the render walk both call it in place of the `is_item_active` filter they run
today (`src/render/mod.rs:1520`, `:1931`), so the sequence they zip cannot differ. Activity is decided
inside it, because an instance's children carry `when:` predicates that only mean something under the
binding.

The alternative is to expand once, before measurement, into a materialised `Vec<LayoutItem>` of clones.
It would leave both walks untouched, and it fails on the thing that matters: the clone still has to be
walked under the element's binding, so the bindings would have to be carried alongside the tree anyway,
and the tree the render walks would no longer be the tree the template holds. Cloning a subtree per
element also pays a memory cost proportional to the layout, where the binding pays one proportional to
the parameters.

### 5. A render failure inside an instance names the instance, as `#<n>`

Paths are built by string append today (`format!("{path_prefix}[{orig_idx}]")`), and the render's
overflow errors name the child that did not fit. With N instances of one authored container, that name
is ambiguous exactly when it matters most: the third pill overran and the message names the pill.
Appending `#3` to the authored path is one format string, and `#` occurs in no JSON path segment, so an
instance path can never be mistaken for one an author could write. A load-time refusal keeps the plain
path, because at load there are no instances.

Rejected: `items[0][3]`, which reads as a second array index into a layout that has none, and a message
naming the element's *value*, which is data in a diagnostic and is not unique.

### 6. An absent list is `422 MissingField`, and an empty one draws nothing

`list-params` spends a paragraph keeping `[]` apart from an omission, and `param-resolution` makes an
absent parameter an error exactly when an active item reads it. A repeat reads it. The alternative,
treating absent as zero elements, would make the one construct whose whole purpose is a data-driven
count the one place where "you sent nothing" and "you sent an empty list" print the same label.

### 7. The per-label input paths expand; `inputs.all` walks once

`template-inputs` says entries are decided "by the same rule the renderer applies", so `inputs.default`
and `POST /api/templates/{id}/inputs` expand, and a repeat over an absent or empty list contributes only
its own name. That is what a gated branch already does to those lists.

`inputs.all` cannot do that, and this is the one place a naive implementation is silently wrong rather
than loudly wrong. It ignores gates because it is the union of what *any* label could produce, and it is
what the thumbnail fills its placeholder data from. Expanding it against a label carrying no data yields
no instances, so a tag strip's parameter would be reported as read by nothing, `interpolated` would be
false, the thumbnail would invent nothing, and every such template's preview would be an empty strip
with no error anywhere. Walking the subtree once is the union rule applied, not an exception to it.

### 8. A `repeat:` written and left empty is refused

`conditional-visibility` treats a null `when:` as an absent predicate, and `list-params` and
`datetime-params` refuse a parameter attribute written and left empty. A `repeat:` follows the second:
that capability's own explanation is that a key holding a *container* has nothing to gate on when it is
null, while a key holding a *value* is one an author wrote and left out. `repeat:` holds a name.
`deserialize_present_typed` on `ContainerRaw` is how the distinction reaches `convert.rs`.

### 9. Which stage refuses, and therefore what a write reports

Where a refusal is decided is what a caller sees, per the Context above. #319 asks for
`template_parse_failed` on every refusal a `repeat:` introduces, and every one of the eight is decided
inside `parse_template`, so the issue's line holds with no exception written into it.

Three fall out of the conversion the model already does. Two are there for nothing: an unknown `repeat`
on a `text` is a serde rejection, and a `repeat:` written and left empty is the `Some(None)` case
`ContainerRaw` already distinguishes and `try_into_container` already refuses for `stroke`, `shape` and
`flow`. The parent-is-flow refusal is structural and `try_from_raw` already carries `is_packed`, which
is exactly "my parent arranges by flow" (`src/convert.rs:351`, `:162`). The rest need `params:`, which
`TryFrom<TemplateDefinitionRaw>` converts **after** the layout (`src/convert.rs:714-729`), so they run
as one pass at the end of that conversion, over the converted layout and the converted params:
the undeclared-name and wrong-type refusals, the nested-same-name refusal, which wants the scope the
same walk builds, and the two scoped token refusals below. Running last is what keeps every existing
template's message unchanged: every conversion error that can fire today still fires first, and nothing
is reordered.

The alternative for the params-aware ones was to reorder the conversion so params come first. It buys
nothing and changes which error a template with two faults reports, which is observable and is not this
change's to alter.

**The two scoped token refusals join them, in that same final pass.** A join or a bare reader on the
repeated name is decided from three things the pass already holds: the token, the declaration, and the
scope. So it is decided there, reported as the other six are, and owned by `repetition`, which is what
makes `template_parse_failed` true of all eight and satisfies #319's acceptance line without an
exception in it.

Owning them there is not a relabelling. `interpolation-tokens` writes its two list rules for a name that
denotes a list, and inside a scope the name denotes an element, so those rules do not reach the scope
and the refusals inside it are not refusals that capability owns. Its blanket sentence, that every load
refusal it owns reports `template_validation_failed`, therefore stays true and untouched, which is what
the alternative could not manage: leaving the pair in `validate_interpolated_string` gives two of the
eight a different slug, and an earlier draft of this plan did exactly that and was right to be refused,
because #319's acceptance line admits no split.

**What it costs is that two walks know about repeat scopes.** The conversion pass computes them, for
these two refusals and for the nested-repeat one; `validate_item_references` computes them too, because
it must *permit* what it otherwise refuses, a bare `{tags}` and a `when:` key naming `tags` inside the
scope. Neither duplicates a decision: each walk recurses containers already, and a scope is one name
pushed when a walk descends through a `repeat:`. The alternative, moving token validation wholesale to
the parse stage, would change the reason for refusals this change never touches.

The `when:` refusal is not in the pass and is not moved: it is `conditional-visibility`'s existing
refusal, firing today on templates with no `repeat:` in them, and #319 says it stands unchanged.

### 10. A repeating container carries its own extent, and the specs pin what happens when it does not

An extent-less packed container resolves to `[fill, fill]` (`layout-sizing`), and two fill children of
one flow container collide (`flow-layout`). Under a repeat that becomes: one element renders, two fail
with `item_out_of_frame`. Nothing about that is new, and this change deliberately adds no special
default for a repeating container, because a spelling that resolved differently according to whether its
container repeats would make the reader check the key before reading the size. What is new is that the
count comes from the request, so the same template renders for one caller and fails for the next, which
is worth a scenario rather than a discovery. `size: [content, content]` on the pill and on its text is
what the canonical example writes, and the alternative considered, defaulting a repeating container to
`[content, content]`, is exactly the parent-dependent spelling `flow-layout` already refused to invent.

### 11. Which walk decides what, in one list

Decision 9 puts every refusal in the conversion; the permissions stay in validation, because that is
where the rules they relax live. Written out, so an implementer has one architecture and not two:

| Where | What it decides |
| --- | --- |
| serde, `raw.rs` | `repeat:` on a `text`, `qr`, `image` or `line`, as an unknown field |
| `try_into_container`, `convert.rs` | `repeat:` written and left empty, the `Some(None)` case it already handles for `stroke`, `shape` and `flow`; and the parent-is-flow refusal, from the `is_packed` flag `try_from_raw` already threads (`src/convert.rs:351`, `:162`) |
| a final pass in `TryFrom<TemplateDefinitionRaw>` | the undeclared-name and wrong-type refusals, the nested-same-name refusal, and the two scoped token refusals, all with `params:` and the scope in hand |
| `validate_item_references` and what it calls | the **permissions**: a bare `{tags}` and a `when:` key naming `tags` inside a scope, which `validate_interpolated_string` and `validate_when_references` refuse today and must not refuse there |

The root call of each walk starts with "no flow parent" and "nothing repeated", which is what refuses a
`repeat:` on a root-level item and what makes an item outside every scope read exactly as it does now.

### 12. A `repeat:` is an interpolated read, and that is what keeps previews working

`interpolated` reads like a statement about content, and it is really a statement about what a preview
must invent: `placeholder_data` fills an entry only when `interpolated && required`
(`src/templates.rs:163-165`) and `sampleData` mirrors it (`ui/src/lib/preview.ts:20`). A `repeat:` over
an undefaulted list that prints nothing from it would be `interpolated: false` under the flag's
pre-change wording, get no placeholder, and fail with `422 MissingField` on a template that is valid and
that the same response reported a control for. An earlier draft of this plan said exactly that, and this
decision reverses it.

So the flag's line is drawn where the render's is: a name whose absence the render survives is
structural, and a name whose absence fails it is a value read. A gate is the first, because an absent
parameter makes it false; a `repeat:` is the second, because an absent parameter is `MissingField`
(decision 6). Nothing else about the flag moves, and the thumbnail requirement needs no change: its
`list` fill already produces a one-element list holding the entry's own name, which is exactly one
instance carrying a visible placeholder.

The alternative was to leave the flag structural and widen the thumbnail's fill rule to cover repeat
reads. It needs the input list to publish which names a repeat reads, which is the flag, spelled twice.

**The client preview needs the arm the server already has.** `sampleData` has no `list` case and falls
through to `data[name] = name`, sending a string where the API takes a JSON array, so a required
interpolated `list` makes the preview `400 InvalidRequest`. That defect is #213's and is live today for
`{tags:join(', ')}` on an undefaulted list; this change fixes it because the repeat makes it the
feature's own preview path, and because `template-inputs` already requires a client's preview to supply
a legal value for every input the service reports as required. It gets its own test written against the
join spelling, so the fix is legible as the pre-existing bug it closes rather than as part of the
repeat.

### 13. No `flow-layout` delta

#319 expected one. Once `repetition` says an instance is an ordinary packed child and the instances take
the authored container's place in child order, every sentence `flow-layout` already carries applies
unchanged: order, `gap`, `wrap`, `line_gap`, secondary alignment, the two overflow checks, the `at`/`to`
refusal, and "when no child occupies the primary axis the assembled extent is zero", which is what makes
an empty list a strip with no pills and no error. A `MODIFIED` that changed nothing would republish 650
lines for a cross-reference, and the corpus's own habit is to leave a capability alone when its rules
still hold.

### 14. The repeated subtree is validated once at load

Every instance carries the same authored geometry, and the count is request data, so load validates the
authored container once. What load can check it still checks: an authored extent larger than the padded
inner box, a `when:` naming an undeclared name, a token naming an undeclared parameter. What it cannot
check is the accumulation, which is render's job for packed children already and is unchanged.

## Risks / Trade-offs

- **The two render walks drift, and a `Measured` node is zipped against the wrong instance.** This is
  the sharpest failure mode in the change, and it is silent: a pill is drawn at another pill's size.
  → One expansion function called by both walks (decision 4), and a test that renders three unequal tags
  and asserts each instance's drawn geometry, not merely that a PNG came back.
- **A repeat multiplies work, a nested repeat multiplies it again, and the layout bounds none of it.**
  A 50-element list nested inside a 50-element list is 2,500 measured subtrees from a template that
  reads as four lines of YAML, and both time and peak memory scale with that product: every active
  child is measured before the arrangement is computed, `Measured` retains a `children` tree
  (`src/render/mod.rs:1266-1270`) that the render walk reads afterwards, and `flow-layout` states in
  terms that a **trimmed child is still sized and evaluated** and only stops being *drawn*. So neither
  overflow policy bounds the work: `trim` stops packing after everything has been measured, and `fail`
  raises after it too. An earlier draft of this section claimed the opposite; it was wrong, and the
  layout limits bound only what appears on the label.
  → This change adds no cap, because #319 settles that it must not, and a compute guard would be one
  wearing another name. What it does instead is keep the per-instance cost to the work the feature
  inherently is: no subtree is cloned (decision 4), so an instance costs one measured subtree plus one
  small map, and 2,500 instances cost 2,500 subtrees rather than 2,500 copies of the template.
  → The bounds that do exist are the request's, and they are worth stating because they are what an
  operator has: a label's list arrives in the request body under axum's global `DefaultBodyLimit`
  (~2 MiB; 64 KiB on `POST /api/print`, `src/api.rs:276`), a batch is capped at 500 labels
  (`src/api.rs:47`), and a render is synchronous, so the work is bounded per request and never queued
  behind the caller's back. They bound the *elements* well and the *product* badly: two nested lists of
  500 fit in a few kilobytes and ask for 250,000 subtrees.
  → That residual is real and is stated rather than mitigated away. If it ever needs a bound, the bound
  belongs where an operator can see it, as a configured limit with its own error, and that is a
  decision about caller-driven cost across the whole service rather than something to smuggle into this
  key's semantics.
- **Both previews depend on a read the walker does not make today, and get it wrong loudly.** A repeat
  the derivation did not record leaves the entry off the list entirely; one recorded as structural
  leaves it on the list with `interpolated: false`, which is worse, because the thumbnail then invents
  nothing and the render of a valid template is `422 MissingField` for a name the same response
  reported a control for.
  → Decision 12 makes a `repeat:` an interpolated read, the `template-inputs` delta publishes it, and
  the tests assert the outcome at both fill sites: a thumbnail of a repeat-only template that draws one
  instance, and a `sampleData` case that sends an array. Asserting "no error" would pass against a
  blank strip.
- **A load-time refusal that stops firing looks exactly like a template that is legal.** The eight new
  refusals are the kind that rot quietly, and the two scoped ones doubly so, because the same token is
  legal one level up.
  → Each gets a test asserting the refusal *and* the message naming the layout path, and the pairs that
  differ only by scope (a join inside versus outside, a `when:` inside versus outside) are asserted as
  pairs so that a rule which stopped distinguishing them fails.
- **The clone per instance is measurable on a large batch.** A sheet of 30 labels each drawing 20
  instances clones 600 small maps.
  → The map holds a label's declared parameters, not its rendered content. If it ever matters, the
  overlay in decision 2 is the answer and it is a local change.

## Migration Plan

None. `repeat:` is a new optional key on `container`: a template that does not carry one parses,
validates, renders and round-trips exactly as it does today, and nothing that loads today stops loading.
No stored data moves, no setting changes, and the API's response shapes are unchanged apart from the new
optional field on a container.

## Open Questions

None. Everything #319 left open is settled above and in the specs, and nothing here is deferred to
implementation.
