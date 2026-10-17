## MODIFIED Requirements

### Requirement: Tool surface

The adapter SHALL publish exactly the following tools with descriptions and JSON input schemas. All arguments shown without `?` are required; arguments ending in `?` are optional and SHALL be omitted from forwarded requests when absent. Tool arguments and label entries SHALL refuse unlisted keys, while `data` SHALL allow arbitrary parameter names and JSON values. `start_slot` SHALL accept nonnegative integers and refuse explicit null. `chain_and_continue` SHALL accept only boolean values and refuse explicit null. Template identifiers in the seven template-management tools SHALL follow the id rule in `templates`; invalid identifiers SHALL return tool execution errors before calling REST. Every valid identifier, including `reload`, SHALL address that template rather than a different operation. The `cut_printer` identifier SHALL follow the printer id rule in `printing`; an invalid identifier SHALL return a tool execution error before calling REST.

| Tool | Arguments | Operation |
|---|---|---|
| `list_templates` | `{}` | `GET /api/templates`, including each template's published `params` |
| `get_template` | `{template: string}` | `GET /api/templates/{id}` |
| `get_template_source` | `{template: string}` | `GET /api/templates/{id}/source` |
| `get_template_thumbnail` | `{template: string}` | `GET /api/templates/{id}/thumbnail` |
| `create_template` | `{template: string, yaml: string}` | `POST /api/templates/{id}` with raw YAML |
| `replace_template` | `{template: string, yaml: string}` | `PUT /api/templates/{id}` with raw YAML |
| `delete_template` | `{template: string}` | `DELETE /api/templates/{id}` |
| `reload_templates` | `{}` | `POST /api/template-registry/reload` |
| `render_label` | `{template: string, data: object}` | `POST /api/render/label?format=png` |
| `list_printers` | `{}` | `GET /api/printers` |
| `print_labels` | `{template: string, printer: string, labels: [{data: object}], start_slot?: integer, chain_and_continue?: boolean}` | `POST /api/print` |
| `cut_printer` | `{printer: string}` | `POST /api/printers/{id}/cut` |

`list_templates`, `get_template`, `get_template_source`, `get_template_thumbnail`, `render_label` and `list_printers` SHALL carry `readOnlyHint: true`. `create_template`, `replace_template`, `delete_template`, `reload_templates`, `print_labels` and `cut_printer` SHALL carry `readOnlyHint: false`, `destructiveHint: true` and `idempotentHint: false`. The `list_templates` description SHALL state that each summary includes published parameters. The write descriptions SHALL distinguish create from complete-source replacement and deletion; `reload_templates` SHALL describe replacing the live registry from disk. The `print_labels` description SHALL state that it sends physical print jobs and explain that `chain_and_continue: true` selects chain and continue, while false or omission selects ordinary printing (`printing`). The `cut_printer` description SHALL state that it cuts the tape to release the last label held by chain and continue, on printers whose probe reports `cut`.

#### Scenario: Client discovers print arguments

- **WHEN** a client requests the tool list
- **THEN** it sees twelve tools and a `print_labels` schema requiring `template`, `printer` and `labels`, with physical-print annotations
- **AND** `chain_and_continue` is an optional, nonnullable boolean with no null default, and the description explains both selections

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

#### Scenario: Invalid finishing argument

- **WHEN** a client calls `print_labels` with `chain_and_continue` set to null, a string or a number
- **THEN** the adapter returns a tool execution error with `isError: true` without calling REST

#### Scenario: An invalid printer identifier

- **WHEN** `cut_printer` receives `printer: "../other"`
- **THEN** it returns an execution error without making an HTTP request
