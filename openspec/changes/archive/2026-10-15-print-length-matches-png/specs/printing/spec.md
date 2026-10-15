## MODIFIED Requirements

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
