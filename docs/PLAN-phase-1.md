# Phase 1 (MVP) — Work Plan

**Status:** Planning artifact. This breaks Phase 1 into issues with dependencies, milestones, and
acceptance criteria. It seeds the GitHub issues; per [CLAUDE.md](../CLAUDE.md), the issues themselves
are the live tracker. Plan IDs (`P1-xx`) are stable and used to express dependencies; GitHub issue
numbers will differ, so record the mapping in the "GH #" column when issues are filed.

Scope is the **MVP** tier of [CAPABILITIES.md](CAPABILITIES.md), plus a Homebox data-source integration
pulled forward into Phase 1 (M7). The GUI editor and the broader integration framework (InvenTree, more
connectors) stay **Phase 2**.

**Progress.** GitHub milestones M1–M6 hold live status; completed items are also marked **DONE** (with
their commit) in the issue list below. Done so far: prerequisite ADRs P1-D1 / #1 (printer architecture, ADR-0007) and P1-D2 / #2 (UI delivery,
ADR-0008); M1 — P1-11 / #3 (image), P1-12 / #4 (single-label PDF), P1-14 / #6 (line at/to); M2 —
P1-21 / #7 (reload), P1-22 / #10 (CRUD), P1-23 / #11 (tape templates); M3 — P1-31 / #8 (store),
P1-32 / #12 (printer CRUD), P1-33 / #16 (CUPS/IPP), P1-34 / #13 (download), P1-35 / #19 (/print).
The standalone `/print` and `/render/batch` endpoints were later removed and absorbed into the unified
`POST /batch` endpoint (ADR-0011, #30), which delivers sheet-to-printer and multi-page sheets.
P1-13 / #5 (copies) was deferred pending the per-label batch-composition ADR (#28); with `/batch`,
copies is expanded client-side and #28 is moot. Remaining: M4 (integrations), M5 (UI), M6 (packaging).

## 1. Phase 1 goal

A self-hosted, Dockerized label service that an SMB/home user can run, point at hand-authored YAML
templates (plus a bundled starter library), fill in data through a basic web UI or the API, preview
the result, and print to an office/sheet printer via CUPS or download the artifact. CSV batch import
and an inbound print webhook are the Phase 1 integrations.

### Explicitly deferred to Phase 2 (not in Phase 1)

GUI template editor; 1D barcodes; ZPL/Zebra and Brother QL/Dymo network printing; the broader
integration framework beyond Homebox (InvenTree, additional connectors, declarative connector DSL);
CSV field-mapping UI (Phase 1 ships header-name auto-match only); API token / app-level auth; printer
status. (Homebox itself moved into Phase 1 as M7.)

## 2. Prerequisite decisions (ADRs)

These unblock implementation milestones and should be written first.

| ID | ADR | Blocks | Acceptance |
| --- | --- | --- | --- |
| P1-D1 (#1) | Printer architecture and transport model (machine instances; CUPS-first; PDF as the print payload) | M3 | ADR-0007 accepted; defines the printer entity, the driver/transport abstraction, and how CUPS is invoked from the container. |
| P1-D2 (#2) | UI delivery (SPA vs server-rendered; served by the same service vs separate frontend) | M5 | ADR-0008 accepted; names the framework/approach and the build/serve story inside the Docker image. |

## 3. Milestones

| Milestone | Goal | Exit criteria |
| --- | --- | --- |
| **M1 Rendering completeness** | Render everything Phase 1 needs | Image item, single-label PDF, and copies all render and are covered by tests. |
| **M2 Template management** | Author and persist templates without a restart | Hot-reload works; upload/replace/delete via API persists and validates; starter library bundled. |
| **M3 State and printing** | Get a rendered label onto paper | App-state store live; printers CRUD; print to CUPS; file-download fallback; unified `/batch` dispatch (formerly `/print`). |
| **M4 Integrations and import** | Drive printing from data and events | QR base-URL mapping; CSV batch import; documented inbound print webhook. |
| **M5 Basic web UI** | Operate the service without curl | Shell + browse/preview + render/print form + CSV screen + settings, all working end to end. |
| **M6 Packaging and deployment** | One-command self-host | Docker image; compose with persistent volumes; CUPS access documented and wired; env config. |
| **M7 External data sources (Homebox)** | Print from real inventory | Browse Homebox in the UI, select records, map to a template, print/download via `/batch`; built on a scoped `Connector` spine. |

## 4. Issues

Each issue lists intent, dependencies (by plan ID), and acceptance criteria. "AC" are testable
conditions; backend issues require unit/integration tests per the verification rules in CLAUDE.md.

### M1 — Rendering completeness

#### P1-11 Image layout item · GH #3 · DONE (00e1a30)
Add an `image` layout item that embeds a raster/SVG into the rendered label via Typst.
- **Depends on:** none.
- **AC:** template with an `image` item (data-bound source and/or static asset) renders in both PNG and
  PDF; schema added across `raw.rs`/`models.rs`/`convert.rs` (ADR-0002); bounds validated like other
  items; positive and negative tests pass.

#### P1-12 Single-label PDF output · GH #4 · DONE (39ccd08)
Render `format: single` templates to PDF in addition to PNG (needed for office printing of one label).
- **Depends on:** none.
- **AC:** the render path returns PDF for single-format templates (via format selector or content
  negotiation); PNG path unchanged; test asserts a valid `%PDF` for a single template.

#### P1-13 Copies / quantity · GH #5 · BLOCKED (needs #28)
Support per-label quantities. **Deferred from the initial M1 pass:** copies is one facet of per-label
batch composition, which needs a design decision first (single-label physical copies are already handled
by the printer/CUPS/browser, so app-level copies is really a sheet concern).
- **Depends on:** #28 (ADR: per-label configuration and batch composition).
- **AC:** defined by #28; implement against the decided batch model.
- **Note (ADR-0011, #30):** the unified `POST /batch` endpoint delivered sheet-to-printer and multi-page
  sheets, and server-side `copies` is moot under it (a client expands copies into repeated `labels`
  entries). #28 is rescoped/closeable accordingly; no separate server-side `copies` parameter is planned.

#### P1-14 Fix `line` `size` semantics inconsistency · GH #6 · DONE (b8c3cf2)
For most items `size` is a box `[w, h]`; for `line` it is a delta `[dx, dy]` from `at`. This is a
schema wart that hurts intuitiveness (see CAPABILITIES §3.1). Resolve it so `line` geometry reads
consistently (e.g. explicit `to`/endpoint, or a documented dedicated field), updating schema, render,
validation, and the starter/sample templates.
- **Depends on:** none.
- **AC:** `line` geometry uses a clear, documented representation distinct from box `size`; existing
  templates migrated; SPEC and ADR-0005/the model docs updated; render output for lines unchanged
  pixel-wise; tests cover the new form and reject the old ambiguous one.

### M2 — Template management

#### P1-21 Template hot-reload · GH #7 · DONE (6ac0ec3)
Reload the template registry when files in the manual store change, without a server restart.
- **Depends on:** none.
- **AC:** adding/editing/removing a `.yaml` in `templates/` updates `/templates` within a bounded delay
  or on an explicit reload endpoint; a newly invalid file is reported and does not crash the service;
  tested.

#### P1-22 Template upload / replace / delete API · GH #10 · DONE (c7bdbd3)
CRUD for manual templates over the API, writing validated YAML to the manual store.
- **Depends on:** P1-21.
- **AC:** `POST`/`PUT`/`DELETE` template endpoints; invalid YAML rejected with the existing path-aware
  error before any write; changes persist across restart; duplicate-id rejected; tested. (GUI-owned
  store and edit-ownership per ADR-0006 are Phase 2.)

#### P1-23 Starter template library · GH #11 · DONE (713ecd0)
Bundle ready-to-use templates (e.g. Avery 5160, Avery 5163, Brother 12mm, Dymo 30252).
- **Depends on:** P1-11 (image item, if any starter uses a logo) — soft.
- **AC:** templates present on a fresh install; each renders correctly to its format; documented in
  README/SPEC; covered by a render smoke test.

### M3 — State and printing

#### P1-31 App-state store (SQLite) · GH #8 · DONE (f7a6641)
Introduce a persistent store for app state: printers, settings, and a minimal job record.
- **Depends on:** none.
- **AC:** SQLite file created/migrated on startup under a configurable data dir; survives restart;
  templates remain files (ADR-0006), only app state is in SQLite; tested.

#### P1-32 Printer configuration CRUD · GH #12 · DONE (e786ac7)
Model printers as configured "machine" instances (name, type, transport, options) and CRUD them.
- **Depends on:** P1-31, P1-D1.
- **AC:** create/list/update/delete printers via API; persisted; a CUPS printer can be registered by
  queue name; validated; tested.

#### P1-33 CUPS / IPP output backend · GH #16 · DONE (31fdb49)
Send a rendered PDF to a CUPS print queue.
- **Depends on:** P1-32, P1-12.
- **AC:** given a reachable CUPS queue, a single and a sheet job both print (verified against a CUPS
  instance or a mock/IPP capture); media/copies options passed through; failures surface as a clear
  error.

#### P1-34 File-download output · GH #13 · DONE (9fd67da)
Return the rendered artifact (PNG/PDF) as a download when no printer is selected.
- **Depends on:** P1-12.
- **AC:** endpoint returns the artifact with correct `Content-Type` and filename headers; works for
  both formats; tested.

#### P1-35 Unified `/print` dispatch endpoint · GH #19 · DONE (e147439) · SUPERSEDED by `/batch` (#30)
One endpoint that renders `template` + `data` (+`copies`) and either routes to a chosen printer or
returns the file. **Superseded:** `/print` was later removed and absorbed into `POST /batch`
(ADR-0011, #30), which generalizes dispatch to batches across both template formats.
- **Depends on:** P1-32, P1-33, P1-34, P1-13.
- **AC:** `POST /print` prints to a named printer or returns the artifact when none is given; honors
  copies; 404 on unknown template/printer; missing-field and option errors preserved; tested.

### M4 — Integrations and import

#### P1-41 Configurable QR base-URL + id-field mapping · GH #14 — DONE (M4 branch)
A setting so QR content can be composed as `{base_url}/{id_field_value}`. Delivered via the substitution
interpolation layer (ADR-0010): a `value` field on text/qr items resolves `{settings.qr_base_url}/{id}`.
See the `homebox-qr` demo template.
- **Depends on:** P1-31.
- **AC:** configured base URL plus a per-template/request id field produces a URL-encoded QR; absent
  config falls back to literal QR content; tested.

#### P1-42 CSV import (one label per row) · GH #21 — DONE (M4 branch)
Upload a CSV and render/print one label per row, mapping columns to fields by header name.
Delivered as `POST /import/csv`: `mode=download` returns an atomic ZIP (one file per row),
`mode=print` dispatches per-row jobs and returns a `{ total, succeeded, failed }` summary.
- **Depends on:** P1-35.
- **AC:** CSV with headers matching template fields produces N labels (shared `/batch` path); per-row
  missing field reported by index; a downloadable starter CSV is provided; quoted fields and BOM handled;
  tested. (Interactive field-mapping UI is Phase 2.)

#### P1-43 Inbound print webhook (contract + LAN hardening) · GH #22 — DEFERRED (out of M4)
Document and finalize the batch print path (`POST /batch` with `mode: print`) as the integration
webhook for tools like Grocy.
- **Depends on:** P1-35.
- **AC:** documented payload schema and examples in SPEC; trusted-LAN assumption stated; oversized/
  malformed payloads rejected cleanly; example receiver config documented.

### M5 — Basic web UI

#### P1-51 Web UI shell and serving · GH #15
App shell (navigation, layout, theming) served by the service per ADR-0008.
- **Depends on:** P1-D2.
- **AC:** UI served at `/` (or documented path); responsive; builds within the Docker image; no console
  errors on load.

#### P1-52 Template browse + preview · GH #17
List templates with a rendered preview thumbnail and a detail view.
- **Depends on:** P1-51, P1-12.
- **AC:** templates list from `/templates`; selecting one shows a server-rendered preview for sample
  data; sheet vs single indicated.

#### P1-53 Render / print form · GH #20 — DONE
Pick a template, fill fields (and options), preview, then print to a printer or download.
- **Depends on:** P1-51, P1-35.
- **AC:** end-to-end: choose template → form generated from fields → live/triggered preview → print or
  download succeeds; errors shown inline.

#### P1-54 CSV import screen · GH #24 · DONE (f5a84d4)
Upload a CSV, review rows, and batch print/download.
- **Depends on:** P1-51, P1-42.
- **AC:** from the UI, a CSV produces a batch via `/batch`; row/field errors shown; download or print
  selectable.

#### P1-55 Settings + printers screen · GH #23 · DONE (efbc668)
Configure QR base URL, printers, and view the webhook endpoint.
- **Depends on:** P1-51, P1-32, P1-41.
- **AC:** add/edit/remove printers; set QR base URL; settings persist; webhook URL/example shown.

### M6 — Packaging and deployment

#### P1-61 Dockerfile (single image) · GH #18 · DONE (d5f24b9)
Build the service into one image with fonts bundled.
- **Depends on:** core service runnable (M1–M3 in practice).
- **AC:** `docker build` produces an image that runs the server and serves the UI; Inter fonts present;
  image documented. (Multi-stage distroless image; verified by build + smoke.)

#### P1-62 docker-compose + persistent volumes · GH #25 · DONE (165c9a1)
Compose file with volumes for templates and the app-state store.
- **Depends on:** P1-61, P1-31, P1-22.
- **AC:** `docker compose up` starts the service; templates and SQLite persist across recreate;
  documented. (Verified: seeded templates + setting persisted across `down`/`up`.)

#### P1-63 CUPS access documentation + wiring · GH #26 · DONE (db1639a)
Document and wire how the container reaches CUPS (socket mount or host gateway).
- **Depends on:** P1-33, P1-62.
- **AC met (rescoped):** the implementation supports **network IPP only** (ADR-0016, user-approved);
  `docs/DEPLOY.md` documents the network-CUPS reachability prerequisites AND explains why the
  socket-mount option is intentionally not provided (the app is an `ipp`-crate TCP client, not a
  libcups/socket client; socket mounts are Linux-host-only). The "both options with trade-offs"
  requirement is met by documenting and justifying the rejection.

#### P1-64 Env-var configuration + sample env · GH #9 · DONE (165c9a1)
Consolidate configuration (PORT, data dir, QR base URL, log level) into documented env vars.
- **Depends on:** none.
- **AC met (rescoped):** config is env-driven with defaults (`PORT`, `LABELER_DATA_DIR`,
  `LABELER_UI_DIR`, `LABELER_ASSETS_DIR`, `RUST_LOG`), `.env.sample` + `docs/DEPLOY.md` document the
  contract, and the service starts with zero required config. The QR base URL is a runtime *setting*
  (`PUT /api/settings/qr_base_url`, ADR-0010), not an env var; making the template/font dirs
  env-configurable is deferred to #38.

### M7 — External data sources (Homebox)

Pulled forward from Phase 2 so Phase 1 can print from real inventory. Builds the minimum integration
"spine" (design: [api integration framework](superpowers/specs/2026-06-16-api-integration-framework-design.md)),
scoped to Homebox. The broader framework (InvenTree, more connectors, declarative DSL, full app-auth) stays
Phase 2 ([#34](https://github.com/pfa230/labeler/issues/34), [#33](https://github.com/pfa230/labeler/issues/33)).

#### P1-71 Connector spine · GH #35
Backend `Connector` trait + registry (like printer drivers); connections store + CRUD with write-only,
redacted credentials; browse model endpoints (`schema`/`browse`/`materialize`); a hardened outbound HTTP
client (scheme/port policy, timeouts, max bytes, no cross-host redirects, IP/metadata checks, private-LAN
egress documented, secret/cursor redaction).
- **Depends on:** P1-35 (`/batch`).
- **AC:** a fake connector browses, paginates (bound cursor), and materializes selected rows under a
  fanout budget; egress policy rejects cross-host redirect, oversized response, and metadata IPs; tests
  cover schema/browse/materialize + error categories.

#### P1-72 Homebox connector · GH #35
The first concrete `Connector`: Homebox `/v1/entities` model, token auth (pasted or login), the
location→contained-items relation.
- **Depends on:** P1-71.
- **AC:** against a realistic Homebox fixture, browse locations/items, drill a location into its items,
  and materialize fields; auth-failure and empty-page paths covered.

#### P1-73 Integration UI + mapping · GH #35
One generic browse component (grid + drill-down driven by `schema`) and a `(connection, template)` field
mapping that materializes selected rows into the existing editable grid → `/batch`.
- **Depends on:** P1-71, P1-51 (UI shell), P1-54 (editable grid).
- **AC:** end-to-end from the UI: connect to Homebox → browse → select → map → print/download; mapping
  drift (missing template field) is surfaced.

Trust model (interim): M7 ships under the existing LAN-trust posture (no app auth yet); residual risk
equals today's unauthenticated print/template endpoints because connectors are server-side code calling
known APIs (no generic credentialed proxy). App-level auth ([#33](https://github.com/pfa230/labeler/issues/33))
is the Phase 2 hardening; document the trust model in SPEC.

## 5. Dependency graph

```
P1-D1 ─────────────► P1-32 ─► P1-33 ─┐
P1-31 ──┬──► P1-32   P1-12 ─► P1-33  ├─► P1-35 ─┬─► P1-42 ─► P1-54
        ├──► P1-41           P1-34 ──┘          ├─► P1-43
        └──► P1-55           P1-13 ─────────────┘   P1-35 ─► P1-53

P1-12 ─► P1-34
P1-12 ─► P1-52
P1-11 ─► P1-23 (soft)
P1-21 ─► P1-22

P1-D2 ─► P1-51 ─┬─► P1-52
                ├─► P1-53
                ├─► P1-54
                └─► P1-55

P1-61 ─► P1-62 ─► P1-63
P1-31, P1-22 ─► P1-62
P1-33 ─► P1-63
```

Roots (no dependencies, can start immediately): P1-D1, P1-D2, P1-11, P1-12, P1-14, P1-21, P1-31, P1-64.
(P1-13 is blocked by #28.)

## 6. Critical path

`P1-D1 → P1-32 → P1-33 → P1-35 → P1-53` (printer model → CUPS → dispatch → print form) is the longest
chain to an end-to-end "print from the UI" demo and should be prioritized. `P1-31` and `P1-D2` feed it
in parallel.

## 7. Phase 1 definition of done

- A user can `docker compose up`, open the UI, see the starter templates, render a preview, and print
  to a CUPS printer or download a PDF/PNG.
- The same is achievable over the API, including `POST /batch` and CSV batch.
- A user can connect Homebox, browse its inventory in the UI, select records, map them to a template, and
  print/download, under the documented LAN-trust posture.
- Templates can be hand-authored as YAML and managed over the API; invalid templates are rejected with
  precise errors and never crash the service.
- All backend changes have tests; `cargo fmt`, `cargo clippy --all-targets --all-features`, and
  `cargo test` are clean.
- SPEC.md and the relevant ADRs reflect what shipped.
