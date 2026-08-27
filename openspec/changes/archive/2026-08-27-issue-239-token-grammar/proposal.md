## Why

Implements [#239](https://github.com/pfa230/labeler/issues/239), carrying
[#240](https://github.com/pfa230/labeler/issues/240) with it: both rewrite the same token grammar, and
#240's own dependency note says settling them apart invites the second to re-litigate the first.

The dot does two unrelated jobs inside one token and a reader cannot tell which is in play.
`{vars.qr_base_url}` navigates a namespace; `{datetime.long_date}` selects a format, where `long_date`
is not a key on anything but an entry in the `datetime_formats` app setting. Both resolve by
prefix-matching raw token text (`src/render/helpers.rs:75-90`, `src/datetime_fmt.rs:102`, `:116`), and
the collision is papered over by banning dots in parameter names outright (`src/templates.rs:766-771`).
That ban is a symptom, not a rule: a parameter may not contain a dot because the grammar cannot tell
`a.b` the path from `a.b` the format. No prominent system spells format attachment with a dot; the
field uses a colon, a pipe filter, or a comma clause.

The same grammar leaves one of three value sources unmarked. A caller parameter is bare, the variables
store is `vars.`-prefixed, and the service's own clock is *also* bare, as `{datetime}`. So a system
value can collide with a parameter name, which is why a reserved-word list exists at all
(`src/templates.rs:766`, specified at `openspec/specs/datetime-params/spec.md`), and why every future
ambient value would grow that list and break whoever already used the word.

## What Changes

- **BREAKING. A format name is attached with a colon**, not a dot: `{printed_on:long_date}`,
  `{sys.now:iso_date}`. One optional format name, no arguments, no chaining, so interpolation stays
  substitution-only per ADR-0010 and ADR-0055.
- **BREAKING. A dot navigates a namespace and does nothing else.** Exactly two roots exist: `vars` and
  the new `sys`. A dotted token under any other root is a **template validation error at load**, so the
  file is quarantined and the server still starts. This is what retires the old spelling: nothing
  detects `{datetime.long_date}` as a legacy form, it simply names a source that does not exist.
- **BREAKING. `{datetime}` and `{datetime.<fmt>}` become `{sys.now}` and `{sys.now:<fmt>}`.** A bare
  token is a request parameter; every other source carries a prefix. `sys` is a sibling of `vars`
  rather than a second idea.
- **The reserved-word list is deleted.** `datetime` and `vars` become ordinary parameter names, because
  the grammar, not a word list, is what separates a parameter from a namespace. Adding a second system
  value (`{sys.hostname}`, `{sys.template_id}`) will require no change to parameter-name validation.
- **Parameter names keep `^[a-zA-Z0-9_-]+$`, with a reason a reader can follow**: a dot separates a
  namespace from a key and a colon separates a value from a format, so a bare name may contain neither.
- **BREAKING. The same rule binds every bare name, not just declared parameters**: a request `data` key
  a template means to read, and an `image` item's `name:`, SHALL be a legal bare name too. Today a data
  key may be spelled in ways a parameter never could (`{my field}`, `{a.b}`), which is the same split
  this change removes. Consequences: a CSV import header and a connector field mapped to a template
  field must both be legal bare names.
- **BREAKING. A connector field key carrying a separator is no longer nameable by a token.** Homebox's
  per-item custom fields are keyed `custom:<name>` (`src/connector/homebox.rs:511`), so a template field
  spelled `custom:Internal SKU` reads as the value `custom` with the format `Internal SKU` and is refused
  at load. Such a field stays reachable: the operator names the template field legally
  (`internal_sku`) and maps it to the connector key in the connector grid, which already maps a template
  field to any connector key. What stops working is only the same-key auto-prefill
  (`ui/src/lib/connectorRows.ts:9-13`), which for these keys could only ever prefill a field name that is
  now illegal.
- **Errors distinguish the three failures the issue asks for.** An unknown source and an unknown system
  value are decidable without runtime state, so both fail at load and quarantine the file. An unknown
  format name and an absent value stay `422 MissingField` at render, because `datetime_formats` and the
  variables store are mutable settings.
- **A format on a value that is not an instant is refused at load.** `{title:long_date}` names a format
  on a string; the head is neither `sys.now` nor a declared `type: datetime` parameter, and that is
  decidable when the template is read.
- **No compatibility window and no migration code.** The two repository fixtures using the old spelling
  are rewritten; an operator template is quarantined with a message naming the colon fix.
- `type: datetime` and the `datetime_formats` app setting **keep their names**. Both name a kind of
  value rather than an instant, and renaming the setting would cost a stored-state migration plus an
  API key change (`GET`/`PUT /api/settings/datetime_formats`, `POST /api/datetime-formats/preview`) for
  no behavior gain.

One consequence is worth stating in the open, because it is the single silent case: a bare `{datetime}`
does not fail at load. After the change it is a well-formed bare token, which means a request parameter,
indistinguishable from `{id}` or `{title}` when the template is read. A template printing it starts
advertising `datetime` as a caller-supplied field and returns `422 MissingField` when printed without
one. Closing that would take a reserved word for `datetime`, which is the list this change deletes.

## Capabilities

### New Capabilities

- `interpolation-tokens`: the complete post-change token grammar. What a token is, the two namespace
  roots, colon format attachment, which failures are decidable at load and which at render, and the
  parameter-name rules that follow from the grammar. Supersedes the "Token types and precedence" list
  in `docs/SPEC.md` §8 and the namespace/reserved-name block in §3.0.

### Modified Capabilities

- `datetime-params`: the requirement owning the `{datetime}`-shaped namespace moves wholesale into
  `interpolation-tokens` and is removed here; the parameter-declaration, default-instant and override
  requirements are restated in the new spelling.
- `connector-field-transforms`: a derived capture-group name is refused because it is not a legal bare
  token name, not because it matches a reserved word. `datetime` becomes an acceptable derived name. The
  connector-declared `custom:<name>` keys are covered by `interpolation-tokens`, which owns the rule
  about names no bare token can reach; stating it twice is what let the dot mean two things.

## Impact

- `src/render/helpers.rs` — `interpolate` and its doc comment: token parsing gains the colon split and
  the two-root dispatch, and loses the fall-through that let any unrecognised token become a data key.
- `src/datetime_fmt.rs` — `resolve` / `resolve_param` stop splitting on `.`; the `sys` root and its
  value set land here.
- `src/templates.rs` — `validate_param_name` loses the reserved-word branches; `validate_references`
  gains a walk over `text`/`qr` `value:` and `image` `src:` strings, which is the only place the new
  load-time errors can be raised. Text tokens are unchecked at load today (`validate_item_references`,
  `src/templates.rs:875`, checks `font_weight`, sizes and `when:` only).
- `src/render/mod.rs` — `collect_data_tokens` and the two `is_datetime_param` closures behind
  `template_fields` / `placeholder_data` split on the colon and exclude the `sys` root.
- `src/connector/mod.rs:244` — the reserved-namespace check becomes a bare-token-name check.
- `ui/src/lib/templateFields.ts:207-215`, `:275` — the walker hardcodes `vars.`, `datetime` and
  `datetime.`; it gains `sys.` and the colon split.
- `ui/src/pages/settings/DatetimeFormatsSection.tsx:266` — the settings help text tells the operator the
  patterns are available as `{datetime.<name>}`.
- `ui/src/lib/connectorRows.ts:9-13` (`defaultMapping`) and `src/connector/homebox.rs:511` — the same-key
  auto-prefill and the `custom:<name>` keys it prefills from, per the breaking bullet above. The CSV
  import path is affected by the same rule: a header must be a legal bare name to be a template field.
- `docs/AUTHORING.md:540-570` — the datetime section, which teaches the dotted form.
- `tests/fixtures/templates/homebox-qr.yaml:33` (`{datetime.iso_date}`) and
  `brother_24mm_printed_on.yaml:28` (`{printed_on.short_date}`). No catalog template uses a datetime
  token, so nothing under `catalog/` changes.
- `docs/adr/` — one ADR superseding ADR-0028 (the `{datetime.*}` token) and the token-list portion of
  ADR-0068, plus its row in `docs/adr/README.md`. Numbering is contested: `main`'s highest is 0076
  (`0076-the-filesystem-answers-the-case-question.md`), and the in-flight #226 worktree claims 0076, 0077
  and 0078 — its own 0076 (`0076-unify-size-resolution.md`) already collides with `main`'s. Confirm the
  next free number against `main` and every live worktree when the ADR is written.
- Operator templates using either old spelling. `{datetime.<fmt>}` and `{<param>.<fmt>}` are quarantined
  at load with a message naming the fix; bare `{datetime}` degrades to a request field as described above.
