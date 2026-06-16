# M6 Packaging & Deployment — design

Service: a Rust/axum label-rendering REST service that also serves a built React SPA. Renders via typst-as-lib; stores app state (printers, settings, job log) in SQLite via rusqlite (feature `bundled`); prints over IPP via the `ipp` crate (kind `cups`, config `{ uri }`, uri must start with `ipp://` or `ipps://`).

Runtime path facts (verified in source):
- Templates loaded at startup from `templates/` (relative to CWD); the upload/replace/delete API writes into this same dir and the registry hot-reloads. So it is READ-WRITE at runtime.
- Fonts loaded from `fonts/` (relative to CWD) via typst-kit `include_dirs(["fonts"])`. Bundled file: `fonts/InterVariable.ttf`.
- UI served from `LABELER_UI_DIR` (default `ui/dist`, relative to CWD).
- Data dir: `LABELER_DATA_DIR` (default `data/`); SQLite db at `<data>/labeler.db`; created at startup with `create_dir_all`.
- `PORT` (default 8080), binds `0.0.0.0:PORT`.
- `RUST_LOG` via tracing EnvFilter (default `labeler=info,tower_http=info`).
- Templates dir and fonts dir are HARDCODED relative paths ("templates", "fonts"); not env-configurable.
- `main.rs` panics on template-load, data-dir-create, or store-open failure (so an unwritable data volume is a crash loop, which the deployment design must prevent).

## Decisions (settled with the user, refined through adversarial review)
1. **CUPS access: NETWORK IPP ONLY.** The container is a pure IPP client; the printer config uri must be reachable from the container network. No socket mounts, no privileged mode, no host-CUPS coupling, no cupsd in the image. #26 is documentation of reachability patterns.
2. **Templates persistence: a NAMED VOLUME** at `/app/templates`, seeded from the image's bundled starters and owned by the nonroot uid via a compose **init service that runs the labeler image** (it is the first mounter, so Docker copy-up seeds the volume from the labeler image, and it chowns the volume to 65532). User uploads persist across image updates.
3. **Runtime base image: `gcr.io/distroless/cc-debian12:debug`** (BusyBox shell for `docker exec` debugging + BusyBox `wget` for HEALTHCHECK; glibc + CA certs; runs nonroot 65532; no package manager). No system libs needed (sqlite bundled). The `:debug` variant is an intentional tradeoff (shell/healthcheck/init convenience over pure-minimal posture), recorded in the ADR.
4. **Architectures: single-arch host-native for MVP** (`docker build`); document `docker buildx --platform linux/arm64` for Pi users; a follow-up issue tracks first-class multi-arch publishing.

## Scope
Issues #18 (Dockerfile), #25 (compose + volumes), #9 (env config + sample), #26 (CUPS docs). One spec/plan. No application code change.

## 1. Multi-stage Dockerfile (#18)
WORKDIRs are explicit in every stage so the COPY-from paths are deterministic.
```dockerfile
FROM node:22-bookworm-slim AS ui
WORKDIR /ui
COPY ui/package*.json ./
RUN npm ci
COPY ui/ ./
RUN npm run build            # tsc -b && vite build -> /ui/dist

FROM rust:1-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --release --locked   # binary at /app/target/release/labeler (gcc present for rusqlite bundled)
RUN mkdir -p /seed/data /seed/assets   # empty writable data dir + empty assets dir for the runtime

FROM gcr.io/distroless/cc-debian12:debug AS runtime
WORKDIR /app
COPY --from=build /app/target/release/labeler /app/labeler
COPY --chown=65532:65532 templates/ /app/templates/   # starter templates; named-volume seed source
COPY fonts/ /app/fonts/
COPY --from=ui /ui/dist /app/ui/dist
COPY --chown=65532:65532 --from=build /seed/data /app/data
COPY --from=build /seed/assets /app/assets
USER nonroot                 # uid 65532
EXPOSE 8080
ENV PORT=8080
ENV LABELER_DATA_DIR=/app/data
ENV LABELER_UI_DIR=/app/ui/dist
ENV LABELER_ASSETS_DIR=/app/assets
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s \
  CMD ["/busybox/wget","-qO-","http://127.0.0.1:8080/api/health"]   # busybox lives at /busybox in distroless :debug
ENTRYPOINT ["/app/labeler"]
```
- `.dockerignore`: exclude `target/`, `ui/node_modules`, `ui/dist`, `data/`, `*.pdf`, `.git`, local artifacts (the image rebuilds `ui/dist` and the binary internally).
- Caching: no separate `cargo fetch` layer. This repo's `Cargo.toml` has no `[[bin]]`/`[lib]` section (it auto-discovers `src/main.rs`), so cargo cannot parse the manifest until `src/` is present; `src/` is copied before `cargo build --release --locked`. BuildKit cache mounts or cargo-chef can speed rebuilds later. Not a correctness concern for MVP.
- Base tags (`node:22-bookworm-slim`, `rust:1-bookworm`, distroless `:debug`) float within a tag; for reproducible release builds, pin the tested digests (record them in the plan). MVP uses the tags and records the digest it was built/verified against. Note: a transitive dependency (`ipp` 6) uses Rust edition 2024, so the build needs a recent toolchain (rustc >= 1.85); `rust:1-bookworm` currently satisfies this, and pinning prevents an accidentally old toolchain.

Rationale for chown: a Docker NAMED VOLUME (and only an empty one) initializes its contents AND ownership from the image directory at the mountpoint on first use ("copy-up"), so /app/templates and /app/data must be owned by uid 65532 or the nonroot process cannot write through the volume. This does NOT apply to bind mounts or pre-existing volumes (see Volumes caveats); the compose init service makes the ownership guarantee robust.

## 2. docker-compose + volumes (#25)
```yaml
# Both services share ONE locally-built image. pull_policy: build forces a local build (never a registry
# pull of an unrelated labeler:latest), so the init container is guaranteed to be the labeler image.
x-labeler-image: &labeler-image
  build: .
  image: labeler:latest
  pull_policy: build

services:
  # One-shot init: makes the named volumes writable by the nonroot app uid (65532). It MUST use the
  # labeler image (not busybox): Docker seeds a new empty volume from the image of the FIRST container
  # that mounts it, so the init container must carry the starter templates, or the templates volume
  # would initialize empty. It overrides the entrypoint to chown via the bundled busybox.
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
    ports: ["${HOST_PORT:-8080}:8080"]   # container port is fixed at 8080; remap the HOST side only
    environment:
      RUST_LOG: ${RUST_LOG:-labeler=info,tower_http=info}
    volumes:
      - labeler-data:/app/data
      - labeler-templates:/app/templates
    extra_hosts:
      - "host.docker.internal:host-gateway"   # maps host gateway on Linux; see CUPS prerequisites
    healthcheck:
      test: ["CMD","/busybox/wget","-qO-","http://127.0.0.1:8080/api/health"]
      interval: 30s
      timeout: 3s
      retries: 3
      start_period: 5s
    restart: unless-stopped
volumes:
  labeler-data:
  labeler-templates:
```
- **Why the init service uses the labeler image:** it serves double duty. (1) Seeding: because the init container is the first to mount `labeler-templates`, Docker's copy-up initializes that volume from *its* image, so it must be the labeler image (which has starters at `/app/templates`); a busybox init would seed the volume empty and the starters would never appear. (2) Ownership: its entrypoint chowns `/app/data` and `/app/templates` to 65532 so the nonroot app can open the sqlite db and accept template uploads, independent of copy-up ownership quirks (the empty `/app/data` is the case most likely to land `root:root`, which would make `Store::open` panic). It runs idempotently on every `up` and repairs existing named volumes. The image `COPY --chown` is also kept for the bare `docker run -v` path (no compose), where the labeler container is itself the first mounter.
- **Bind mounts are a separate, documented opt-in, not the default.** A `docker-compose.bind.yml` override (or doc snippet) shows bind-mounting host dirs, with an explicit warning that `labeler-init`'s `chown -R` would then rewrite *host* directory ownership to 65532. Bind-mount users either accept that, or pre-`chown` the host dirs and drop the init service. The default named-volume path never mutates host files.

### Environment wiring (avoids a PORT/healthcheck inconsistency)
- The **container port is fixed at 8080**, set as `ENV PORT=8080` in the image and documented as reserved; the image `HEALTHCHECK` and compose healthcheck both probe `127.0.0.1:8080`, which is always correct. Operators remap the **host** port via `${HOST_PORT:-8080}` in `ports:` only. `PORT`/`LABELER_DATA_DIR`/`LABELER_UI_DIR` are image-internal and not meant to be overridden in the container (overriding `PORT` would desync the healthchecks).
- Compose injects overridable values by **referencing** them in the model (`RUST_LOG: ${RUST_LOG:-...}`, `ports: "${HOST_PORT:-8080}:8080"`); a plain `.env` file only feeds Compose interpolation, it does not auto-inject container env. `.env.sample` documents `HOST_PORT` and `RUST_LOG` (the only two knobs).
- Both named volumes persist across `down`/`up` and recreate.

### Volumes caveats (documented for operators)
- **`docker compose down -v` (and `docker volume rm`) DELETES app state.** Printers, settings, and the job log live in `labeler-data`; uploaded templates live in `labeler-templates`. `down -v` wipes both: settings/printers are lost and templates re-seed from the image. The deploy doc calls this out in bold.
- **Back up with the app stopped (SQLite is live).** A file-level tar of an open SQLite db can capture an inconsistent state, so the doc says to `docker compose stop labeler` first, then `docker run --rm -v labeler-data:/d -v "$PWD":/b busybox tar czf /b/labeler-data.tgz -C /d .` (and the inverse to restore), then `docker compose start labeler`. (A future app-level SQLite backup-API command is noted as a nicer alternative.)
- **Bind mounts do not auto-seed and keep host ownership.** A bind-mounted templates dir starts empty (no starters); the `labeler-init` service chowns it (and the data dir) to 65532 so writes work, but starters must be copied in manually if wanted.
- **A pre-existing named volume is not re-seeded.** Upgrading the image does not refresh starters into an already-initialized templates volume (by design: user edits win).
- **Zero-templates is non-fatal but worth surfacing.** Startup loads whatever is in `/app/templates`; an empty dir yields zero templates (the registry is fine, just empty). The deploy doc notes this so an operator who bind-mounted an empty dir understands why no templates appear.

## 3. Environment config + sample (#9)
- **Two operator knobs:** `HOST_PORT` (host-side port, default 8080) and `RUST_LOG` (default `labeler=info,tower_http=info`). `.env.sample` documents exactly these two, with comments. `.env` is consumed by Compose **interpolation** only; the compose `environment:`/`ports:` keys reference the vars explicitly.
- **Fixed in-container (not deployment knobs):** `PORT`=8080 (container listen port), `LABELER_DATA_DIR`=/app/data, `LABELER_UI_DIR`=/app/ui/dist, templates dir=/app/templates, fonts dir=/app/fonts. The deploy doc states these are image-internal; changing the listen port is done by remapping `HOST_PORT`, not `PORT`, so the healthchecks stay valid.

## 4. CUPS reachability documentation (#26)
A "Printing / CUPS" section in `docs/DEPLOY.md` covering:
- The service is an IPP client; the printer `config.uri` must be reachable from the container network and start with `ipp://` or `ipps://`.
- Patterns: (a) a network/IPP-Everywhere printer `ipp://printer.lan:631/ipp/print`; (b) a CUPS server (incl. the host) `ipp://host.docker.internal:631/printers/<queue>`; (c) another CUPS box on the LAN by IP/hostname.
- **Host CUPS prerequisites (host.docker.internal alone is not enough).** A default host cupsd listens only on `localhost`, so the container cannot reach it even with the gateway mapping. To use host CUPS the operator must: have cupsd `Listen` on a non-loopback interface or `Port 631` on all interfaces; allow the Docker bridge subnet in the CUPS `<Location>` access policy; share the target queue; and open host firewall port 631 to the bridge. The doc gives an in-container reachability test using the debug shell: `docker compose exec labeler /busybox/wget -S -O- http://host.docker.internal:631/`.
- **`host-gateway` requires Docker Engine 20.10+ / Compose v2.** On older engines `extra_hosts: host.docker.internal:host-gateway` is rejected; the documented fallback is to use the Docker bridge gateway IP (e.g. `172.17.0.1`) or the printer's LAN IP directly. Docker Desktop provides its own mapping, so Desktop users can drop the line if host resolution misbehaves.
- `ipps://` uses TLS via the `ipp` crate (rustls; no OpenSSL/system lib needed) and verifies against the image's CA bundle (distroless cc ships public CAs). A self-signed CUPS certificate fails verification. MVP supported path: use `ipp://` on a trusted LAN. Trusting a private CA requires a derived image (distroless has no `update-ca-certificates`): in a Debian stage, drop the CA into `/usr/local/share/ca-certificates/` and run `update-ca-certificates`, then `COPY --from=<debian-stage> /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt` into the distroless image. The deploy doc shows this exact snippet. There is no client-side TLS-skip and no custom-CA env knob in MVP (a possible later enhancement).
- **Unauthenticated queues only (MVP).** Printer config is just `{ uri }` and printing calls `AsyncIppClient::new(uri)` with no credentials, so a CUPS queue requiring authentication will be reachable but fail with 401/403. The doc states MVP supports only unauthenticated IPP/CUPS queues; basic-auth (via a future `config` credential field and `AsyncIppClient::builder(uri).basic_auth(...)`) is a noted later enhancement.
- Explicitly: no host socket mount, no `--network host`, no privileged mode required.

## ADR
ADR-0016 "Deployment and packaging" records: the single-image multi-stage build; the **intentional choice of the distroless `:debug` variant** (BusyBox shell relied on for the healthcheck `wget`, the compose init chown, and operator debugging) and the explicit note that this trades a little of pure-distroless's minimal-attack-surface posture for that convenience (a future `runtime` vs `runtime-debug` split with an app-native healthcheck is the path to a leaner production image); network-IPP-only printing with the unauthenticated-queue limitation; init-service-driven volume seeding + ownership; nonroot uid 65532; single-arch MVP. SPEC.md gets no API change; a changelog line notes the deployment artifacts were added.

## Docs placement
New `docs/DEPLOY.md` (Docker build, compose up, volumes, env, CUPS) + a short "Deployment" pointer from `README.md`.

## Testing / verification (no app code change)
Manual smoke steps in the plan:
- `docker build -t labeler .` succeeds.
- BusyBox path is real (the healthcheck depends on it): `docker run --rm --entrypoint=/busybox/wget labeler:latest --help` exits 0. If a future distroless tag moves busybox, switch the healthcheck/docs to a busybox `wget` copied from `busybox:1.37` into a fixed path.
- `docker compose up -d`; `GET /api/health` -> 200; `GET /` serves the SPA; `GET /api/templates` lists the bundled starters.
- Upload a template via the UI/API; `docker compose down && docker compose up -d` (NOT `-v`); the uploaded template and any printer/setting persist (named volumes).
- `docker compose exec labeler /busybox/sh` to inspect (BusyBox shell under `/busybox` in distroless `:debug`; this is not a full distro and has no package manager).
- **Verify volume writability (the must-test invariant):** as the app user, create and delete a file in each mounted dir, e.g. `docker compose exec labeler /busybox/sh -c 'touch /app/data/.w /app/templates/.w && rm /app/data/.w /app/templates/.w && echo writable'`. This proves the init-service chown took effect and the app can open the sqlite db and accept template uploads.

## Out of scope / follow-ups (file as issues)
- First-class multi-arch (amd64+arm64) publishing.
- A registry/CI publish pipeline (no CI yet).
- Making templates/fonts dirs env-configurable.
- App-level custom-CA support and IPP basic-auth credentials.
