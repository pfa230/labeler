## Context

See `proposal.md` for motivation. What shapes the approach is where the field is read today, which is
four places rather than one:

- **Load.** `raw.rs` deserializes `line_spacing` through a hand-written `RawLineSpacing` that admits a
  YAML number and rejects everything else with a message naming what it saw (`src/raw.rs:214-244`).
  `convert.rs` bounds-checks the literal (`src/convert.rs:411-437`). `templates.rs` bounds-checks it a
  second time on the domain model, so an item built by any route is checked
  (`validate_line_spacing`, `src/templates.rs:2322`).
- **Load-time reference checking.** `validate_item_references` calls `check_param_ref` for
  `font_weight`, `color` and each dynamic extent (`src/templates.rs:1540-1560`). `line_spacing` has no
  entry there because it has no reference form yet.
- **Measurement.** `Renderer::intrinsic`'s `Text` arm builds a `TextLayoutItem` and calls `layout_text`
  (`src/render/mod.rs:1663-1704`). `layout_text` threads `line_spacing` into the auto-shrink search
  (`largest_fitting_font`), the fit test (`text_fits`) and the reported block height
  (`src/render/helpers.rs:736-870`). This pass is unconditional for a rendered text item; the emitter
  unwraps its `TextFit` (`src/render/mod.rs:2097`).
- **Emission.** `render_single_item` passes the item's `line_spacing` separately into
  `render_text_item`, which turns it into Typst `par(leading:)`
  (`src/render/mod.rs:2072-2101`, `2203-2204`).

Two existing facts constrain the design. First, `Template::validate` does not run on the authored
template: it runs on a copy from `instantiate_with_defaults`, which substitutes each `font_weight` and
each dynamic extent reference with the referenced parameter's declared default
(`src/templates.rs:1077-1079`, `1710-1720`, `1822-1826`). Second, load never measures text, so
`line_spacing` currently feeds no load-time geometry at all: `templates.rs` calls no block-height
helper. The instantiated value of this one field therefore reaches nothing but `validate_line_spacing`.

## Goals / Non-Goals

**Goals:**

- One resolution per rendered item, consumed by the fitter and the emitter, so the two cannot be
  handed different pitches.
- Exactly two authored spellings, with every other scalar keeping the refusal #363 published.
- Load-time reference checks reuse `check_param_ref`, so the messages read like every other
  undeclared-or-mistyped reference.

**Non-Goals:**

- Fixing `font_weight`'s measure/emit divergence (#382). This design must not adopt its shape, but it
  does not repair it either.
- Changing anything shared by every numeric parameter: neither `coerce_param_value`'s handling of a
  non-finite number (`src/render/mod.rs:122-138`) nor `resolve_dynamic_value_f32`'s string lenience.
  Both shape what a supplied pitch does, and both are ruled out here. See the last two decisions.
- Any UI work. `ui/` does not reference `line_spacing`, so the `ui/` gates are untouched by this diff.

## Decisions

### The raw field keeps its own deserializer instead of becoming `DynamicValue<f32>`

`font_weight` is spelled `Option<Dynamic<u16>>` in `raw.rs`, so the obvious move is
`Option<DynamicValue<f32>>` for `line_spacing` too. It is wrong here.
`DynamicValue`'s `Deserialize` (`src/models.rs:367-390`) accepts far more than two spellings from a
string: it reads `"1.2"` as a literal, strips an `mm`/`in` suffix so `"1.2mm"` is a literal too, and
treats anything between an outer `{` and `}` as a reference, so `"{{ pitch }}"` would become a
reference named `{ pitch }`. Adopting it would silently widen the accepted literal spellings, killing
the published "there is exactly one spelling of a value", and would turn a doubled-brace string from a
clear refusal into a reference nobody declared.

So `RawLineSpacing` gains a `Ref(String)` variant and keeps its explicit discrimination: a YAML number
is `Float`; a string is a `Ref` only when it opens with exactly one `{`, closes with exactly one `}`
and has no inner brace, with the inner text trimmed; every other scalar stays `Invalid` and carries the
existing message. Cost: one more hand-written deserializer arm rather than a reused one. Bought: the
`"1.2"`, `"1.2mm"` and `"{{ pitch }}"` refusals are decided in one readable place instead of inherited
by accident, and the delta can state them as contract.

The **domain** type is still `Option<DynamicValue<f32>>` (`models.rs`), because that is what every
consumer downstream of `convert.rs` already knows how to hold, serialize and pattern-match.
`DynamicValue<f32>`'s `Serialize` emits `{name}` for a reference, which is exactly the read-back the
spec requires, and `DynamicValue<f32>` already has a published utoipa schema
(`src/models.rs:542`), so the OpenAPI surface changes shape without new schema code.

### A reference is left unsubstituted by `instantiate_with_defaults`

`instantiate_item_defaults` replaces a `font_weight` reference with the parameter's declared default
before validation, which means a template declaring `default: 250` for a weight parameter is
quarantined at load. Doing the same for `line_spacing` would quarantine a template whose pitch
parameter declares `default: 0`.

This design clones the reference through instead, so `validate_line_spacing` sees a non-literal and
skips it, and that template loads. Reasons, in order: the issue fixes the contract as "a reference is
checked for being declared and for its type, and its value is not checked"; there is exactly one place
a pitch value is judged at render, and adding a second, load-only judgement of one particular source of
that value is the drift `resolver.rs` was restructured to prevent (#226); and the substituted value
reaches nothing else, because load does not measure text, so the substitution would exist solely to
create that second judgement. The cost is real and stated: a template author who declares
`default: 0` learns of it at render, not at load. It surfaces immediately as a broken thumbnail, since
the template list renders each template against its declared defaults.

Alternative considered and rejected: substitute like `font_weight` and let the bound catch a bad
default at load. It buys an earlier error for one authoring mistake and pays with two places that
decide whether a pitch is usable, which will disagree the first time one of them changes.

### The reference is collected by the input walker, in the same arm as `font_weight` and `color`

A reference is not only something to validate and resolve: it is a name an operator must be given a
control for. `TemplateContent`'s input walker collects every reference an *active* item makes —
placement extents, `font_weight`, `color`, and bare tokens in `value` — through `record_ref`
(`src/templates.rs:201-209`, the `Text` arm at `272-290`), and a parameter that no walk collected is
dropped from the input list entirely (`src/templates.rs:413-415`). Adding a field that reads a
parameter without adding it to that arm produces a template whose render demands a value no form
offers, which `template-inputs` forbids: it requires an entry "for a name the label's render will
read", naming a parameter "read only by a layout attribute" as exactly that case
(`openspec/specs/template-inputs/spec.md:33-36`).

So the `Text` arm gains `if let Some(DynamicValue::Ref(r)) = line_spacing { record_ref(r, false, false) }`,
the same call `font_weight` and `color` make. `interpolated: false` is the substantive half and is
correct rather than incidental: pitch is a layout attribute, nothing prints it, and that flag is what
tells the thumbnail and a client's own preview which names to invent a value for. A pitch parameter
must not be invented for, exactly as a colour reference is not — which is also why a template
referencing an undefaulted pitch has no thumbnail, the colour-reference precedent this design already
inherits.

The `when:` gate needs nothing added: the walker skips inactive items before the arm runs, so a pitch
reference inside a gated-off item contributes no entry while the same template with the gate true
contributes one. Both branches get a scenario, because the gate is what makes the walk
request-dependent and a field added on the wrong side of it stays invisible until an operator meets an
empty form.

No `template-inputs` delta is needed: its rule already covers a layout attribute's reference, and this
change adds an attribute rather than a rule.

### The resolved value is carried on `TextFit`, not passed to the emitter separately

Resolution happens in `Renderer::intrinsic`'s `Text` arm, before `layout_text` is called, so the fitter
receives an `Option<f32>` that is already resolved and `TextLayoutItem.line_spacing` keeps its current
type. `layout_text` records that value on its returned `TextFit`, and `render_text_item` reads it from
there; `TextRenderArgs.line_spacing` is removed, and `render_single_item` stops reading the item's
`line_spacing` at all.

Passing the resolved value to the emitter as a second argument would work and is a smaller diff. It
also leaves the emitter free to be handed a different value than the fitter used, which is precisely
the `font_weight` defect (#382): two resolutions of one field in two passes, one of them lenient.
Making the emitter's only access path the fitter's own output makes the published "the fitter SHALL
reserve and the emitter SHALL emit against the same pitch" a property of the types rather than a
property of two call sites agreeing.

Because the layout pass is unconditional and runs first, a missing, mistyped or out-of-range parameter
fails there, before any Typst source is emitted. A `when`-gated item that does not render is not
measured, so it is not resolved either, matching how its `value` interpolation already behaves.

### The bounds refusal gets its own `Reason`, `line_spacing_param_invalid`

Modelled on `ColorParamInvalid` (`src/errors.rs:278`, `src/reason.rs:83`): a new `Reason` variant, a
new `AppError` constructor built on `invalid_request`, so `400 InvalidRequest` with the slug in
`details.reason`. Message names the item's layout path and the parameter.

Reusing `RequestBodyInvalid` would collapse "you sent a pitch of 0" into "you sent something that is
not a number", and a client that wants to tell a caller which of the two happened could not. The slug
must appear in backticks in a published or active delta spec or
`spec_documents_every_reason_and_invents_none` (`src/errors.rs:664`) fails; the delta carries it.

The bound is checked once, on the resolved value, immediately after resolution. A literal cannot reach
it out of range because load refuses one, so in practice it fires only for a reference — but it is
written against the resolved value rather than against the reference, so it stays correct if a literal
ever gets a route around load validation.

### Resolution reuses `resolve_dynamic_value_f32`, and its string lenience is inherited, not blessed

The resolver already handles the reference: absent parameter → `missing_field`, a JSON number →
the number, a string → parsed, anything else → `RequestBodyInvalid` naming the parameter
(`src/render/helpers.rs:187-225`). Reusing it gives the missing-parameter and type-failure behaviours
the issue asks for with no new code.

Note where the type failure is actually raised, because it is not the resolver for most inputs.
`resolve_parameters` coerces every supplied value against its declared type *before* any item is
measured, and a `number` parameter that cannot coerce is refused there with `400 InvalidRequest` and
`request_body_invalid`, "parameter 'pitch' is not a valid number"
(`coerce_param_value`, `src/render/mod.rs:122-138`; the refusal, `src/render/mod.rs:372-378`). The
resolver's own `RequestBodyInvalid` arm is reached only by a value that coerced and then arrived as
something it cannot read — which, for a `number` parameter, is exactly the non-finite case below. Both
gates carry the same status, code, reason and parameter name, so the contract in the delta is stated
against that outcome rather than against either gate.

It also means a caller who supplies the *string* `"1.2"` for a `number` parameter gets pitch 1.2, and
one who supplies `"1.2mm"` gets 1.2 with the suffix stripped — which sits awkwardly beside refusing a
`length` parameter at load. Writing a pitch-specific resolver that demands a JSON number would remove
that, and would create a second numeric-resolution path that every other numeric reference does not
share. That is the worse trade: the lenience is a property of how this service reads numeric
parameters, not of this field, and it should be changed for all of them or none. The delta therefore
states the rule as "exactly the supplied forms numeric parameter resolution accepts" and does not
enumerate the suffix as a feature of pitch. Narrowing the shared coercion is a separate question and,
if wanted, a separate issue.

### A non-finite value is a type failure, not a bounds failure

**This departs from one bullet of the issue's acceptance list, deliberately.** #364 asks that a
supplied NaN or infinity refuse with the new slug alongside `0` and `-0.5`. It cannot, and the reason
is upstream of this field.

Parameter coercion turns a `number` or `length` parameter into `serde_json::json!(f)` from an `f32`
(`src/render/mod.rs:122-138`), and `serde_json`'s `From<f32> for Value` maps a non-finite float to
`Value::Null`. So a supplied `"NaN"`, `"inf"` or `1e300` all coerce "successfully" and then sit in the
resolved data as null, having lost the fact that they were numbers at all.
`resolve_dynamic_value_f32` reports null as `request_body_invalid` — "parameter 'pitch' is not a valid
number" (`src/render/helpers.rs:218`). The bounds check never runs, whatever order it is written in.

**A declared default takes a different route, and lands somewhere else.** The candidate is built
first, `ParamValue::Float(f)` → `json!(f)` (`src/render/mod.rs:531`), so a `.nan` default is already
null *before* coercion; coercion then rejects null for a `number` outright, and
`resolve_and_coerce_default` turns that into `AppError::param_default_unresolvable`
(`src/render/mod.rs:639`) — `422 TemplateInvalid` with `param_default_unresolvable`. That is not the
same response as the supplied case, and the first draft of this design wrongly called the two
equivalent. It is also exactly right: `param-resolution` already rules that a default and a supplied
value share *what counts as invalid* but not *who is told*
(`openspec/specs/param-resolution/spec.md:32-37`), and a default that cannot resolve is the template's
fault. This change preserves that contract untouched.

What the two routes leave is one asymmetry worth naming, since the delta states it: a `.nan` default
is the template's fault (`422 TemplateInvalid`), while a `default: 0` coerces fine, resolves like any
value, and is caught by the pitch bound as `400 InvalidRequest` — a caller-facing code for a fault the
caller did not commit. The bound cannot tell provenance apart, and the two ways to make it agree are
both worse: threading each value's origin through resolution, or bounds-checking the default at load,
which would refuse a template for a default that every other reference to that same parameter answers
at render. Stated in the delta rather than smoothed over.

Two ways to deliver the issue's literal wording were considered and both rejected:

- **Read null as "was non-finite" and raise `line_spacing_param_invalid` for it.** A null under a
  `number` parameter does appear to be reachable only through that coercion today, since a
  caller-supplied `null` fails coercion outright and is refused before this point. But the inference
  is the fragile kind: it decodes a cause from an absence, it holds only while `serde_json` keeps that
  `From` impl, and it makes pitch the one numeric field in the service that answers a non-finite input
  differently from every other. It would be exactly the "silent fallback" shape the project forbids,
  pointed the other way.
- **Make coercion refuse a non-finite number instead of nulling it.** This is arguably the real fix,
  and it is a change to how *every* numeric parameter behaves — including ones no layout item
  references, and interpolated `{n}` tokens that today print `null`. That is a separate issue,
  outside #364's accepted scope, and the plan review ruled it out for this change.

So the contract says what the service does: a non-finite supplied pitch is refused as a value that is
not a number, with `request_body_invalid`, naming the parameter. `line_spacing_param_invalid` is
reserved for a value that *is* a number and is not a usable pitch — `0` and any negative, which coerce
finitely and do reach the bound, along with a `default` of the same shape. Nothing the issue actually
protects is lost: no non-finite value renders, none reaches the fitter, none clamps and none panics.
What changes is which of two 400s the caller reads.

### An `integer` parameter saturates where a `number` one nulls, and pitch adds no ceiling

The analysis above is about `number`. `integer` coerces through a different arm, and that arm answers
an oversized value three ways depending on how it was written (`src/render/mod.rs:108-121`):

- **A positive JSON number.** `n.as_i64()` fails on a float, so `n.as_f64().map(|f| f.round() as i64)`
  runs, and a float-to-integer `as` cast in Rust saturates: `1e300` becomes `i64::MAX`, not null.
  `json!(i64::MAX)` is an ordinary JSON number, `resolve_dynamic_value_f32` reads it as a finite `f32`
  around `9.2e18`, and it passes the bound.
- **A negative JSON number.** The same cast saturates toward the other end, so `-1e300` becomes
  `i64::MIN` and arrives as roughly `-9.2e18` — finite, and negative. It fails the bound and is
  `line_spacing_param_invalid`, exactly as a supplied `-0.5` is.
- **A string.** The string arm is `s.trim().parse::<i64>()`, which has no saturation:
  `"9223372036854775808"` overflows `i64` and fails to parse, so the value is refused at parameter
  resolution as not a valid integer — `request_body_invalid`.

The first draft promised `request_body_invalid` for any magnitude beyond a parameter's range, which is
true for `number` and false for `integer`; the second promised that any oversized `integer` became a
positive legal pitch, which is true only for the first case above. The fix is to state the three
outcomes and add no ceiling. A pitch of `9.2e18` is a legal pitch by the same rule that makes `1e9`
one: finite and greater than zero. What it produces is a block that does not fit its box, which is the
`overflow` rule's job and is already specified — `fail` refuses with `text_does_not_fit`, `ellipsis`
truncates. Inventing a maximum pitch here would be a bound the issue never asked for, would need a
number nobody can justify, and would refuse templates that authors can already write with a literal.

The saturation itself is shared coercion behaviour affecting every `integer` parameter, `font_weight`
included, and is not this change's to alter.

**This is why verification runs through the endpoints.** Every refusal scenario in the delta is stated
against `POST /api/render/label` or `POST /api/batch` for this reason: a test that called the numeric
helper directly would hand it an intact `f32::NAN`, watch the bound fire, and prove a behaviour no
request can produce. The task list must not admit a unit test as evidence for any of them.

## Risks / Trade-offs

- **The emitter no longer reads the item, so a future caller could build a `TextFit` without a pitch
  and get 1.2 silently.** → `TextFit`'s field is the same `Option<f32>` the existing helpers already
  interpret as "absent means 1.2", and `layout_text` is the only constructor. The risk is a new
  constructor, which the diff review would see.
- **A template whose pitch parameter declares an unusable default loads and then fails to render**, and
  the two kinds of unusable default fail differently: `.nan` as `422 TemplateInvalid`, `0` as
  `400 InvalidRequest`. → Accepted, specified and given a scenario each. Both surface as a broken
  thumbnail on the template list immediately, the same signal the colour-reference case already gives.
- **Adding a field that reads a parameter is also a change to input discovery, and nothing fails
  loudly if that half is skipped**: the template loads, the render demands a value, and the form
  offers none. → One `record_ref` call in the walker's `Text` arm, with a scenario for a pitch-only
  parameter and one for a gated-off item, so the omission fails a test rather than an operator.
- **An oversized `integer` pitch has three answers, not one**: a positive JSON number saturates to a
  finite legal pitch and meets the `overflow` rule, a negative one saturates to a negative pitch and
  meets the bound, and an oversized *string* fails integer parsing and is refused as not a value of
  its type. → None is a defect of pitch; each is what the declared type already does with that input.
  The delta enumerates all three with a scenario each and promises no magnitude ceiling. Changing
  `integer` coercion's saturation would change every `integer` parameter and belongs to whoever takes
  that on.
- **`models.rs`'s `line_spacing` type change ripples into every `LayoutItem::Text` literal in the
  tree** (`src/batch.rs:237`, and the fixtures at `src/templates.rs:3827` and following). → All are
  `line_spacing: None` and unaffected in meaning; the compiler finds every one.
- **The OpenAPI schema for the field changes from a number to a literal-or-reference union.** → It is
  the shape `font_weight` and every dynamic extent already publish, so no client meets a new pattern;
  and a client sending a bare number is unaffected.
- **Load and render disagree on which `check_param_ref` types are legal.** Load refuses a `length`
  parameter, render would happily resolve one. → Only load can see the declaration, so this is not
  drift but the single point of enforcement; the render-time bound covers what load cannot know.
- **The new reason fires for a narrower set of inputs than #364 lists**, because non-finite values are
  already gone by the time pitch is read. → Decided and recorded above with its evidence, and stated
  in the delta so the published contract matches what the service does. If the shared coercion is ever
  changed to carry or refuse a non-finite number, this field's contract narrows to "zero or negative"
  by itself and needs no edit: the bound is written against the resolved value, not against the route
  the value took.
- **A test that exercises the bound through the helper instead of the endpoint proves nothing.** →
  Every refusal scenario names `POST /api/render/label` or `POST /api/batch`; this is the finding that
  sent the first plan back, and the task list inherits it.

## Migration Plan

None. A bare number keeps every #363 behaviour, no existing template declares a reference, and no
stored data carries the field. Rollback is reverting the commit.
