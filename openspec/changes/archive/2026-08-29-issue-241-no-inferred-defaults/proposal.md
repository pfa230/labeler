## Why

Implements [#241](https://github.com/pfa230/labeler/issues/241).

A default a template author never wrote is still a default, and the service invents three of them.
When a request omits a parameter, `apply_param_default` (`src/render/mod.rs:286-312`) resolves a
`boolean` with no `default:` to `false` and an `enum` with no `default:` to the first entry of `values`,
and `src/render/mod.rs:70-95` resolves a `datetime` to the render instant. Since #200 the service also
*publishes* that inference: `derive_inputs_internal` (`src/templates.rs:389-401`) reports such a
parameter as `required: false` and hands clients `false` or `values[0]` as its `default`. Nothing in the template says the author wanted `false`, or
the first enum value, or today. A label prints, so nobody finds out, which is worse than an error: a
caller that forgot a field gets a plausible wrong label instead of a rejection.

Four parameter types already hold the correct rule — `string`, `length`, `integer` and `number` are
required unless the template declares a default. This change makes that rule universal, and gives
`datetime` a way to keep saying "today" that is a declaration rather than a guess: a `default:` is
resolved through the same interpolation grammar a `text` value is, so `default: "{sys.now}"` states the
render date in the template, where a reader can see it.

## What Changes

- **BREAKING.** The inferred `boolean` → `false` and `enum` → first-value resolutions are removed. A
  parameter with no declared `default:` that an active layout item references is `422 MissingField`
  naming the parameter, exactly as a `string` is today. A parameter gated out by `when:` is still not
  required, unchanged.

- **A `default:` is an interpolated string.** It is resolved through the token grammar the
  `interpolation-tokens` capability defines — the same grammar, the same load-time refusals — making
  `params.<name>.default` a fourth interpolated site alongside `text.value`, `qr.value` and `image.src`.
  A default carrying no token resolves to itself, so every literal default in existence is unaffected
  except that `{` and `}` now need escaping as `{{` and `}}`, on exactly the terms every other
  interpolated string already carries.

- **Only namespaced tokens.** A default may name `{sys.…}` and `{vars.…}` and nothing else. A **bare**
  token in a default (`default: "{message}"`) is a load-time validation error naming the parameter, so
  the template is quarantined at load and refused at `POST`/`PUT`. Defaults therefore resolve against
  sources that are already fixed when a request arrives: there is no parameter dependency graph, no
  resolution order, and no cycle to detect.

- **BREAKING.** `datetime` stops being special. The load-time rejection of `default:` on a `datetime`
  parameter is removed, and so is its render-instant fallback. `default: "{sys.now}"` is how a template
  declares "the render date" — local midnight, because `{sys.now}` renders `%Y-%m-%d`, so a `time: true`
  parameter defaulted that way prints `00:00` where the removed fallback printed the wall clock. A
  `datetime` with no `default:` is required like every other type.
  Because a request captures one instant and reads the variables store once, and every default resolves
  against that snapshot, a run crossing midnight still cannot print two dates.

- **A default that cannot be resolved is the template's error, not the caller's.** A default whose token
  has no value (`{vars.absent}`), or that resolves to something the parameter's own declaration forbids
  (a string outside an `enum`'s `values`, an unparseable `datetime`), fails with `422 TemplateInvalid`
  and a new `details.reason` naming the parameter, the token and the resolved value. It is not reported
  as a `MissingField`, because the caller sent nothing and has nothing to fix.

- **BREAKING.** A resolved default is validated **and coerced** exactly as a supplied value is. Today a
  literal default is inserted with no validation at all, so `boolean` parameters declaring
  `default: "yes"` or `default: 1` load and render; from this change they fail on the same terms the
  request value `"yes"` fails. Coercion also *rewrites* a default that passes on the render path: a `length`
  declaring `default: "80mm"` prints `80mm` today and `80` from this change, because the suffix is
  stripped on the path a supplied value takes. The input list still publishes the declared `"80mm"`
  verbatim — this change does not canonicalise what a client is handed, which is #262's subject. Nothing
  in this repository declares either shape. One asymmetry is accepted rather than engineered away: a
  `boolean` declaring `default: 1` is held as a float and rejected where a request sending `1` succeeds,
  because the conversion that reads a template drops the authored scalar's kind. An author writes
  `default: true`; the underlying defect is #270.

- **A client stops inventing defaults, and stops seeding what it cannot resolve.** The UI re-implements
  the server's inference in TypeScript (`hasServerDefault`, `ui/src/lib/templateFields.ts:1-11`), now
  dead code; that goes. A control is seeded from a declared default only when the default carries no token, because a
  client cannot resolve `{vars.base}` and seeding the raw text would submit it as a data value and print
  it verbatim. A tokened default leaves the control empty and the parameter still counts as defaulted,
  so submitting omits it and the service resolves it. **Publishing the *resolved* default to the client,
  so that empty control can show what will print and a broken default is visible before someone presses
  print, is deliberately not built here: it is [#262](https://github.com/pfa230/labeler/issues/262).**

- **A preview keeps inventing values, and the specs now say so.** A thumbnail has no request behind it,
  so every value it prints is one the service chose — including the first-allowed-value option selection
  `docs/SPEC.md` §2.0 documents (`docs/SPEC.md:116-117`), which is applied by `src/api.rs:1207` and is *not* removed here. What
  changes is only that a `datetime` parameter's instant becomes a placeholder the preview supplies
  rather than a fallback the render path supplies, and that a `boolean` a token reads is invented for with
  `false` where the render path used to supply it, while one only a `when:` predicate names gets nothing and
  its branch drops out.

- **BREAKING.** The legacy top-level `options:` map keeps desugaring to an `enum` parameter with no
  default (`src/convert.rs:371-381`), so a template still using it becomes required like any other
  undefaulted enum. #241's text proposed writing `values[0]` as an explicit default to preserve the
  legacy rendering; that preservation is deliberately not built. One rule, no carve-out. Nothing in this
  repository uses the top-level `options:` map.

- **No migration.** No fixture and no bundled catalog template gains a default to keep rendering as it
  does today. `tests/fixtures/templates/avery5163_asset_tag.yaml`'s undefaulted `outline` enum and
  `tests/fixtures/templates/brother_24mm_printed_on.yaml`'s undefaulted `printed_on` datetime keep their
  declarations and change what they do; the tests over them change to assert the new contract, which is
  what makes them the proof of it. The bundled catalog declares only `string` parameters and is
  untouched.

## Capabilities

### New Capabilities
- `param-resolution`: how a parameter's value for one request is decided — supplied, resolved from a
  declared default, or absent — including how a default is interpolated, what a resolution failure
  reports, and what a preview may invent in place of a request.

### Modified Capabilities
- `template-inputs`: six of its requirements change. Three encode the inference directly — "An input
  list describes the controls one label needs" makes `required` false for a `boolean`, `enum` or
  `datetime` by type and publishes `false` and `values[0]` as defaults; "The service computes an input
  list for a given label" repeats it in its lenient-resolution rule; "The thumbnail renders the default
  selection from placeholder data" says a `checkbox`, `date` or `datetime` entry is never invented for
  because each "resolves on its own". Two more are falsified by it: "One derivation serves the thumbnail
  and the catalog index", whose scenario lists a field set an undefaulted `datetime` now joins, and "A
  screen renders the reported inputs and decides nothing else", which cites `datetime-params` for a
  browser-clock seeding rule `datetime-params` now forbids and states a preview fill rule that now
  differs from the thumbnail's for `select`. A sixth, "The template detail carries the lists a client
  needs before it has a label", carries a scenario assuming an undefaulted `enum` still selects a branch
  for `inputs.default`.
- `request-error-envelope`: its `Internal` requirement says every other row of `docs/SPEC.md` §10's code
  table "remains authoritative"; this change supersedes the `TemplateInvalid` row, so that sentence is
  narrowed to say the published set is unambiguous rather than relying on a reading.
- `layout-sizing`: the requirement "Load-time validation and render-time resolution are one algorithm"
  carries a scenario whose premise is "`debug` defaults to `false`" for a `boolean`. That is the removed
  inference; the scenario's outcome is unaffected, so the premise becomes an explicit `default: false`.
- `interpolation-tokens`: the requirement "A token names one value and may attach one format" lists the
  interpolated strings the grammar governs (`text.value`, `qr.value`, `image.src`). A parameter's
  `default:` joins that list, carrying the restriction that only a namespaced `value-path` is legal
  there.
- `datetime-params`: all four of its requirements change. Its other two — "A request may override a
  datetime parameter" and "The print form and the row grids carry a datetime parameter" — assert the
  removed fallback directly ("Clearing the control SHALL be valid and SHALL defer to the server's
  instant"; "A blank `datetime` parameter SHALL NOT be flagged as a missing required value"), so leaving
  them untouched would archive a capability that contradicts itself. They are `MODIFIED`.
  Its declaration and defaulting requirements are removed and replaced.
  "A template declares a datetime parameter as an instant, not a rendering" rejects `default:` on a
  `datetime` parameter, carries the parameter-type table whose "Behavior when omitted" column states the
  inferred `boolean`, `enum` and `datetime` values, and asserts the rejection in a scenario, so it cannot
  be modified in place without keeping a scenario that contradicts the change. "A datetime parameter
  defaults to the render instant of its request" *is* the fallback being removed. Replacements restate
  every rule that survives, including the `docs/SPEC.md` §3.0 supersession.

## Impact

- **Code.** `src/render/mod.rs` (`resolve_parameters`: the three fallbacks, default resolution through
  `interpolate`, and `placeholder_data` gaining the datetime instant); `src/convert.rs` (datetime stops
  rejecting `default:` and starts carrying it; a bare token in any default is refused);
  `src/templates.rs` (load-time validation: the remaining token refusals over `params[*].default`, and
  the existing default-instantiation checks apply to literal defaults only, and `derive_inputs_internal`
  stops publishing the inference while `placeholder_data` starts inventing for the controls that become
  required); `src/render/helpers.rs` (a formatted bare token must name its parameter, not the token spelling, once the
  datetime fallback stops registering an instant for every declared `datetime`); `src/datetime_fmt.rs` (a
  date-only value whose local midnight does not exist resolves forward rather than failing, without which
  `default: "{sys.now}"` fails for a day each year in a zone transitioning at `00:00`); `src/reason.rs` (the new
  `ParamDefaultUnresolvable` variant in the `reasons!` registry, which owns the wire slug) and
  `src/errors.rs` (the constructor that carries it). Two render entry points change, not one: `compile_label_doc` for a single label and a
  batch, and the sheet loop, which calls `resolve_parameters` directly (`src/render/mod.rs:705`).
  `src/batch.rs` is *not* restructured — a resolution failure flows through its existing per-label
  failure path. Three callers of `resolve_parameters` change, not two: the single-label path, the sheet
  loop, and the lenient path behind `POST /api/templates/{id}/inputs`. No API *model* changes, though what
  `InputSpec.required` and `InputSpec.default` carry does change.
- **API.** No endpoint gains or loses a field. Render and batch endpoints gain `422 MissingField` and
  `422 TemplateInvalid` (`param_default_unresolvable`) outcomes for requests that succeed today, and the
  `TemplateInvalid` code stops meaning "structural validation only".
- **UI.** Much smaller since #200, which deleted the client's field walk and its seeding helpers.
  `initialParamValues`, `defaultParamValues` and `isDataField` no longer exist, and `hasServerDefault`
  (`ui/src/lib/templateFields.ts:1-11`) and `reconcileRowOptions` (`:110-119`) have no caller outside
  their own test, so they are deleted rather than edited. What must change: `initialDataFromInputs`
  (`ui/src/pages/print/PrintForm.tsx:22-37`), whose `date`/`datetime` branch is live and seeds the
  browser clock unconditionally and whose `checkbox` and `select` branches are dormant only while the
  server still sends those defaults; `ParamInput.tsx:165-167` and `:190`, two more dormant fallbacks;
  `sampleData` (`ui/src/lib/preview.ts:12-26`), whose unconditional `else` posts an input's own name,
  so a newly-required `datetime` is rejected as an unparseable instant and a newly-required `enum` as a
  value outside its `values`; both grids'
  blank-date check, which tests the control before `input.required` and so never flags a blank cell
  (`Import.tsx:143-147`, `Connect.tsx:174-178`); and the grids' enum selection, which must reach `data`
  rather than the sibling `option` object serde drops (#214, declared-`enum` half only).
- **Docs.** A new ADR superseding, in part, ADR-0056's `boolean` and `enum` default lines, ADR-0068's
  datetime default, ADR-0022's first-allowed-value grid columns and ADR-0013's first-option form
  defaulting, plus its row in `docs/adr/README.md`; and `docs/AUTHORING.md:543`, `:566` and `:571`,
  which document the three inferred defaults and the print form's browser-clock pre-fill.
- **Tests.** `src/lib.rs:1348-1417` (datetime omission and blank), `src/render/mod.rs` datetime
  parameter tests, `src/convert.rs:519` (`datetime_param_rejects_forbidden_attributes` must stop
  asserting `default` is rejected), `src/templates.rs:3704` (`reject_enum_default_not_in_values`) and the
  CSV import tests over `avery5163_asset_tag`.
