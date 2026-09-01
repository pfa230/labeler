## Why

Implements [#262](https://github.com/pfa230/labeler/issues/262).

#241 made a parameter's `default:` an interpolated string, resolved per request against the captured
instant and the variables store. Nothing resolves one until a render begins, so a default that cannot
be resolved — `{vars.base}` naming nothing, or a value the parameter's own declaration forbids — is
`422 TemplateInvalid` at the moment an operator presses print, and nothing before that says the
template is unprintable.

The client is worse off than late. It cannot resolve a default at all, and the service publishes
nothing usable for one: `derive_inputs_internal` omits `InputSpec.default` whenever the declared text
contains `{` or `}` (`src/templates.rs:402-412`) while setting `required = spec.default.is_none()`
(`:401`). A parameter with a tokened default therefore reaches every client as `required: false` with
no default — told it need not be filled in, and given nothing to fill it with — which contradicts the
field's own published contract, "the value the service would use if the label omitted this name"
(`openspec/specs/template-inputs/spec.md:24`).

## What Changes

- **`GET /api/templates/{id}` gains `param_defaults`.** An object keyed by parameter name, with one
  entry for every parameter that declares a `default:` and none for any that does not, so an absent
  entry means "declares no default" and never "this endpoint did not resolve". Each entry carries
  exactly one of `resolved` — the post-coercion value the render path would use, resolved against the
  variables the store holds and this request's instant — or `error`, carrying
  `{ reason: "param_default_unresolvable", message, token?, value? }`. The endpoint still returns
  `200` when resolution fails: a template with an unresolvable default renders for any caller who
  supplies the parameter, and it is not quarantined.

  The report is keyed over `template.params`, the set `resolve_parameters_mode` iterates, not over the
  input list: a render resolves every declared parameter before it evaluates any `when:`, so a parameter
  declared with a broken default still fails every render that omits it whether or not the layout
  references it, and whether or not the branch that references it is active. A report keyed on `inputs`
  would omit exactly those two cases. It lives on `TemplateDetail` and not on `ParamSpec`, because
  `ParamSpec` is inside the `Arc`-shared `TemplateContent` and is serialized verbatim by
  `TemplateSummary` too (`src/models.rs:62`, `:203-214`), so a request-dependent value has no business
  on it. `GET /api/templates` is unchanged.

  One request resolves each declared default once and every projection reads that one result:
  `param_defaults`, `inputs.default`, `inputs.all` and, on the inputs endpoint, every label's list. Two
  requests may legitimately differ, because each reads the store when it is served and captures its own
  instant; what the contract fixes is the rule and the sources, and equality within one request.

- **BREAKING. `InputSpec.default` carries the resolved, coerced value.** It stops carrying the declared
  text and stops being withheld for containing a brace. A `length` declaring `default: "80mm"`
  publishes `80`, which is what the render path uses; a `string` declaring `default: "{vars.base}"`
  publishes what `vars.base` holds; a `boolean` declaring `default: "yes"` publishes no default at all,
  because that value fails the same coercion a request sending `"yes"` fails.

- **BREAKING. `InputSpec.required` means "this parameter has no usable resolved default".** It stops
  meaning `spec.default.is_none()`. A parameter whose default fails to resolve is `required: true`,
  because the operator must supply a value for the print to succeed.

- **`InputSpec` gains `default_error`,** carrying the same `{ reason, message, token?, value? }` payload
  as the report's `error`, so a client rendering only the input list can say *why* a control is empty
  without joining against `param_defaults`.

- **`POST /api/templates/{id}/inputs` resolves identically.** It returns `InputSpec` too, and
  `Import.tsx:117` and `Connect.tsx:154` mix its rows with `detail.inputs.default` in one form, so a
  parameter's default must not depend on which call the row came from. Both endpoints resolve against
  the same three inputs: `state.store().all_variables()`,
  `crate::settings::resolve_datetime_formats(state.store())` and one instant captured per request.

- **BREAKING. The input-list path stops being blind to a tokened default.** Because
  `derive_inputs_for_label` now has the variables and the formats, its lenient resolution resolves a
  tokened default like any other, so a gate naming one that **resolves** is evaluated as the render
  evaluates it. The divergence `template-inputs` records today — a branch gated on a tokened default
  reported inactive while a render draws it — closes for that case. It does not close for a default that
  fails to resolve, and cannot: this path absorbs the failure and answers `200` with the parameter absent,
  while a render of the same label refuses it before evaluating any gate. That is the difference between
  reporting what an operator must supply and printing.

- **A parameter whose default fails to resolve is `required`, so a thumbnail invents for it.**
  `placeholder_data` fills every entry that is `interpolated` and `required`, so the same template that
  returns `422` from a render today renders a preview with a placeholder from this change. That is the
  uniform rule applied, not a carve-out: the preview says nothing about whether a caller's render will
  succeed, and `param_defaults` is what does.

- **BREAKING. `details` on `param_default_unresolvable` becomes structured.** The constructor names the
  parameter, the token and the value in its `message` only (`src/errors.rs:288-310`), so a report
  carrying `token` and `value` as fields would have to parse English. One payload is built where the
  failure arises and serialized two ways. On a render path's `422` the message stays in `error.message`
  and `error.details` gains `param` alongside `reason`, plus `token` or `value` where one exists. On a
  read-only report there is no envelope, so the object itself carries `reason`, `message`, `token?` and
  `value?`, and no `param`, because it is always reached as the value of a key that already names the
  parameter. The strings are shared; the parameter and the message sit in different places.

- **Two store reads move onto every path that publishes a default.** `GET /api/templates/{id}` and
  `POST /api/templates/{id}/inputs` each take one variables read and one settings read per request,
  which is the read the thumbnail handler already takes (`src/api.rs:1206-1216`); a failure is
  `AppError::internal`, as it is there. The four paths that write a template and return its detail take
  the same two reads **before** they mutate anything, so a store failure refuses the request rather than
  reporting `500` for a template that was already written or moved. This re-examines rather than inherits ADR-0068's rejection of
  resolving a default in `GET /templates/{id}` on the ground that it "would make a cacheable response
  time-dependent" (`docs/adr/0068-datetime-parameter-type.md:82-84`): that response sets no `ETag` and
  no `Cache-Control` today (`src/api.rs` sets those only on the thumbnail, `:1220-1243`), so the
  concrete cost is the store read and nothing else.

- **The catalog index resolves against no install.** `src/bin/catalog-index.rs:97` filters `inputs_all()`
  by `required` and has no store, so it resolves declared defaults against an empty variables set and
  the built-in `datetime_formats`. A default naming `{vars.…}` does not resolve there and its parameter
  is listed as a field, which is true of the install that has not set the variable. Nothing under
  `catalog/` declares a default today.

- **The client seeds from the resolved default and stops inferring requiredness.** A control is seeded
  from `InputSpec.default`, which replaces #241's "seed nothing when the declared text carries a
  token". An entry carrying `default_error` is presented as one with no usable default: empty control,
  the diagnostic shown against it when the template loads, and the control marked required, so the
  operator is told what is wrong and can still print by supplying a value. Requiredness is read from
  `InputSpec.required` and never re-derived from the presence of a default. The `TemplateDetail` page
  shows the declared default as authored **and** the resolved value or its diagnostic.

- **A resolved `datetime` default is published as the render path's value, and the client widens it.**
  `coerce_param_value` formats a datetime to `BARE_DATETIME_FORMAT`, `%Y-%m-%d` (`src/datetime_fmt.rs:13`,
  `src/render/mod.rs:51-67`). `<input type="date">` holds that directly; `<input type="datetime-local">`
  does not, so the seeding helper widens a bare date to `YYYY-MM-DDT00:00`. `resolved` and
  `InputSpec.default` stay literally what the render path would use; the published value is not
  reshaped per control, because the point of this change is that the operator sees what will print.

- **No new resolver.** `resolve_and_coerce_default` (`src/render/mod.rs:317-391`) already performs this
  resolution and already returns `AppError::param_default_unresolvable` in `ResolveMode::Strict`. A
  default that resolved differently here from how it resolves at render time would be worse than
  publishing nothing, because the operator would be shown a value the printer will not use.

## Capabilities

### Modified Capabilities

- `template-inputs`: six requirements change. "An input list describes the controls one label needs"
  defines `default` as the declared text minus anything containing a brace and `required` as
  `default.is_none()`, and reserves both questions to #262; both rules are replaced and `default_error`
  is added. "The service computes an input list for a given label" states that a tokened default is not
  resolved on that path and carries a scenario asserting the resulting gate divergence; both go. "The
  template detail carries the lists a client needs before it has a label" enumerates what
  `GET /api/templates/{id}` includes and must name `param_defaults`. "The thumbnail renders the default
  selection from placeholder data" derives which entries are invented for from `required`, whose
  meaning changes. "One derivation serves the thumbnail and the catalog index" defines the catalog
  field list as the `required` names, which now depends on a resolution the generator must supply a
  context for. "A screen renders the reported inputs and decides nothing else" states the seeding rule,
  the deferral affordance and its label, and twice defers the usability of a published default to #262.

  One requirement is `ADDED`: the `param_defaults` report itself. A second is `ADDED` for the template
  page, which documents a template rather than collecting a label and so is governed by none of the
  screen rules above.

- `param-resolution`: two requirements change. "A declared default is resolved against one
  request-scoped snapshot" states that a default carrying interpolation syntax "SHALL NOT be published
  as a usable default in the input list a client renders from", that the lenient path does not resolve
  one, and that a client seeds nothing for it — three rules this change replaces, with two scenarios
  asserting the empty control. "A default that cannot be resolved is the template's fault, not the
  caller's" owns the failure's payload, which gains structured `details`, and must say that the same
  failure reported by a read-only endpoint is a `200`-carried report rather than a `422`.

- `datetime-params`: "The print form and the row grids carry a datetime parameter" says the form seeds
  from the published `default` and leaves the control empty for a tokened one, naming #262 as what
  would change it. It gains the widening rule for `datetime-local` and the `default_error` case.

## Impact

- **API.** `TemplateDetail` gains `param_defaults`; `InputSpec` gains `default_error` and changes what
  `default` and `required` carry, on `GET /api/templates/{id}` and `POST /api/templates/{id}/inputs`
  alike. `GET /api/templates` is unchanged. `param_default_unresolvable` responses gain structured
  `details`. Every new model registers in `src/openapi.rs`.
- **Code.** `src/render/mod.rs` (a public wrapper over `resolve_and_coerce_default` for one parameter in
  strict mode; no change to how a render resolves); `src/errors.rs` (structured `details` on
  `param_default_unresolvable`); `src/models.rs` (`param_defaults`, `default_error`, and the report
  types); `src/templates.rs` (`derive_inputs_internal`, `derive_inputs_for_label`, `inputs_all`,
  `inputs_default`, `placeholder_data` and `TemplateRegistry::detail` take the resolution context;
  `TemplateDetail::from` is replaced by a builder that carries it); `src/api.rs` (the six `detail()`
  call sites at `:837`, `:867`, `:921`, `:1003`, `:1056`, `:1148`, plus `template_inputs` at `:1261`,
  each reading variables and settings); `src/bin/catalog-index.rs` (an empty variables set and the
  built-in formats); `src/openapi.rs`.
- **UI.** `PrintForm.tsx:26-56` and `:85` (seeding and arrivals), `FieldForm.tsx:36-91` (#236's "use
  default" affordance, which must not offer a default that failed to resolve, and the diagnostic),
  `ParamInput.tsx:119-217` (stops substituting a default of its own for an absent value; the datetime
  control's seeded value), `Import.tsx:106-172` and `Connect.tsx:142-200` (requiredness read from
  `required`), `TemplateDetail.tsx:303-305` (declared plus resolved), and `api/types.ts`.
- **Docs.** A new ADR superseding, in part, ADR-0068's consequence that resolving a default in
  `GET /templates/{id}` is rejected for cacheability, plus its row in `docs/adr/README.md`.
- **Tests.** The existing input-derivation tests in `src/templates.rs` and `src/lib.rs`, whose signatures
  and expectations change, plus new coverage for every behavior this change adds. HTTP level, on the two
  read endpoints, per the issue's verification list: a `{vars.<key>}` default the store holds; the same
  key absent; a `{sys.now:<format>}` default resolving against the store's formats map; a parameter with
  a broken default that the layout never references, present in `param_defaults` and absent from
  `inputs`; a `boolean` default of `"yes"` and a `length` default of `"80mm"`; the two endpoints agreeing
  under one snapshot; and `GET /api/templates` unchanged. Beyond that list, and each named because the
  plan adds the behavior:

  - the render path's structured `422`, asserting `error.details.reason`, `param`, and `token` or
    `value`, and that the read-only report carries the same strings without `param`;
  - a store read failing on a write path, asserting `500` **and** that no template file was written,
    moved or replaced;
  - a successful write returning a detail body carrying `param_defaults` for the template as written;
  - the thumbnail for a template whose default cannot resolve, asserting it renders with a placeholder
    while the same template's render still returns `param_default_unresolvable`;
  - the catalog derivation with no install, asserting a `{vars.…}`-defaulted parameter is listed as a
    field and a `{sys.…}`-defaulted one is not;
  - UI: the `TemplateDetail` page showing declared and resolved side by side and the diagnostic in place
    of a resolved value; `PrintForm`/`FieldForm` seeding from a resolved default, offering no deferral
    for an entry carrying `default_error`, surfacing its message, and widening a bare date into a
    `datetime-local` control; `ParamInput` no longer substituting a default of its own; and `Import` and
    `Connect` reading requiredness from `required` and flagging a row whose entry carries
    `default_error`.
