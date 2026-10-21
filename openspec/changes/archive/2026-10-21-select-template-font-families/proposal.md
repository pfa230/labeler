## Why

Text fitting and emission are hard-coded to Inter, so putting another font in the image does not make it usable in a template. Authors need a small shipped selection and custom fonts whose actual metrics determine the label layout.

Source issue: https://github.com/pfa230/labeler/issues/187

Fixes #187

## What Changes

- Add a literal `font` family name using the renderer's normalized names, defaulting to Inter; select the shipped Source Sans 3 font as `SourceSans3VF` and Roboto Condensed as `Roboto`.
- Ship Source Sans 3, JetBrains Mono and Roboto Condensed, with their licenses.
- Load custom fonts additively from `{LABELER_CONFIG_DIR}/fonts` on process restart, without host system fonts or an image rebuild.
- Resolve family and weight consistently for measurement and emission, including static faces and variable fonts without optical-size axes.
- Refuse unknown families when validating templates; retain loaded font bytes for the process lifetime.

## Capabilities

### New Capabilities

### Modified Capabilities

- `text`: family selection, available fonts, face matching and fitting with the selected face.

## Impact

Text raw/domain/conversion models and template validation; `src/render/helpers.rs` and `src/render/mod.rs`; bundled font files and licenses; authoring and deployment guidance. The image already copies `fonts/`. No UI or discovery endpoint is needed. Apply the existing unmapped-character check to the selected face.
