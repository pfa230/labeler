# 60. Connection-scoped field transforms

Date: 2026-08-21

## Status

Accepted. Issue [#161](https://github.com/pfa230/labeler/issues/161).

## Context

Upstream inventory backends often encode composite structures into single text fields. For example, Homebox locations frequently carry compound strings such as `"BOX.123 | Garage / Shelf A"`, and item names or asset tags frequently embed category prefixes and numeric identifiers like `"TOOL-0042"`.

Label templates typically need individual pieces: a QR code needs the machine identifier (`"BOX.123"`), while human-readable text needs the friendly name (`"Garage / Shelf A"`). Prior to this change, templates had no clean way to unpack compound fields into granular tokens without hardcoding or requiring upstream schema modifications.

Templates are shared artifacts across connections and users, whereas field naming and delimiter conventions are specific to each inventory instance or workspace. Putting transform rules in the template would couple portable templates to local upstream data conventions.

## Decision

**Field transforms are connection-scoped regular expressions with named capture groups that derive virtual fields across all connector read paths.**

**1. Connection-scoped configuration.** Transforms are configured per-connection in `ConnectionInput` and stored on the `Connection` record as a list of rules: `{ resource, source, pattern }`. They belong to the connection, keeping templates decoupled and portable.

**2. Single-pass, flat non-chaining execution.** Rules on a resource run in a single pass against source values. All outputs are staged into a temporary map and merged once after all rules run. A derived field produced by one rule cannot be used as the source for another rule, eliminating recursion, ordering sensitivity, and cycle hazards.

**3. All-or-nothing participation for named capture groups.** Every named capture group defined in a rule's regular expression must participate in the match for the rule to yield any output. If any named group falls within an unexercised alternation or optional branch, the rule produces no derived fields for that row.

**4. Save-time validation against static connector descriptors.** Connectors expose static `ResourceDescriptor`s defining their resource identifiers, static columns, and dynamic prefix support (e.g., `custom:` for Homebox). Save-time validation (`validate_transforms`) rejects invalid regular expressions, missing named groups, source fields that do not exist in the descriptor, target names colliding with declared columns, duplicate derived names on the same resource, and reserved namespace prefixes (`datetime`, `datetime.*`, `vars.*`). Violations fail with `400 InvalidRequest` and reason `connection_transform_invalid`, identifying the zero-based rule index in the message.

**5. Inert behavior on resource withdrawal.** If a connector subsequently retires a resource that a stored transform references, the transform becomes inert across `schema`, `browse`, and `materialize` without breaking requests or causing errors.

**6. Universal pass across schema, browse, and materialize.**
  - `schema`: Appends derived columns with type `text` and tier `derived`.
  - `browse`: Evaluates transforms on row cells in place.
  - `materialize`: Rewrites the requested field list to request source fields from the connector, runs the connector, evaluates transforms on `data`, and projects results back to exactly the caller's requested field set.

## Consequences

- Templates can bind directly to derived field names (e.g. `{location_id}`) exactly like native connector fields.
- Upstream connector code remains focused on HTTP transport and standard mapping, while the `Connectors` wrapper uniformizes derived field handling across all connectors.
- Because derived fields are projected back at materialize time, requesting a derived field alone does not leak its underlying source field into the rendered label data.
- Non-matching rows leave derived fields absent rather than empty strings, preserving conditional layout semantics (e.g., `when: location_id`).
- Stored transforms are transparently returned on `ConnectionView` alongside non-sensitive connection details.
- Previewing transforms against live fetched rows in the UI before saving is deferred to [#195](https://github.com/pfa230/labeler/issues/195).
