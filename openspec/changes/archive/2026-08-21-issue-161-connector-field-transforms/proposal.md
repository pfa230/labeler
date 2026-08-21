## Why

Implements [#161](https://github.com/pfa230/labeler/issues/161).

Connector fields reach a label exactly as the upstream returns them. When the upstream cannot model a
concept, users encode it in a field it does have, and the label is then stuck printing the encoding.
Homebox has no custom id for locations, so a location is named `BOX.123 | Motorcycle parts`; a label
wants `BOX.123` in the QR payload and `Motorcycle parts` in the heading, and today it can have neither.
There is no seam between "what the connector returned" and "what the template binds to".

## What Changes

- A connection stores an ordered list of **field transforms**. Each names a `resource`, a `source`
  field key on that resource, and a regex `pattern` whose named capture groups become new fields.
- Transforms are applied by the connector-agnostic layer, not by any one connector, in all three
  read paths: `schema` (derived fields appear as `Derived`-tier columns), `browse` (derived cells are
  computed on the returned rows), and `materialize` (derived keys land in `LabelRow.data`). A derived
  cell shown in the Connect grid is what the label will get; browse never contradicts materialize,
  though it can stay blank where materialize will fill in, because it does not fetch per-row detail.
- A transform is bound to one resource. `entities.location` and `locations.name` are different rules;
  a rule never fires on a resource it does not name.
- **No match means the derived keys are absent**, not empty and not the source value. A pattern that
  matches while one of its named groups does not participate counts as no match for the whole rule,
  so a rule never half-fills a row. The grid shows
  a blank editable cell; a direct API caller gets `MissingField` at render, which fails that one label
  in a batch rather than the batch.
- Transforms are validated when the connection is saved, not when a label prints: the pattern must
  compile, it must declare at least one named capture group, the resource must exist on that
  connector, no derived name may collide with an upstream field key, with another output of the same
  rule, or with another rule on the same resource, and no derived name may sit in the `vars.` or
  `datetime` namespaces the renderer resolves before request data. A rejected save returns `400 InvalidRequest`
  and changes nothing.
- Transforms are a flat single pass: every rule reads only the fields the connector produced. A rule
  cannot read another rule's output, and the collision rule makes that unreachable rather than merely
  discouraged.
- `GET /connections` and `GET /connections/{id}` return the stored transforms; `POST` and `PUT` accept
  them. Omitting the field on `PUT` keeps the stored rules, matching how `credential` already behaves.
- Settings > Connections grows an editor for the rule list and surfaces the save-time rejection.
- Not in this change: previewing a rule against a live fetched row. The Connect grid is the preview.
  A follow-up issue is filed for a real-row preview if that proves insufficient.

## Capabilities

### New Capabilities

- `connector-field-transforms`: what a connection-scoped field transform is, when it is accepted,
  where it is applied, and what a row looks like when it does not match.

### Modified Capabilities

None. `openspec/specs/` holds only `template-registry`, which this change does not touch. The
connector contract this change extends lives in frozen `docs/SPEC.md` §12, so the new capability
carries the complete post-change contract for the parts it supersedes and names them.

## Impact

- **Code.** `src/connector/mod.rs` (the transform type, the pass, and its application in the
  `Connectors` wrappers), `src/connector/homebox.rs` (a static per-resource column table so
  save-time validation and `schema()` cannot drift), `src/store.rs` (a `transforms` column on
  `connections` plus a migration), `src/api.rs` (connection create/update validation and the
  response shape), `src/reason.rs` and `src/errors.rs` (one new `details.reason`), `src/openapi.rs`
  (the new model).
- **UI.** `ui/src/pages/settings/ConnectionsSection.tsx` and `ui/src/api/connectors.ts`.
- **Dependencies.** Adds the `regex` crate as a direct dependency.
- **Docs.** ADR-0059 and its row in `docs/adr/README.md`.
- **Compatibility.** Additive. A connection with no transforms behaves exactly as today; the stored
  column is absent on existing rows and reads as an empty list.
