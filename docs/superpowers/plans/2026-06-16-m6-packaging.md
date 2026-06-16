# M6 Packaging & Deployment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the service as a single Docker image with a docker-compose deployment (persistent volumes, env config) and documented CUPS reachability, with no application code change.

**Architecture:** A 3-stage Dockerfile (node build of `ui/dist` → cargo release build → distroless `:debug` runtime) produces one image that serves the API and SPA. A compose file runs a one-shot init container (the labeler image, as root) that seeds + chowns two named volumes, then the app as nonroot. All deployment behavior is documented in `docs/DEPLOY.md`.

**Tech Stack:** Docker multi-stage build, docker compose v2, distroless `gcr.io/distroless/cc-debian12:debug`, BusyBox (healthcheck/init), the existing Rust/axum binary + Vite SPA build.

---

## Context the implementer needs

- **Spec (source of truth):** `docs/superpowers/specs/2026-06-16-m6-packaging-design.md`. It was reviewed through 5 codex passes; follow it exactly. This plan reproduces the final artifacts verbatim.
- **This is packaging only: NO application/Rust/TS code changes.** The verification is `docker build` + `docker compose` smoke steps, not unit tests. **These steps require a working Docker daemon** (Docker Engine 20.10+ / Compose v2). If the execution environment has no Docker, stop at the affected step and report it so the operator can run the smoke checks; do not fake them.
- **Runtime path facts (do not change the app):** templates load from `templates/` (CWD-relative, read-write, hot-reloaded), fonts from `fonts/` (CWD-relative; bundled `fonts/InterVariable.ttf`), UI from `LABELER_UI_DIR` (default `ui/dist`), data from `LABELER_DATA_DIR` (default `data/`, sqlite at `<data>/labeler.db`), `PORT` (default 8080, binds `0.0.0.0`), `RUST_LOG`. `main.rs` panics on template-load / data-dir-create / store-open failure, so an unwritable data volume is a crash loop the design prevents via the init container.
- **The binary name is `labeler`** (`Cargo.toml`), built to `/app/target/release/labeler` (the build stage sets `WORKDIR /app`). Vite outputs to `dist` (the ui stage sets `WORKDIR /ui` → `/ui/dist`).
- **Repo conventions (`CLAUDE.md`):** no em dashes in code/docs; update SPEC changelog + add an ADR on a behavior change; commit/push without prompting after the review loop and a clean gate; work on a short-lived branch, merge to `main`; codex review is capped at 5 passes (stop at the first no-MAJOR pass).
- **PLAN entries to mark DONE (Task 5, after review):** P1-61 (#18), P1-62 (#25), P1-63 (#26), P1-64 (#9).
- **Deferred follow-ups already filed as issues:** #36 multi-arch, #37 CI/registry publish, #38 env-configurable dirs, #39 IPP basic-auth + custom-CA. Reference them in DEPLOY.md where relevant; do not implement them.
- **Branch:** do this work on `m6-packaging`; the final task merges to `main`.

## File structure

| File | Responsibility |
| --- | --- |
| `.dockerignore` (create) | Keep the build context small and deterministic (exclude build outputs, vcs, local artifacts). |
| `Dockerfile` (create) | 3-stage build → distroless `:debug` runtime image. |
| `docker-compose.yml` (create) | Init container (seed + chown volumes) + app service + two named volumes + healthcheck. |
| `.env.sample` (create) | The two operator knobs (`HOST_PORT`, `RUST_LOG`). |
| `docs/DEPLOY.md` (create) | Deployment guide: build, compose, env, volumes/backup, CUPS reachability. |
| `README.md` (modify) | A short "Deployment" pointer to `docs/DEPLOY.md`. |
| `docs/adr/0016-deployment-and-packaging.md` (create) | ADR recording the packaging decisions. |
| `docs/adr/README.md` (modify) | Index row for ADR-0016. |
| `docs/SPEC.md` (modify) | Changelog line (no API change). |
| `docs/PLAN-phase-1.md` (modify, Task 5) | Mark P1-61..P1-64 DONE. |

---

### Task 0: Branch setup

- [ ] **Step 1: Create the branch**

```bash
git checkout main && git pull && git checkout -b m6-packaging
```

---

### Task 1: Dockerfile + .dockerignore (#18)

**Files:**
- Create: `.dockerignore`
- Create: `Dockerfile`

- [ ] **Step 1: Create `.dockerignore`**

Keeps the build context minimal and avoids copying host build outputs into the image (the image rebuilds `ui/dist` and the binary itself). `templates/` and `fonts/` are NOT excluded (the runtime stage copies them from the context).
```gitignore
target/
ui/node_modules/
ui/dist/
data/
*.pdf
*.db
.git/
.env
docs/
```

- [ ] **Step 2: Create `Dockerfile`**

Verbatim from the spec (explicit WORKDIRs make the COPY-from paths deterministic):
```dockerfile
FROM node:22-bookworm-slim AS ui
WORKDIR /ui
COPY ui/package*.json ./
RUN npm ci
COPY ui/ ./
RUN npm run build

FROM rust:1-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --release --locked
RUN mkdir -p /seed/data /seed/assets

FROM gcr.io/distroless/cc-debian12:debug AS runtime
WORKDIR /app
COPY --from=build /app/target/release/labeler /app/labeler
COPY --chown=65532:65532 templates/ /app/templates/
COPY fonts/ /app/fonts/
COPY --from=ui /ui/dist /app/ui/dist
COPY --chown=65532:65532 --from=build /seed/data /app/data
COPY --from=build /seed/assets /app/assets
USER nonroot
EXPOSE 8080
ENV PORT=8080
ENV LABELER_DATA_DIR=/app/data
ENV LABELER_UI_DIR=/app/ui/dist
ENV LABELER_ASSETS_DIR=/app/assets
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s \
  CMD ["/busybox/wget","-qO-","http://127.0.0.1:8080/api/health"]
ENTRYPOINT ["/app/labeler"]
```

- [ ] **Step 3: Build the image**

Run: `docker build -t labeler:latest .`
Expected: build succeeds through all three stages; final line reports the `labeler:latest` image. (Requires a Docker daemon; if absent, report and stop.)

- [ ] **Step 4: Verify the BusyBox healthcheck path exists**

The HEALTHCHECK depends on `/busybox/wget` being present in the distroless `:debug` image.
Run: `docker run --rm --entrypoint=/busybox/wget labeler:latest --help`
Expected: exits 0 and prints BusyBox wget usage. If this fails (a future distroless tag moved busybox), copy a pinned busybox `wget` from `busybox:1.37` into a fixed path in the runtime stage and point the healthcheck at it, then rebuild.

- [ ] **Step 5: Smoke-run the bare image**

Run:
```bash
set -euo pipefail
docker run --rm -d -p 8080:8080 --name labeler-smoke labeler:latest
trap 'docker rm -f labeler-smoke >/dev/null 2>&1 || true' EXIT
for i in $(seq 1 30); do curl -fsS http://127.0.0.1:8080/api/health >/dev/null 2>&1 && break; sleep 1; done
curl -fsS http://127.0.0.1:8080/api/health; echo
curl -fsS http://127.0.0.1:8080/api/templates -o /tmp/m6-templates.json
grep -q '"id":"avery5163"' /tmp/m6-templates.json && grep -q '"id":"homebox-qr"' /tmp/m6-templates.json && echo "starters present"
curl -fsS -o /dev/null -w "root=%{http_code}\n" http://127.0.0.1:8080/
```
Expected: `/api/health` returns a small JSON body (it is under `/api`; hitting `/health` would wrongly return the SPA HTML with 200, which is why the path matters), the grep asserts print `starters present` (an empty/mis-seeded registry returns `{"templates":[]}` with 200, so we must assert the starter ids, not just fetch), and `root=200` (the SPA `index.html`). `set -euo pipefail`, the health poll (instead of a fixed sleep), saving the body to a file, and the `trap` cleanup make the step fail honestly.

- [ ] **Step 6: Commit**

```bash
git add .dockerignore Dockerfile
git commit -m "build: multi-stage Dockerfile (distroless) bundling UI, fonts, templates (#18)"
```

---

### Task 2: docker-compose + volumes + env (#25, #9)

**Files:**
- Create: `docker-compose.yml`
- Create: `.env.sample`

- [ ] **Step 1: Create `docker-compose.yml`**

Verbatim from the spec. The init service uses the labeler image (so it is the first mounter that seeds the templates volume) and chowns both volumes; the app waits on it.
```yaml
x-labeler-image: &labeler-image
  build: .
  image: labeler:latest
  pull_policy: build

services:
  labeler-init:
    <<: *labeler-image
    user: "0:0"
    entrypoint: ["/busybox/sh","-c","chown -R 65532:65532 /app/data /app/templates"]
    healthcheck:
      disable: true   # short-lived init container; do not run the inherited app healthcheck
    volumes:
      - labeler-data:/app/data
      - labeler-templates:/app/templates
  labeler:
    <<: *labeler-image
    depends_on:
      labeler-init:
        condition: service_completed_successfully
    ports: ["${HOST_PORT:-8080}:8080"]
    environment:
      RUST_LOG: ${RUST_LOG:-labeler=info,tower_http=info}
    volumes:
      - labeler-data:/app/data
      - labeler-templates:/app/templates
    extra_hosts:
      - "host.docker.internal:host-gateway"
    healthcheck:
      test: ["CMD","/busybox/wget","-qO-","http://127.0.0.1:8080/api/health"]
      interval: 30s
      timeout: 3s
      retries: 3
      start_period: 5s
    restart: unless-stopped
volumes:
  labeler-data:
    name: labeler-data          # explicit name (not project-prefixed) so the DEPLOY.md backup commands match
  labeler-templates:
    name: labeler-templates
```

- [ ] **Step 2: Create `.env.sample`**

```bash
# Copy to .env to override. docker compose interpolates these two values into docker-compose.yml.
# These are the ONLY operator knobs; everything else is fixed inside the image.

# Host port to publish the service on (the container always listens on 8080 internally).
HOST_PORT=8080

# Log filter, tracing EnvFilter syntax.
RUST_LOG=labeler=info,tower_http=info
```

- [ ] **Step 3: Bring the stack up and verify it serves**

Run:
```bash
set -euo pipefail
HOST_PORT=8080 docker compose up -d --build   # force the published port so the curls below are deterministic
for i in $(seq 1 30); do curl -fsS http://127.0.0.1:8080/api/health >/dev/null 2>&1 && break; sleep 1; done
curl -fsS http://127.0.0.1:8080/api/health; echo
curl -fsS http://127.0.0.1:8080/api/templates -o /tmp/m6-templates.json
grep -q '"id":"avery5163"' /tmp/m6-templates.json && grep -q '"id":"homebox-qr"' /tmp/m6-templates.json && echo "starters seeded"
docker compose ps -a
test "$(docker inspect -f '{{.State.ExitCode}}' "$(docker compose ps -aq labeler-init)")" = 0 && echo "init ok"
```
Expected: `/api/health` 200; the grep asserts print `starters seeded` (proves the init container seeded the templates volume from the labeler image, not empty); `docker compose ps -a` shows `labeler` healthy; the init assertion prints `init ok` (exit code 0). (Use `ps -a` and the explicit inspect, asserted with `test`, because a plain `ps` may hide the already-exited init container and a bare `inspect` would print a non-zero code without failing the step.)

- [ ] **Step 4: Verify volume writability (the must-test invariant)**

Run (note `-T`: scripted `docker compose exec` must disable TTY allocation or it fails in a non-interactive shell):
```bash
docker compose exec -T labeler /busybox/sh -c 'touch /app/data/.w /app/templates/.w && rm /app/data/.w /app/templates/.w && echo writable'
```
Expected: prints `writable`. This proves the init-service chown took effect so the nonroot app can open the sqlite db and accept template uploads. (If it fails, the init service did not run or the uid is wrong; re-check `depends_on`/`user`/`entrypoint`.)

- [ ] **Step 5: Verify persistence across recreate**

Run:
```bash
set -euo pipefail
curl -fsS -X PUT http://127.0.0.1:8080/api/settings/m6_probe -H 'content-type: application/json' -d '{"value":"kept"}'
docker compose down            # NOT -v
HOST_PORT=8080 docker compose up -d
for i in $(seq 1 30); do curl -fsS http://127.0.0.1:8080/api/health >/dev/null 2>&1 && break; sleep 1; done
curl -fsS http://127.0.0.1:8080/api/settings | grep m6_probe
```
Expected: the `m6_probe` setting survives the recreate (named volume `labeler-data` persisted the sqlite db). Clean up afterward: `docker compose down`.

- [ ] **Step 6: Commit**

```bash
git add docker-compose.yml .env.sample
git commit -m "build: docker-compose with seeded+owned named volumes and env config (#25, #9)"
```

---

### Task 3: Deployment guide incl. CUPS (#26)

**Files:**
- Create: `docs/DEPLOY.md`
- Modify: `README.md`

- [ ] **Step 1: Create `docs/DEPLOY.md`**

```markdown
# Deployment

Labeler ships as a single Docker image that serves the REST API and the web UI. The supported path is
`docker compose`; a bare `docker run` works too.

## Requirements

Docker Engine 20.10+ (for `host-gateway`) and Docker Compose v2 with support for `pull_policy: build` and
long-form `depends_on` with `condition: service_completed_successfully` (Compose v2.x; verify your file
parses with `docker compose config`).

## Quick start

```bash
cp .env.sample .env        # optional; edit HOST_PORT / RUST_LOG
docker compose up -d --build
# open http://localhost:8080
```

`docker compose up` builds the image locally (`pull_policy: build`), runs a one-shot `labeler-init`
container that seeds and fixes ownership on the volumes, then starts the service. Health is at
`GET /api/health`.

## Configuration

Only two operator knobs, set in `.env` (Compose interpolates them):

| Var | Default | Meaning |
| --- | --- | --- |
| `HOST_PORT` | `8080` | Host port published to the container's fixed internal port 8080. |
| `RUST_LOG` | `labeler=info,tower_http=info` | Log filter (tracing EnvFilter syntax). |

Everything else is fixed inside the image: the container always listens on `8080` (`PORT` is reserved so
the healthcheck stays valid; remap the host side with `HOST_PORT`), data lives at `/app/data`, the UI at
`/app/ui/dist`, templates at `/app/templates`, fonts at `/app/fonts`.

### Full environment contract

The application itself is fully env-driven with safe defaults and starts with zero required
configuration. The Docker image pins the deployment-sensitive variables so the healthcheck and volume
paths stay consistent; change behavior through `HOST_PORT`/`RUST_LOG` and the volume mounts, not by
overriding these:

| Var | App default | In the image | Change via |
| --- | --- | --- | --- |
| `PORT` | `8080` | fixed `8080` (reserved) | remap the host side with `HOST_PORT` |
| `RUST_LOG` | `labeler=info,tower_http=info` | from `.env` | `.env` |
| `LABELER_DATA_DIR` | `data/` | `/app/data` | mount the `labeler-data` volume |
| `LABELER_UI_DIR` | `ui/dist` | `/app/ui/dist` | baked |
| `LABELER_ASSETS_DIR` | `assets/` | `/app/assets` (empty) | bind-mount a host assets dir (see below) |

Templates (`/app/templates`) and fonts (`/app/fonts`) are CWD-relative app paths fixed in the image;
making them env-configurable is tracked in issue #38. The QR base URL is a runtime *setting* (Settings
screen / `PUT /api/settings/qr_base_url`), not an env var.

### Image assets (templates using `image.src`)

Templates can reference a bundled image by path (`image.src`), resolved under the assets root
(`LABELER_ASSETS_DIR`, `/app/assets` in the image). The image ships an empty `/app/assets`. If you use
`image.src` templates, provide the files by bind-mounting a host directory read-only, e.g. add to the
`labeler` service:

```yaml
    volumes:
      - labeler-data:/app/data
      - labeler-templates:/app/templates
      - ./assets:/app/assets:ro
```

Templates that supply images as data URIs (`image.name`) need no assets directory.

## Data, volumes, and backups

Two named volumes hold all state:

- `labeler-data`: the SQLite database (printers, settings, job log).
- `labeler-templates`: label templates (seeded with the bundled starters on first creation; your uploads
  persist here across image updates).

**`docker compose down -v` and `docker volume rm` DELETE this state.** A plain `docker compose down`
keeps the volumes; only `-v` wipes them. After a wipe, templates re-seed from the image and
settings/printers are gone.

Back up with the app stopped (a file-level copy of a live SQLite db can be inconsistent):

```bash
docker compose stop labeler
docker run --rm -v labeler-data:/d -v "$PWD":/b busybox tar czf /b/labeler-data.tgz -C /d .
docker run --rm -v labeler-templates:/d -v "$PWD":/b busybox tar czf /b/labeler-templates.tgz -C /d .
docker compose start labeler
```

Restore by extracting the tarballs back into the volumes (app stopped), e.g.
`docker run --rm -v labeler-data:/d -v "$PWD":/b busybox tar xzf /b/labeler-data.tgz -C /d`.

### Bind mounts (advanced)

The default uses named volumes. If you bind-mount host directories instead, note that `labeler-init`
runs `chown -R 65532:65532` on the mounted paths, which rewrites the *host* directory ownership. Either
accept that, or pre-`chown` the host dirs to `65532:65532` and remove the `labeler-init` service. A
bind-mounted templates dir does not get the starters; copy them in if you want them. An empty templates
dir is not an error, the service just starts with zero templates.

## Debugging

The image uses distroless `:debug`, which includes a BusyBox shell:

```bash
docker compose exec labeler /busybox/sh
```

This is not a full distro and has no package manager.

## Printing (CUPS / IPP)

The service is an **IPP client**: each printer's `config.uri` must be reachable from the container
network and start with `ipp://` or `ipps://`. No host socket mount, `--network host`, or privileged mode
is required.

Reachability patterns:

- **A network / IPP-Everywhere printer:** `ipp://printer.lan:631/ipp/print`.
- **A CUPS server on the LAN:** `ipp://cups-host:631/printers/<queue>`.
- **CUPS on the Docker host:** `ipp://host.docker.internal:631/printers/<queue>`. The compose file maps
  `host.docker.internal` to the host gateway (`extra_hosts: host.docker.internal:host-gateway`), which
  needs Docker Engine 20.10+ / Compose v2. On older engines, use the Docker bridge gateway IP (often
  `172.17.0.1`) or the printer's LAN IP. Docker Desktop provides its own mapping, so Desktop users can
  remove the `extra_hosts` line if name resolution misbehaves.

**Host CUPS prerequisites (the gateway mapping alone is not enough).** A default host `cupsd` listens
only on `localhost`, so the container cannot reach it. To use host CUPS you must: have `cupsd` `Listen`
on a non-loopback interface (or `Port 631` on all interfaces); allow the Docker bridge subnet in the CUPS
`<Location>` access policy; share the target queue; and open host firewall port 631 to the bridge. Test that the port is reachable
from inside the container (this is a coarse TCP/HTTP probe, not a print test):

```bash
docker compose exec labeler /busybox/wget -S -O- http://host.docker.internal:631/
```

Any HTTP response (even `401`/`403`/`405`, since CUPS may deny the web root or have the web UI disabled)
proves the port is reachable; a connection refused or timeout means it is not. Actual printing is then
validated by registering the queue's `ipp://...` URI and sending a label.

**TLS (`ipps://`).** Verification uses the image's public CA bundle. A self-signed CUPS certificate will
fail. For MVP, use `ipp://` on a trusted LAN. To trust a private CA, build a derived image (distroless
has no `update-ca-certificates`):

```dockerfile
FROM debian:bookworm-slim AS ca
COPY my-ca.crt /usr/local/share/ca-certificates/my-ca.crt
RUN apt-get update && apt-get install -y ca-certificates && update-ca-certificates

FROM labeler:latest
COPY --from=ca /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
```

**Authenticated queues are not supported in MVP.** The printer config is just `{ uri }`; a queue
requiring authentication will be reachable but fail with 401/403. Basic-auth and a custom-CA option are
tracked in issue #39.

### Why not mount the host CUPS socket?

A common Docker pattern bind-mounts the host's `/var/run/cups/cups.sock` (or uses `--network host`) so a
container can drive host CUPS queues. Labeler does not use that: it is an IPP *network* client (it speaks
IPP over TCP to a `uri`), not a libcups/socket client, so a mounted socket would never be used. The
network-IPP approach is portable (identical on Linux, macOS, and Windows Docker) and needs no privileged
access; the tradeoff is that the printer or CUPS endpoint must be reachable over the network (the
prerequisites above), whereas a socket mount would only work on a Linux host with that exact host's
queues. The socket-mount / host-network option is therefore intentionally not provided.

## Architectures

The image builds for the host architecture. For arm64 (e.g. Raspberry Pi):
`docker buildx build --platform linux/arm64 -t labeler:latest .`. First-class multi-arch publishing is
tracked in issue #36.
```

- [ ] **Step 2: Add a Deployment pointer to `README.md`**

After the existing `## Web UI` section in `README.md`, add:
```markdown
## Deployment

Run the whole thing with Docker:

```bash
docker compose up -d --build      # serves on http://localhost:${HOST_PORT:-8080}
```

See [`docs/DEPLOY.md`](docs/DEPLOY.md) for configuration, persistent volumes and backups, and CUPS/IPP
printing setup.
```

- [ ] **Step 3: Verify the compose file the doc references is valid (no daemon needed)**

Run: `docker compose config >/dev/null && echo "compose valid"`
Expected: prints `compose valid`. This only validates the compose YAML the deploy doc references; the `docker compose exec ... wget` CUPS reachability command in `docs/DEPLOY.md` is environment-specific and is validated by the operator against a real queue.

- [ ] **Step 4: Commit**

```bash
git add docs/DEPLOY.md README.md
git commit -m "docs: deployment guide incl. CUPS reachability (#26)"
```

---

### Task 4: ADR + SPEC changelog

**Files:**
- Create: `docs/adr/0016-deployment-and-packaging.md`
- Modify: `docs/adr/README.md`
- Modify: `docs/SPEC.md`

- [ ] **Step 1: Write ADR-0016**

Create `docs/adr/0016-deployment-and-packaging.md`:
```markdown
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
```

- [ ] **Step 2: Add the index row**

In `docs/adr/README.md`, after the ADR-0015 row, add:
```markdown
| [0016](0016-deployment-and-packaging.md) | Deployment and packaging | Accepted |
```

- [ ] **Step 3: Add the SPEC changelog entry**

In `docs/SPEC.md`, at the top of the `## Changelog` list (match the colon style; no em dash):
```markdown
- **2026-06-16**: Packaging & deployment (M6): a multi-stage `Dockerfile` (distroless), `docker-compose.yml`
  with seeded+owned named volumes, `.env.sample`, and `docs/DEPLOY.md` (build, volumes/backups, CUPS/IPP).
  No API change (ADR-0016; #18, #25, #26, #9).
```

- [ ] **Step 4: Commit**

```bash
git add docs/adr/0016-deployment-and-packaging.md docs/adr/README.md docs/SPEC.md
git commit -m "docs: ADR-0016 deployment & packaging; SPEC changelog"
```

---

### Task 5: Adversarial review loop and integrate

The per-task commits are local WIP on `m6-packaging`; nothing reaches `main` until this review passes (CLAUDE.md "Working on an issue"; codex capped at 5 passes, stop at the first no-MAJOR).

- [ ] **Step 1: Adversarial review of the whole diff**

Dispatch an adversarial reviewer against `git diff main...m6-packaging`. It audits the Dockerfile (stage paths, COPY-from, chown, healthcheck), compose (init-image seeding invariant, `pull_policy: build`, depends_on, host port mapping, healthcheck), `.dockerignore` (not excluding `templates/`/`fonts/`/`src/`/`Cargo.*`), `.env.sample`, `docs/DEPLOY.md` accuracy (CUPS prerequisites, `down -v` warning, backup-with-app-stopped, TLS/auth limits), and the ADR/SPEC, against the spec and #18/#25/#26/#9 acceptance criteria. Require file:line evidence.

- [ ] **Step 2: Fix every meaningful finding, then re-review**

Address each finding (fix it, or justify with evidence). Re-dispatch until a pass surfaces no meaningful fixes (consciously declined nits do not count).

- [ ] **Step 3: Re-run the full smoke verification on the final artifacts**

From the repo root (requires Docker). NOTE: the leading `down -v` DELETES the local `labeler-data` and `labeler-templates` named volumes to verify a clean from-empty seed. That is fine on a dev/CI box, but do NOT run it on a host holding real labeler state.
```bash
set -euo pipefail
docker compose down -v 2>/dev/null || true
HOST_PORT=8080 docker compose up -d --build
for i in $(seq 1 30); do curl -fsS http://127.0.0.1:8080/api/health >/dev/null 2>&1 && break; sleep 1; done
curl -fsS http://127.0.0.1:8080/api/health; echo
# seeding
curl -fsS http://127.0.0.1:8080/api/templates -o /tmp/m6-templates.json
grep -q '"id":"avery5163"' /tmp/m6-templates.json && grep -q '"id":"homebox-qr"' /tmp/m6-templates.json && echo "starters seeded"
# writability
docker compose exec -T labeler /busybox/sh -c 'touch /app/data/.w /app/templates/.w && rm /app/data/.w /app/templates/.w && echo writable'
# persistence across recreate (re-checked here in case a review fix touched volumes/compose)
curl -fsS -X PUT http://127.0.0.1:8080/api/settings/m6_probe -H 'content-type: application/json' -d '{"value":"kept"}'
docker compose down            # NOT -v
HOST_PORT=8080 docker compose up -d
for i in $(seq 1 30); do curl -fsS http://127.0.0.1:8080/api/health >/dev/null 2>&1 && break; sleep 1; done
curl -fsS http://127.0.0.1:8080/api/settings -o /tmp/m6-settings.json
grep -q '"m6_probe":"kept"' /tmp/m6-settings.json && echo "persisted"
docker compose down
```
Expected: health 200, `starters seeded`, `writable`, `persisted`. If Docker is unavailable here, report it and hand the smoke run to the operator before merge.

- [ ] **Step 4: Mark P1-61..P1-64 DONE**

Capture the Dockerfile commit hash (`git log --grep='multi-stage Dockerfile' --format=%h -n 1`) and in `docs/PLAN-phase-1.md` change the four headings (match the existing `· DONE (hash)` style; no em dash), using each item's own implementing commit hash:
```markdown
#### P1-61 Dockerfile (single image) · GH #18 · DONE (<dockerfile-commit>)
#### P1-62 docker-compose + persistent volumes · GH #25 · DONE (<compose-commit>)
#### P1-63 CUPS access documentation + wiring · GH #26 · DONE (<deploy-doc-commit>)
#### P1-64 Env-var configuration + sample env · GH #9 · DONE (<compose-commit>)
```
**Rescope note for P1-63 (#26):** the original AC said to describe "both socket-mount and network-CUPS options with trade-offs." The implementation intentionally supports **network IPP only**; ADR-0016 and the `docs/DEPLOY.md` "Why not mount the host CUPS socket?" section document the socket-mount option and explain why it is rejected (the app is an `ipp`-crate TCP client, not a libcups/socket client; socket mounts are Linux-host-only and host-coupled). Edit the P1-63 body in the same change to reflect this: "AC met by documenting network-IPP reachability and explaining why the socket-mount option is intentionally not provided (ADR-0016)." Then mark DONE and close #26.

**Rescope note for P1-64 (#9):** the original AC listed "PORT, data dir, QR base URL, log level" as env vars. Two of those were superseded by later decisions: the app is env-driven (`PORT`, `LABELER_DATA_DIR`, `LABELER_UI_DIR`, `RUST_LOG`, all with defaults, zero required config) and `docs/DEPLOY.md` documents the full env contract, but **the QR base URL is a runtime setting** (`PUT /api/settings/qr_base_url`, per ADR-0010, not an env var) and **making the templates/fonts dirs env-configurable is deferred to #38**. Record this by editing the P1-64 body in the same change to read: "AC met: config is env-driven with defaults (PORT, data dir, UI dir, log level) and a sample `.env` is documented; QR base URL is a runtime setting (ADR-0010); env-configurable template/font dirs deferred to #38." Only then mark it DONE and close #9.
(`git log --oneline main..m6-packaging` lists the commits to pull hashes from.) Then commit this docs change with the issue-closing keywords in the body (one keyword per issue; `Closes #18 #25 #26 #9` on one line does NOT close all four). This commit lands on `main` via the merge, which is what closes the issues:
```bash
git add docs/PLAN-phase-1.md
git commit -m "docs: mark P1-61..P1-64 (M6 packaging) done

Fixes #18
Fixes #25
Fixes #26
Fixes #9"
```

- [ ] **Step 5: Final gate and integrate**

```bash
(cd ui && npm ci && npm run lint && npm run test && npm run build)
cargo fmt && cargo clippy --all-targets --all-features && cargo test
git checkout main && git merge m6-packaging && git push
```
No app code changed, so the cargo/UI gates pass unchanged but are run per the repo rule. The UI gate runs in a subshell so a failure there does not leave you in `ui/` for the cargo commands. The four `Fixes #N` keywords in the Step 4 commit body close #18/#25/#26/#9 when the branch merges to `main`; verify with `gh issue view <n>` after push and close any that did not auto-close.

---

## Self-Review

**1. Spec coverage:**
- Multi-stage Dockerfile, distroless `:debug`, WORKDIRs, chown, healthcheck, ENV → Task 1 (verbatim).
- `.dockerignore` not excluding `templates/`/`fonts/` → Task 1 Step 1.
- compose with init-image seeding + chown, `pull_policy: build`, host-port remap, healthcheck, named volumes → Task 2.
- Two env knobs + `.env.sample` + fixed-in-image vars → Task 2 + DEPLOY.md.
- CUPS network-IPP-only, host-CUPS prerequisites, host-gateway engine note, `ipps://` TLS + derived-image snippet, unauthenticated-only → Task 3 (DEPLOY.md).
- Volume caveats: `down -v` warning, backup-app-stopped, bind-mount opt-in, zero-templates → Task 3.
- ADR-0016 + SPEC changelog → Task 4.
- Smoke verification (build, busybox path, /api/health, SPA, templates, writability, persistence) → Tasks 1, 2, 5.
- DONE markers P1-61..64 → Task 5.

**2. Placeholder scan:** No TBD/TODO; all file contents are complete; commands have expected output. The only `<...>` are commit-hash placeholders in the DONE markers, filled from `git log` at execution time.

**3. Consistency:** Image name `labeler:latest`, uid `65532`, port `8080`, volume names `labeler-data`/`labeler-templates`, and the `/app/*` paths are identical across the Dockerfile, compose, DEPLOY.md, and ADR. The init container uses the labeler image (not busybox) everywhere it appears.

**Note on verification:** all real verification is `docker build` + `docker compose` smoke steps requiring a Docker daemon. There are no unit tests to add (no code change). If the execution environment lacks Docker, the implementer must say so at the affected step rather than marking it passed.
