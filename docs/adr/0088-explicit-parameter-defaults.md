# 88. A parameter is required unless its template declares a default

Date: 2026-08-29

## Status

Accepted. Issue [#241](https://github.com/pfa230/labeler/issues/241). Supersedes implicit defaults from [ADR-0056](0056-parameterized-templates.md) and [ADR-0068](0068-datetime-parameter-type.md); supersedes in part [ADR-0013](0013-render-print-ux.md) and [ADR-0022](0022-import-option-model.md); amends [ADR-0070](0070-service-derives-the-input-list.md).

## Context

Previously, parameters without an explicit `default` were assigned implicit values by type:
- `boolean` defaulted implicitly to `false`.
- `enum` defaulted implicitly to its first allowed value (`values[0]`).
- `datetime` defaulted implicitly to the server clock at render time.

This implicit defaulting coupled the UI, thumbnail, and render layers in subtle ways and created multiple classes of bugs:
- Required parameters could not be enforced for boolean, enum, and datetime types; omitting them silently printed synthetic values.
- Dynamic default evaluation (`{sys.now}`, `{vars.*}`) was untracked during input derivation and produced discrepancies between declared defaults and derived input requirements.
- The thumbnail generator and live preview diverged in behavior when encountering option gates or missing parameter values.
- Lenient parameter resolution (used by input derivation) needed to remain strictly infallible even when user templates contained broken default interpolation tokens.

## Decision

1. **No Inferred Defaults**:
   Parameters without an explicit `default:` remain absent unless supplied in the request. An omitted parameter referenced by an active layout item fails strict render with `422 MissingField`. An omitted parameter referenced only by an inactive or evaluated `when:` predicate does not fail and evaluates to false.

2. **Interpolated Defaults**:
   Parameter defaults may contain interpolation tokens (e.g. `default: "{sys.now}"` or `default: "{vars.site_name}"`).
   - Token syntax and balance are validated at template load time (`TemplateValidationFailed`).
   - Token resolution and coercion to parameter types occur at render time in Strict mode, reporting unresolvable defaults as `422 TemplateInvalid` with reason `param_default_unresolvable`.
   - Datetime parameters accept string defaults (such as `"{sys.now}"`), while non-string defaults are refused at load time.

3. **Strict Input Requirement Derivation**:
   Input derivation marks an input as `required: spec.default.is_none()`.
   - Literal defaults are published verbatim in `InputSpec.default`.
   - Tokened defaults (containing `{` or `}`) are published with `default: None`, while `required` remains `false` (since the server resolves them if unsupplied).

4. **Infallible Lenient Mode**:
   Lenient parameter resolution used during input derivation absorbs unresolvable or invalid defaults gracefully without raising errors or panicking.

5. **UI Unset State**:
   The web UI reflects the explicit parameter model:
   - Checkbox controls display an "Unset" label when no value or default is present.
   - Select dropdowns display a placeholder matching no valid option when unset.
   - Numeric inputs with bounds only render sliders when an explicit default is defined; otherwise they render plain numeric inputs.
   - Live preview and thumbnail generation both name the same instant for datetime placeholders, using UTC RFC 3339 formatted timestamps with Z in the browser and offset-free local time strings on the server.

## Consequences

- Template contracts are explicit and self-documenting; missing values fail fast rather than silently printing unintended fallback data.
- Dynamic defaults evaluate consistently at render time.
- Input introspection correctly communicates field requirements to client forms and batch importers.
- The UI accurately distinguishes between false/empty values and unset parameter states.
