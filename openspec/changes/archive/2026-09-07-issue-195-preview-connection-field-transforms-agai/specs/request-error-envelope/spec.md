## MODIFIED Requirements

### Requirement: Every JSON endpoint returns the mapping

The mapping above SHALL hold for every endpoint in the API that reads a JSON request body. No endpoint
is exempt, and an endpoint added later is bound by it.

The service SHALL define its JSON and path extractors in a single module, and handlers SHALL use those
rather than the web framework's own. This is what makes the mapping the default a handler gets by
writing the obvious thing, instead of something each handler opts into.

This is a convention backed by a distinct type, not a structural guarantee: no mechanism in the
language or the framework can prevent a handler from naming the framework's extractor directly. The
requirement is held by the scenario below, which enumerates the endpoints that exist, and by review.

#### Scenario: Every JSON endpoint rejects a malformed body identically

- **WHEN** an admitted caller — authenticated as required by §11, with an acceptable origin — sends a
  syntactically invalid JSON body to each endpoint in the API that reads a JSON body, including
  `POST /api/printers`, `POST /api/printers/probe`, `PUT /api/printers/{id}`,
  `PUT /api/variables/{key}`, `PUT /api/settings/{key}`, `POST /api/datetime-formats/preview`,
  `POST /api/connections`, `PUT /api/connections/{id}`, `POST /api/connections/{id}/browse`,
  `POST /api/connections/{id}/materialize`, `POST /api/connections/{id}/transforms/preview`,
  `POST /api/auth/setup`, `POST /api/auth/login`,
  `POST /api/auth/password`, `POST /api/users`, `POST /api/tokens`,
  `PUT /api/templates/{id}/group`, `POST /api/batch`, `POST /api/print` and `POST /api/render/label`
- **THEN** every response has status `400`, `error.code` `InvalidRequest` and
  `error.details.reason` `json_malformed`

#### Scenario: A body rejection is never plain text

- **WHEN** any of the endpoints above rejects a body for any reason in the mapping table
- **THEN** the response `Content-Type` is `application/json`
- **AND** the body parses as JSON with a top-level `error` object carrying `code` and `message`

