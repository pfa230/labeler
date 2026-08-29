## Context

See `proposal.md` - Why. Three facts shape the approach:

- `update_connection_h` (`src/api.rs:1685`) already loads the stored connection first and already
  reads its `existing.connector` to pick the connector for transform validation (`src/api.rs:1695`).
  The stored value is therefore in hand before any field is validated; nothing new has to be fetched.
- `homebox` is the only registered connector (`src/connector/mod.rs:597-604`). Every mismatched
  payload reachable today names something the registry does not know.
- `PUT` and `POST` share one payload type, `ConnectionInput` (`src/api.rs:1491`), which is what
  `openapi.rs` publishes for both.

## Goals / Non-Goals

**Goals:**

- One rule that holds whatever the connector registry contains, now and after a second connector
  ships.
- No change to the payload shape, the OpenAPI models, the store, or the UI.

**Non-Goals:**

- Making the connector mutable. It stays fixed at creation; this change only reports the
  contradiction instead of absorbing it.
- Touching `POST /api/connections`, whose `connector_unknown` rejection (`src/api.rs:1603`) is
  correct and stays as it is.
- Tightening `PUT` against other stray keys. `ConnectionInput` has no `deny_unknown_fields` and does
  not gain one here.

## Decisions

**Compare for exact equality with the stored value; do not consult the registry.** The invariant is
"this connection's connector cannot change", not "this name is a connector", so the stored value is
the only correct thing to compare against. The alternative, looking the payload's name up in the
registry and splitting the outcome into `connector_unknown` for an unregistered name and
`connector_immutable` for a registered but different one, was rejected: with one connector
registered, every mismatch is an unregistered name, so the split would leave `connector_immutable`
unreachable and would report a typo and an edit aimed at the wrong connection with the same code
until a second connector ships. Introducing that split later would be a change to this contract, not
an elaboration of it: the rule as written requires every mismatch, an unregistered name included, to
report `connector_immutable` without consulting the registry, so narrowing the cases that keep the
reason would need its own change and its own ADR.

**Byte equality, no trimming and no case folding.** `ConnectorRegistry::get` matches ids with a
literal `match id` (`src/connector/mod.rs:598`), so `"Homebox"` is not a connector and
`" homebox"` is not a connector. A comparison looser than the registry's own would accept a value
the rest of the system would reject.

**A new reason code, `connector_immutable`.** Reusing `connector_unknown` would misname the case the
rule exists for: a payload naming a real connector that is not this connection's is not unknown, it
is refused. The frozen `docs/SPEC.md` §10.1 is not edited; the spec delta is the published home for
the new entry and states the whole mapping (`InvalidRequest` / `400` / `connector_immutable`). This
follows what the repo already does: `public_url_invalid`, `connection_transform_invalid` and
`datetime_param_invalid` all live in `src/reason.rs` and are absent from that table. §10.1's parity
promise is enforced, by `spec_documents_every_reason_and_invents_none` in `src/errors.rs`, but it
reads the §10.1 table *plus* `openspec/specs/**/spec.md`, so a post-freeze reason satisfies it from
its OpenSpec home. It counts nothing else on purpose: a change folder holds proposed truth, and
accepting one was removed as a defect in commit `1dc9991`. The test is therefore red from the moment
the enum variant lands until archive syncs this delta into `openspec/specs/connections/spec.md`, and
green after. No task should widen the scanner to close that window; `docs/SPEC.md` stays unedited.

**Check immediately after the id lookup, before URL and transform validation and before the write
lock is taken.** Three consequences, all intended: a `PUT` to an unknown id stays `404` whatever its
`connector` says, because the lookup still runs first; a payload carrying both a mismatched
`connector` and an invalid `base_url` reports `connector_immutable`, because the question "are you
editing the connection you think you are" outranks "is this field well formed", a precedence the
spec pins in its own scenario so it is a contract rather than an accident of statement order; and the
rejection costs no lock. There is no race to guard: a connector never changes after creation, so the
value read before the lock cannot go stale.

**Leave `ConnectionInput` shared and required.** Giving `PUT` its own type without `connector` was
the issue's other option; the user chose rejection. Keeping one type means no OpenAPI model change,
no divergence between the two payload shapes, and no UI change: the settings form seeds the field
from the stored connection and renders it disabled
(`ui/src/pages/settings/ConnectionsSection.tsx:33`, `:60`, `:82`).

**Keep the scenario label "Connector in an update payload is ignored".** OpenSpec's validator refuses
a `MODIFIED` block that drops a scenario name the current spec still has, and offers no operation for
retiring one (a `REMOVED` plus `ADDED` of the same requirement name is rejected outright). The label
survives honestly: the supplied connector is still never written to the record, it is now compared
first. Its body states the surviving half of the old rule, and the new scenario alongside it states
the rejection.

**ADR-0087, "A connection's connector is immutable, and a contradiction is reported".** It records
why the contradiction is a `400` rather than a silent `200` and why the comparison ignores the
registry. It supersedes nothing; ADR-0063 and ADR-0060 cover neighbouring parts of the connection
record and stay as they are. Confirm 0087 is still free against `main` before writing the file:
`docs/adr/` on `main` runs to 0069 with 0067 absent.

## Risks / Trade-offs

- **A client that today sends a contradicting `connector` and relies on `200` starts getting `400`.**
  → That client has the bug this change exists to surface. The only shipped client cannot hit it: the
  UI sends the stored value from a disabled control. The break is stated in the proposal, the spec
  delta and the ADR.
- **`connector_immutable` is unreachable for a registered-but-different name until a second connector
  exists**, so the "different registered connector" half of the rule ships untested by construction.
  → The rule is written so it needs no revision then: exact equality already covers that input, and
  the test that exercises an unregistered name exercises the same branch.
- **A mismatched connector now masks a malformed `base_url` in the same payload**, so a client fixing
  one error at a time makes two round trips. → Accepted; the ordering is deliberate, and the response
  names the field it rejected.

## Migration Plan

None. No stored data changes, no setting changes, no template changes. Rollback is a revert of the
commit; nothing persists that a rolled-back server would misread.
