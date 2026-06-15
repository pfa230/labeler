# 7. Printer architecture and transport model

**Status:** Accepted

## Context

The service renders labels (PNG/PDF via Typst) but cannot yet send them to a physical printer. Printing
is the milestone-M3 work (#8 app-state store, #12 printer CRUD, #16 CUPS backend, #13 file download, #19
`/print` dispatch), and the product's value depends on supporting the printers people actually own:
office/sheet printers, tape printers (Brother), and thermal label printers (Zebra/Dymo). Research into
the transport landscape established that the easy path from a Docker container is network/IP printing
(raw TCP 9100 or IPP), that printer families speak different payloads (PDF for office/IPP, ZPL graphic
fields for Zebra, raster for Brother), and that USB and printer-status read-back are hard and best
deferred. This ADR fixes the printer architecture so the Phase-1 CUPS path can ship while the later
families (Zebra ZPL, Brother raster, Dymo) drop in without rework.

## Decision

- **Printer entity ("machine" instance).** A configured printer is a persisted record
  `{ id, name, kind, config, enabled }`. `kind` selects a driver; `config` is an **opaque
  kind-specific JSON blob** that the driver parses (no per-family DB migrations). Many printers may
  share one driver kind (one CUPS driver, N configured queues). The persisted shape is owned by #8/#12;
  this ADR fixes its meaning.

- **Driver abstraction.** A `PrinterDriver` trait, dispatched dynamically:
  ```rust
  enum ArtifactFormat { Pdf, Png, Zpl, Raster }   // grows with families
  trait PrinterDriver {
      fn accepted_format(&self) -> ArtifactFormat;
      fn send(&self, artifact: &[u8], opts: &PrintOptions) -> Result<(), PrintError>;
  }
  ```
  A driver declares the artifact it consumes; the dispatcher renders the label to exactly that format
  before calling `send`. A driver registry maps `kind` → a constructor that builds a `Box<dyn
  PrinterDriver>` from the persisted `config`. New families are new impls plus a registry entry, with no
  change to the dispatcher; a future third-party plugin interface is just an externally-registered
  driver. (Chosen over a closed `enum` of kinds because broad, extensible printer support is the
  product's core value.)

- **CUPS driver via IPP (Phase 1).** The single Phase-1 driver, `kind = "cups"`, sends the rendered PDF
  as an IPP `Print-Job` using the pure-Rust `ipp` crate. No `lp` binary or CUPS package in the image.
  Its `config` carries an IPP URI that points either at a CUPS server queue
  (`ipp://host:631/printers/<queue>`) or directly at an IPP-Everywhere printer
  (`ipp://printer.local/ipp/print`), plus optional media/copies defaults. From the container this is
  outbound TCP to that host (host-gateway / `host.docker.internal:631` for a host CUPS, or the LAN for a
  network printer); the container does not run CUPS.

- **Dispatch and the download sink.** `POST /print { template, data, copies?, printer?, format? }`: with
  a `printer`, load it → build its driver → render the template to `driver.accepted_format()` (single →
  PDF via `render_single_label_pdf`, sheet → PDF via `render_sheet_labels`) → `driver.send()`. With no
  `printer`, render to the requested `format` and return the bytes. **File download is a sink, not a
  driver.** Errors map to the stable `AppError` contract (unknown printer → 404; send failure → 502).

## Consequences

- Phase 1 ships one `cups` driver (PDF over IPP) plus the file-download sink; the trait, registry,
  entity shape, and dispatch flow are settled, so #12/#13/#16/#19 implement against a fixed contract.
- Later families slot in by `accepted_format`: Zebra ZPL (render PNG → `^GF` graphic field), Brother QL
  (PNG raster over TCP 9100), Dymo (IPP/CUPS) — none requiring dispatcher changes.
- Because the transport is already IPP, an optional **LPrint** sidecar (which exposes Zebra/Dymo/Brother
  as IPP printers) needs no new code: those appear as IPP URIs the CUPS driver already drives.
- Adds the `ipp` dependency. The container reaches printers over the network; a host CUPS must be
  reachable on its IPP port (documented in #16).
- **Out of scope (deferred):** printer status/health read-back, USB passthrough, and browser-side
  printing (all "Later" in CAPABILITIES §G).
- `config` as an opaque JSON blob trades compile-time typing of printer settings for zero per-family
  migrations; each driver validates its own config and surfaces errors at build/print time.
