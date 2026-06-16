# Phase 2 — Plan (living)

Living roadmap for work beyond Phase 1. GitHub issues remain the source of truth for status; this doc
keeps the Phase 2 picture readable and records dependencies and design links. Phase 1 milestones (M5 web
UI, M6 packaging, and earlier) live in [`PLAN-phase-1.md`](PLAN-phase-1.md).

## Goal

Turn the renderer/printer into "the InvenTree of labeling": drive labels from real inventory systems,
author templates visually, and support more printer families, while keeping the lightweight
single-container, self-hosted model.

## Status at a glance

| Item | Issue | Design | State |
| --- | --- | --- | --- |
| App-level authentication | [#33](https://github.com/pfa230/labeler/issues/33) | integration spec §Security | Designed (prerequisite) |
| API integration framework | [#34](https://github.com/pfa230/labeler/issues/34) | [spec](superpowers/specs/2026-06-16-api-integration-framework-design.md) | Designed |
| Inbound print webhook | [#22](https://github.com/pfa230/labeler/issues/22) | — | Backlog |
| `option.<name>` columns for `/import/csv` | [#32](https://github.com/pfa230/labeler/issues/32) | — | Backlog (low) |
| Job-log retention / pruning | [#29](https://github.com/pfa230/labeler/issues/29) | — | Backlog |
| GUI template editor | to file | [ADR-0006](adr/0006-template-edit-ownership.md) | Vision |
| Printer families: ZPL/Zebra, Brother QL, Dymo | to file | [ADR-0007](adr/0007-printer-architecture-and-transport-model.md) | Vision |
| 1D barcodes | to file | — | Vision |
| CSV interactive field-mapping UI | to file | — | Vision |
| Printer status read-back | to file | — | Vision |
| Declarative connector DSL | (folded into #34) | integration spec §Approaches rejected | Deferred behind the `Connector` trait |

## Track 1 — External integrations

The headline Phase 2 capability. Sequenced because the integration framework cannot ship safely without
app auth.

1. **App-level authentication ([#33](https://github.com/pfa230/labeler/issues/33)) — prerequisite.**
   The service binds `0.0.0.0` with no auth today. Storing third-party API tokens and browsing inventory
   through them raises the blast radius, so this lands first: app token auth, CSRF/origin protection on
   state-changing calls, and admin-only connection CRUD.

2. **API integration framework ([#34](https://github.com/pfa230/labeler/issues/34)).** Design approved
   ([spec](superpowers/specs/2026-06-16-api-integration-framework-design.md)), vetted across three
   adversarial reviews. Connectors are backend code behind a `Connector` trait (registered like printer
   drivers), normalizing each external API into a shared browse model that one generic frontend renders;
   `browse` (display) is split from `materialize` (hydration of selected rows); relationships and expansion
   (per-serial/quantity) are first-class; cursors are server-bound. First connectors: **Homebox** and
   **InvenTree**. Selection → field mapping → the editable grid → `/batch` (#30, done). Depends on #33 and
   the hardened egress policy in the spec.

   **Deferred behind the trait seam:** a declarative connector DSL and user-authored connector types,
   revisit only with real demand and 2-3 concrete `Connector` impls to generalize from. The trait keeps
   this option open without paying for it now.

3. **Inbound print webhook ([#22](https://github.com/pfa230/labeler/issues/22)).** The push direction
   (an external system tells labeler to print), as opposed to the pull direction above. Would target
   `/batch` as a batch-of-one; finalize its contract and LAN hardening once the auth model from #33 exists.

## Track 2 — Authoring

- **GUI template editor** (vision; ownership model in [ADR-0006](adr/0006-template-edit-ownership.md)).
  The largest Phase 2 build; React/Konva or pdfme-style. To be designed and issued when scheduled.
- **CSV interactive field-mapping UI** (vision). Phase 1 ships header-name auto-match only.

## Track 3 — Printing breadth

- **More printer families** (vision): ZPL/Zebra, Brother QL raster, Dymo. The driver abstraction from
  [ADR-0007](adr/0007-printer-architecture-and-transport-model.md) is the seam; each is a new `PrinterDriver`
  registered without touching dispatch.
- **1D barcodes** (vision): a new layout item alongside `qr`.
- **Printer status read-back** (vision): query queue/printer state via IPP.

## Maintenance / smaller items

- **Job-log retention/pruning ([#29](https://github.com/pfa230/labeler/issues/29)).**
- **`option.<name>` columns for the self-contained `/import/csv` API ([#32](https://github.com/pfa230/labeler/issues/32))** — low priority; the M5 UI resolves options client-side and does not need it.

## Notes

- **Resolved/rescoped during Phase 1:** the unified batch endpoint (#30, ADR-0011) delivered sheet
  printing and multi-page sheets and made server-side `copies` (#5) moot (copies are client-side
  expansion); job options were settled as format-intrinsic `/batch` params (ADR-0012). #28's batch-
  composition concern is largely subsumed by ADR-0011.
- This file is append/update-friendly: as a vision item is scheduled, file its issue, link it here, and
  move it from "Vision" to a track with a design link.
