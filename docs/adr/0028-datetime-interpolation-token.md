# 28. Current-time interpolation token ({datetime.*})

**Status:** Accepted. Extends [ADR-0010](0010-variable-interpolation-layer.md) (does not supersede).

## Context

Issue #76 identified the need for a render-time current-timestamp token. Labels that serve as
printed records (shelf labels, asset tags, print dates on shipping documents) need the print date
embedded without requiring the caller to supply it as a data field. Supplying it from the client is
fragile: the client clock and server clock may differ, and the format is a server-side concern.

The existing interpolation layer (ADR-0010) supports `{field}` (data), `{vars.<key>}` (variables
store), and `{{`/`}}` (literal braces). A third namespace, `{datetime}` / `{datetime.<name>}`,
extends the layer without changing its contract.

The formatting requirements vary by use case: ISO date for logs, short date for consumer labels,
localized long-form for shipping documents. A configurable format map avoids hard-coding one style
and lets operators tune formats without recompiling.

ADR-0020 and ADR-0024 drew a clear boundary: app settings parameterize rendering but are not
themselves interpolation tokens. The `datetime_formats` setting is on the settings side of that
boundary: it holds format strings (strftime patterns) that the interpolation layer resolves by name.
The formatted date string appears in the label; the pattern never does.

## Decision

- **`{datetime}` (bare):** resolves to the current local date formatted as `%Y-%m-%d`. Always
  succeeds; no external configuration required.
- **`{datetime.<name>}` (named format):** resolves the format string for `<name>` from the
  `datetime_formats` app setting, formats `now` with that pattern, and substitutes the result.
  An unknown `<name>` is a `422 MissingField` (same status code as an absent data key or variable).
- **`datetime_formats` app setting:** a JSON object mapping format names to strftime patterns
  (`{"iso_date": "%Y-%m-%d", ...}`). Patterns are validated via `chrono::format::StrftimeItems`
  at settings-write time, so a bad pattern is a `400` at the PUT, not a `422` at render time.
  Default seeded values:

  | Name | Pattern | Example |
  | --- | --- | --- |
  | `iso_date` | `%Y-%m-%d` | `2026-06-25` |
  | `iso_date_time` | `%Y-%m-%d %H:%M` | `2026-06-25 14:30` |
  | `short_date` | `%m/%d/%Y` | `06/25/2026` |
  | `long_date` | `%B %-d, %Y` | `June 25, 2026` |
  | `time` | `%H:%M` | `14:30` |

- **Precedence:** datetime tokens are resolved first, then `vars.` tokens, then data tokens. A
  `{datetime}` or `{datetime.<name>}` token shadows any same-named data or vars key (the
  `datetime` namespace is reserved).
- **`now` captured once per render request:** a single `Local::now()` call at the start of each
  render (single-label or batch) ensures that every token on a multi-label sheet shows the same
  instant.
- **Server-local timezone:** the `TZ` environment variable controls the local timezone. No per-user
  or per-request timezone override.
- **Resolution at render time:** format names are resolved from the mutable settings store at render
  time (not at template-load time). This is consistent with how `vars.` tokens work. It means an
  operator can change a format and the next render picks it up without a reload.
- **Chrono dependency:** formatting uses the `chrono` crate (already present via `typst-as-lib`
  dependencies). `Local::now()` is the time source.

## Consequences

- **Reserved namespace.** A data field or variable named `datetime` (bare) or matching
  `datetime.<name>` will be shadowed. No bundled template uses such a field; the incompatibility
  is negligible in practice and documented.
- **Limited scope.** The `datetime_formats` infrastructure handles timestamp formatting only.
  Other potential built-in tokens (`{uuid}`, sequence numbers, batch position) are out of scope
  and would use a different namespace.
- **No template-load validation of format names.** A template referencing `{datetime.unknown}`
  passes validation and fails only at render time with `422 MissingField`. This matches the
  behavior of `{vars.<key>}`.

## Alternatives considered

- **Per-request `now` field from the client.** Rejected: client/server clock skew, format
  variability, and unnecessary caller burden for a server-side concern.
- **Hard-coded format list.** Rejected: operators cannot add locale-specific or custom formats
  without a code change.
- **Template-load validation of format names.** Rejected: format names live in mutable settings;
  requiring them at load time would couple template validity to the current settings state and
  break the immutable-template invariant.
