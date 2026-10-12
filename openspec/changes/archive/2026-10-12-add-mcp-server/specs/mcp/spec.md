## Purpose

Expose labeler's template discovery, filled PNG previews and configured-printer batch dispatch to MCP clients through a stdio adapter to the REST service.

## ADDED Requirements

### Requirement: Stdio connection

`labeler-mcp` SHALL accept MCP over stdin and stdout, negotiate initialization and advertise the tools capability. Stdout MUST contain only MCP messages; diagnostics SHALL use stderr. Closing stdin SHALL end the process.

#### Scenario: Client starts the adapter

- **WHEN** a client starts `labeler-mcp` and completes MCP initialization
- **THEN** it can list and call tools over stdio without diagnostic text in protocol output

### Requirement: Service connection and credentials

The adapter SHALL connect to the service origin in `LABELER_URL`, defaulting to `http://127.0.0.1:8080`, and send `LABELER_TOKEN`, when set, as a bearer token on every REST request. `LABELER_URL` MUST be an absolute HTTP or HTTPS origin without userinfo, query or fragment; an invalid value SHALL fail startup with a diagnostic and nonzero exit. An unset token SHALL send no authentication credential. Authentication and print actors SHALL follow `auth`; the adapter SHALL neither log the token nor return it in tool results.

#### Scenario: Token belongs to a user

- **WHEN** the adapter prints through user A's valid personal token
- **THEN** the service records the print actor as A

#### Scenario: Local no-auth service

- **WHEN** `LABELER_TOKEN` is unset and the target service has no-auth mode enabled
- **THEN** the adapter's tool calls are accepted as actor `local`

#### Scenario: Invalid service origin

- **WHEN** `LABELER_URL` is `file:///tmp/labeler`
- **THEN** the process exits unsuccessfully before accepting MCP calls

### Requirement: Tool surface

The adapter SHALL publish exactly the following tools with descriptions and JSON input schemas. All arguments shown without `?` are required; arguments ending in `?` are optional and SHALL be omitted from forwarded requests when absent. Tool arguments and label entries SHALL refuse unlisted keys, while `data` SHALL allow arbitrary parameter names and JSON values. `start_slot` SHALL accept nonnegative integers and refuse explicit null.

| Tool | Arguments | Operation |
|---|---|---|
| `list_templates` | `{}` | `GET /api/templates`, including each template's published `params` |
| `render_label` | `{template: string, data: object}` | `POST /api/render/label?format=png` |
| `list_printers` | `{}` | `GET /api/printers` |
| `print_labels` | `{template: string, printer: string, labels: [{data: object}], start_slot?: integer}` | `POST /api/print` |

The first three tools SHALL carry `readOnlyHint: true`. The `list_templates` description SHALL state that each summary includes published parameters. `print_labels` SHALL carry `readOnlyHint: false`, `destructiveHint: true` and `idempotentHint: false`, and its description SHALL state that it sends physical print jobs.

#### Scenario: Client discovers print arguments

- **WHEN** a client requests the tool list
- **THEN** it sees four tools and a `print_labels` schema requiring `template`, `printer` and `labels`, with physical-print annotations

#### Scenario: An unexpected print option

- **WHEN** a client calls `print_labels` with an additional `copies` argument
- **THEN** the adapter returns a tool execution error with `isError: true` without calling the REST service

#### Scenario: Reserved-looking parameter names

- **WHEN** `render_label` receives `data` containing `constructor` or `__proto__`
- **THEN** those keys and their values are forwarded unchanged

### Requirement: Catalog results

Successful `list_templates` and `list_printers` calls SHALL return the respective REST JSON response as MCP text content containing JSON, preserving fields and values. Published parameter metadata follows `templates`; printer responses follow `printing`.

#### Scenario: Template parameters are available to the agent

- **WHEN** `list_templates` succeeds and includes a template with declared parameters
- **THEN** that template's summary includes the service's published `params` without inferring replacement defaults or controls

#### Scenario: Quarantined templates are reported

- **WHEN** the service's template list includes a `broken` collection
- **THEN** the MCP result retains that collection

### Requirement: PNG preview

Successful `render_label` SHALL return the REST PNG bytes as one MCP image content block with `mimeType: image/png` and base64 data, without printing. Its description SHALL state that it previews a `single` template; sheet refusals and parameter resolution follow `rendering` and `parameters`.

#### Scenario: A filled label is previewed

- **WHEN** an agent renders a single-label template with supplied parameter values
- **THEN** the decoded MCP image equals the PNG returned by the service for those arguments and no print request is made

#### Scenario: A sheet preview is refused

- **WHEN** `render_label` names a sheet template
- **THEN** the tool returns the service's `format_unsupported` refusal as an execution error

### Requirement: Print results

`print_labels` SHALL forward its arguments without substituting parameter values, choosing a printer or expanding copies. A `200` response SHALL retain the complete print summary as JSON text content; `isError` SHALL be true exactly when `failed` is nonempty. Batch validation, pagination and dispatch follow `rendering`; printer negotiation and job records follow `printing`. The adapter MUST NOT automatically retry a print request, and its description SHALL state that lost responses can leave dispatch outcome unknown.

#### Scenario: A sheet batch starts partway through a sheet

- **WHEN** `print_labels` receives a sheet batch with `start_slot: 2`
- **THEN** it forwards that slot and returns the service's print summary

#### Scenario: One job fails after another is accepted

- **WHEN** `/api/print` returns `200` with both accepted labels and a nonempty `failed` list
- **THEN** the tool returns the unchanged summary with `isError: true`

#### Scenario: Print response is lost

- **WHEN** the connection fails after the adapter has submitted a print request
- **THEN** the tool reports the failure and unknown dispatch outcome without resubmitting it

### Requirement: Tool errors

A REST non-success response with labeler's JSON error envelope SHALL become an MCP tool execution result with `isError: true` and JSON text `{status: <HTTP status>, error: <service error object>}`, preserving its code, message and details from `errors`. Missing required tool arguments, unlisted argument keys and invalid argument values SHALL return `isError: true` with an explanatory text message without calling REST. Connection failures, responses that cannot be decoded as the expected JSON, and successful preview responses failing the PNG signature check SHALL return `isError: true` with an explanatory text message, without fabricated REST codes or reasons. Unknown tools and malformed `CallToolRequest` envelopes SHALL use MCP protocol errors.

#### Scenario: Required print arguments are missing

- **WHEN** a valid `tools/call` envelope names `print_labels` but its arguments omit `labels`
- **THEN** the result has `isError: true` and identifies the missing argument without calling REST

#### Scenario: Call envelope is malformed

- **WHEN** a `tools/call` request omits the tool name required by the `CallToolRequest` schema
- **THEN** the adapter returns a protocol error without calling REST

#### Scenario: Batch validation fails

- **WHEN** the service refuses a print batch with `422 BatchInvalid`
- **THEN** the execution result includes status `422` and preserves every indexed failure in the service's error details

#### Scenario: Token is revoked

- **WHEN** a tool's REST request returns `401 Unauthorized`
- **THEN** the tool returns an execution error containing status `401` and the service's error object

#### Scenario: Upstream body is not a PNG

- **WHEN** a preview request receives a successful HTML response instead of PNG
- **THEN** the tool returns an execution error rather than advertising the body as an image
