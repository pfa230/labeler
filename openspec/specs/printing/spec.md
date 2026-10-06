# Printing

## Purpose

Configured printers and how labeler talks to them: printer CRUD and the default printer, the `cups` driver over IPP, capability probing and negotiation, the wire format per job, and the media-width check.

## Requirements

### Requirement: Printer records

A printer SHALL be `{id, name, kind, config, is_default}`; `kind` selects the driver and the only kind is `cups`. `is_default` is read-only and ignored on write.

| Method | Path | Success | Refusals |
|---|---|---|---|
| GET | `/api/printers` | list by id | |
| POST | `/api/printers` | `201`, `is_default: false` | `409 PrinterExists` |
| GET | `/api/printers/{id}` | printer | `404 PrinterNotFound` |
| PUT | `/api/printers/{id}` | printer, `is_default` kept | `400` reason `printer_id_mismatch` if body id ≠ path; `404 PrinterNotFound` |
| DELETE | `/api/printers/{id}` | `204` | `404 PrinterNotFound` |
| POST | `/api/printers/{id}/default` | `204`; the only default | `404 PrinterNotFound`, old default kept |
| DELETE | `/api/printers/{id}/default` | `204`, idempotent for any id | |

Before the existence checks, create and replace SHALL refuse an id that is empty or outside `[A-Za-z0-9_-]` with `400` reason `printer_id_invalid`, and a blank name, unknown kind or invalid config with `422 PrinterInvalid`.

#### Scenario: Moving the default

- **WHEN** `p1` is default and a client posts `/api/printers/p2/default`
- **THEN** `p2` is the only printer with `is_default: true`

### Requirement: cups config

| Key | Type | Rule |
|---|---|---|
| `uri` | string, required | `ipp://` or `ipps://` URL with a host |
| `username` | string | basic auth, sent only with `password` |
| `password` | string | write-only, stored plaintext |
| `ca_cert` | string | contains `-----BEGIN CERTIFICATE-----`; trusted for this printer only |
| `insecure` | bool | default `false`; `true` skips TLS verification, overriding `ca_cert` |
| `render.color_mode` | string | `color` or `bilevel`; absent means negotiate |
| `render.resolution` | integer | 1 to 1200 DPI; absent means negotiate |

A broken rule SHALL be `422 PrinterInvalid`; other keys SHALL be stored as sent. `password` SHALL never appear in a response. On `PUT` an absent `password` keeps the stored one, `null` clears it and a string replaces it.

#### Scenario: Password survives an edit

- **WHEN** a printer with a stored password is replaced by a config without `password`
- **THEN** prints still authenticate with the stored password

### Requirement: Address screening

Before any IPP request labeler SHALL refuse a printer host resolving to any loopback, link-local, unspecified or multicast address (IPv4-mapped IPv6 judged as IPv4); private LAN addresses are allowed. Refusal makes a probe `unreachable`, a capability query fail open and a send fail.

#### Scenario: Loopback printer

- **WHEN** a client probes `ipp://127.0.0.1/ipp/print`
- **THEN** the probe answers `200` with `status: "unreachable"`

### Requirement: Printer capabilities

Capabilities SHALL come from IPP `Get-Printer-Attributes` with a 3-second timeout; any failure means none.

- **bilevel**: `print-color-mode-supported` or `-default` includes `bi-level`, or `pwg-raster-document-type-supported` includes `black_1` and none of `srgb_8`, `sgray_8`, `cmyk_8`, `adobe-rgb_8`, `srgb_16`.
- **accepts PNG**: `document-format-supported` includes `image/png`.
- **resolution**: `printer-resolution-default` in DPI, square, 1 to 1200.
- **media width**: `media-col-ready` → `media-size` → `x-dimension` in hundredths of a millimetre, when positive.
- **model**: `printer-make-and-model`.

`POST /api/printers/probe` with `{kind?, config}` (`kind` defaults to `cups`, no stored password used) SHALL validate like create (`422 PrinterInvalid`) and answer `200` with `{status: "ok", capabilities: {model, media_width_mm, resolution_dpi, color, accepts_png}}`, `color` being `bilevel`, `color`, or `unknown` when no colour mode or raster type was advertised, or `{status: "unreachable", detail}`.

#### Scenario: Grey raster is not bilevel

- **WHEN** a printer advertises raster types `black_1` and `sgray_8` and no colour modes
- **THEN** the probe reports `color: "color"`

### Requirement: Render negotiation

For a print job, colour mode and resolution SHALL each resolve independently to the `render` override, else the negotiated value: colour `bilevel` only when the printer is bilevel and accepts PNG, else `color`; resolution the reported DPI, else the template `dpi`. Capabilities SHALL be queried only when a field is unset or a media check is pending.

#### Scenario: One field overridden

- **WHEN** a printer sets only `render.resolution: 300` and reports bilevel with PNG
- **THEN** jobs render bilevel at 300 DPI

### Requirement: Wire format

A `single` template with resolved colour mode `bilevel` SHALL be sent as the 1-bit PNG that `color_mode=bilevel` produces on the render endpoint, with IPP `document-format: image/png`; every other job, including every sheet template, SHALL be a PDF sent as `application/pdf`. Each job is one IPP `Print-Job` titled `labeler`, and a transport error or non-success status fails it.

#### Scenario: Sheet on a bilevel printer

- **WHEN** a sheet template prints to a printer with `render.color_mode: bilevel`
- **THEN** it is sent as `application/pdf`

### Requirement: Media check

Before rendering a `single` template that declares `media_width`, labeler SHALL compare it in millimetres (1 in = 25.4 mm) with the loaded media width and refuse a difference over 1 mm with `409 MediaMismatch`, message `template requires {want}mm media but {got}mm is loaded`, sending nothing. No `media_width`, no reported width or a failed query SHALL let the print proceed.

#### Scenario: Wrong tape loaded

- **WHEN** a template declares `media_width: 24` mm and the printer reports 12 mm
- **THEN** the print is `409 MediaMismatch` and no job is sent

### Requirement: Job log

Every job sent SHALL be recorded with template, printer, status `ok` or `failed`, error text and the caller's actor id. Retention is the `settings` key `job_log_retention_days`.

#### Scenario: Failed job

- **WHEN** a send fails
- **THEN** a `failed` job with that error is recorded

### Requirement: Printers UI

Settings SHALL list printers (name, kind, URI, Default radio) plus a "No default printer" radio that clears the default. The add/edit form SHALL hold id (new only), name, address, Test connection and an "Advanced: override printer settings" disclosure with colour mode (`auto (use printer)`, `color`, `bilevel`) and resolution (empty is auto); auto values are omitted from `render`. It SHALL refuse a bad id, empty name or non-`ipp(s)://` address before sending, and an edit SHALL keep the stored `username`, `ca_cert` and `insecure`. Test connection SHALL show the model (or "Printer reachable") with media width, DPI, colour and PNG, or "Couldn't reach printer: {detail}". The print form SHALL preselect the default printer, else the only printer, until the user picks.

#### Scenario: Sole printer is preselected

- **WHEN** exactly one printer exists, none is default, and the user opens the print form
- **THEN** that printer is selected
