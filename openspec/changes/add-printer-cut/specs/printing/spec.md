## MODIFIED Requirements

### Requirement: Printer capabilities

Capabilities SHALL come from IPP `Get-Printer-Attributes` with a 3-second timeout; any failure means none.

- **bilevel**: `print-color-mode-supported` or `-default` includes `bi-level`, or `pwg-raster-document-type-supported` includes `black_1` and none of `srgb_8`, `sgray_8`, `cmyk_8`, `adobe-rgb_8`, `srgb_16`.
- **accepts PNG**: `document-format-supported` includes `image/png`.
- **resolution**: `printer-resolution-default` in DPI, square, 1 to 1200.
- **media width**: `media-col-ready` → `media-size` → `x-dimension` in hundredths of a millimetre, when positive.
- **model**: `printer-make-and-model`.
- **trim**: `finishings-supported` includes enum `11`.
- **cut**: `operations-supported` includes enum `0x5f01` (`24321`), the project-local `Ptouch-Cut` operation.
- **chain and continue**: `finishings-col-supported` includes keyword `finishing-template`, and `finishing-template-supported` includes keyword or name `ptouch-chain`. Values may be scalar or sets. Missing or wrongly typed advertisements establish no support.

`POST /api/printers/probe` with the printer's connection fields (`uri`, `username`, `password`, `ca_cert`, `insecure`, `render`; no `id` or `name`, no stored password used) SHALL validate them like create (`400` reason `printer_invalid`) and answer `200` with `{status: "ok", capabilities: {model, media_width_mm, resolution_dpi, color, accepts_png, chain_and_continue, cut}}`, `color` being `bilevel`, `color`, or `unknown` when no colour mode or raster type was advertised, or `{status: "unreachable", detail}`. The `chain_and_continue` and `cut` capabilities SHALL be booleans reflecting the advertised support above.

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

#### Scenario: Advertised cut operation

- **WHEN** a printer's `operations-supported` includes enum `0x5f01`
- **THEN** its probe reports `cut: true`, and reports `cut: false` when the value is absent

## ADDED Requirements

### Requirement: Printer cut

`POST /api/printers/{id}/cut` SHALL take no request body and send one IPP request with operation code `0x5f01` (`Ptouch-Cut`) to the stored printer's URI, with the standard operation attributes, the stored credentials and transport settings, and no document. It SHALL answer `200` with `{status: "ok"}` when the printer replies with a successful status, and otherwise `{status: "failed", detail}`, `detail` naming the transport error or the IPP status. The request SHALL be sent once, without a prior capability query and without resending. Unknown ids SHALL return `404 NotFound`.

#### Scenario: Printer releases the held label

- **WHEN** a client cuts a stored printer that answers the cut with `successful-ok`
- **THEN** the response is `200` with `status: "ok"`

#### Scenario: Printer refuses the cut

- **WHEN** the printer answers the cut with `server-error-operation-not-supported`
- **THEN** the response is `200` with `status: "failed"` and a detail naming that status, and no further request is sent

#### Scenario: Unknown printer

- **WHEN** a client cuts a nonexistent printer id
- **THEN** the response is `404 NotFound` with `details.kind: printer` and that id
