# 93. A declared default is published as it resolves

Date: 2026-09-01

## Status

Accepted. Issue [#262](https://github.com/pfa230/labeler/issues/262). Supersedes in part [ADR-0068](0068-datetime-parameter-type.md).

## Context

A template parameter declaring a `default:` was previously published verbatim in `InputSpec.default` (derived from `ParamSpec.default`). When that default contained tokens (`{vars.site}`, `{sys.now}`) or required unit conversion (`"80mm"` for `type: length`), the authored string was sent directly to clients. `ParamSpec.default` itself is untouched and still carries the declared value verbatim, which `TemplateDetail` renders as the authored default alongside the resolved one.

This created several problems:
1. Numeric and date controls in client interfaces (`<input type="number">`, `<input type="date">`, `<input type="datetime-local">`) cannot accept strings like `"80mm"` or `{sys.now}`.
2. Derivation previously omitted token-bearing defaults from `InputSpec.default`, causing parameters with authored defaults to appear as not required yet unseeded in forms (required was false because a default was declared, but no value was published to seed the control).
3. [ADR-0068](0068-datetime-parameter-type.md) previously rejected resolving defaults in `GET /templates/{id}` out of concern for cacheability. However, the template detail response sets no `ETag` and no `Cache-Control` header; it already performs dynamic reads, and the concrete cost of resolving defaults is a single store read.

## Decision

1. **Resolution and Coercion on Read.** `GET /templates/{id}` and `POST /templates/{id}/inputs` resolve every declared `default:` in strict mode against the current variables store and datetime format settings. Coerced values (`serde_json::Value` to `ParamValue`: integers as `Integer`, other numbers as `Float`, booleans as `Boolean`, strings as `String`) are published in `InputSpec.default` and `TemplateDetail.param_defaults`.

 2. **Structured Diagnostic Failure Reporting.** When a declared default fails to resolve (e.g. an unreferenced variable `{vars.missing}` or invalid syntax), the endpoint does not fail with 422/500. Instead, it returns `200 OK` and reports the error in `TemplateDetail.param_defaults` as `ParamDefaultReport::Error` and in `InputSpec.default_error`. The input is marked `required: true` and publishes no `default` (the key is absent).

3. **Strict Render-Time Rejection.** At render time (`/api/print`, `/api/render/*`), strict resolution is enforced. If a parameter with an unresolvable default is omitted from request data, the render fails with `422 TemplateInvalid` containing structured details (`reason: "param_default_unresolvable"`, `param`, and `token` or `value`).

4. **Single-Snapshot Request Consistency.** Handlers capture a single clock instant and variables snapshot per request, ensuring that multi-label batch input derivations (`POST /api/templates/{id}/inputs`) evaluate all labels against consistent defaults. On template write paths (`PUT /api/templates/{id}`, `PUT /api/templates/{id}/group`), the context is captured before mutation and applied to the reloaded detail response without redundant post-mutation store reads.

5. **Client Adaptation and Deferral.** Form components (`PrintForm`, `FieldForm`) seed from published `InputSpec.default` values and widen bare `YYYY-MM-DD` strings to `YYYY-MM-DDT00:00` for `datetime` controls. Entries with `default_error` render no "Use default" checkbox and display the error diagnostic message.

## Consequences

- Supersedes the consequence in ADR-0068 that default resolution in `GET /templates/{id}` is rejected for cacheability reasons.
- `TemplateSummary` (returned by `GET /api/templates`) remains lightweight and does not include `param_defaults`.
- Clients always receive coerced, typed default values matching their respective input controls.
