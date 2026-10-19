## Why

Fitting an unmapped character with Inter's missing-glyph metrics can disagree with the fallback face used for output. Refuse such text rather than promise a fit measured with a different glyph.

Source issue: https://github.com/pfa230/labeler/issues/254

Fixes #254

## What Changes

- **BREAKING:** Refuse a text value containing a drawable character Inter cannot map before fitting can shorten it or emit fallback output.
- Return a specific layout error identifying the character and the affected text-value path.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `text`: Replace the Fonts requirement's unmapped-character allowance with refusal.
- `errors`: Register the unmapped-character reason and its identifying details.

## Impact

Touches the text-layout entry point, `AppError` constructors, `Reason` and existing text-render/error tests. No new font families, dependencies, schema keys or endpoints.

## Non-goals

Selectable fonts (#187), non-text fallback sites, missing-font handling and numeric-default validation.
