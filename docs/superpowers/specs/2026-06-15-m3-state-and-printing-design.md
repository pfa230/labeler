# Design: M3 state and printing (#8, #12, #16, #13, #19)

**Date:** 2026-06-15
**Issues:** #8 (SQLite app-state), #12 (printer CRUD), #16 (CUPS/IPP backend), #13 (file-download),
#19 (`/print` dispatch). Milestone M3.
**Status:** Approved design, pre-implementation. Implements [ADR-0007](../../adr/0007-printer-architecture-and-transport-model.md).

## Context

The service renders labels but cannot print. M3 adds persistent app state and the Phase-1 printing path
defined by ADR-0007: configured printers (machine instances), a `PrinterDriver` trait, one CUPS driver
sending PDF over IPP, a file-download sink, and a unified `/print` dispatch. Templates stay files
(ADR-0006); only app state goes in SQLite.

## Decisions (approved)

- **SQLite via `rusqlite`** (bundled) + `rusqlite_migration`, behind an **async-shaped `store`
  abstraction** so persistence is swappable (sqlx/Postgres later) and fakeable in tests.
- Printer `id` is a **validated slug** (same charset rule as template ids); `config` is opaque per-kind
  JSON (ADR-0007).
- **`/print` handles single-format templates only.** Sheet/batch printing and `copies` are deferred to
  #28 (per-label batch composition).
- New M3 endpoints mount at **root**; ADR-0008's `/api` move happens in M5 (#15).

## A. Storage foundation (#8)

- `rusqlite` with the `bundled` feature (SQLite compiled in, no system dependency) and
  `rusqlite_migration` for versioned schema applied at startup.
- A `store` module exposes **async-shaped** methods (`list_printers`, `get_printer`, `upsert_printer`,
  `delete_printer`, `get_setting`, `set_setting`, `record_job`). Handlers call the store, never raw SQL.
  A `Mutex<Connection>` guards the single connection (low concurrency; queries are sub-millisecond,
  consistent with M2's accepted local sync I/O). Making the methods `async fn` now keeps the interface
  stable across a future swap to async sqlx.
- The DB file lives under a configurable **data dir** (default `data/`, env-overridable), separate from
  the templates dir.
- Tables: `printers (id, name, kind, config, enabled, created_at)`, `settings (key, value)`,
  `jobs (id, ts, template, printer, status, error)` (minimal print log).
- Wired into `AppState`: the existing `ArcSwap<TemplateRegistry>` gains a `store` handle (and the data
  dir). `main.rs` opens/migrates the DB at startup.

## B. Printer model + CRUD (#12)

- `Printer { id, name, kind, config: serde_json::Value, enabled }` (per ADR-0007). `id` is a validated
  slug; `kind` selects a driver (`"cups"`); `config` is opaque per-kind JSON.
- REST CRUD at `/printers`: `GET /printers` (list), `POST /printers` (create), `GET /printers/{id}`,
  `PUT /printers/{id}`, `DELETE /printers/{id}`. JSON bodies.
- Create/update validate that `kind` is known and `config` parses for that driver via a driver
  `validate_config`. Errors map to the stable `AppError` contract (unknown kind / bad config → 422;
  duplicate id → 409; missing → 404).

## C. PrinterDriver trait + CUPS driver (#16)

- The trait from ADR-0007:
  ```rust
  enum ArtifactFormat { Pdf, Png, Zpl, Raster }
  trait PrinterDriver {
      fn accepted_format(&self) -> ArtifactFormat;
      fn send(&self, artifact: &[u8], opts: &PrintOptions) -> Result<(), PrintError>;
  }
  ```
- A **driver registry** maps `kind` → a constructor that builds a `Box<dyn PrinterDriver>` from the
  stored `config` (and a `validate_config` for #12). New families register here without touching
  dispatch.
- `CupsDriver` (`kind = "cups"`): `config = { uri: String, media?: String, copies?: u32 }`,
  `accepted_format = Pdf`. `send` submits an IPP `Print-Job` with the PDF via the `ipp` crate to the
  configured URI (a CUPS queue or an IPP-Everywhere printer). `PrintError` maps to `AppError`
  (send/transport failure → 502).

## D. File-download sink (#13)

The no-printer branch of `/print`: render to the requested `format` and return the bytes with the
correct `Content-Type` and a `Content-Disposition` filename. A sink, not a driver.

## E. `/print` dispatch (#19)

`POST /print { template, data, printer?, format? }`:
- **printer given:** load it from the store → build its driver via the registry → render the
  single-format template to `driver.accepted_format()` (PDF via `render_single_label_pdf`) →
  `driver.send()` → record a `jobs` row (status ok/failed).
- **no printer:** render to `format` (default png) and return the file (the #13 sink).
- Unknown template → 404; unknown printer → 404; a sheet-format template → 422 ("use /render/batch");
  send failure → 502. A job row is recorded for printer dispatches either way.

## F. Testing

- **Store:** against a temp DB file — migrations apply, and printer/setting/job round-trips work.
- **Dispatch / registry / format-selection:** with a **fake `PrinterDriver`** (records what it received,
  no network), proving the dispatch renders to the driver's format and routes correctly, and that the
  no-printer branch returns the file.
- **Printer CRUD:** http tests (create/list/get/update/delete, 409/404/422), store-backed via a temp DB.
- **Real IPP `send`:** an `#[ignore]`-by-default integration test pointing at `LABELER_TEST_IPP_URI`,
  run manually against a real CUPS (CI has no IPP server).
- `cargo fmt --check`, `clippy --all-targets --all-features -- -D warnings`, `cargo test` all clean.

## G. Scope, dependencies, endpoints

- New deps: `rusqlite` (bundled), `rusqlite_migration`, `ipp`.
- Endpoints at root; the `/api` move is M5 (#15).
- **Out of scope (P2/Later):** network ZPL/Brother/Dymo drivers, printer status read-back, USB,
  browser-side printing, batch-to-printer, `copies` (the latter two → #28).

## H. Delivery and testing

One branch `feat/m3-state-and-printing`, commits per issue in dependency order: **#8 store → #12
printers → #16 driver+CUPS → #13 file-download → #19 dispatch**, each marking its plan entry DONE
(commit hash). New deps added in the commit that first needs them. After implementation: one adversarial
review pass over the diff; `fmt`/`clippy -D warnings`/`test` green (plus the manual IPP check noted in
§F). SPEC updated (new endpoints, app-state note); merge to `main`, push.

## Acceptance criteria

- App state persists in SQLite under the data dir; migrations apply at startup; the `store` abstraction
  is the only SQL touchpoint.
- Printers CRUD works and persists; invalid kind/config rejected; the CUPS driver sends a PDF to an IPP
  endpoint (verified manually) and the fake-driver tests prove dispatch.
- `/print` routes to a printer or returns a file; sheet templates and copies are rejected/deferred.
- Full verification suite green; SPEC and plan updated.
