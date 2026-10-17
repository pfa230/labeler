# MCP adapter

`labeler-mcp` is a stdio [MCP](https://modelcontextprotocol.io) adapter for AI agents such as Claude Code. It forwards twelve tools to a running labeler REST service: the whole template API (list, detail, source, thumbnail, create, replace, delete, reload), filled PNG previews, printer discovery, batch printing and cutting the tape to release a held label. The service owns validation, rendering, printer connections and job attribution; the adapter owns only the MCP protocol and the HTTP calls.

## Build

From a checkout of this repository, with the pinned Rust toolchain (`rust-toolchain.toml`):

```bash
cargo build --release --locked --bin labeler-mcp
```

The binary is at `target/release/labeler-mcp`. Use an absolute path when registering it with a client.

## Connect Claude Code

Create a personal API token in the web UI (Settings → API tokens), then register the adapter. Placeholder values stand in for your paths and credentials; keep the real token outside git:

```bash
claude mcp add labeler -s user \
  -e LABELER_URL=http://127.0.0.1:8080 \
  -e LABELER_TOKEN=your-personal-token \
  -- /absolute/path/to/labeler-mcp
```

For a local service in no-auth mode, omit `-e LABELER_TOKEN`; prints are then attributed to the actor `local`.

| Variable | Meaning |
|---|---|
| `LABELER_URL` | Service origin, `http` or `https`, root path only. Default `http://127.0.0.1:8080`. An invalid value fails startup before any MCP call. |
| `LABELER_TOKEN` | Personal API token sent as `Authorization: Bearer <token>` on every request. Unset means no credential. |

## Tools

| Tool | Arguments | What it does |
|---|---|---|
| `list_templates` | — | `GET /api/templates`. Each summary carries the template's published `params`; files the service refused are listed under `broken`. |
| `get_template` | `template` | `GET /api/templates/{id}`. The published detail: identity, format, parameters and referenced variables. A quarantined template has none. |
| `get_template_source` | `template` | `GET /api/templates/{id}/source`. The stored YAML as one exact text block, comments and trailing whitespace included. Works for quarantined templates too. |
| `get_template_thumbnail` | `template` | `GET /api/templates/{id}/thumbnail`. A placeholder-filled PNG; a sheet template yields one slot, not the page. Unconditional, and never prints. |
| `create_template` | `template`, `yaml` | `POST /api/templates/{id}` with the raw YAML. An id that already exists is refused (409), never overwritten. |
| `replace_template` | `template`, `yaml` | `PUT /api/templates/{id}` with the raw YAML. The whole file is replaced; an id that does not exist is refused (404), never created. |
| `delete_template` | `template` | `DELETE /api/templates/{id}`. Removes the stored file and drops it from the live registry. |
| `reload_templates` | — | `POST /api/template-registry/reload`. Re-reads the templates folder and replaces the live registry: valid files are served, broken ones quarantined. |
| `render_label` | `template`, `data` | `POST /api/render/label?format=png`. Returns the filled label as an inline PNG image. |
| `list_printers` | — | `GET /api/printers`. The service's configured printers. |
| `print_labels` | `template`, `printer`, `labels`, `start_slot?`, `chain_and_continue?` | `POST /api/print`. Sends physical print jobs for a batch of labels. |
| `cut_printer` | `printer` | `POST /api/printers/{id}/cut`. Cuts the tape to release the last label held by chain and continue, on printers whose probe reports cut support. |

`template` is a template id: non-empty, matching `^[a-zA-Z0-9_-]+$` (letters, digits, `-` and `_`). The six path-based template-management tools (`get_template`, `get_template_source`, `get_template_thumbnail`, `create_template`, `replace_template`, `delete_template`) refuse an id outside that rule before any request is sent; `render_label` and `print_labels` forward their `template` argument without adapter validation. `printer` in `cut_printer` is a printer id and follows the same rule, refused before any request too. Any argument key outside a tool's schema is refused before any request too. Every valid id addresses that template, including the id `reload`.

`render_label` previews **single-label templates only**; the service refuses sheet templates (print those with `print_labels`). Use `get_template_thumbnail` for a sheet's one-slot preview.

Read tools are advertised as read-only. The write tools, `print_labels` and `cut_printer` are advertised as non-read-only, destructive and non-idempotent, so a client can warn before they run.

`chain_and_continue` selects the finishing of the dispatched jobs: `true` chains them and keeps the final end attached for the next print; `false` or omission prints ordinarily, with each job finished as the printer normally would. The service refuses `chain_and_continue: true` with `chain_and_continue_unsupported` before dispatch when the printer does not advertise support, rather than quietly switching to ordinary printing. A non-boolean or null value is refused by the adapter before any request is sent.

### Writes, refusals and unknown outcomes

`create_template` and `replace_template` send the `yaml` argument unchanged as the whole file, with `Content-Type: text/yaml`; the service parses and validates it, so comments and formatting survive a read-edit-write round trip. There is no partial edit: a replacement always overwrites the complete source.

Each write (create, replace, delete, reload) sends exactly one request and never follows a redirect. If the connection drops before the response arrives, the tool reports the outcome as **unknown**: the change may already be applied, and the adapter never replays it. Inspect the source or the listing before deciding on the next call.

REST refusals (invalid batch, unknown template or printer, revoked token, oversized batch, duplicate id, invalid YAML, unsupported chain-and-continue) surface as tool errors that keep the service's status and error object unchanged. A `200` print summary with failed rows comes back as a tool error carrying the full summary, so a partial failure is visible. A successful delete answers with an empty content array, matching the service's `204`.

## Authoring a template

Write the complete YAML, store it, and read it back before trusting it. A minimal single-label template:

```yaml
# shelf label
name: Shelf Label
unit: mm
dpi: 200
params:
  - name: message
    type: string
format: {type: single, width: 50, height: 20}
layout:
  - type: text
    value: "{message}"
    at: [0.0, 0.0]
    size: [50, 20]
    font_size: 12
```

Through the tools:

1. `create_template` with `template: "shelf_label"` and that document as `yaml`. The service answers with the published detail, and `shelf_label.yaml` now holds the exact bytes.
2. `get_template_source` with `template: "shelf_label"` reads that document back, comments included; `get_template` reads the published detail instead.
3. Edit the text just read — nothing else — and send the whole document again with `replace_template`. The file is overwritten; there is no merge.
4. Check the result with `render_label` (`data: {"message": "Hello"}`) or `get_template_thumbnail`.
5. `delete_template` removes the file and the served entry.

## Repairing a broken template

A file the service cannot parse or validate is quarantined: it stays on disk, appears in `list_templates` under `broken`, has no detail (`get_template` answers `404`), and is not rendered. Repair it from its source:

1. `get_template_source` for the broken id returns the file as it stands.
2. Fix the problem in that text and send it back with `replace_template`.
3. Any write, or `reload_templates`, republishes the registry; the reload answer reports how many templates are served and how many are still broken.

[`AUTHORING.md`](AUTHORING.md) documents the template format itself.

## Acceptance

Before trusting the integration, connect a local Claude Code to a local labeler, discover templates with their published parameters and the configured printers, author a template through create → source → replace → preview → delete, repair one broken file the same way, render a filled single-label PNG, and print that label to a designated real printer. Confirm the physical output: an accepted-job response alone does not show that a label emerged. Record the client results in the pull request review, keeping redacted local transcripts and render artifacts under `.agent-runs/`.
