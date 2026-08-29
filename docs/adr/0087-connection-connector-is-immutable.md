# 87. A connection's connector is immutable, and a contradiction is reported

Date: 2026-08-29

## Status

Accepted. Implements [#197](https://github.com/pfa230/labeler/issues/197).

## Context

A connection's connector type (e.g. `homebox`) is fixed when the connection is created via `POST
/api/connections`. Previously, `PUT /api/connections/{id}` never inspected the `connector` field in
the request payload. As a result, an update request carrying a different, mismatched, or
unregistered connector name was accepted with status `200` while silently ignoring the field.

Silently accepting a payload that contradicts stored state masks client bugs: a client believes it
changed the connector type, the server did not change it, and neither party reports the
disagreement.

## Decision

1. **Reject connector mismatches on update with `400 InvalidRequest` and reason
   `connector_immutable`.** When updating an existing connection via `PUT /api/connections/{id}`,
   the payload's `connector` value is compared to the stored `existing.connector`. If they differ,
   the request is rejected with `400 Bad Request`, `error.code = "InvalidRequest"`, and
   `details.reason = "connector_immutable"`. No changes are applied.

2. **Exact string equality without consulting the connector registry.** The invariant is that this
   specific connection's connector cannot be modified, not that the submitted string is a valid
   registered connector. Comparing exact string equality against the stored record ensures that
   unregistered names, typos, and other registered connector types are treated identically.

3. **Check precedence.** The connection ID lookup occurs first (`404` for non-existent IDs precedes
   mismatch validation). The connector immutability check executes before URL and transform
   validations, outranking errors like `base_url_invalid`. The check is performed before acquiring
   the write lock.

4. **Retain shared `ConnectionInput` payload model.** `connector` remains a required key in
   `ConnectionInput`, matching the shape used for `POST /api/connections`. No OpenAPI schema changes
   or UI changes are needed.

## Consequences

- Clients sending a `connector` value that differs from the stored connection receive a `400 Bad
  Request` with machine-readable reason `connector_immutable` instead of a silent `200 OK`.
- Existing clients sending the matching connector continue to succeed with `200 OK`.
- The error contract gains the `connector_immutable` reason code.
