## Why

The UI still uses Tailwind 3's PostCSS integration and stylesheet directives. Moving to Tailwind 4 requires explicit preservation of the existing rendered defaults and keyboard focus, rather than treating a passing build as visual equivalence.

Source issue: https://github.com/pfa230/labeler/issues/139

Fixes #139

## What Changes

- Upgrade to Tailwind 4 with its Vite integration and CSS import entry.
- Preserve the existing palette, `.dark` toggle, utility appearance and relevant preflight defaults with narrowly justified class/CSS adjustments.
- Remove the superseded Tailwind/PostCSS configuration and dependencies no longer used by the project.
- Capture equivalent before/after browser views and keyboard-focus states, then run a clean lockfile install and UI gates.

## Capabilities

### New Capabilities

### Modified Capabilities

None. This dependency/build migration preserves behavior and sets `skip_specs: true`.

## Impact

UI package/lockfile, Vite/configuration, `theme.css` and affected utility usages. Existing palette/grid tests remain. No theme redesign, new UI behavior or committed visual-test framework.
