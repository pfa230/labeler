## 1. Error contract

- [x] 1.1 Add `ConnectorImmutable => "connector_immutable"` to the `InvalidRequest` group of the
  `Reason` enum in `src/reason.rs`, keeping it next to `ConnectorUnknown`.

## 2. Handler

- [x] 2.1 In `update_connection_h` (`src/api.rs`), immediately after the `get_connection(&id)` lookup
  that yields `existing` and before `validate_and_normalize_url` runs, reject `body.connector !=
  existing.connector` with `AppError::invalid_request(Reason::ConnectorImmutable, ...)`. Exact string
  comparison, no trim, no case folding, no registry lookup. The message names both the stored
  connector and the one sent.
- [x] 2.2 Confirm nothing else moved: the `404` for an unknown id still precedes the check, the write
  lock is still taken after it, and `create_connection` is untouched.

## 3. Tests (`src/lib.rs`)

- [x] 3.1 Rewrite `update_connection_ignores_the_connector_in_the_payload` into the new contract: a
  `PUT` sending `"not-a-connector"` returns `400` with `error.code` `InvalidRequest` and
  `details.reason` `connector_immutable`. Confirm it fails against the pre-change handler (it asserts
  `200` today) before the handler is edited, so the test is proved red then green.
- [x] 3.2 Add a test that a `PUT` sending the stored connector still returns `200` and applies the
  rest of the payload, with the connector unchanged.
- [x] 3.3 Add a test that a rejected `PUT` changes nothing: read the connection back and assert the
  fields the rejected payload would have written are as they were.
- [x] 3.4 Add a test for the precedence scenario: a `PUT` whose `connector` is mismatched *and* whose
  `base_url` is `not a url` returns `connector_immutable`, not `base_url_invalid`.
- [x] 3.5 Extend `connection_endpoints_report_404_for_an_unknown_id`, or add a case beside it, so an
  unknown id with a mismatched `connector` is still `404`.
- [x] 3.6 Add a test that a `PUT` sending a connector differing only in case is rejected, so the
  byte-equality comparison cannot be loosened without a test failing.
- [x] 3.7 Add a test pinning the other end of the precedence rule: a body that does not deserialize
  is rejected by the request layer and never reports `connector_immutable`.

## 4. Decision record

- [x] 4.1 Write `docs/adr/0087-connection-connector-is-immutable.md` (Nygard: Context / Decision /
  Consequences, `Status: Accepted`), recording why a contradicting `connector` is a `400` rather than
  a silent `200` and why the comparison ignores the registry. Verify 0087 is still unused against
  `main` first.
- [x] 4.2 Add the ADR-0087 row to `docs/adr/README.md`.

## 5. Gates

- [x] 5.1 Run `cargo fmt`, `cargo clippy --all-targets --all-features`, and `cargo test`; no
  `#[allow(clippy::...)]` added. All three pass with the delta still live:
  `spec_documents_every_reason_and_invents_none` scans active change deltas as well as
  `openspec/specs/**/spec.md` (#217), so `connector_immutable` counts as documented before archive
  publishes it. Do not widen that scanner further (commit `1dc9991` removed exactly that widening).
- [x] 5.2 Exercise the new rejection against a running server
  (`LABELER_CONFIG_DIR=./config-dev LABELER_NO_AUTH=true cargo run`): create a connection, `PUT` it
  with a wrong `connector`, and confirm the response body is `400` with
  `error.details.reason == "connector_immutable"`; then `PUT` it with the stored connector and
  confirm `200`.
