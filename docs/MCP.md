# MCP adapter

`labeler-mcp` is a stdio [MCP](https://modelcontextprotocol.io) adapter for AI agents such as Claude Code. It forwards four tools to a running labeler REST service: template discovery, filled PNG previews, printer discovery and batch printing. The service owns validation, rendering, printer connections and job attribution; the adapter owns only the MCP protocol and the HTTP calls.

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

| Tool | What it does |
|---|---|
| `list_templates` | `GET /api/templates`. Each summary carries the template's published `params`. |
| `render_label` | `POST /api/render/label?format=png`. Returns the filled label as an inline PNG image. |
| `list_printers` | `GET /api/printers`. The service's configured printers. |
| `print_labels` | `POST /api/print`. Sends physical print jobs for a batch of labels. |

`render_label` previews **single-label templates only**; the service refuses sheet templates (print those with `print_labels`). Argument keys outside each tool's schema are rejected before any request is sent.

`print_labels` has a physical effect. It never retries: one call sends at most one print request, and a lost response is reported as an unknown dispatch outcome rather than resent. A `200` summary with failed rows comes back as a tool error carrying the full summary, so a partial failure is visible.

REST refusals (invalid batch, unknown template or printer, revoked token, oversized batch) surface as tool errors that keep the service's status and error object unchanged.

## Acceptance

Before trusting the integration, connect a local Claude Code to a local labeler, discover templates with their published parameters and the configured printers, render a filled single-label PNG, and print that label to a designated real printer. Confirm the physical output: an accepted-job response alone does not show that a label emerged. Record the client results in the pull request review, keeping redacted local transcripts and render artifacts under `.agent-runs/`.
