# Printing

## Purpose

Configured printers and how labeler talks to them over IPP: printer CRUD and the default printer, capability probing and negotiation, the wire format and media size per job, and the job log.

## Requirements

### Requirement: Printer records

A printer SHALL be `{id, name, uri, username?, password?, ca_cert?, insecure?, render?}` (fields in `Printer fields`). `POST` SHALL take the whole record; `PUT` SHALL take it without `id`, the path naming the printer, and SHALL replace the printer with it.

| Method | Path | Success | Refusals |
|---|---|---|---|
| GET | `/api/printers` | list by id | |
| POST | `/api/printers` | `201` | `409 Conflict` when the id is taken |
| GET | `/api/printers/{id}` | printer | `404 NotFound` |
| PUT | `/api/printers/{id}` | printer | `404 NotFound` |
| DELETE | `/api/printers/{id}` | `204` | `404 NotFound` |

Create and replace SHALL refuse an id that is empty or outside `[A-Za-z0-9_-]` with `400` reason `printer_id_invalid`, and a blank name or a field that breaks its rule with `400` reason `printer_invalid`.

#### Scenario: Replacing a printer

- **WHEN** a printer with `insecure: true` is replaced by a body without `insecure`
- **THEN** the stored printer has `insecure: false`

### Requirement: Printer fields

| Key | Type | Rule |
|---|---|---|
| `uri` | string, required | `ipp://` or `ipps://` URL with a host |
| `username` | string | basic auth, sent only with `password` |
| `password` | string | write-only, stored plaintext |
| `ca_cert` | string | contains `-----BEGIN CERTIFICATE-----`; trusted for this printer only |
| `insecure` | bool | default `false`; `true` skips TLS verification, overriding `ca_cert` |
| `render.color_mode` | string | `color` or `bilevel`; absent means negotiate |
| `render.resolution` | integer | 1 to 1200 DPI; absent means negotiate |

`password` SHALL never appear in a response. On `PUT` an omitted `password` keeps the stored one, `""` clears it and any other string replaces it.

#### Scenario: Password survives an edit

- **WHEN** a printer with a stored password is replaced by a body without `password`
- **THEN** prints still authenticate with the stored password

### Requirement: Default printer

`default_printer_id` SHALL be a known setting (see `settings`) whose in-code default is `null`. `PUT /api/settings/default_printer_id` with `{ "value": <string> }` SHALL trim the string, store it and reflect it back, and SHALL fail with `400` and `details.reason` `setting_value_invalid` when the value is not a string, is blank, or names no existing printer. `DELETE` SHALL clear it with `204`. Deleting the printer it names SHALL clear it in the same atomic operation.

#### Scenario: Deleting the default printer

- **WHEN** `default_printer_id` names `p1` and a client deletes `p1`
- **THEN** the response is `204` and `GET /api/settings` reports `default_printer_id` as `null` with `is_default: true`

### Requirement: Printer capabilities

Capabilities SHALL come from IPP `Get-Printer-Attributes` with a 3-second timeout; any failure means none.

- **bilevel**: `print-color-mode-supported` or `-default` includes `bi-level`, or `pwg-raster-document-type-supported` includes `black_1` and none of `srgb_8`, `sgray_8`, `cmyk_8`, `adobe-rgb_8`, `srgb_16`.
- **accepts PNG**: `document-format-supported` includes `image/png`.
- **resolution**: `printer-resolution-default` in DPI, square, 1 to 1200.
- **media width**: `media-col-ready` → `media-size` → `x-dimension` in hundredths of a millimetre, when positive.
- **model**: `printer-make-and-model`.

`POST /api/printers/probe` with the printer's connection fields (`uri`, `username`, `password`, `ca_cert`, `insecure`, `render`; no `id` or `name`, no stored password used) SHALL validate them like create (`400` reason `printer_invalid`) and answer `200` with `{status: "ok", capabilities: {model, media_width_mm, resolution_dpi, color, accepts_png}}`, `color` being `bilevel`, `color`, or `unknown` when no colour mode or raster type was advertised, or `{status: "unreachable", detail}`.

#### Scenario: Grey raster is not bilevel

- **WHEN** a printer advertises raster types `black_1` and `sgray_8` and no colour modes
- **THEN** the probe reports `color: "color"`

### Requirement: Render negotiation

For a print job, colour mode and resolution SHALL each resolve independently to the `render` override, else the negotiated value: colour `bilevel` only when the printer is bilevel and accepts PNG, else `color`; resolution the reported DPI, else the template `dpi`. Capabilities SHALL be queried only when a field is unset.

#### Scenario: One field overridden

- **WHEN** a printer sets only `render.resolution: 300` and reports bilevel with PNG
- **THEN** jobs render bilevel at 300 DPI

### Requirement: Wire format

A `single` template with resolved colour mode `bilevel` SHALL be sent as the 1-bit PNG that `color_mode=bilevel` produces on the render endpoint, with IPP `document-format: image/png`; every other job, including every sheet template, SHALL be a PDF sent as `application/pdf`. Each job is one IPP `Print-Job` titled `labeler`, and a transport error or non-success status fails it.

#### Scenario: Sheet on a bilevel printer

- **WHEN** a sheet template prints to a printer with `render.color_mode: bilevel`
- **THEN** it is sent as `application/pdf`

### Requirement: Media size

Every job for a template that declares `media_width` SHALL carry IPP `media-col` whose `media-size` has `x-dimension` equal to `media_width` and `y-dimension` equal to the label's resolved `width`, the length along the feed, each in hundredths of a millimetre (1 in = 25.4 mm), rounded to the nearest integer. A job for a template without `media_width` SHALL carry no `media-col`. The printer judges whether its loaded media fits; a job it refuses fails like any other.

#### Scenario: Tape width sent with the job

- **WHEN** a template declares `media_width: 24` mm and a label resolves to a `width` of 62.5 mm
- **THEN** that label's job carries `media-col` with `media-size` `x-dimension` `2400` and `y-dimension` `6250`

#### Scenario: Content-sized labels send their own lengths

- **WHEN** two labels of one content-sized template that declares `media_width` resolve to widths of 40 mm and 80 mm
- **THEN** their jobs carry `y-dimension` `4000` and `8000` respectively

### Requirement: Job log

Each print request SHALL record its template and the caller's actor: the user, whether signed in or acting through one of their API tokens, or `local` in no-auth mode.

#### Scenario: A token prints as its owner

- **WHEN** user A prints template `pallet` through one of A's API tokens
- **THEN** the job log records `pallet` with actor A

### Requirement: Printers UI

Settings SHALL list printers (name, URI, Default radio) plus a "No default printer" radio; choosing a radio writes or clears `default_printer_id`. The add/edit form SHALL hold id (new only), name, address, Test connection and an "Advanced: override printer settings" disclosure with colour mode (`auto (use printer)`, `color`, `bilevel`) and resolution (empty is auto); auto values are omitted from `render`. It SHALL refuse a bad id, empty name or non-`ipp(s)://` address before sending, and an edit SHALL keep the stored `username`, `ca_cert` and `insecure`. Test connection SHALL show the model (or "Printer reachable") with media width, DPI, colour and PNG, or "Couldn't reach printer: {detail}". The print form SHALL preselect the default printer, else the only printer, until the user picks.

#### Scenario: Sole printer is preselected

- **WHEN** exactly one printer exists, none is default, and the user opens the print form
- **THEN** that printer is selected
