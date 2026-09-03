## Why

Implements [#319](https://github.com/pfa230/labeler/issues/319).

Nothing renders one authored item once per element of a list. `flow` packs N children into a strip
(#212) and `type: list` supplies the N as data read through `{tags:join(', ')}` (#213), but the count
of pills on a label is still whoever wrote the template: a fixed maximum of containers, each gated on
its own parameter by `when`, one parameter per pill and a hard ceiling. This change is the third
piece, and the one that makes the count come from the data.

## What Changes

**`repeat:` is a key on `container`, and the token grammar gains nothing.**

```yaml
params:
  tags: { type: list }

layout:
  - type: container
    at: [1, 1]
    size: [38, 8]
    flow: { direction: row, gap: 1, wrap: true }
    items:
      - type: container
        repeat: tags
        size: [content, content]
        rounded: 1
        background: "#eee"
        padding: 0.8
        items:
          - type: text
            value: "{tags}"
            size: [content, content]
            font_size: 3
```

**Every extent in that example is written, and it has to be.** A packed container giving neither `size`
nor `to` resolves to `[fill, fill]` and takes its parent's whole padded inner box (`layout-sizing`,
`flow-layout`), so an extent-less repeating container renders for a one-element list and fails with
`item_out_of_frame` for every longer one. A `text` carrying neither is refused at load, as it is
anywhere. Neither rule is changed here and neither is new; what is new is that a repeat makes the first
of them fire on the data rather than on the template, so the specs state the consequence and pin it with
a scenario. `size: [content, content]` is what a pill that hugs its own tag says.

**It multiplies the container that carries it.** That container becomes one sibling per element, in
element order, in the place its parent's child order already gave it, and the parent's arrangement
places them. It does not repeat its own `items:` block in place, so `repeat:` goes on the pill and
never on the strip.

**Inside that subtree the repeated name is bound to the element.** `{tags}` there is one string. Two
existing rules then hold with one stated exception each, and both remain decidable from the template's
own text:

- `interpolation-tokens`: a bare name must be a declared `params:` entry (#322). `tags` is one, under
  any spelling, at every depth.
- `list-params` and `interpolation-tokens`: a bare `{tags}` on a declared list is a load refusal and a
  join is the only reader. Inside a subtree repeating `tags` that inverts and says why: the name is an
  element there, so the bare token is the spelling and `{tags:join(', ')}` is the refusal.

**Eight refusals, all at load, each naming the offending item's layout path, and every one of them
`template_parse_failed` through a template write**, which is the contract #319 sets. Every one
quarantines the file under the `template-registry` rules while the server still starts, and the same
content arriving through a template write is `422 TemplateInvalid` with that reason. All eight are
decided inside `parse_template`, from the file's own text and the declarations it carries, so
`parse_and_validate` reports them as a parse failure (`src/api.rs:640-646`) exactly as `list-params`
reaches the same slug for its own.

| What | Why |
| --- | --- |
| `repeat:` on a `text`, `qr`, `image` or `line` | Not a field of those items; `deny_unknown_fields` refuses it, naming the path |
| `repeat:` written and left empty | A key an author wrote and left without a value, exactly as a parameter attribute is |
| `repeat:` naming an undeclared parameter | The template must declare what it repeats |
| `repeat:` naming a parameter of any type but `list` | Only a list has elements |
| A repeating container whose parent is not a flow container | N copies at one coordinate is overprinting, not an arrangement |
| A nested `repeat:` over the same parameter | It would rebind an element to an element |
| `{tags:join('<sep>')}` inside a subtree repeating `tags` | A join reads a list; there the name is one string |
| `{tags:<format>}` inside that subtree | A bare reader is a format name and a string is not an instant |

The last two are **`repetition`'s** refusals, not `interpolation-tokens`', and that is what lets all
eight share one reason. Neither exists without a `repeat:`, both are decided from the repeat's structure
and the declaration together, and `interpolation-tokens` writes its two list rules for a name that
denotes a list, which inside the scope it does not. That capability's own refusals, decided from
`params:` alone, keep the reason it publishes and are untouched.

One refusal a repeat can trip is **not** one it introduces, and it is left exactly as it is: a `when:`
key naming a declared list, which `conditional-visibility` refuses today and goes on refusing on the
repeating container itself. #319 says of it in as many words that it "stands there unchanged", so this
change does not move it and does not restate what it reports.

And two permissions that are exceptions to a published refusal, each stating its own reason: a bare
`{tags}` **inside** the subtree, and a `when:` key naming `tags` **inside** it, which compares the
bound element.

**Everything about placing the instances is `flow-layout`'s and is already written.** An instance is an
ordinary packed child: order, `gap`, `wrap`, `line_gap`, the secondary-axis alignment, the overflow
policy and the `at`/`to` refusal all apply to it unchanged, and none is restated. Zero elements assemble
to nothing because a flow container packs what it has, not because this change writes a rule for it.
There is no cap on the instance count: too many pills for the box is `overflow`, which already answers
it, `fail` by default and `trim` where the author asked for it.

**A repeat is a read.** An active repeating container whose list parameter is absent is `422
MissingField` naming it, exactly as a token read of an absent parameter is, and one whose `when:` gate
does not match reads nothing. The input list reports the name, so an operator gets the control that
decides how many pills there are.

**Not a breaking change.** `repeat:` is a new optional key; a template that does not carry one parses,
validates, renders and round-trips byte for byte as it does today. Nothing that loads today stops
loading: every refusal above needs a `repeat:` key somewhere in the file, and the two permissions are
relaxations of existing refusals.

### Eight decisions #319 left open, settled here

The issue's ten settled decisions are the contract and are carried into the specs unchanged. Eight
questions it does not reach have to be answered before the contract is complete; `design.md` carries the
reasoning.

- **How far the binding reaches: to what reads the name as text, and no further.** Inside the subtree
  the element is what an interpolation token prints and what a `when:` key compares. Every slot that
  reads a parameter as a *typed* value keeps reading the declared type, which is `list`: a `size`,
  `max_w` or `max_h` `ref:`, a `font_weight`, a `color` or `background` `ref:`, and an `image` `name:`.
  `list-params` refuses a list at all six today and this change leaves all six refusals standing. The
  concrete failure the wider rule causes is at load: validation instantiates one value per parameter and
  checks the template's geometry and colours against it (`src/templates.rs:1695-1790`), and a repeated
  name has no single value there, so a per-instance extent or colour is one no load could check.
  Permitting it later is additive; the reverse is not.
- **A render failure inside an instance names the instance.** The layout path is the authored path with
  the zero-based element index appended after a `#`, so the fourth pill overrunning its strip reports
  `layout[0].items[0]#3`. A `#` appears in no JSON path segment, and without it N identical messages
  name one authored item.
- **An absent list is `422 MissingField`, not zero pills.** `list-params` spends a paragraph keeping `[]`
  distinct from an omission; folding an absent parameter into an empty strip would collapse exactly that
  distinction, and it is the answer a token read of the same parameter already gets.
- **The per-label input paths expand a repeat the way a render does**, so a repeat over an absent or
  empty list contributes only its own name and the subtree's other controls appear once a label carries
  elements. That is what a gated branch already does to `inputs.default`, and `inputs.all` is what a
  screen reads to see everything.
- **`inputs.all` walks a repeated subtree once**, whatever the element count, exactly as it ignores every
  `when:`. It is the union of what any label could produce, and some label supplies a non-empty list.
  Expanding it against a label carrying no data would report a tag strip's own parameter as read by
  nothing, and the thumbnail, which fills from `inputs.all`, would then invent nothing and print an
  empty strip for every template using this feature.
- **Where each refusal is decided, which is what fixes the reason it reports.** All eight are decided
  inside `parse_template`: four fall out of the conversion the model already does, and the name-and-type
  pair and the two scoped token refusals are one pass at the end of that conversion, after `params:` is
  converted and after every existing conversion error has had its chance to fire. That is what makes
  `template_parse_failed` true of all eight, as #319 requires, without reordering anything a template
  reports today.
- **A `repeat:` key is an interpolated read of its parameter**, so the input list reports
  `interpolated: true` for it whether or not the subtree prints the element. The flag decides what a
  thumbnail and a client preview invent a value for, and a repeat with an absent parameter is
  `422 MissingField` where a gate with one is merely false, so reporting it as structural would break
  the preview of every strip whose instances print fixed content.
- **A repeating container states its own extent, and the specs pin what happens when it does not.** An
  extent-less packed container is `[fill, fill]` and takes the whole strip, so a repeat of one element
  renders and a repeat of two fails with `item_out_of_frame`. That is `layout-sizing` and `flow-layout`
  unchanged, reached through a count that now comes from the request, which is exactly the case worth a
  scenario rather than a discovery.

## Capabilities

### New Capabilities

- `repetition`: the `repeat:` key. Where it may be written, what it may name, what its parent must be,
  how the instances are produced and ordered, what the repeated name means inside the subtree it
  creates, and which of the corpus's existing rules that scope bends and which it leaves alone.

  Its first requirement is a **first-touch** one: `repeat:` is a field of `container`, whose field list
  is documented only in the frozen `docs/SPEC.md` §4.1, so the requirement names that bullet's field
  list and supersedes it to the extent of adding the key, which is exactly what `flow-layout` did for
  `flow`. Every other field §4.1 lists, and every other statement in it, stays authoritative.

### Modified Capabilities

- `interpolation-tokens`: one requirement, "A colon attaches a reader: a format to an instant, or a join
  to a list". Its two type-keyed rules are decided from `params:` alone today, and inside a repeat scope
  they invert: a join on the repeated name is refused there, a bare token on it is the spelling, and a
  bare reader on it is a format on a string. Its other requirements are unchanged and are deliberately
  not restated.
- `conditional-visibility`: one requirement, "A `when:` map holds conditions on declared parameters, and
  a list is not one". Its refusal of a `when:` key naming a declared list gains one exception, inside a
  subtree repeating that parameter, and stands unchanged everywhere else including on the repeating
  container's own `when:`.
- `list-params`: two requirements. "A `list` parameter holds an ordered list of strings" says a list
  claims `{p:join('<sep>')}` and no other token, which is no longer true inside a repeat. "A list cannot
  resolve a layout attribute or bind an image" is where the binding stops, and says so rather than
  leaving a reader to infer it.
- `param-resolution`: one requirement, "A parameter is required unless the template declares a default".
  It says the value a token reads comes from exactly two places and that there is no third; a repeat
  binding is a third, scoped to a subtree, and the same requirement is where an absent repeated
  parameter is answered with `422 MissingField`.
- `template-inputs`: two requirements. "An input list describes the controls one label needs" defines
  `interpolated` by a closed list of value reads, and a `repeat:` key joins it, because the flag decides
  what a preview invents a value for and an absent repeated parameter fails the render where an absent
  gate merely draws nothing. "The template detail carries the lists a client needs before it has a
  label" gains the `inputs.all` walk. The thumbnail requirement is deliberately **not** modified: its
  rule fills every required, interpolated entry, and its table already fills a `list` with a one-element
  list holding the entry's own name, so the flag being right is the whole of what a repeat needs from
  it.

## Impact

- **Template model**: `raw.rs` (`ContainerRaw` gains `repeat`, read through `deserialize_present_typed`
  so a key written and left empty is distinguishable), `models.rs` (`LayoutItem::Container` gains
  `repeat: Option<String>`), `convert.rs` (the `TryFrom` that carries it across and refuses an empty
  one). The three files a new layout field always moves together, and every one of the eight refusals
  lands here: three fall out of the conversion already (serde, the `Some(None)` case, and `is_packed`,
  which is the parent's arrangement), and the other five are one pass at the end of
  `TryFrom<TemplateDefinitionRaw>`, after `params:` has been converted and after every existing
  conversion error has had its chance to fire, so no template's current message changes.
- **Load-time validation**: `templates.rs`. `validate_item_references` gains the set of names currently
  repeated and threads it to `validate_interpolated_string` and `validate_when_references`, which is
  what decides the two scoped token rules and the scoped `when:` permission; it already carries the
  layout path, which those two new token refusals now name. The repeated subtree is validated once,
  because every instance has the same authored geometry.
- **Render**: `render/mod.rs`. The measurement pre-pass and the render walk expand a repeating container
  through one shared function, so the two cannot disagree about the count, and each instance is walked
  under a `RenderContext` whose data map carries the element under the repeated name. Nothing in
  `resolver.rs` moves: an instance is sized exactly as the authored container would be.
- **Input derivation**: `templates.rs`, where the read-set walk records a `repeat:` key as an
  interpolated read of its parameter (`record_ref`, `src/templates.rs:205-213`), expands per label and
  walks once for `inputs.all`. Nothing in `placeholder_data` moves: it already fills a required,
  interpolated `list` entry with a one-element list holding the entry's own name
  (`src/templates.rs:184-188`), so a repeat-only template's thumbnail draws one instance as soon as the
  flag is right.
- **API**: `LayoutItem` is already registered in `src/openapi.rs`, so the new field is carried by the
  existing `ToSchema` derive. `GET /api/templates/{id}` returns the authored container carrying
  `repeat:`, one item and not N, so a template read back and resubmitted is unchanged. No new
  `details.reason` slug and no change to an existing one: each refusal takes the slug the table above
  gives it, and an overrun inside an instance is the `item_out_of_frame` a packed child already raises.
- **UI**: one function. `repeat:` is authored in YAML and the strip it renders needs no control, and the
  editor for a `list` parameter is still #318's. What does move is the **client preview's** sample data:
  `sampleData` (`ui/src/lib/preview.ts:14-33`) mirrors the thumbnail's fill rule and has no `list` arm,
  so it falls through to `data[name] = name` and sends a string where the API requires a JSON array,
  which is `400 InvalidRequest` with `request_body_invalid` (`list-params`). It needs the arm the server
  already has, a one-element list holding the entry's own name.

  **That gap is #213's and predates this change**: it is reachable today by any template declaring an
  undefaulted `list` and printing `{tags:join(', ')}`, whose detail-page preview fails the same way.
  It is fixed here rather than filed because a repeat makes it the feature's own preview path, and
  because `template-inputs` already publishes the obligation it breaches: a client building its own
  preview "SHALL supply a legal value for every input its preview references that the service reports as
  required". No spec changes for it; it gets its own test naming the join spelling, so what is fixed is
  visible as the pre-existing defect it is.
