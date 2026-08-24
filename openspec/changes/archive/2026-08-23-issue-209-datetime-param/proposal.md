## Why

Implements [#209](https://github.com/pfa230/labeler/issues/209).

A template can print the render instant with `{datetime}` / `{datetime.<name>}`, but nothing can
override it: the resolver is built with a hardcoded `chrono::Local::now()` at all three call sites
(`src/api.rs:948`, `:1952`, `:2261`). Reprinting yesterday's run, or printing after midnight for the
previous day's work, has no answer today short of abandoning the datetime tokens and passing a
pre-formatted string as a plain field. The fix belongs in `params:`, which is already the documented
way to make a template configurable (ADR-0056) and already carries description, validation, a form
input, a CSV column and a connector mapping.

## What Changes

- **A new parameter type, `type: datetime`.** It declares one *instant*, not a rendering. Its only
  own attribute is `time:` (bool, default `false`), which picks the form control: a date picker or a
  date-and-time picker. `format:`, `default:`, `min:`, `max:`, `multiline:`, `values:` and `enum:`
  are rejected on it at template load.
- **Presentation stays in the token, not in the parameter.** A datetime parameter named `printed_on`
  claims the interpolation namespace `printed_on`: bare `{printed_on}` prints ISO `%Y-%m-%d` and
  `{printed_on.<name>}` prints the named `datetime_formats` pattern, exactly as `{datetime}` and
  `{datetime.<name>}` do. An unknown `<name>` is `422 MissingField` at render, since
  `datetime_formats` is runtime state and cannot be checked at load.
- **The default is the render instant.** When a request omits the parameter it resolves to the same
  `now` every other datetime token on that request uses, so a sheet spanning midnight cannot print
  two different dates. `resolve_parameters` (`src/render/mod.rs:27`) is given that instant instead of
  reading the clock itself.
- **The caller may override it** with `YYYY-MM-DD`, a bare local `YYYY-MM-DDTHH:MM[:SS]` (what
  `<input type="datetime-local">` submits), or an RFC 3339 timestamp with an offset, which is
  converted to server-local time. An unparseable or non-existent local value is `400 InvalidRequest`
  carrying a new `details.reason`.
- **The UI gains the control.** `ParamInput` renders `<input type="date">` or
  `<input type="datetime-local">` per `time:`. A blank datetime field is valid everywhere a value is
  required today, because the server fills it. The CSV import grid and the connector grid keep a text
  cell that accepts the same three input forms and flags an unparseable one per row.
- **`{datetime}` and `{datetime.<name>}` are unchanged and not deprecated.** The ADR states the
  split: the bare tokens are the zero-config "now", a `datetime` parameter is what a template reaches
  for when the caller must be able to say *which* instant.
- Not breaking: `type: datetime` is new syntax, and no existing template can name a parameter that
  changes meaning.

## Capabilities

### New Capabilities
- `datetime-params`: the `datetime` parameter type, its interpolation namespace, how the render
  instant defaults and how a request overrides it, and the form and grid controls that carry it.
  Its requirements supersede `docs/SPEC.md` §3.0 (the parameter type table and namespace rules) and
  the token precedence list in §8, restating each in full per the first-touch rule.

### Modified Capabilities
<!-- none: no existing openspec/specs/ capability changes its requirements. -->

## Impact

- **Template schema.** `src/raw.rs` (`RawParamType::DateTime`, `time:`), `src/models.rs`
  (`ParamType::DateTime`), `src/convert.rs`, `src/templates.rs` (`validate_param_spec`,
  `check_param_ref`, reserved-name rules).
- **Render.** `src/render/mod.rs` (`resolve_parameters` takes the request instant and returns the
  per-label instants; `template_fields` and `placeholder_data` stop reporting `{p.<fmt>}` as a
  request field), `src/render/helpers.rs` (`interpolate` resolves the parameter namespace),
  `src/datetime_fmt.rs` (parsing an override, formatting an arbitrary instant).
- **API.** `src/api.rs` passes the already-captured `now` into rendering instead of letting
  `resolve_parameters` read the clock; `src/reason.rs` gains one slug; `src/openapi.rs` registers the
  extended `ParamType`.
- **UI.** `ui/src/api/types.ts`, `ui/src/components/ParamInput.tsx`,
  `ui/src/lib/templateFields.ts` (drop `{p.<fmt>}` tokens from referenced fields), and the three
  places that decide whether a blank value is an error: `ui/src/pages/print/FieldForm.tsx`,
  `ui/src/pages/Import.tsx`, `ui/src/pages/Connect.tsx`.
- **Docs.** A new ADR (expect `0068`: `0066` is claimed by the in-flight changes for #201 and #203),
  its row in `docs/adr/README.md`, and `docs/AUTHORING.md`.
- No new dependency: `chrono` is already in the tree.
