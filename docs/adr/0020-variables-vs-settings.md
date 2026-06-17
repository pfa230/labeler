# 20. Variables vs settings (template substitution vs app config)

Date: 2026-06-17

## Status

Accepted (model). Implementation tracked by [#52](https://github.com/pfa230/labeler/issues/52) (rename `settings` to `variables`) and [#53](https://github.com/pfa230/labeler/issues/53) (typed app settings, with job-log retention as the first setting); the code still uses the single `settings` store until those land. Reworks the env-var knob added for [#29](https://github.com/pfa230/labeler/issues/29).

## Context

The `settings` key/value table is a grab-bag that conflates two different concerns. `render/helpers.rs`
resolves `{settings.X}` from `store.all_settings()` (the entire table), so **every settings row is
interpolatable into rendered labels**. Today the table holds `qr_base_url`, which is a genuine template
substitution variable (interpolated into a QR code). But it is also the tempting home for application
configuration, e.g. the job-log retention period (#29). Putting app config there would (a) leak it into
the `{settings.X}` template namespace and (b) drop it into the same free-text key/value UI meant for
content variables, with no typing or validation.

Surveyed prior art consistently separates the two: GitHub Actions `vars.*` vs `github.*`; Hugo
`.Site.Params` vs root config; Craft/Payload "Globals" vs "Settings"; WordPress `wp_postmeta` vs
`wp_options`; Django/Jinja/InvenTree, where app config is invisible to templates unless explicitly
promoted; and 12-factor, where config is operational values, not content. None of the systems surveyed
name their interpolation bucket "settings".

## Decision

Split into two concepts, two stores, two names.

- **Variables** — user-defined values interpolated into labels. This is the existing `settings`
  key/value store, renamed `variables`. The interpolation token becomes `{vars.X}` (was `{settings.X}`).
  Free-text key/value is acceptable (it is content). UI: a **"Variables"** section. The renderer's
  interpolation map is built **only** from the variables store.
- **Settings** — typed application configuration that changes behavior and is **never** interpolated.
  Stored separately from variables and structurally unreachable from `{vars.X}`. UI: a typed
  **"Settings"** section with validated inputs.

Application-config defaults live **in code** (one place). The config read path returns the **resolved**
value (in-code defaults merged with any stored overrides), and the config API/UI surface that resolved
value, so the effective configuration is always visible and never silently active. A stored row is
written only when an operator overrides a default. Consequently app config is invisible to templates by
construction, the inverse of today's "expose everything".

Nothing is released yet, so there are **no back-compat aliases**: a clean rename, and the one bundled
template that uses the old token (`templates/homebox-qr.yaml`) is migrated from `{settings.X}` to
`{vars.X}`. Specifics this implies for #52: the existing `settings` SQLite table is renamed in place
preserving its rows (e.g. `ALTER TABLE settings RENAME TO variables`), because `qr_base_url` is real
data to keep (no reseed); the variables store keeps a key/value API and UI (reserving `/api/variables`),
while app configuration gets its own typed endpoint and "Settings" UI (exact schema owned by #53);
interpolation error semantics are unchanged (a missing `{vars.X}` is still `422 MissingField`), and
`{settings.X}` is removed outright with no alias.

This ADR fixes the *model*. It deliberately does not decide the retention default value (90 vs an
opt-in `0`) or the exact config storage schema and API shape; those are settled when the settings
subsystem is implemented (#53). The ADR only mandates: a store separate from variables, holding
overrides only, with typed in-code defaults resolved on read.

## Consequences

- App configuration can no longer leak into rendered labels; the two concerns evolve independently.
- Defaults are evolvable (bump one in a release and every non-overriding install follows) and the
  "operator has not chosen" state stays distinct from "operator explicitly chose this value".
- One-time churn: rename the store/API/UI, change the interpolation token, migrate the bundled template,
  and rework #29's env var into a setting (#53).
- The Variables UI stays free-text (no typing); typed validation applies only to Settings.
- A future third category (secrets) would get its own store as well, not be folded into either of these.
