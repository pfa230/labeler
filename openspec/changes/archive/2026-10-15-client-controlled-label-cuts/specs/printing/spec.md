## MODIFIED Requirements

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

## ADDED Requirements

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
