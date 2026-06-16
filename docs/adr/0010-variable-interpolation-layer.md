# 10. Variable interpolation layer

**Status:** Accepted

## Context

CAPABILITIES §3.1 flagged the absence of a named-variable layer as the biggest gap versus commercial
engines: those bind objects to named variables, while our items bind directly to `data` keys. The
concrete need that forced the issue was QR content composed from a configurable host joined to an item
id (`{base_url}/{id}`), which a bare data key cannot express. We wanted the smallest mechanism that
closes the composition gap without committing to a full expression language.

## Decision

- **Substitution-only interpolation.** A template string may embed tokens in braces. `{field}` resolves
  from the request `data` map (stringified via `value_to_string`), and `{settings.<key>}` resolves from
  the settings store. `{{` and `}}` emit literal braces. An unresolved token (absent data key or absent
  setting) is `422 MissingField`. There are no operators, functions, or conditionals.
- **A `value` field on `text` and `qr` items**, as an alternative to `name`. Exactly one of `name` /
  `value` is required. `name` keeps the single-key data binding unchanged; `value` carries an
  interpolated template. Interpolation applies to text content and QR content only.
- **Subsumes id-field mapping.** The earlier "configurable QR base-URL + id-field mapping" idea
  (#14) collapses into referencing the id key directly inside a `value` string
  (`{settings.qr_base_url}/{id}`); no separate id-mapping config is needed.

## Consequences

- Existing templates are unchanged: `name` still works exactly as before, so the starter tape and Avery
  templates need no edits.
- `value`-based items are anonymous (no `name`), so they are skipped by the sibling name-uniqueness
  check; only `name`-bound items participate in that constraint.
- Deferred to later milestones: formulas/arithmetic, data-driven conditionals, and
  default/fallback syntax (`{field|default}`). The substitution grammar leaves room for these additively.

## Alternatives considered

- **A narrow QR `link` boolean flag** that prepends a configured base URL to a data key. Rejected: it
  solves only the QR-URL case, does not generalize to text or to multi-token composition, and leaves the
  named-variable-layer gap open. The interpolation layer covers the same need and more with one
  mechanism.
