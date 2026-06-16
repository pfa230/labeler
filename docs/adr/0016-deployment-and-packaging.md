# 16. Deployment and packaging

**Status:** Accepted

## Context

M6 packages the service for one-command self-hosting (issues #18, #25, #26, #9). The service is a single
Rust/axum binary that also serves the built React SPA, stores state in SQLite (rusqlite `bundled`), loads
templates/fonts from CWD-relative dirs, and prints over IPP. The packaging has to seed starter templates,
persist user state, run unprivileged, and reach printers, without an application code change.

## Decision

- **Single multi-stage image.** node builds `ui/dist`, cargo builds the release binary, and a distroless
  runtime stage bundles the binary + `templates/` + `fonts/` + `ui/dist`. Everything resolves under
  `/app`.
- **Runtime base is `gcr.io/distroless/cc-debian12:debug`.** glibc + CA certs + nonroot (uid 65532) + no
  package manager, and the `:debug` variant's BusyBox shell powers the `wget` HEALTHCHECK, the compose
  init chown, and `docker exec` debugging. This is an intentional tradeoff: it is not the leanest
  distroless posture. A future `runtime` vs `runtime-debug` split with an app-native healthcheck is the
  path to a smaller production image.
- **Persistence via two named volumes seeded and owned by a compose init container.** `labeler-init`
  runs the labeler image as root: because it is the first to mount `labeler-templates`, Docker copy-up
  seeds that volume from the image's starters, and it chowns both volumes to 65532 so the nonroot app can
  open the sqlite db and accept uploads (an unwritable data dir would panic `main.rs`). Idempotent on
  every `up`.
- **Network-IPP-only printing.** The container is a pure IPP client; printer URIs must be reachable from
  the container network. No socket mount, host networking, privileged mode, or in-container cupsd.
  Authenticated queues and self-signed `ipps://` are out of MVP (issue #39).
- **Two operator knobs** (`HOST_PORT`, `RUST_LOG`); the container port is fixed at 8080 so healthchecks
  stay valid. Single-arch MVP (issue #36 tracks multi-arch).

## Consequences

- `docker compose up` is the supported one-command deploy; bind mounts are a documented advanced opt-in
  (the init chown rewrites host ownership).
- `docker compose down -v` destroys state; backups require stopping the app (live SQLite).
- No application/API change; `docs/SPEC.md` gets only a changelog entry and `docs/DEPLOY.md` is added.

## Alternatives considered

- **Mount the host CUPS socket / run cupsd in the image.** Rejected: host-coupled and Linux-only, or
  heavyweight and stateful; network IPP is portable.
- **Plain busybox init container.** Rejected: it would be the first volume mounter and seed the templates
  volume empty; the init must run the labeler image.
- **Non-debug distroless + app-native healthcheck.** Deferred: more code now; revisit for a leaner
  production image.
