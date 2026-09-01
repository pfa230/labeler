# 68. Template parameter type for datetime with dynamic rendering and override support

Date: 2026-08-24

## Status

Accepted (token list and formatting syntax superseded by [ADR-0079](0079-token-grammar.md); cacheability superseded by [ADR-0093](0093-a-declared-default-is-published-as-it-resolves.md)). Issue [#209](https://github.com/pfa230/labeler/issues/209).

## Context

Before this change, templates supported datetime interpolation only through the ambient `{datetime}` and
`{datetime.<format>}` token namespace (ADR-0028), backed by the server's local clock at render time.

When label authors needed template-declared date fields—such as `printed_on`, `best_by`, or
`manufactured_date`—they had two imperfect alternatives:
1. Use ambient `{datetime}`, which cannot be overridden per label in batches, CSV imports, or connector syncs.
2. Use a `type: string` parameter, which accepts arbitrary freeform text, lacks date picker UI controls, and
   cannot leverage named datetime formatting patterns (`{param.short_date}`, `{param.time}`).

Authors needed a first-class `datetime` parameter type that defaults to the render instant when omitted or blank,
can be overridden with explicit ISO date/time strings, and formats consistently using declared `datetime_formats`
app settings.

## Decision

**1. A typed `datetime` parameter with optional `time` modifier.**
Templates can declare parameters with `type: datetime` and an optional boolean `time` field (defaulting to `false`).
When `time: false` (or omitted), the UI presents a date picker (`<input type="date">`). When `time: true`, the UI
presents a date-and-time picker (`<input type="datetime-local">`). `time` is serialized explicitly in the API
schema (`"time": false` or `"time": true`).

**2. Strict attribute rejection.**
Datetime parameters reject `default`, `min`, `max`, `multiline`, `values`, `enum`, and `format` at parse time with
clear validation errors. Non-datetime parameters reject `time`. The `format` property is rejected on all parameter
types, directing authors to specify the format in the interpolation token (e.g. `{param.format_name}`).

**3. Format interpolation and precedence.**
Declared datetime parameters introduce a token namespace:
- Bare `{param}` interpolates the instant formatted as ISO 8601 date (`%Y-%m-%d`).
- Dotted `{param.<format_name>}` looks up `<format_name>` in the configured `datetime_formats` setting and formats
  the parameter's instant. An unknown format name returns `422 MissingField` at render time.
- Precedence: ambient `{datetime}`/`{datetime.*}` takes precedence, followed by declared datetime parameter
  namespaces (`{<p>}` / `{<p>.*}`), then `{vars.*}`, and finally general request data fields.
- Dotted parameter tokens cannot be shadowed by literal request data keys named `p.<format_name>`.

**4. Single-instant capture and override parsing.**
When a datetime parameter is omitted, `null`, or blank, it defaults to the single captured render instant (`env.datetime.now`),
ensuring all date tokens on a label or multi-label sheet remain consistent. When an override string is supplied,
it is parsed in server-local time accepting `%Y-%m-%d` (midnight), `%Y-%m-%dT%H:%M[:%S]` (wall-clock), or RFC 3339
with explicit timezone offset. Invalid datetime override values produce `400 Bad Request` with reason `datetime_param_invalid`.

**5. Ambiguous and non-existent local times.**
The naive forms resolve through the server's timezone. A local time that is ambiguous because of a
daylight-saving transition resolves to the earlier of the two instants; one that does not exist at all
(a spring-forward gap) is refused with the same `datetime_param_invalid` reason, rather than being
silently shifted or allowed to panic in a render handler.

**6. `{datetime}` is not deprecated, and the two have different jobs.**
A new template reaches for `{datetime}` / `{datetime.<name>}` when the label should always say when it
was printed and the caller has no say in it: zero configuration, nothing to declare, nothing to send.
It reaches for a `datetime` parameter when the caller must be able to choose the instant, which is what
reprinting yesterday's run and printing after midnight for the previous day both need. Both resolve
through the same `datetime_formats` patterns, so switching from one to the other does not change how a
label reads.

**7. UI integration and form behavior.**
- In `PrintForm`, datetime parameters are seeded with the browser's current local date/time so labels can be printed
  or previewed immediately without manual entry.
- In CSV import and Connector grids, datetime parameters are represented as a single column named after the parameter.
  Blank values are accepted and default server-side; non-blank values are validated for calendar correctness and shape.
- `template_fields` and `placeholder_data` exclude declared datetime parameter namespaces by token head.

## Consequences

- Templates can declare reusable, formatted date/time fields that behave naturally across single-label printing,
  sheet batches, CSV imports, and connector integrations.
- The `datetime_param_invalid` error reason is added to the `InvalidRequest` set. `docs/SPEC.md` §10.1 is
  frozen (ADR-0057) and therefore does not list it; its published home is
  `openspec/specs/datetime-params/spec.md`, which the reason-completeness test in `src/errors.rs` accepts
  as documentation.
- The seeded value in the print form is the *browser's* date, so an operator whose timezone straddles a
  date boundary with the server prints the browser's date. Clearing the control restores the server as
  the authority. The alternative, resolving the default in `GET /templates/{id}`, would make a cacheable
  response time-dependent.
- Formatting patterns remain centralized in `datetime_formats` app settings.
