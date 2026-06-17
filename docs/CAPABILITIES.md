# Labeler — Capability List

**Status:** Living document. This is the capability inventory that drives `SPEC.md` and the work plan.
It is derived from research into commercial systems (BarTender, NiceLabel/Loftware, TEKLYNX, Brady,
Zebra Designer) and the open-source landscape (InvenTree, Homebox, gLabels, OpenLabelMaker, brother_ql,
LPrint, zebrafy, pdfme/Konva). Sources are listed at the end. Decisions that fix an architecture choice
become ADRs under [`adr/`](adr/).

## 1. Product vision and scope

A lightweight, open-source, self-hosted label system for homes and small businesses: the "InvenTree
of labeling" without the enterprise weight. It manages label templates (including a GUI editor),
renders them, integrates with the printers people actually own (sheet/office, tape, thermal/Zebra),
and pulls the things-to-label from tools like Homebox or a CSV.

**In scope:** Dockerized server with a web UI and REST API; template management and a WYSIWYG editor;
rendering to PNG/PDF/ZPL; printing to network thermal, tape, and office printers; CSV import and
inventory-system integrations; QR/barcode generation.

**Out of scope (now):** Native desktop app; enterprise compliance (FDA 21 CFR Part 11, audit trails,
e-signatures); multi-site print servers; RFID encoding; ERP/SAP connectors; proprietary printer SDKs
that cannot run headless in a container.

### 1.1 Guiding principles

- **Raster/PDF-first rendering.** We already render with Typst to PNG (single) and PDF (sheet). Treat
  that rendered output as canonical and adapt it per printer (PNG to ZPL graphic, PNG raster to Brother
  QL, PDF to CUPS). This avoids authoring per-vendor command languages by hand. See [§7](#7-printer-integration-and-transport).
- **Network and CUPS before USB.** Printing from inside Docker is easy for network/IP printers and
  CUPS over a socket, and hard for direct USB. Ship the easy transports first.
- **Decouple data from design.** A template binds named fields; data arrives later from a request,
  CSV row, or integration. This is the universal commercial pattern (named variables / provisional
  values) and matches our existing `data` map.
- **Be a good integration citizen.** Accept inbound print webhooks and offer outbound pull. Encode QR
  as a URL by default so any phone camera resolves it.

## 2. Capability areas

Each capability is tagged: **MVP** (first usable release), **P2** (next), **Later**, or
**Out** (explicit non-goal). "Table stakes" marks capabilities present in essentially every commercial
tool, so their absence is conspicuous.

### A. Template model and schema

| Capability | Tier | Notes |
| --- | --- | --- |
| Declarative template (YAML) with layout tree | MVP | Exists: text, qr, line, container with frame/padding/options. |
| Units mm/in; configurable DPI | MVP | Exists. Table stakes. |
| Single (continuous/roll) and sheet formats | MVP | Exists. |
| Image/logo object | MVP | Table stakes; needed for most real labels. Not yet implemented. |
| Rectangle/ellipse shape objects | P2 | Container frame covers rectangle today; add filled shapes + ellipse. |
| 1D barcode object (Code128, Code39, EAN/UPC, ITF) | P2 | Only QR today. Table stakes. |
| Additional 2D (Data Matrix, PDF417, Aztec) | Later | QR covers most home/SMB needs. |
| Rich text / multiple styled runs in one object | Later | Single style per text object today. |
| Free-angle rotation | P2 | `rotate` exists; verify arbitrary angles render correctly. |
| Auto-shrink-to-fit text | MVP | Exists (`font_size: {min,max}`). Table stakes. |
| Template-level options/variants (gated containers) | MVP | Exists; our differentiator vs. one-template-per-variant. |
| Conditional visibility by data value (e.g. show if field non-empty) | Later | Beyond option-gating; expression-based. |
| Serialization/counter fields, date/time fields | P2 | Table stakes for batch label runs. |

### B. Template management

| Capability | Tier | Notes |
| --- | --- | --- |
| Load templates from directory at startup | MVP | Exists. |
| List/detail templates over API | MVP | Exists. |
| Create/update/delete templates over API + persistence | MVP | GUI-owned store is writable YAML files in `templates-gui/`; manual templates stay in `templates/`. See ADR-0006. |
| Edit ownership: manual (file, GUI read-only) vs GUI-owned | Later | Ships with the editor. One source of edits per template; ownership = location. ADR-0006. |
| "Convert to GUI edit" (one-way move manual → GUI store) | Later | Ships with the editor. Move, not copy; avoids dup ids and divergence. Reverse = export YAML. ADR-0006. |
| Built-in starter template library (Avery 5160/5163, Brother 12mm, Dymo 30252, etc.) | MVP | Lowers time-to-first-label; vendors all ship these. |
| Template duplicate/clone | P2 | |
| Template versioning / revision history | Later | Enterprise tools center on this; lightweight git-style versioning later. |
| Template validation with precise error paths | MVP | Exists (two-stage parse + `serde_path_to_error`). |

### C. GUI template editor (WYSIWYG)

**Priority: deferred to Later as a whole.** Rendering/backend, basic UI, and integrations ship first;
the editor is not in the MVP or Phase 2 critical path. The tiers in the table below are relative
ordering *within* the editor effort once it starts, not commitments for the earlier milestones.

The single biggest build, and the highest-risk one. Unlike the rest of the web UI (§K), which is forms
and lists over the existing API, the editor is a custom interactive canvas engine: drag, resize,
snapping, undo, live geometry. That is a qualitatively harder kind of frontend, and the library choice
constrains what is feasible, which is why only the editor gets an ADR. Research conclusion: maintain
our YAML layout tree as the source of truth and treat the canvas as a view (the ProseMirror/Konva
pattern); convert coordinates only at the import/export boundary (our bottom-left/y-up to the canvas
top-left). Candidate stacks: **Konva.js + react-konva** (own the model, most control) or **pdfme**
(native mm canvas, QR + data binding built in, JSON template ≈ our model, but PDF-output-oriented).
Decide via ADR.

| Capability | Tier | Notes |
| --- | --- | --- |
| Fixed mm canvas with visible label boundary | P2 | |
| Toolbox: select, text, qr, line, container, image | P2 | Mirror our layout item types. |
| Drag-to-place, click-select, resize handles | P2 | Table stakes. |
| Property inspector (x/y/w/h in mm + per-type props) | P2 | Table stakes. |
| Snap to grid (configurable) + smart alignment guides | P2 | Table stakes; both Konva and pdfme need custom/guide config. |
| Variable field binding with provisional/sample value | P2 | Mark text/qr content as a bound field; show placeholder at design time. |
| Design view vs. preview-with-sample-data | P2 | Step through CSV rows in preview. |
| Round-trip to/from YAML (lossless for known fields; preserve unknown) | P2 | Co-locate schema+render+serialize per element type to avoid drift. |
| Undo/redo (snapshot-based) | P2 | Table stakes. |
| Zoom, rulers, multi-select, align/distribute toolbar | P2/Later | Zoom+rulers P2; align/distribute Later. |
| Option/variant preview toggles | Later | Toggle template options to see gated containers appear/disappear. |
| Layers panel, lock objects, grouping | Later | Enterprise polish. |

### D. Data and variable binding

| Capability | Tier | Notes |
| --- | --- | --- |
| Per-label `data` map bound to named fields | MVP | Exists. |
| Missing-field error with field name | MVP | Exists (`MissingField`). |
| Counters/serialization, date/time, simple formulas | P2 | Table stakes for runs. |
| Default values / fallback per field | P2 | |
| Field validation rules (regex, type) | Later | |

### E. Data import and integrations

| Capability | Tier | Notes |
| --- | --- | --- |
| CSV/TSV import, one label per row | MVP | Table stakes. Ship a downloadable template CSV with standard headers. |
| Field-mapping UI (auto-match headers, show sample values, save mapping) | P2 | Highest-ROI import UX. |
| Excel (.xlsx) import | P2 | |
| Google Sheets (public URL → CSV; OAuth later) | Later | Public-URL path is cheap; OAuth second. |
| Inbound print webhook: `POST /batch` with `mode: print` | MVP | Lets Grocy-style tools and scripts drive printing. Clean integration surface. |
| Homebox integration (pull entities, render labels, QR → instance URL) | P2 | Priority target. Read `/v1/entities`; QR = `{base}/item/{uuid}` or `/a/{assetId}`. Treat as read-only. |
| InvenTree / Snipe-IT pull integrations | Later | InvenTree's machine-driver model informs our printer abstraction; Snipe-IT `/api/v1/hardware`. |
| Configurable QR "base URL + id field" mapping | MVP | Covers Homebox/Snipe-IT/any tool with one setting. |

### F. Rendering and output formats

| Capability | Tier | Notes |
| --- | --- | --- |
| PNG (single label) | MVP | Exists (typst-render). |
| PDF (sheet, and single for office printing) | MVP | Exists for sheet (typst-pdf); add single-to-PDF. |
| ZPL (PNG → `^GF` graphic field) for Zebra | P2 | Render raster, embed as ZPL graphic (zebrafy approach; implement in Rust or via a helper). |
| Raw raster for Brother QL/PT | P2 | Feed rendered PNG to the QL/PT raster protocol. |
| Print preview (server-rendered image of the actual output) | MVP | Reuse the render path; table stakes and a top user pain point ("WYSIWYG that matches print"). |
| Configurable output DPI per template/printer | MVP | Exists per template; align to printer (203/300). |

### G. Printer integration and transport

Strategy from research: easiest-first by transport, raster/PDF adapted per family. Consider an optional
**LPrint** sidecar (IPP for Zebra/Dymo/Brother/TSPL) as a unifying path for USB/local printers later.

| Capability | Tier | Notes |
| --- | --- | --- |
| Office/sheet printer via CUPS (PDF over `lp`/IPP) | MVP | Easiest; covers Avery-style sheets on any laser/inkjet. |
| "Download file" output (PNG/PDF/ZPL) with no printer | MVP | Always-works fallback; user prints however they like. |
| Network Zebra over raw TCP 9100 (ZPL) | P2 | Zero infra beyond a TCP client. |
| Network Brother QL over TCP 9100 (raster) | P2 | brother_ql `tcp://ip:9100` model. |
| Dymo LabelWriter via CUPS (incl. network as JetDirect 9100) | P2 | Official CUPS drivers exist. |
| Printer abstraction (driver/transport per "machine" instance) | P2 | Model on InvenTree's machine-driver framework: one driver, many configured machines (IP, media). |
| Printer status (online, media-out, error) | Later | brother_ql network backend cannot read status; best-effort only. |
| USB-attached printers (device passthrough or CUPS-in-container) | Later | Hard in Docker; defer. Network/CUPS first. |
| Browser-side printing (QZ Tray / Zebra Browser Print) | Later | Client-side agent; loses server "fire-and-forget." Document as an option. |
| TSPL/TSC printers | Later | Niche for home/SMB. |

### H. Print workflow

| Capability | Tier | Notes |
| --- | --- | --- |
| Single label render/print | MVP | Exists (render); add print dispatch. |
| Batch/merge print (CSV or list → one job) | MVP | Exists for sheet PDF; extend to per-printer batch. |
| Sheet slot selection / start offset | MVP | Exists (`start_slot`); surface in UI for partially-used sheets. |
| Reprint last job | P2 | |
| Print queue with job history | Later | Lightweight; not the enterprise Control Center. |
| Copies / quantity per label | MVP | |

### I. Barcode / QR and encoding

| Capability | Tier | Notes |
| --- | --- | --- |
| QR with content = URL (configurable base + id) | MVP | Default; resolves with any phone camera. |
| QR error-correction, module size, quiet zone | MVP | Exists. |
| 1D barcodes (Code128 default; Code39, EAN/UPC, ITF) | P2 | Table stakes. |
| Data Matrix | Later | For very small items. |
| GS1 Digital Link / GS1-128 | Later | Only if users enter retail/supply chains; advanced option. |

### J. API and extensibility

| Capability | Tier | Notes |
| --- | --- | --- |
| REST API for templates, render, print | MVP | Exists for render; add print + template CRUD. |
| OpenAPI doc + Swagger UI | MVP | Exists. |
| Stable JSON error contract | MVP | Exists. |
| Inbound webhook receiver (print on event) | MVP | See §E. |
| Outbound webhook / events (printed, failed) | Later | |
| Plugin/driver interface for printers | P2 | Internal trait now; third-party plugins Later. |
| API auth (tokens) | P2 | Needed once it leaves localhost; integrations use bearer tokens. |

### K. Web UI / app shell

| Capability | Tier | Notes |
| --- | --- | --- |
| Template list/browse with preview thumbnails | MVP | |
| Render/print form (pick template, fill data, choose printer, preview) | MVP | |
| CSV import + mapping + batch print screen | P2 | |
| Printer management screen | P2 | |
| Template editor (see §C) | Later | Deprioritized; backend/rendering, basic UI, and integrations first. |
| Integration settings (Homebox URL/token, QR base URL, webhook) | P2 | |
| Decent, responsive, themeable UI | MVP | "Decent UI" is an explicit goal; avoid generic AI look. |

### L. Deployment and operations

| Capability | Tier | Notes |
| --- | --- | --- |
| Single Docker image | MVP | Core requirement. |
| docker-compose with persistent volume (templates, settings, DB) | MVP | |
| Config via env vars (PORT exists) | MVP | |
| Bundled fonts (Inter) | MVP | Exists; allow user font upload Later. |
| CUPS access pattern documented (socket mount / host gateway) | MVP | Required for office/Dymo printing from container. |
| Healthcheck endpoint | MVP | Exists (`/health`). |
| Backup/restore of templates + settings | P2 | |

### M. Security and multi-user

| Capability | Tier | Notes |
| --- | --- | --- |
| App authentication (flat user accounts) | DONE | Session cookies + API tokens; first-run setup; resolves #33 minus roles (ADR-0017). |
| Roles / granular permissions / OIDC | Later | Deferred from the flat-auth scope. |
| Single-user / trusted-LAN assumption | superseded | The 0.0.0.0 LAN-trust posture is replaced by app auth (ADR-0017). |
| Multi-user, roles, audit | Out | Enterprise; explicit non-goal. |
| Typst-source injection hardening (escaping) | MVP | Exists (escape helpers); keep as a security invariant since we generate Typst from user data. |

## 3. Where we sit vs. commercial tools

- **Table stakes we must reach for credibility:** image objects, 1D barcodes, CSV import, print
  preview that matches output, snap/align in the editor, a starter template library, undo/redo.
- **Our deliberate differentiators:** open-source and self-hosted; Dockerized; option-gated container
  variants in one template; first-class inventory integrations (Homebox) and webhook-driven printing;
  raster/PDF-first rendering that targets office + thermal + tape from one pipeline.
- **What we deliberately skip:** enterprise print servers, compliance/audit, RFID, proprietary SDKs,
  100+ symbologies, multi-site management. These are where BarTender/NiceLabel/TEKLYNX earn their
  license cost and where we stay lightweight.

## 3.1 Template model assessment (intuitiveness, ease of use, expandability)

How our layout model (options/orientations, recursive containers, bottom-left coords) stacks up against
commercial engines (BarTender, NiceLabel, TEKLYNX, ZebraDesigner) and the open-source field (InvenTree
HTML/CSS, gLabels XML).

**Where we match or exceed:**
- *Variant handling is genuinely stronger than most.* Orientation via option-gated recursive containers
  expresses "one template, N layouts" cleanly and compositionally. Commercial tools do this with
  separate templates or per-object conditional-visibility; gLabels effectively can't.
- *Containers as coordinate frames* (children measured against the padded inner box) give true nested
  composition, closer to CSS/Figma frames than to the flat object lists in gLabels or ZebraDesigner.
- *Auto-size / fit-to-box* (`font_size: {min,max}`, `auto` sizing) is present; table stakes.

**Where we are behind:**
- *Limited variable layer*. A `value` field now gives text/qr items substitution interpolation
  (`{field}` from data, `{settings.<key>}` from settings; ADR-0010), closing the QR-base-URL composition
  gap. Still missing versus commercial engines: counters/serialization, date/time, defaults, and
  formulas/conditionals, which remain out of scope.
- *Few object types*: no image or 1D barcode yet, single style per text run.
- *Static gating, not data-driven conditions*: option-gating keys off a selection, not field values.
- *Absolute positioning only*: no flow/distribute; aligning N items with even spacing is manual.

**Ratings:**
- *Intuitiveness (hand-authoring): 3.5/5.* Tree reads well and nesting mirrors grouping; but bottom-left
  y-up surprises CSS users, `size` means a box for most items yet a delta for `line` (a wart), and the
  `auto`/`max_*` rules need a mental model. Precise validation paths soften the curve.
- *Ease of use: 3/5* for hand-authoring (the workflow until the GUI ships). Capable once learned; no
  feedback loop without rendering, nothing dynamic without the caller supplying it. A GUI lifts this to
  ~4.5.
- *Expandability: 4.5/5* (strongest axis). Two-stage parse + tagged `LayoutItem` enum + recursive
  containers make a new object type a mechanical three-file change (ADR-0002); Typst gives shaping and
  PDF/PNG breadth for free. An "element type registry" (co-locate schema + render + serialize per type)
  would raise this further and keep the future GUI in sync.

**Verdict:** structurally ahead of the open-source field and competitive with commercial engines on
layout composition; clearly behind on the data/variable model and object-type breadth. The gaps are
additive, not architectural dead-ends. Independent of features, fix the `line` `size`-means-delta
inconsistency (tracked in the plan).

## 4. Proposed phasing (to be refined in the plan)

- **MVP:** template CRUD + persistence; starter library; image object; render PNG/PDF + preview;
  print via CUPS and file download; CSV import (basic) + inbound print webhook; render/print web UI;
  Docker + compose + CUPS docs.
- **Phase 2:** 1D barcodes; network Zebra (ZPL) and Brother QL; Dymo via CUPS; printer abstraction +
  management UI; CSV field-mapping UI; Homebox integration; API tokens. (No editor.)
- **Later:** WYSIWYG editor (and its edit-ownership/convert flow, ADR-0006); versioning, queue/history,
  Data Matrix/GS1, USB/browser printing, more integrations, conditional visibility, font upload,
  plugin printers.

Rationale: rendering/backend, a basic operational UI (browse, render/print, CSV import), and
integrations deliver a usable product without the editor. Hand-authored YAML templates plus the
starter library cover authoring until the editor is built. See §C.

## 5. Decisions to ratify as ADRs (before/while writing the spec)

1. ~~Template storage and edit ownership~~ — **decided in ADR-0006**: YAML files in two folders
   (`templates/` manual, `templates-gui/` GUI-owned); single edit owner per template; SQLite reserved
   for app state, not templates.
2. GUI editor stack: Konva.js (own-the-model) vs. pdfme (native mm/PDF) vs. other.
3. Printer architecture: in-process drivers vs. LPrint/CUPS sidecar; the "machine instance" model.
4. ZPL generation approach: PNG-to-`^GF` in-process (Rust) vs. external helper.
5. Integration model: pull (poll inventory APIs) vs. push (inbound webhook) as the primary path.
6. UI delivery: server-rendered vs. SPA; same service vs. separate frontend.

Each becomes an ADR (Nygard format) and updates `SPEC.md`. Work items derived from this list are
tracked as GitHub issues, not here.

## 6. Sources

Commercial software: seagullscientific.com (BarTender), nicelabel.com / loftware.com, teklynx.com,
bradyid.com, zebra.com (ZebraDesigner). Editor UX: help.seagullscientific.com, help.nicelabel.com,
avery.com, openlabelmaker.com, BarTender/NiceLabel/ZebraDesigner user guides.
Printers/transport: zpl.ai, support.zebra.com, github.com/pklaus/brother_ql,
github.com/miikanissi/zebrafy, github.com/michaelrsweet/lprint, openprinting.github.io/cups,
daniel-lange.com (Dymo on Linux), github.com/qzind/tray.
Integrations/data: sysadminsmedia/homebox (API + labelmaker), docs.inventree.org (label templates +
machine drivers), snipe-it.readme.io, github.com/grocy/grocy (label webhook), docs.part-db.de,
github.com/Donkie/Spoolman, GS1 Digital Link guides, papaparse.com, gspread/opensheet.
Editor tech/round-trip: konvajs.org, fabricjs.com, tldraw.dev, github.com/pdfme/pdfme, prosemirror.net,
github.com/j-evins/glabels-qt, apryse.com (PDF coordinates), Figma docs (text resize, component props).
