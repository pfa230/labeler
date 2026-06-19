# 24. App settings storage and API

Date: 2026-06-19

## Status

Accepted. Implements the application-config half of [ADR-0020](0020-variables-vs-settings.md) for
issue [#53](https://github.com/pfa230/labeler/issues/53).

## Context

ADR-0020 ratified the model (settings are typed app config, stored separately from `variables`, never
interpolated, with in-code defaults resolved on read) but deliberately left the default value, storage
schema, API shape, and the rework of the #29 retention env var to #53.

## Decision

- Storage: a new `app_settings(key TEXT PRIMARY KEY, value TEXT NOT NULL)` table holding override rows
  only, separate from `variables`. The name avoids the historical `settings` table that #52 renamed.
- Registry: a typed in-code registry (`src/settings.rs`) owns the known keys, their defaults, and
  per-key validation. The first and only setting is `job_log_retention_days`, default `90` (`0`
  disables pruning). `90` prunes out of the box but is visible via the resolved-config API.
- API: `GET /api/settings` returns the resolved value for every registry-known setting, each flagged
  `is_default`. `PUT /api/settings/{key}` validates a per-key JSON value (retention: integer
  `0..=u32::MAX`) and writes an override. `DELETE /api/settings/{key}` removes the override and is
  idempotent and registry-keyed (a known key is always `204`; an unknown key is `404 SettingNotFound`).
- A corrupt stored override surfaces an error (GET `500`; the prune task logs and skips), never a
  silent fall-back to default, because validation gates every write.
- The daily job-log prune resolves the live retention each run, so edits take effect without a restart.
  It is eventually consistent with setting writes (no shared lock with the background task). The
  `LABELER_JOB_LOG_RETENTION_DAYS` env var is removed: the setting is the single source.

## Consequences

- App config cannot leak into rendered labels and evolves independently of template variables.
- Operators see the effective value (default or override) and can reset to the evolvable default.
- Adding a future setting is a localized change to the registry plus its validation; non-numeric
  settings fit the `serde_json::Value` API shape without a contract change.
