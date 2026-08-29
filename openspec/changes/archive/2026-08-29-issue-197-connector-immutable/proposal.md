## Why

Implements [#197](https://github.com/pfa230/labeler/issues/197).

`update_connection_h` never reads `body.connector` (`src/api.rs:1685`), so a `PUT` whose payload
names a different or unknown connector is accepted with `200` and changes nothing, while `POST`
rejects an unknown connector with `400` and `connector_unknown` (`src/api.rs:1603`). A connection's
connector is fixed at creation, so ignoring the key is not wrong at rest, but silently accepting a
value that contradicts the stored one hides a client bug: the client believes it changed the
connector, the server believes it did not, and nothing reports the disagreement.

## What Changes

- `PUT /api/connections/{id}` compares the payload's `connector` to the stored one and rejects any
  difference with `400` and `details.reason` `connector_immutable`. The comparison is exact string
  equality against the stored value; the connector registry is not consulted, so an unregistered
  name is rejected by the same rule and gets the same reason.
- A new `connector_immutable` reason code joins the error contract.
- **BREAKING** for a client that today sends a `connector` contradicting the stored one and relies
  on `200`. The web UI is not such a client: it seeds the field from the stored connection and
  renders it disabled (`ui/src/pages/settings/ConnectionsSection.tsx:33`, `:60`, `:82`), so every
  `PUT` it sends already matches.
- `connector` stays a required key of the `PUT` payload, and a matching value still succeeds. The
  payload shape does not change, so `ConnectionInput` is still shared with `POST` and no OpenAPI
  model changes.
- Order of checks: the id lookup still runs first, so a `PUT` to an unknown id is `404` whatever the
  payload's `connector` says.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `connections`: the "Updating a connection" requirement currently states that `connector` "SHALL
  NOT be applied" and that a payload naming a different or unknown connector "SHALL neither change
  it nor fail the request". That sentence and its scenario ("Connector in an update payload is
  ignored") are replaced by the mismatch rejection, and the reason table gains
  `connector_immutable`. Everything else in the requirement (`name`, `base_url`, `public_url`,
  `enabled`, `credential`, `transforms`, `404`) is unchanged and restated intact.

## Impact

- `src/api.rs`: `update_connection_h` gains the comparison; `create_connection` and
  `ConnectionInput` are untouched.
- `src/reason.rs`: one new `Reason` variant, `ConnectorImmutable => "connector_immutable"`.
- `tests/`: the HTTP integration tests covering connection updates gain a rejection case, and any
  existing test asserting that a mismatched connector is ignored is inverted.
- `docs/adr/0087-*.md` and `docs/adr/README.md`: a new ADR recording that the connector is immutable
  and that the contradiction is reported rather than absorbed.
- No UI change, no OpenAPI model change, no store or migration change.
