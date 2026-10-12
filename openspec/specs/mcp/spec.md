# mcp Specification

## Purpose
Expose labeler's template discovery, filled PNG previews and configured-printer batch dispatch to MCP clients through a stdio adapter to the REST service.

## Requirements

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

The adapter SHALL publish exactly the following tools with descriptions and JSON input schemas. All arguments shown without `?` are required; arguments ending in `?` are optional and SHALL be omitted from forwarded requests when absent. Tool arguments and label entries SHALL refuse unlisted keys, while `data` SHALL allow arbitrary parameter names and JSON values. `start_slot` SHALL accept nonnegative integers and refuse explicit null. Template identifiers in the seven template-management tools SHALL follow the id rule in `templates`; invalid identifiers SHALL return tool execution errors before calling REST. Every valid identifier, including `reload`, SHALL address that template rather than a different operation.

| Tool | Arguments | Operation |
|---|---|---|
| `list_templates` | `{}` | `GET /api/templates`, including each template's published `params` |
| `get_template` | `{template: string}` | `GET /api/templates/{id}` |
| `get_template_source` | `{template: string}` | `GET /api/templates/{id}/source` |
| `get_template_thumbnail` | `{template: string}` | `GET /api/templates/{id}/thumbnail` |
| `create_template` | `{template: string, yaml: string}` | `POST /api/templates/{id}` with raw YAML |
| `replace_template` | `{template: string, yaml: string}` | `PUT /api/templates/{id}` with raw YAML |
| `delete_template` | `{template: string}` | `DELETE /api/templates/{id}` |
| `reload_templates` | `{}` | `POST /api/templates/reload` |
| `render_label` | `{template: string, data: object}` | `POST /api/render/label?format=png` |
| `list_printers` | `{}` | `GET /api/printers` |
| `print_labels` | `{template: string, printer: string, labels: [{data: object}], start_slot?: integer}` | `POST /api/print` |

`list_templates`, `get_template`, `get_template_source`, `get_template_thumbnail`, `render_label` and `list_printers` SHALL carry `readOnlyHint: true`. `create_template`, `replace_template`, `delete_template`, `reload_templates` and `print_labels` SHALL carry `readOnlyHint: false`, `destructiveHint: true` and `idempotentHint: false`. The `list_templates` description SHALL state that each summary includes published parameters. The write descriptions SHALL distinguish create from complete-source replacement and deletion; `reload_templates` SHALL describe replacing the live registry from disk. The `print_labels` description SHALL state that it sends physical print jobs.

#### Scenario: Client discovers print arguments

- **WHEN** a client requests the tool list
- **THEN** it sees eleven tools and a `print_labels` schema requiring `template`, `printer` and `labels`, with physical-print annotations

#### Scenario: An unexpected print option

- **WHEN** a client calls `print_labels` with an additional `copies` argument
- **THEN** the adapter returns a tool execution error with `isError: true` without calling the REST service

#### Scenario: Reserved-looking parameter names

- **WHEN** `render_label` receives `data` containing `constructor` or `__proto__`
- **THEN** those keys and their values are forwarded unchanged

#### Scenario: An agent discovers editing

- **WHEN** a client requests the tool list
- **THEN** `replace_template` requires a template identifier and a YAML string and advertises complete-source replacement as a mutation

#### Scenario: A template identifier cannot change the operation

- **WHEN** `create_template` is called for the valid identifier `reload`
- **THEN** it targets template creation and does not invoke registry reload in place of creation

#### Scenario: An invalid template identifier

- **WHEN** `get_template_source` receives `template: "../other"`
- **THEN** it returns an execution error without making an HTTP request

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

A REST non-success response with labeler's JSON error envelope SHALL become an MCP tool execution result with `isError: true` and JSON text `{status: <HTTP status>, error: <service error object>}`, preserving its code, message and details from `errors`. Missing required tool arguments, unlisted argument keys and invalid argument values SHALL return `isError: true` with an explanatory text message without calling REST. Connection failures, responses that cannot be decoded as the expected JSON or UTF-8 source text, and successful PNG responses failing the PNG signature check SHALL return `isError: true` with an explanatory text message, without fabricated REST codes or reasons. Unknown tools and malformed `CallToolRequest` envelopes SHALL use MCP protocol errors.

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

- **WHEN** a preview or thumbnail request receives a successful HTML response instead of PNG
- **THEN** the tool returns an execution error rather than advertising the body as an image

#### Scenario: Source bytes cannot be represented as text

- **WHEN** the source endpoint returns a successful body that is not valid UTF-8
- **THEN** the tool returns an execution error rather than replacing bytes with lossy text

### Requirement: Template detail results

Successful `get_template` SHALL return the complete REST TemplateDetail as MCP text content containing JSON, preserving fields and values. Detail and published metadata follow `templates`.

#### Scenario: Referenced variables remain visible

- **WHEN** the service returns a detail with `params` and `variables`
- **THEN** both fields and their values appear unchanged in the tool result

### Requirement: Template source results

Successful `get_template_source` SHALL return the stored UTF-8 YAML as one MCP text content block without parsing, reformatting or wrapping it in JSON. Source access for served and broken templates follows `templates`.

#### Scenario: Read an editable source

- **WHEN** the stored YAML contains comments and trailing whitespace
- **THEN** the tool's text equals the stored source, including those comments and whitespace

#### Scenario: Read a broken template

- **WHEN** the service returns the source of a quarantined template
- **THEN** the source tool succeeds even though the detail tool returns the service's not-found error

### Requirement: Template thumbnail results

Successful `get_template_thumbnail` SHALL return the REST PNG bytes as one MCP image content block with `mimeType: image/png` and base64 data. It SHALL make an unconditional thumbnail request without printing. Placeholder values and single-template or sheet-slot dimensions follow `templates`.

#### Scenario: A sheet thumbnail is available

- **WHEN** the agent calls `get_template_thumbnail` for a sheet template
- **THEN** the decoded image equals the service's one-slot thumbnail and no print request is sent

### Requirement: Template mutation results

`create_template` and `replace_template` SHALL send the supplied YAML string unchanged as the raw request body with `Content-Type: text/yaml`. Their successful results SHALL preserve the complete returned TemplateDetail as JSON text content. Successful `delete_template` SHALL represent the service's `204` as a successful MCP tool result with an empty content array. Validation, existence checks, atomic writes, registry updates and deletion effects follow `templates`.

#### Scenario: Create and then edit a template

- **WHEN** an agent creates a new template with YAML containing comments and then replaces it with edited YAML
- **THEN** both requests send the exact supplied YAML and return the corresponding service detail

#### Scenario: A taken identifier is not overwritten

- **WHEN** the service refuses a create with `409 Conflict`
- **THEN** the tool returns that refusal as an execution error and does not attempt replacement

#### Scenario: An absent template is not created by editing

- **WHEN** the service refuses a replacement with `404 NotFound`
- **THEN** the tool returns that refusal and does not attempt creation

#### Scenario: Invalid YAML remains a service refusal

- **WHEN** the service refuses a write with `422 TemplateInvalid`
- **THEN** the tool retains its status and error object, including the validation path

#### Scenario: Deletion has no JSON response body

- **WHEN** the service answers a delete with `204` and no body
- **THEN** the MCP result succeeds with `content: []`

### Requirement: Registry reload results

Successful `reload_templates` SHALL return the complete REST reload JSON as MCP text content. Registry replacement, refusal reporting and preservation on reload failure follow `templates`; a nonzero `broken_count` in a successful reload SHALL NOT make the tool result an error.

#### Scenario: Reload includes quarantined files

- **WHEN** the service returns `200` with `count: 2` and `broken_count: 1`
- **THEN** the tool preserves both counts in a successful result

### Requirement: Template mutation request delivery

The adapter MUST NOT automatically retry or follow redirects for create, replace, delete or reload requests. A connection failure after submission SHALL report an execution error and an unknown mutation outcome; the tool descriptions SHALL explain that lost responses can leave changes applied. The adapter SHALL NOT attempt compensating writes or create/replace fallbacks.

#### Scenario: A write response is lost

- **WHEN** the service receives a template write and the connection fails before the response reaches the adapter
- **THEN** the tool reports an unknown outcome and submits no second mutation request

#### Scenario: A mutation redirects

- **WHEN** a mutation request receives a redirect response
- **THEN** the adapter does not follow it or replay the mutation at the redirect target
