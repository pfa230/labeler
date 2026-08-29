## Context

See `proposal.md` — Why. This plan is written against `main` at `de11197`, which merged #200 ("Let the
service decide which inputs a label needs"). #200 moved input derivation to the server and deleted the
client's copy of it, and that changes several of this change's targets. What the code looks like now:

- **The inference sits in two places, and one is new.** `apply_param_default` (`src/render/mod.rs:286-312`)
  holds the `boolean` → `false` and `enum` → first-value branches, called twice from
  `resolve_parameters_mode`'s catch-all arm (`:270`, `:274`). The `datetime` → render-instant branch
  is separate and inline (`:70-95`), and it never consults `spec.default` at all, because a `datetime`
  could not declare one. Separately, `derive_inputs_internal` (`src/templates.rs:389-401`) *publishes*
  the same inference as data: `required` excludes `Boolean | Enum | Datetime` from being required, and
  `default` fills `Boolean(false)` and `values[0]`. That is the half #200 added.
- **Three production callers of `resolve_parameters`, not two.** `compile_label_doc`
  (`src/render/mod.rs:412`), the sheet loop (`:705`), and — new — `TemplateContent::derive_inputs_for_label`
  (`src/templates.rs:145`), which calls it in `ResolveMode::Lenient` behind
  `POST /api/templates/{id}/inputs`.
- **`placeholder_data` moved and lost its walker.** It is now `TemplateContent::placeholder_data`
  (`src/templates.rs:156-179`), built from `inputs_all()` and gated on `input.interpolated && input.required`,
  inventing by `control`: the sample PNG for `image`, the name for `text`/`textarea`, `min` or `1` for
  `integer`/`number`, and `_ => continue` for everything else. It takes **no clock**. `walk_placeholder`
  and `template_fields` are gone; `derive_inputs_internal` (`src/templates.rs:183-478`) is the one
  derivation, serving the thumbnail, the detail response and the catalog index.
- **A `datetime` is invisible to the preview today only because it is never required.** `required` is
  forced false for it, so `placeholder_data`'s gate skips it and the render-time instant fills the gap.
  Make it required and the gate admits it, and `_ => continue` then leaves it unfilled.
- **`default_option_selection` survives unchanged** (`src/render/mod.rs:1860-1869`), still built from
  `TemplateContent::options()` (`src/templates.rs:89-100`) over every declared `enum`, still passed by the
  thumbnail handler (`src/api.rs:1212-1215`). It is a third, preview-only first-value mechanism and this
  change leaves it alone.
- **`MissingField` is still raised late.** Nothing marks a parameter required at render time;
  `interpolate` (`src/render/helpers.rs`) raises it when a token has no value, and inactive `when:`
  branches are skipped before they are measured, so required-ness stays a property of one request's
  active layout.
- **The client no longer derives, but it still infers.** `initialParamValues` and `defaultParamValues`
  are gone; `hasServerDefault` and `reconcileRowOptions` survive with no caller outside their own test.
  The live inference is `initialDataFromInputs` (`ui/src/pages/print/PrintForm.tsx:22-37`), whose
  `date`/`datetime` branch is checked **first** and unconditionally seeds the browser clock, ignoring
  `input.default` — the client twin of the render-instant fallback. Its `checkbox` → `false` and
  `select` → `values[0]` branches, and `ParamInput.tsx:165-167` and `:190`, are dormant only because the
  server currently sends those defaults; they reactivate the moment it stops.
- **The client preview mirrors the server's gate.** `sampleData` (`ui/src/lib/preview.ts:12-26`) filters
  `inputs.all` on `interpolated && required` and has no branch for a `date`/`datetime` control, so an
  input this change makes required arrives with no value.
- **The grids are unchanged and still mismatched.** `resolveLabels` (`ui/src/lib/labelGrid.ts:42-56`)
  emits a sibling `option` object, and `LabelInput` (`src/models.rs:929-932`) has only `data` and no
  `deny_unknown_fields`, so serde drops it silently (#214).
- **`TemplateInvalid` already exists at `422`** (`src/errors.rs:259-267`) with six reasons in
  `src/reason.rs:33-38`, and the reason registry is the `reasons!` macro there, not in `errors.rs`.

## Goals / Non-Goals

**Goals:**

- One resolution rule for the value a token reads, for every parameter type, in one place.
- A default is a declaration the model holds, expressive enough that `datetime` needs no exemption.
- A broken default is attributed to the template rather than to the caller.

**Non-Goals:**

- **Publishing a resolved default on the template detail endpoint (#262).** Filed separately, with the
  UI diagnostic contract that goes with it. See decision 6.
- **#236's "Use default" checkbox.** How the form *offers* deferral to an operator is that issue's.
- **#215's preview enum placeholder.** Untouched — but see the risk below: this change invalidates the
  remedy #215 currently proposes.
- **#238's `min`/`max` enforcement.** A resolved default is validated exactly as a supplied value is,
  which today means `min`/`max` go unenforced for both. That stays equally unenforced.
- **The numeric `ref:` geometry fallbacks.** `load_geometry_values` and `resolve_f32_default` /
  `resolve_u16_default` (`src/templates.rs:1514-1529`, `:1531-1544`, `:1546-1556`) and their render-time
  twin (`src/render/mod.rs:927-946`) are a *second*, independent silent-default mechanism, deriving a
  value from `min`, then `max`, then `0.0`, or `400` for a `u16`. It is the same defect this issue names,
  in another place. It is out of scope, and `param-resolution`'s first requirement names it with its
  file:line evidence rather than asserting an absolute the merged tree would falsify. It is tracked as
  **#261**, filed rather than left as planning prose.
- **Removing the preview's default option selection.** It is documented behavior (`docs/SPEC.md` §2.0, `:116-117`),
  it is preview-only, and dropping it would blank every gated branch out of every catalog thumbnail.
- **Reworking what the input list reports beyond `required` and `default`.** #200's derivation decides
  `control`, `interpolated`, ordering and much else; this change touches the two fields that encode the
  removed inference and leaves the rest alone. The `template_fields` walker that once excluded `datetime`
  from an advertised field list no longer exists, so the exclusion this change was going to remove is
  already gone.
- **Any template migration.** Stated in the proposal and not revisited.

## Decisions

### 1. A default resolves through the existing `interpolate`, not a second resolver

`resolve_parameters_mode` gains the same `variables` and `DateTimeResolver` the render path already
threads into `interpolate`, and `apply_param_default` calls it on a `ParamValue::String` default. The two
**render** callers pass them: `compile_label_doc` and the sheet loop. The third caller,
`derive_inputs_for_label`, passes no variables map and resolves no tokened default. The
missing input is the **variables store**, not a clock: the endpoint above it already captures an instant
and threads it in (`src/api.rs:1276-1279`), but never reads `all_variables()`, and its other entry point
is a synchronous `From<&TemplateDefinition>` with no application state at all
(`src/templates.rs:2110-2129`), so neither could supply one without a new dependency. `template-inputs` states that and accepts the gate divergence it causes;
#262 is what closes it. A default that is not a string
carries no token and is used as written.

*Alternative rejected:* a purpose-built substituter for defaults. Two implementations of one grammar is
exactly the drift `interpolation-tokens` was written to end, and #150/#155 are this project's record of
what happens when one rule has two implementations.

`interpolate` raises `AppError::missing_field` for an absent variable (`src/render/helpers.rs:123`) and
for a format attached to a `vars` key (`:117-118`), and `DateTimeResolver::format` raises it for an
unknown format name (`src/datetime_fmt.rs:101-115`). The bare-token-with-a-format branch (`:128-130`)
cannot fire inside a `default:`, because a bare token there is refused at load. Inside a default those must be **remapped** to
`param_default_unresolvable` before they leave `resolve_parameters`; the caller did not omit
`vars.base`, the template did.

### 2. Namespaced-only is enforced in two places, each where it can be — and only over well-formed tokens

- **The bare-token refusal** goes in `TryFrom<RawParamSpec>` (`src/convert.rs:227-346`), beside the
  other per-attribute refusals. It needs no template context: `scan_tokens` the default, `parse` each
  token, refuse `Source::Bare` with a `TemplateError::Validation { path: "default", .. }`. Both the
  quarantine path and the `POST`/`PUT` refusal already funnel through this conversion.
- **Everything else the grammar refuses** — unknown source, unknown `sys` value, malformed name, a
  format attached to something that is not an instant — belongs in `templates.rs`'s `validate`, beside
  the existing per-string token validation, extended to iterate `params[*].default`. The
  format-on-a-non-instant rule needs to know whether the token names a `datetime` parameter, which is
  template context the converter does not have.

*Alternative rejected:* refusing bare tokens at render. A template could then be saved that can never
resolve, and the point of a namespaced-only rule is that it is structural.

### 3. `datetime` loses its arm in `resolve_parameters` and keeps its `instants` entry

**Removing that arm makes a dead branch live, and it reports the wrong name.** `interpolate`'s bare-token
arm raises `AppError::missing_field(inner)` when a token carries a format and no instant is registered
(`src/render/helpers.rs:128-130`), where `inner` is the token text *including* the format — so an omitted
`printed_on` read as `{printed_on:short_date}` is reported as a missing field named
`printed_on:short_date`, in the message and in `details.field` (`src/errors.rs:211-218`). That is not a
`data` key any caller can add, and the UI maps errors by `details.field`. The branch is unreachable today
only because every declared `datetime` is unconditionally given an instant by the arm this decision
deletes. So the change must also make it raise `missing_field(name)`, and the test must assert the field
name rather than the status code — asserting the code alone passes against the broken behaviour.


The `ParamType::Datetime` branch currently owns omission, blank, `null`, override parsing, and instant
capture. After the change it owns only value parsing: a supplied value, or a resolved default, is parsed
by `parse_datetime_in_tz` and its result recorded in `instants` so `{p:<fmt>}` still formats. Omission,
blank and `null` all become "absent", handed to the same default lookup every other type uses.

The blank-and-`null`-are-omission rule stays because it is about *reading a value*, not about
defaulting: a cleared date control submits `""`, and the form should not have to distinguish that from
not sending the key.

A **non-string** `default:` on a `datetime` is refused at load rather than left to fail per request. It
can only ever fail: this capability defines no epoch convention, so `default: 20260819` parses as a
number and `parse_datetime_in_tz` rejects it every time. The refusal is decidable without a request, so
it belongs at load, where the template is quarantined and the parameter is named.

**Order matters here.** `RawParamSpec.default` is presence-preserving, so an explicitly written
`default:` with no value arrives as `Some(Value::Null)` (`src/raw.rs:24-30`), and the delta requires that
to be an *absent* default rather than a refusal. The datetime branch today rejects on
`raw.default.is_some()` before any of that (`src/convert.rs:238-243`), while null is collapsed to `None`
only on the non-datetime path (`:322-324`). So the collapse must happen **first**, for every type, and
the non-string refusal applies to what survives it.

### 4. `default: "{sys.now}"` means local midnight, and no new built-in format is shipped

`{sys.now}` renders `%Y-%m-%d`, so a `datetime` default of `"{sys.now}"` resolves to local midnight of
the render date. For `{printed_on:short_date}` that is indistinguishable from today's behavior; for a
`time: true` parameter it prints `00:00`. A template needing the time of day attaches a format whose
output the parser accepts, meaning a `datetime_formats` entry producing `YYYY-MM-DDTHH:MM[:SS]`. None of
the five shipped formats does (`src/settings.rs:21-29`: `iso_date_time` is `%Y-%m-%d %H:%M`, space-
separated, and the parser requires `T`).

An earlier draft of this design added a sixth built-in to close that. It is cut, for three reasons:
`resolve_datetime_formats_from` (`src/settings.rs:147-161`) returns the built-in map **only when nothing
is stored**, so a stored override replaces it wholesale and any deployment that has ever edited the
setting would not receive the new entry — the promise would be false exactly where it was aimed;
nothing in #241 requires second precision; and a sixth built-in changes what `GET /api/settings` and the
settings UI list, which no requirement here covers.

*Alternatives rejected:* teaching `parse_datetime_in_tz` to accept a space separator, which quietly
widens what every *request* may send for unrelated reasons; and special-casing a `{sys.now}` default on
a `datetime` parameter to bind the captured instant directly, which is the carve-out this change exists
to delete.

### 5. An unresolvable default is `422 TemplateInvalid` with a new reason, reported per label

A new `Reason` variant `ParamDefaultUnresolvable => "param_default_unresolvable"` is added to the
`reasons!` registry in **`src/reason.rs:9-38`**, which owns the vocabulary and the wire slugs;
`AppError::template_invalid` (`src/errors.rs:259-267`) then carries it, taking an existing `Reason`
rather than defining one. An earlier draft of this design put the slug in `src/errors.rs`, where it
would not compile and the completeness test would not see it. `TemplateInvalid` already exists at `422` (`src/errors.rs:259-267`), so no `code`
string changes, and the reason-completeness test passes from the delta alone.

Because the delta supersedes the `TemplateInvalid` row of `docs/SPEC.md` §10, it must restate that
code's *whole* contract, not the part this change adds. An earlier draft defined a two-case universe and
would have silently deleted the code's other reasons. The spec now lists all seven: the three in the
frozen §10.1 table (`template_parse_failed`, `template_validation_failed`, `template_duplicate_id`,
`docs/SPEC.md:712-714`), the three group reasons production also emits (`src/errors.rs:270-274`,
`src/fs_safe.rs:106,166,174,540`), and the new one.

*Not a third failure mode, and decided at load.* A malformed brace in a `default:` is refused when the
template loads. This design said the opposite for two rounds, on the grounds that there was no
implementation path, and that was half right: `scan_tokens` yields only closed tokens and skips malformed
sequences (`src/interpolation.rs:178-192`), so the token scanner cannot see it. The conclusion was wrong.
Nor is the render path's helper reusable as-is: `process_literal_chunk` (`src/render/helpers.rs:39-63`)
rejects every undoubled `{`, and the render path only ever hands it the *gaps between* scanned tokens
(`:86-92`, `:141-143`), so running it over a whole default would reject a valid `{sys.now}`.

What is needed is a whole-string syntax validator that walks a default once, honouring `{{`/`}}` and
skipping well-formed tokens, and refuses an unterminated `{` or unmatched `}`. Its escape and token
behaviour must be the *same* behaviour the render path applies, shared rather than reimplemented, or the
two can disagree about which strings are legal — which is the drift `interpolation-tokens` exists to end.

Deciding it at load is what stops the failure being reported as `400 InvalidRequest` against a request
that supplied nothing, for text only a template author wrote — the same argument this change makes about
`MissingField`.

*Alternatives rejected:* `MissingField`, which names a field the caller cannot supply; and a `500`,
which is false — the template renders for any caller who supplies the parameter.

*Batch shape.* An earlier draft required this to "fail the batch as a whole". That is not implementable
without restructuring `src/batch.rs:93-118`, which folds every per-label `Err` into `failures`, and the
restructure buys nothing: resolution runs per label inside `compile_label_doc`, so every label that
omits the parameter fails identically. The spec now says exactly that — one `failures` entry per
affected label, batch still all-or-nothing — which is both what the code does and what a reader can
check.

### 6. The client already stopped deriving; what is left is three dormant copies and one live one

**Scope note.** An earlier draft of this design had the input list canonicalise a published default,
normalise its authored scalar, and withhold one the render would reject. All three are cut. Deciding that
a literal default fails needs the render's own coercion, and for a `datetime` the server timezone, which
the derivation building the list does not have; and a published default that a control cannot hold is a
question about what a client is handed, which is #262; the numeric-kind asymmetry that
normalisation would have papered over is #270. This change publishes the declared default verbatim, minus
the inferred ones it deletes, and stops there.

#200 removed the client's field walk and its default helpers, so decision 6 is much smaller than it was
written to be. `initialParamValues` and `defaultParamValues` no longer exist, and `hasServerDefault` and
`reconcileRowOptions` have no caller outside their own test — they are dead code this change deletes
rather than edits.

What remains is inference keyed on `InputControl` rather than on `ParamType`, in four places:

- **Live and unconditional:** `initialDataFromInputs` (`ui/src/pages/print/PrintForm.tsx:22-37`) tests
  `control === "datetime" || control === "date"` *first* and seeds `new Date()`, ignoring `input.default`
  entirely. This is the client half of the render-instant fallback and it must go: seed from
  `input.default` when the list publishes one, and leave the control empty otherwise.
- **Dormant, and reactivated by this change:** the same function's `checkbox` → `false` and
  `select` → `values[0]` branches, plus `ParamInput.tsx:165-167` (`?? false`) and `:190`
  (`?? spec.values?.[0]`). They never fire today only because the server fills `input.default` for those
  controls. Stop filling it and they become the inference, in the client, unreviewed. They go with it.
- **The client preview:** `sampleData` (`ui/src/lib/preview.ts:12-26`) filters on
  `interpolated && required` and falls through an unconditional `else` that assigns the input's own name
  (`:20-22`). Two inputs this change newly makes required break there, and neither fails as an omission:
  an undefaulted `datetime` posts `printed_on: "printed_on"`, which is `400 InvalidRequest` with reason
  `datetime_param_invalid`, and an undefaulted `enum` posts `size: "size"`, which is
  `422 InvalidOptionValue` (`src/render/mod.rs:157-167`). It needs a control-keyed table like the
  server's `placeholder_data`, plus the first allowed value for a `select`, which the server covers with
  an option map no request model can carry.

Because `required` and `default` are now server-published, the brace test for a tokened default belongs
there too, not in each client: `derive_inputs_internal` omits `default` when the declared text carries
interpolation syntax, and the client simply renders what it is given. That is a better answer than the
client-side test an earlier draft specified, and it is #200's architecture paying off.

### 7. The thumbnail must invent for the controls this change makes required

`TemplateContent::placeholder_data` (`src/templates.rs:156-179`) invents only for `image`, `text`,
`textarea`, `integer` and `number`, and skips everything else through `_ => continue`. That is safe today
because a `checkbox` or `date` entry is never `required`. This change makes both required when no
`default:` is declared, so the gate starts admitting them and the fall-through leaves them unfilled —
`MissingField` on every thumbnail of a template that reads one.

So `placeholder_data` gains two arms, `false` for `checkbox` and the instant for `date`/`datetime`, and
therefore gains an instant **argument**: the request's already-captured `DateTimeResolver.now`, which the
thumbnail handler builds at `src/api.rs:1212-1215`. It must not call the clock itself; one render reading
two instants is what `interpolation-tokens` exists to prevent.

`select` stays out of it. `default_option_selection` supplies every declared `enum` for the thumbnail,
preview-only, and `docs/SPEC.md` §2.0 documents it. An earlier draft of this design claimed removing the
render-time fallback would drop enum-gated branches from thumbnails; it would not, because that selection
is a separate mechanism, and this change leaves it alone.

A parameter that declares a default is never invented for, because the gate is `required` and a declared
default makes it false. So its default resolves — in a thumbnail exactly as in a render — whether a token
reads it or only a `when:` predicate names it, and a broken `{vars.…}` default fails the thumbnail either
way. An earlier draft of this design assumed a placeholder outranked a declared default, which was true
of the pre-#200 walker and is not true of the `interpolated && required` gate that replaced it.

### 8. What the narrowed UI change buys, and what it costs

An undefaulted `boolean` or `enum` becomes `required: true` in the published input list, so every screen
demands it without any screen deciding to: `Import.tsx:146` and `Connect.tsx:177` read `input.required`
directly, and `PrintForm`'s validity check (`ui/src/pages/print/PrintForm.tsx:77-83`) is control-agnostic.
That is the whole point of #200's architecture and this change inherits it.

**A `datetime` is the exception, and it needs code.** Both grids test the control before the flag —
`if (input.control === "datetime" || input.control === "date") { … } else if (input.required)`
(`Import.tsx:143-147`, `Connect.tsx:174-178`) — and `datetimeCellError("")` returns `null`
(`ui/src/lib/templateFields.ts:39-41`), so a blank date cell never reaches the required check in either
grid. The `datetime-params` delta requires it to be flagged when the parameter declares no `default:`,
so that branch has to consult `input.required` as well as parse the cell.

The cost is a blank control for a parameter whose default carries a token, since the list publishes no
value for one. #262 is what later fills it in.

### 9. The legacy `options:` desugar is left alone

`src/convert.rs:371-381` keeps writing `default: None`. Decided with the issue's author against #241's
own text; the rationale is in the proposal.

### 10. ADR

This change adds **ADR-0084, "A parameter is required unless its template declares a default"**, and
supersedes *in part* four ADRs, not two:

- **ADR-0056** (`parameterized-templates`), lines 72-73, which define the `boolean` and `enum` fallbacks;
- **ADR-0068** (`datetime-parameter-type`), whose default is the render instant;
- **ADR-0022** (`import-option-model`), lines 20-26 and 39-45, which require every declared option to be
  an always-present grid column "defaulting to its **first allowed value**" and a missing
  `/import/csv` option to take the same — the client half of the enum inference;
- **ADR-0013** (`render-print-ux`), lines 19-22, which record the generated form as option-aware with
  "options default to the first declared value".

The last two are what round 5 caught: the inference does not live only in the renderer, and an ADR that
supersedes only the server half leaves two accepted decisions mandating the client half. Partial
supersession is this repository's existing form — ADR-0068 is already recorded as superseded in part by
ADR-0079. ADR-0068's *other* relevant consequence, the rejection of resolving defaults in
`GET /templates/{id}`, is untouched here and belongs to #262.

It **amends** ADR-0070 (`service-derives-the-input-list`, added by #200) without superseding it: that
decision is that the service derives the list, which stands; this change alters what the derivation
computes for `required` and `default`.

**The number is provisional.** `docs/adr/` on `main` still ends at 0082 — #200 took the free 0070 slot,
not a new one — and `.worktrees/issue-212`'s plan claims 0083. Confirm the next unused number against `main` when writing it, per the worktree trap that
numbering is only unique against the branch you can see.

## Risks / Trade-offs

- **#236 collides on two capabilities, not one.** Its delta carries a `MODIFIED` for
  "A template declares a datetime parameter as an instant, not a rendering", which this change
  `REMOVE`s, *and* it introduces a new capability `param-defaults` whose territory ("how an operator
  says 'use whatever the template declares'") abuts `param-resolution`'s client rules. Its
  deferral-control label table also has no row for a default carrying a token, which it would render to
  an operator as the literal `"{vars.base}"`. → The `datetime-params` half fails **loudly**:
  `archive-merge-check.sh` resolves `MODIFIED` by name, and after this change that name is gone, so
  #236's archive is refused rather than silently rewriting the wrong requirement. The `param-defaults`
  half does not fail on its own, so whichever lands second must re-base against `param-resolution`'s
  client-seeding rule. This change lands first if both are ready. #262 sits between them and should be
  reconciled with #236 when either is worked.

- **#200 is merged, and this change now modifies the capability it introduced.** `template-inputs`
  publishes the inference in three requirements and is falsified by it in two more, so the delta carries
  six `MODIFIED` blocks against that capability and one against `layout-sizing`.
  The landing-order risk this section used to record is gone; what replaces it is that any *other*
  in-flight change touching `template-inputs` now collides with this one. → Nothing else in
  `.worktrees/` currently carries a `template-inputs` delta.

- **#214 is live but not load-bearing here, which an earlier draft of this design got backwards.** It
  claimed a grid's selections are stranded in a sibling `option` object serde drops, and that this change
  could not ship without moving them into `data`. In the merged tree `Import.tsx:211` already builds each
  row as `data: { ...r.option, ...r.data }`, so CSV `option.<name>` cells reach the server in `data`;
  `Connect.tsx` has no option map at all; and neither page passes `optionNames` to `LabelGrid`, so its
  `<select>` option column is dead code and a declared `enum` appears as an ordinary field column driven
  by `row.data`. → No row loses its value. What is actually left is smaller: drop the redundant `option`
  sibling `resolveLabels` still emits, and decide separately whether a grid should offer a `select` cell
  at all. #214 keeps the general problem.

- **#215's filed remedy is invalidated.** Its stated fix is "do not invent a sample for a declared
  parameter that resolves without one. Its default, its first enum value, or `false` is already what the
  server would use". After this change the server uses none of those for an undefaulted parameter, so
  that remedy would produce `422 MissingField` instead of a preview. → Recorded here; the issue needs a
  comment before it is worked, and that is the owner's to make.

- **CSV import gets stricter without anyone editing a CSV.** An empty `option.<name>` cell is
  deliberately not inserted into the row's data (`src/api.rs:2666-2676`, the emptiness test at `:2670`), so it relies on the enum
  first-value inference. Rows that import today start failing with `422 MissingField`. → Intended and
  specified, with a scenario; called out because the CSV, not the template, is what an operator will
  look at first.

- **A literal default a request could never have sent starts failing.** `default: "yes"` on a `boolean`
  loads and renders today because the `None` arm inserts it unvalidated. → Listed as BREAKING in the
  proposal and given a scenario. Nothing in this repository declares one.

- **A tokened default silently widens an existing load-time fallback.** `load_geometry_values` reads
  `spec.default` to bounds-check a numeric parameter used as a geometry `ref:`; a default it cannot
  evaluate makes it take the `min`/`0.0` path it already takes for an undefaulted parameter. →
  Specified as the rule (load treats a tokened default as no default), not left as an accident.

- **Two error shapes now exist for "the label could not be filled in".** A caller must distinguish
  `MissingField` from `param_default_unresolvable`. → That is the distinction the change is for; the
  risk is only that a client lumps them together, which the reason slug makes avoidable.

## Verifying the thumbnail change

This change moves where a `datetime` placeholder comes from and alters which branches a preview gates,
so the automated gates are not sufficient evidence that thumbnails still look right. Before this is
called done, render and **look at** thumbnails for: a template with a `datetime` parameter and no
default (`brother_24mm_printed_on`), one with `time: true`, one with an enum-gated container
(`avery5163_asset_tag`), one with a boolean-gated container, and one whose default cannot be resolved.

This is an execution practice, not a task with a box. Its only evidence is an image no later reader can
retrieve, so a checked box over it would be a claim nothing can verify (#220). `tasks.md` will not carry
one.

## Migration Plan

None, deliberately. No template in this repository or in the bundled catalog is edited to keep rendering
as it does today, and no shim preserves the removed behavior. Deployments carrying templates that rely
on an inferred `boolean`, `enum` or `datetime` value will see `422 MissingField` and must add the
`default:` they meant. Rollback is reverting the commit; nothing is written that a rollback would strand.

## Open Questions

None. The two items that were open to the owner are settled: the numeric `ref:` geometry fallbacks are
filed as **#261**, and **#215** carries a comment recording that its stated remedy ("its default, its
first enum value, or `false` is already what the server would use") assumes the inferred defaults this
change removes.
