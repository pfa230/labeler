## MODIFIED Requirements

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
