# 34. Single config dir: LABELER_CONFIG_DIR

Date: 2026-06-28

## Status

Accepted. Supersedes the data-dir and two-volume parts of
[ADR-0029](0029-runtime-base-debian-slim.md) (the runtime-base and PUID/PGID decisions in ADR-0029
stand) and the per-dir env-var parts of issue #38 (now closed). Issue #93.

## Context

Before this change the service had three per-dir env vars (`LABELER_DATA_DIR`, `LABELER_TEMPLATES_DIR`,
`LABELER_ASSETS_DIR`), a two-volume compose setup (`labeler-data` and `labeler-templates`), and bundled
templates that were never automatically available at runtime: users had to copy them in, or they were
injected once by Docker volume initialization and were then silently overridden on a bind-mount.

The three-dir split had no real benefit. All three dirs are persistent state. Operators had to mount two
separate volumes, and there was no single backup target. The per-dir knobs added complexity with no
corresponding flexibility benefit; the fonts and UI dirs remain sensibly baked in.

The bundled templates (in `templates/` in the repo) are embed sources that were always compiled into
the binary via `include_dir!`. They were never actually served at runtime unless a volume happened to
be seeded at first create. A bind-mounted templates dir would silently hide any bundled starters.

## Decision

1. **One config dir, one env var.** `LABELER_CONFIG_DIR` (default `/config`) holds all persistent
   state: `{config}/labeler.db` (the SQLite database), `{config}/templates/` (user template files),
   and `{config}/assets/` (bundled image assets). Remove `LABELER_DATA_DIR`, `LABELER_TEMPLATES_DIR`,
   and `LABELER_ASSETS_DIR`.

2. **First-run template seeding.** The bundled templates are embedded in the binary at compile time
   (via `include_dir!`). On startup, if the DB flag `templates_seeded` is NOT set, the binary writes
   each bundled template to `{config}/templates/` and sets the flag. The seed runs exactly once: after
   that, `{config}/templates/` is fully user-owned. Deletes are permanent; no re-injection on upgrade.
   Seed-flag write errors propagate (no `.ok()`); a swallowed failure would resurrect deleted templates
   on the next start.

3. **One volume in Docker.** The compose setup uses one named volume (`labeler-config`) mounted at
   `/config`. The entrypoint chowns `/config` to `PUID:PGID` on every start. The `docker run` example
   is `-v labeler-config:/config`.

4. **Clean break; no migration.** There is no automatic migration of data from `labeler-data` or
   `labeler-templates` volumes to `/config`. Existing deploys must stop, back up their state, and
   restore it under `/config` manually. The breaking nature of this change is intentional: a silent
   migration with fallback paths would leave the per-dir vars in the code indefinitely.

5. **Local `cargo run` requires `LABELER_CONFIG_DIR`.** Without the Docker default, running locally
   needs a writable config dir, e.g. `LABELER_CONFIG_DIR=./config-dev cargo run`.

## Consequences

- One mount point and one backup target (`/config`). Simpler compose files, simpler operator docs.
- Bundled templates appear in `{config}/templates/` on the first start and are then editable,
  deletable, and fully user-owned. A bind-mount no longer hides them (seeding writes files into the
  user's dir rather than the binary serving from an internal embed).
- Clean break: existing `labeler-data` and `labeler-templates` volumes are NOT migrated automatically.
  Operators upgrading from a previous release must restore their DB and templates manually into `/config`.
- Local development requires setting `LABELER_CONFIG_DIR` explicitly (e.g. `./config-dev`); omitting
  it will try to use the compiled-in default (`/config`), which may not be writable on the developer's
  machine.
- `templates/` stays in the repo as the embed source. The Docker build stage must `COPY templates/`
  so `include_dir!` can embed them at compile time.
