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
- **trim**: `finishings-supported` includes enum `11`.
- **chain and continue**: `finishings-col-supported` includes keyword `finishing-template`, and `finishing-template-supported` includes keyword or name `ptouch-chain`. Values may be scalar or sets. Missing or wrongly typed advertisements establish no support.

`POST /api/printers/probe` with the printer's connection fields (`uri`, `username`, `password`, `ca_cert`, `insecure`, `render`; no `id` or `name`, no stored password used) SHALL validate them like create (`400` reason `printer_invalid`) and answer `200` with `{status: "ok", capabilities: {model, media_width_mm, resolution_dpi, color, accepts_png, chain_and_continue}}`, `color` being `bilevel`, `color`, or `unknown` when no colour mode or raster type was advertised, or `{status: "unreachable", detail}`. The `chain_and_continue` capability SHALL be a boolean reflecting the advertised support above.

#### Scenario: Grey raster is not bilevel

- **WHEN** a printer advertises raster types `black_1` and `sgray_8` and no colour modes
- **THEN** the probe reports `color: "color"`

#### Scenario: Advertised chain template

- **WHEN** a printer advertises `finishing-template` as a supported collection member and `ptouch-chain` as a supported template
- **THEN** its probe reports `chain_and_continue: true`

#### Scenario: Incomplete continuation advertisement

- **WHEN** either advertisement required for chain and continue is missing or wrongly typed
- **THEN** a successful probe reports `chain_and_continue: false`

#### Scenario: Enum policies do not establish chain support

- **WHEN** a printer advertises finishing enums but no supported `ptouch-chain` template
- **THEN** its probe reports `chain_and_continue: false`

### Requirement: Render negotiation

For a print job, colour mode and resolution SHALL each resolve independently to the `render` override, else the negotiated value: colour `bilevel` only when the printer is bilevel and accepts PNG, else `color`; resolution the reported DPI, else the template `dpi`. Capabilities SHALL be queried once per print request for job finishing, including when both render fields are overridden.

#### Scenario: One field overridden

- **WHEN** a printer sets only `render.resolution: 300` and reports bilevel with PNG
- **THEN** jobs render bilevel at 300 DPI

#### Scenario: Render overrides do not bypass finishing discovery

- **WHEN** both render fields are overridden and the printer advertises chain-and-continue support
- **THEN** explicit continuation still uses its supported finishing template and retains the configured colour mode and resolution

### Requirement: Wire format

A `single` template with resolved colour mode `bilevel` SHALL be sent as the 1-bit PNG that `color_mode=bilevel` produces on the render endpoint, with IPP `document-format: image/png`; every other job, including every sheet template, SHALL be a PDF sent as `application/pdf`. Each job is one IPP `Print-Job` titled `labeler`, and a transport error or non-success status fails it.

Every job SHALL carry the operation attribute `ipp-attribute-fidelity` set to `true`, so a printer refuses a job it cannot print as requested instead of changing it. Every job SHALL carry the integer job attribute `copies`: a `single` entry's copy count (`rendering`), and `1` for a sheet. A `single` entry SHALL be sent as exactly one job, whatever its copy count, in entry order. A job SHALL be sent at most once: a failed job, including one whose reply never arrives, SHALL NOT be sent again.

#### Scenario: Sheet on a bilevel printer

- **WHEN** a sheet template prints to a printer with `render.color_mode: bilevel`
- **THEN** it is sent as `application/pdf`

#### Scenario: Three copies as one job

- **WHEN** a `single` entry with `copies: 3` prints
- **THEN** exactly one Print-Job is sent, carrying `copies` 3, `ipp-attribute-fidelity` true and that label's `media-col`

#### Scenario: One copy

- **WHEN** a `single` entry without `copies` prints
- **THEN** exactly one Print-Job is sent, carrying `copies` 1

#### Scenario: Distinct labels keep their jobs and order

- **WHEN** entries A and B, of different data and lengths, each with `copies: 3`, print
- **THEN** two jobs are sent, A's then B's, each carrying `copies` 3 and its own label's `y-dimension`

#### Scenario: Sheet job

- **WHEN** a `sheet` batch with one entry of `copies: 3` prints
- **THEN** exactly one PDF job is sent, carrying `copies` 1

#### Scenario: No reply is not resent

- **WHEN** a `single` entry with `copies: 3` prints and the connection closes after the job is sent but before the printer replies
- **THEN** the job was sent once, no further job is sent, and labels 0, 1 and 2 are reported failed

### Requirement: Media size

Every job for a template that declares `media_width` SHALL carry IPP `media-col` whose `media-size` has `x-dimension` and `y-dimension` in hundredths of a millimetre (1 in = 25.4 mm). `x-dimension` SHALL be `media_width`, rounded to the nearest integer. `y-dimension` SHALL be the label's length along the feed: for a PDF job, the label's resolved `width`, rounded to the nearest integer; for a PNG job, the image's width in pixels × 2540 / the DPI it was rasterized at, rounded up to an integer. A job for a template without `media_width` SHALL carry no `media-col`. The printer judges whether its loaded media fits; a job it refuses fails like any other.

#### Scenario: Tape width sent with the job

- **WHEN** a template declares `media_width: 24` mm and a label resolves to a `width` of 62.5 mm and prints as a PDF
- **THEN** that label's job carries `media-col` with `media-size` `x-dimension` `2400` and `y-dimension` `6250`

#### Scenario: PNG job declares its image's length

- **WHEN** a label with a resolved `width` of 20 mm prints as a PNG rasterized at 180 DPI, 142 pixels wide
- **THEN** its job carries `y-dimension` `2004`

#### Scenario: Content-sized labels send their own lengths

- **WHEN** two labels of one content-sized template that declares `media_width` resolve to widths of 40 mm and 80 mm and print as PDFs
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

### Requirement: Stored printer probe

`POST /api/printers/{id}/probe` SHALL take no request body, resolve the stored printer's connection fields, and return the same probe result as the connection-fields probe. Unknown ids SHALL return `404 NotFound` for that printer. Probing SHALL use the stored credentials and transport settings without returning the password, submitting a print job or changing the printer record.

#### Scenario: Stored password is used privately

- **WHEN** a client probes a stored printer requiring its saved password
- **THEN** the capability query authenticates using that password and the response contains no password

#### Scenario: Unknown stored printer

- **WHEN** a client probes a nonexistent printer id
- **THEN** the response is `404 NotFound` with `details.kind: printer` and that id

#### Scenario: Unreachable stored printer

- **WHEN** the stored printer's capability query fails
- **THEN** the response is `200` with `status: unreachable` and a detail
- **AND** no print job is submitted and no printer record changes

### Requirement: Print job finishing

A print request with `chain_and_continue: true` SHALL send every dispatched job, including the last, with the IPP job attribute `finishings-col` containing exactly one collection whose `finishing-template` is keyword `ptouch-chain`. It SHALL send no `finishings` enum attribute in those jobs.

A print request with false or omitted `chain_and_continue` SHALL send every job with `finishings` enum `11` when trim is advertised as supported; otherwise it SHALL omit both finishing attributes. Labeler SHALL expose only ordinary printing and chain-and-continue selection, and SHALL NOT submit no-label-cuts or final-cut-only policies as an explicit choice.

Job order, rendering, wire format (including copies and attribute fidelity), media size, job logging and print summaries SHALL retain their existing rules. Continuation SHALL NOT switch the last job to trim, add a finalization job or replay a job after failure or cancellation.

#### Scenario: Three continued jobs

- **WHEN** a supported three-job request sets `chain_and_continue: true`
- **THEN** all three outgoing jobs carry the `ptouch-chain` collection and no enum finishing
- **AND** the third job requests continuation like the first two

#### Scenario: Copies continue in one job

- **WHEN** a supported request holds one single-template entry with `copies: 3` and sets `chain_and_continue: true`
- **THEN** exactly one job carries the `ptouch-chain` collection, integer `copies` 3 and `ipp-attribute-fidelity` true
- **AND** an accepted job reports `total` 3, `sent` 3 and `jobs` 1

#### Scenario: Ordinary supported printing

- **WHEN** a request omits continuation or sets it to false and the printer advertises trim
- **THEN** every outgoing job carries `finishings` as enum `11` without `finishings-col`

#### Scenario: Ordinary printing without advertised trim

- **WHEN** a request omits continuation or sets it to false and trim support is absent or the capability query fails
- **THEN** jobs omit both finishing attributes and retain the existing render and dispatch behavior

#### Scenario: Send failure does not introduce a cut

- **WHEN** an intermediate or last job of a continued request fails to send
- **THEN** each attempted job retains the continuation selection and failures use the existing summary
- **AND** the service sends no additional cut or replay

#### Scenario: Accepted job canceled at the printer

- **WHEN** the printer accepts a continued job and that job is later canceled
- **THEN** labeler sends no cleanup cut or replay and its summary retains its accepted-job meaning

### Requirement: Continuation capability validation

A request with `chain_and_continue: true` SHALL require advertised chain-and-continue support as defined by Printer capabilities. Absent support, including a failed capability query, SHALL cause `400 InvalidRequest` with reason `chain_and_continue_unsupported` and a message identifying the unsupported continuation choice before any label is rendered or any job is sent. The requested continuation SHALL NOT be silently replaced with ordinary printing.

#### Scenario: Unsupported explicit continuation

- **WHEN** a request sets `chain_and_continue: true` but the printer lacks the required advertisement or cannot be probed
- **THEN** the request is refused with reason `chain_and_continue_unsupported` before dispatch

#### Scenario: Capability changed after the form's probe

- **WHEN** a form previously found continuation support but the print request's capability query no longer finds it
- **THEN** the print request is refused with the continuation error rather than sending an ordinary trim job
