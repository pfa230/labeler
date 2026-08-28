## Context

See `proposal.md` for motivation. The state that shapes the approach:

- `GET /templates/{id}` serves the whole layout tree and `params`, and nothing else about inputs. It
  has no `options` key: `TemplateDetail` (`src/models.rs:67`) does not carry one, and
  `TemplateDefinition::options()` (`src/templates.rs:32`) is used only to validate a submitted
  `option` map at render time. Verified against a running instance.
- The renderer decides visibility in `RenderContext::is_item_active` (`src/render/mod.rs:920`),
  comparing `value_to_string` of the *resolved* parameter against the `when:` value. Resolution
  (`resolve_parameters`, `:38`) merges request `data` over declared defaults and coerces by declared
  type: `"0"`/`"1"`/`"true"`/`"false"` to a boolean
  (`:121-156`), a decimal string to an `i64` (`:158-182`), a string with an optional `mm`/`in` suffix
  to an `f32` (`:184-215`), an `enum` of any JSON type through `value_to_string` before its
  membership check (`:106-119`), nothing at all for a `string` (`:215`), and an absent or blank
  `datetime` to the request's captured instant (`:53-103`). A value it cannot coerce is rejected: an
  out-of-range `enum` as `422 InvalidOptionValue`, everything else as `400 InvalidRequest`
  (`src/errors.rs:240`).
- That coercion runs over **every declared parameter**, before any gate is evaluated:
  `compile_label_doc` calls `resolve_parameters` at `src/render/mod.rs:350` and only reaches
  `is_item_active` when it walks items. So an invalid value rejects a render even when the item that
  reads it is inactive. `docs/SPEC.md` §5's lazy rule covers an omitted parameter, not a supplied
  invalid one.
- `resolve_parameters` is the reason no client can decide a gate on its own. To match
  `is_item_active` a client must reproduce that whole table, including Rust's rendering of an `f32`
  widened to `f64` and the server's captured instant in the server's timezone.
- The `option` half of that merge is dead. `LabelInput` is `{ data }` (`src/models.rs:780`) with no
  `deny_unknown_fields`, so an `option` key on a label is silently dropped, and both render paths pass
  `None` for the option selection (`src/api.rs:2295`, `:2306`, `src/batch.rs:93-103`).
  `normalize_option` (`:773`) and `is_item_active`'s `selected_option` fallback (`:926`) are
  unreachable outside tests. The UI still builds and submits the map, which is #214.
- Four field walkers exist today: `templateFields.ts`'s tree walk, `walk_placeholder`
  (`src/render/mod.rs:2107`) feeding both `placeholder_data` for thumbnails and `template_fields` for
  the catalog-index binary, and the renderer itself. Only the last is correct. `walk_placeholder`
  reads `image.src` tokens, which `templateFields.ts` does not, and ignores `when:`, which the
  renderer does not.
- `interpolate` (`src/render/helpers.rs:43`) is the only implementation of the `{token}` grammar,
  including `{{`/`}}` and the resolution order datetime → declared `datetime` parameter → `vars.` →
  `data`. It scans and resolves in one pass and errors on a malformed brace. `image.src` goes through
  it too (`src/render/mod.rs:1649`).
- `useLivePreview` (`ui/src/lib/livePreview.ts`) already POSTs a full server-side render on every
  value change, debounced 300ms, keyed, cached and abortable. The page's per-change round trip is a
  Typst compile.
- Templates are immutable and `Arc`-shared; the registry is rebuilt wholesale on reload.
- Grid cells hold every edited value as a string (`ui/src/components/LabelGrid.tsx:28-37`).
- On the UI side `detail.options` is always `undefined`, so `PrintForm`'s `value.option` is always
  `{}`. Import and Connect reconstruct option columns from enum parameters. Cleaning that up is #214.

## Goals / Non-Goals

**Goals:**

- One implementation of "which inputs does this label need", in the service, sharing its resolution
  and its visibility rule with the renderer.
- Leave the client with nothing to decide, so there is nothing for it to get wrong and nothing to
  drift.
- End the duplication rather than reduce it: four walkers become one.

**Non-Goals:**

- No change to how the renderer evaluates `when:`, to `docs/SPEC.md` §5, to any error code, or to the
  template YAML schema. Nothing about `when:` is narrowed.
- Not retiring the per-label `option` map (#214). The preview's sample values (#215) are *not* a
  non-goal: the thumbnail shares the walker being rewritten and has the same defect, so one rule
  covers both and #215 is subsumed.
- Not making the form's *validity* rules server-side beyond `required`. Shape checks that are already
  client-side, such as `datetimeCellError`, stay client-side.

## Decisions

### The service answers; the client renders

An earlier draft of this change shipped a manifest of `when:` conditions and had the client evaluate
them. That was rejected on review, twice, and correctly: to evaluate a gate the client must reproduce
`resolve_parameters`' coercion table, which is a second implementation of the thing this change
exists to delete, and two of its cases (a `number`/`length` float, a blank `datetime`) are not
reproducible in a browser at all.

The objection to asking the service was latency. It does not survive contact with the code: the print
form already fires a Typst compile through `useLivePreview` on every value change. An input list is
orders of magnitude cheaper than what the page does on the same trigger, with the same debounce, key,
cache and abort machinery already written and tested next to it.

So the wire carries answers, not conditions. `control`, `required`, `default` and the rest of the
control's metadata are all computed service-side. The client never learns that `when:` exists.

`control` splits `integer` from `number` rather than collapsing both into one numeric kind, because
`ParamInput` steps an integer by 1 and reads it with `parseInt`, and steps a float freely and reads it
with `parseFloat` (`ui/src/components/ParamInput.tsx:105`, `:129`, `:157`). A single `number` control
would send the client back to `ParamSpec` to tell them apart. `slider` is then a presentation flag,
true exactly when both bounds are declared, which is the other thing that file branches on (`:104`).

`default` is on the entry for a specific reason: a slider, a checkbox and a select all need the
effective value to render their state, and `ParamInput` reads it from `ParamSpec` today
(`ui/src/components/ParamInput.tsx:108-138`, `:168-195`). Without it on the entry, the client would be
back to deriving from `params`, which is the thing being removed.

`control` is decided declaration-first, use-second, because that is what the print form does today:
`FieldForm` renders a declared parameter straight from its `ParamSpec` and `ParamInput` picks a
textarea from `spec.multiline` (`:45`), not from how a layout item reads it. Preserving that keeps
the existing `datetime-params` parameter contract intact, and leaves `truncated_elsewhere` as the
warning for a declared single-line field a multiline item reads, exactly as `docs/SPEC.md` §4.1
describes. Use decides the control only for a name the template does not declare, plus the one
override that already exists in the UI: an `image` binding turns a `string` parameter's control into
a file picker (`ParamInput.tsx:45-46`).

### The request carries `data` and nothing else

The endpoint takes the label shape `/api/batch` takes, which is `{ data }`. An earlier draft called it
`{ data, option }`; that was wrong, and the way it was wrong matters. The `option` channel does not
reach the renderer at all, so an endpoint that honoured it would report one branch while `/api/batch`
drew another. Ignoring it, exactly as the render paths do, is what keeps the answer true. The UI may
keep posting the key until #214 removes it; it changes nothing on either path.

### Two surfaces, chosen by what the caller has

`POST /api/templates/{id}/inputs` takes labels and returns a list each. `GET /api/templates/{id}`
carries `inputs.default` and `inputs.all`.

The GET fields exist so first paint costs no round trip and so the two views that describe a
*template* rather than a label (the detail page's field list, Connect's mapping palette) are served
without inventing a label. The thumbnail and the preview use `inputs.all` instead, for the closure
reason below.

Rejected alternative: a `GET /templates/{id}/inputs?mode=default|all`. It is a third surface for data
the client already fetches, and it would make the common path two requests where one will do.

### Lenient resolution on the inputs path, strict on the render path

This is the one real subtlety. `resolve_parameters` rejects a value it cannot coerce, and a form is
half-filled by definition. If the inputs endpoint inherited that, the answer would be an
error precisely when the operator most needs the field list.

### A screen submits the names it is showing, and nothing else

Rendering coerces every declared parameter before it gates anything, so a value left over from a
branch the operator has switched away from still fails the render. An earlier draft said such a value
was retained and submitted because "the renderer ignores it"; the renderer does no such thing. The
value is retained in the screen's own state, so switching back restores it, and dropped from the
submitted `data`.

The same asymmetry decides what an empty control means. `ParamInput` writes `""` when a numeric field
is cleared (`ui/src/components/ParamInput.tsx:155`), and `""` fails the `i64` and `f32` parses, so a
cleared numeric field with a declared default currently produces a `400` rather than the default.
Omitting an empty value for the non-text controls fixes that as a side effect of the rule, and the
rule is decided from `control` alone so no screen needs the declared type to apply it.

### Lenient resolution, precisely

So the inputs path resolves leniently, and leniently means one thing only: a value that fails
coercion is treated as though the label had not carried that name. Everything after that is the
ordinary omission path, so the parameter takes its declared default, or `false`, or the first `enum`
value, or the request's instant, and gates are evaluated against *that*. An earlier draft said the
value both fell back to its default and left every gate on it unmatched, which cannot both be true;
this is the coherent half.

`required` stays a property of the declaration, not of the value, so an `enum` is `required: false`
whether its value is valid, invalid, or absent. That is what `hasServerDefault` already encodes, and
it keeps a blank grid cell behaving exactly as it does today.

Rendering keeps rejecting the value it always rejected, with the code it always returned: an
out-of-range `enum` is `422 InvalidOptionValue`; an uncoercible number or boolean, and an unparseable
`datetime`, are `400 InvalidRequest` through `AppError::invalid_request` (`src/errors.rs:240`), the
datetime case carrying reason `datetime_param_invalid` (`src/render/mod.rs:78-102`); and `/api/batch`
wraps a per-label failure as `422 BatchInvalid` carrying that label's own code. An earlier draft
claimed `422` for all of these; only the enum case was right.

Implementation shape: `resolve_parameters` grows a strictness mode rather than being copied, so the
lenient and strict paths cannot diverge in the merge order or the defaulting rules, which is where a
copy would rot first.

Rejected alternative: have the endpoint return partial success per label. It adds an error shape to
every caller for a condition that is the normal state of a form.

### The service's own walkers are deleted, not left alongside

Shipping an input list while keeping another walker in the same binary would leave exactly the
condition that produced #200, one file away instead of one process away. So `walk_placeholder`,
`collect_data_tokens`, `placeholder_data` and `template_fields` all go: the thumbnail
(`src/api.rs:942`) fills from `inputs.all`, and the catalog-index binary
(`src/bin/catalog-index.rs:87`) takes the `required` names of `inputs.all`, which is the rule its
doc comment already states.

This changes thumbnail output for a template with branches: today it invents data for every branch.
That is a fix, and it is specified.

The fill set is built from `inputs.all`, not `inputs.default`, and that is the subtle part. A value
the thumbnail invents is part of the request, so it can decide a gate: a required `string` that some
item prints and some container gates on gets filled with its own name and activates the branch it
names. Filling from `inputs.default` would leave that branch's names unfilled and the render would
fail for missing data, so the rule would not be closed under its own injections. `inputs.all` closes
it, costs an unread key for a branch that does not draw, and matches what `walk_placeholder` did,
since that walker ignored gates outright. Drawing stays gated, because the renderer gates it.

The template preview takes the same set for the same reason. It is the same walker's output filling
the same kind of request, so specifying one closed rule and one open one would be specifying a bug.

What is invented is decided by `control`, and the numeric case is the one the old walker got wrong in
both directions: `placeholder_data` fills a required `length` with its own name, which fails
coercion, and leaving it empty fails with `MissingField` instead, since only `boolean` and `enum`
have a type fallback (`src/render/mod.rs:219-240`). So a required `integer` or `number` is filled with
its declared `min`, or `1`, which is coercible and inside any declared range. That is what makes the
#215 closure claim true for the numeric half as well as the enum half.

The fill rule narrows with it, and the narrowing needs a field on the entry. `walk_placeholder`
collects only value tokens and image bindings, so a gate key has never been invented for. The input
list reports gate keys, and a name in `data` beats a declared default, so filling by `control` alone
is not enough: a `string` parameter gating on its own default has control `text`, and filling it with
its own name selects the wrong branch silently. Hence `interpolated`, which says whether an active
item reads the name as a value rather than only gating or sizing with it. Combined with `required`,
which keeps a parameter that resolves on its own from being overridden by a stand-in that is usually
illegal, the three-part rule reproduces exactly the set `walk_placeholder` filled and drops the ones
it never could.

The `required` half also closes #215: the preview invents by the same rule, and its bug was inventing
`"orientation"` for a printed enum. Keeping the preview on the old rule while writing the new one for
the thumbnail would mean specifying a known defect.

It improves the interpolated `image.src` case without claiming to solve it. `placeholder_data` gives
every image-ish name a 1×1 PNG data URI, but a name read through `src` is a *path* into the assets
root, so a data URI can never resolve. The new rule gives it the `text` fill instead, its own name,
which resolves when an asset of that name exists (`resolve_image_asset`, `src/render/helpers.rs:924`)
and otherwise fails with the same asset error as today. Nothing in `catalog/` or
`tests/fixtures/templates/` uses an interpolated `src`.

### One derivation, over the same walk validation already performs

The derivation walks the template model once, gathering, per item: its `when:` keys, its
`DynamicValue::Ref` attribute references, and the tokens of every interpolated string, which is
`text.value`, `qr.value` and `image.src`, plus `image.name` as a direct data key. The `format`'s
dynamic dimensions are gathered ahead of the layout. That is the same set of sites
`validate_layout_item` (`src/templates.rs:935-1015`) and the `format` check (`:263-299`) already
visit.

The token scanner is lifted out of `interpolate` rather than written again: `interpolate` scans and
resolves in one pass, and the derivation needs the scan plus a classification. The lifted scanner
yields each token and reports whether the string was well formed; `interpolate` keeps its behavior by
erroring on the report, and the derivation ignores it, because a malformed brace is a render-time
failure and not a reason a template's detail request should fail. Classification then walks the same
order `interpolate` resolves in, so a template that also declares a data key called `datetime` cannot
make the two disagree about which wins.

### Drift guards

The duplication is gone, so the guards are about coverage rather than agreement:

1. **Every reference site.** Over every template in the test corpus, every parameter name that
   validation checks, in both the `format` pass (`src/templates.rs:263-299`) and the layout pass
   (`:935-1015`), appears in `inputs.all`. A new reference site added to validation and not to the
   derivation fails here.
2. **Every input category, asserted whole.** One fixture exercising all five item types, nested and
   sibling gates, `image.name`, `image.src`, a `font_weight` ref, a dynamic `size` on each item type
   that takes one, and a dynamic `format` dimension. Its `inputs.default` and `inputs.all` are
   asserted against literals, so a category that stops being emitted fails rather than silently
   shrinking the list.
3. **The list matches what the render reads.** For that fixture, across several labels, every entry
   the endpoint reports is a name the render of the same label actually resolves, and every name the
   render resolves from request `data` is reported. This is the invariant the whole change exists to
   hold, and it is checkable because both sides now live in one process.
4. **The thumbnail's fill set is closed under its own injections.** For a fixture where a placeholder
   value decides a gate (a required `string` both printed and gated on), the thumbnail renders rather
   than failing for missing data. This is the invariant that forced the fill set to `inputs.all`, and
   nothing else in the suite would catch its loss.
5. **The lenient and strict paths agree on everything except acceptance.** For each coercion case (an
   out-of-range `enum`, a non-numeric `integer`, an unparseable `datetime`, a blank `enum`), the
   endpoint answers `200` with the list the same label would get had it omitted the name, and the
   render returns the code named above. A case that starts erroring on the inputs path, or that
   changes a render's code, fails here.

### The UI, after

`templateFields.ts` keeps only what was never about the layout: `datetimeCellError`, the local date
formatters, `reconcileRowOptions`, `hasServerDefault`. The walk, `referencedFields`, `imageFields`,
`multilineFields`, `singleLineTextFields` and `referencedVariables` all go.

A `useLabelInputs` hook mirrors `useLivePreview`: debounce, a key over the label bodies, an LRU cache,
an abort on supersede. It returns the previous list while a request is in flight, which is what keeps
controls from flickering mid-keystroke.

A grid's columns and its controls are different sets, and the spec separates them: columns are the
union across rows so the table keeps a stable shape, while each cell follows its own row's list and is
inert when its name is not on it. `LabelGrid` takes one global `fields` list today
(`ui/src/components/LabelGrid.tsx:67`) and renders an editable cell for every field on every row, so
it gains a per-row predicate. Without that, a union column would silently reintroduce exactly the
over-asking this change removes.

The grids batch: one request carrying every row whose key is not already cached, so a 500-row import
asks once and a single-cell edit asks for one row. A run is blocked while any row's list is
unresolved, which is a stricter gate than today and is what stops a stale required-set from letting
an incomplete label through. The server still validates on submit, so the existing per-row `422`
annotation path remains the backstop.

`FieldForm` becomes a renderer of `InputSpec[]`: no `declaredParams` loop, no `fallbackFields`, no
`fallbackOptions`. `PrintForm`'s validity check becomes "no `required` entry is empty", seeded from
each entry's `default`.

`TemplateDetail.tsx` and Connect's mapping palette read `inputs.all` and `variables` off the detail
response they already fetch, so neither needs the new endpoint at all. The palette's set changes:
today it is `referencedFields(layout, {})` unioned with every declared parameter name, so it offers
parameters the template never reads. `inputs.all` drops those, which is the same narrowing the CSV
Import requirement records.

## Risks / Trade-offs

- **A control the operator needs disappears.** The service decides, so this is a derivation bug rather
  than a client bug, and guard 3 is aimed at it. `when:` keys and attribute references are reported
  as inputs precisely so the branch-selecting and box-sizing controls survive gating.
- **A declared but entirely unreferenced parameter loses its control**, where today it is shown and
  required, and the frozen CSV Import paragraph promised every declared parameter. → Specified, with
  an `ADDED` requirement superseding that paragraph. It matches the renderer, which never asks for
  such a parameter.
- **The grids become asynchronous where they were synchronous.** `validateRow` runs during render
  today. → Blocking a run on an unresolved list is the mitigation, and it is specified with a
  scenario. The failure mode of getting it wrong is a stale required-set, which the server's `422`
  still catches on its existing path.
- **A datetime gate can disagree between first paint and first submit.** `inputs.default` is computed
  against the service's instant; the form seeds its `datetime` controls from the browser. → The form
  requests a list for its seeded label before treating it as complete, specified with the
  first-paint rule. `inputs.default` is a first paint, not an answer about a label.
- **Dropping unshown values from the request is a behavior change.** A caller that relied on a hidden
  field reaching the renderer loses it. → Nothing read it: the item that referenced it was inactive.
  What it could do was reject the render, which is the bug being fixed.
- **A network failure now affects the form.** → Specified: keep the last list, fall back to
  `inputs.all` when there is none, surface the failure, do not block submission. That fallback is
  *not* the status quo, and an earlier draft wrongly said it was: today the form renders every
  declared parameter (`ui/src/pages/print/FieldForm.tsx:68`) and requires every one without a server
  default (`PrintForm.tsx:60`), while `inputs.all` drops parameters the layout never reads. The
  degraded mode is therefore every referenced input, ungated, which is a superset of any branch and a
  subset of today's list.
- **Thumbnail output changes** for a branching template. → A fix, specified, and the fixture corpus
  will show it.
- **Request volume.** One list request per selection change, one per grid commit. → Smaller than the
  render the same events already trigger, and cached by key.
- **Import and Connect keep showing an enum parameter twice**, and the option column of that pair
  still does nothing, because the map it writes is discarded server-side. → Pre-existing and
  unchanged by this design; #214. Worth stating plainly because an earlier draft of that issue, and
  of this design, claimed the renderer merged the two maps. It does not.
- **The mapping palette narrows.** A connector mapping saved against a parameter the template never
  reads has nothing to target after this change. → Such a mapping never reached the renderer either,
  since nothing read the parameter; the palette stops offering a target that did nothing.

## Migration Plan

Additive on the wire: one new endpoint, two new fields on the template-detail body. A client that
ignores them behaves as before, and the same binary serves the API and the SPA, so there is no skew
to sequence. No template becomes invalid, no error code changes, no stored state is touched. Rollback
is reverting the commit.

## ADR

New **ADR-0070**: *Field discovery is the service's answer, not the client's derivation.* It records
that the service reports which inputs a given label needs, that clients render that answer rather
than deriving it, that the gating path resolves leniently while rendering stays strict, and that the
thumbnail's placeholder walker is folded into the same derivation instead of surviving beside it.
Supersedes nothing; ADR-0056 introduced `when:` and this changes only who evaluates it for a client.
