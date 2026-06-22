# Vision

Labeler is a lightweight, open-source, self-hosted label system for homes and small businesses: the
"InvenTree of labeling" without the enterprise weight. It manages label templates, renders them, prints
to the printers people actually own (sheet/office, tape, thermal/Zebra), and pulls the things to label
from tools like Homebox or a CSV.

## In scope

A Dockerized server with a web UI and REST API; template management and a WYSIWYG editor; rendering to
PNG, PDF, and ZPL; printing to network thermal, tape, and office printers; CSV import and
inventory-system integrations; QR and barcode generation.

## Out of scope

A native desktop app; enterprise compliance (FDA 21 CFR Part 11, audit trails, e-signatures);
multi-site print servers; RFID encoding; ERP/SAP connectors; and proprietary printer SDKs that cannot
run headless in a container.

## Guiding principles

- **Raster/PDF-first rendering.** Render with Typst to PNG (single label) and PDF (sheet), treat that
  output as canonical, and adapt it per printer (PNG to a ZPL graphic, raster to Brother QL, PDF to
  CUPS) rather than hand-authoring per-vendor command languages.
- **Network and CUPS before USB.** Printing from inside Docker is easy for network printers and CUPS
  over a socket and hard for direct USB, so the easy transports come first.
- **Decouple data from design.** A template binds named fields; the data arrives later from a request,
  a CSV row, or an integration.
- **Be a good integration citizen.** Accept inbound print webhooks, offer outbound pull, and encode QR
  as a URL by default so any phone camera resolves it.

The current API and template model is specified in [`SPEC.md`](SPEC.md); design decisions are recorded
as [ADRs](adr/).
