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

## Run a published image (GHCR)

CI publishes images to `ghcr.io/pfa230/labeler`:

- `:edge` and `:sha-<short>` on every push to `main`.
- `:X.Y.Z`, `:X.Y`, and `:latest` on a `vX.Y.Z` release tag.

Pull and run it:

```bash
docker run -d -p 8080:8080 -v labeler-data:/app/data -v labeler-templates:/app/templates \
  ghcr.io/pfa230/labeler:latest
```

To use Compose with the published image instead of building locally, change the `x-labeler-image`
anchor in `docker-compose.yml` from the local-build form to:

```yaml
x-labeler-image: &labeler-image
  image: ghcr.io/pfa230/labeler:latest
```

(removing `build: .` and `pull_policy: build`), then `docker compose up -d` (no `--build`). If the very
first automated publish ever fails to link the package to the repo, open the package page on GitHub and
use "Connect repository" / Manage Actions access to link it.

## Configuration

The common knobs, set in `.env` (Compose interpolates them); the full environment contract follows:

| Var | Default | Meaning |
| --- | --- | --- |
| `HOST_PORT` | `8080` | Host port published to the container's fixed internal port 8080. |
| `RUST_LOG` | `labeler=info,tower_http=info` | Log filter (tracing EnvFilter syntax). |

Everything else is fixed inside the image: the container always listens on `8080` (`PORT` is reserved so
the healthcheck stays valid; remap the host side with `HOST_PORT`), data lives at `/app/data`, the UI at
`/app/ui/dist`, templates at `/app/templates`, fonts at `/app/fonts`.

### Full environment contract

The application is fully env-driven with safe defaults and needs no configuration to start. In the
image, the path and port variables are pinned so the healthcheck and volume mounts line up (change those
by remapping `HOST_PORT` or mounting volumes, not by setting the variable directly); the rest
(`RUST_LOG` and the auth/proxy knobs) are set in `.env`. The `Change via` column gives the right lever
for each:

| Var | App default | In the image | Change via |
| --- | --- | --- | --- |
| `PORT` | `8080` | fixed `8080` (reserved) | remap the host side with `HOST_PORT` |
| `RUST_LOG` | `labeler=info,tower_http=info` | from `.env` | `.env` |
| `LABELER_DATA_DIR` | `data/` | `/app/data` | mount the `labeler-data` volume |
| `LABELER_UI_DIR` | `ui/dist` | `/app/ui/dist` | baked |
| `LABELER_ASSETS_DIR` | `assets/` | `/app/assets` (empty) | bind-mount a host assets dir (see below) |
| `LABELER_INIT_USER` | unset | unset | `.env` (first-run bootstrap; see Authentication) |
| `LABELER_INIT_PASSWORD` | unset | unset | `.env` (first-run bootstrap; see Authentication) |
| `LABELER_TRUST_PROXY` | `false` | unset | `.env` (set `true` behind a TLS-terminating proxy) |
| `LABELER_NO_AUTH` | unset | unset | `.env` (set `true` for single-user LAN-trust homelab; see Authentication) |

Templates (`/app/templates`) and fonts (`/app/fonts`) are CWD-relative app paths fixed in the image;
making them env-configurable is tracked in issue #38. The QR base URL is a runtime *variable* (the
Variables section of the Settings screen, or `PUT /api/variables/qr_base_url`), not an env var.

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

## Authentication

Every `/api` route requires authentication (flat user accounts, ADR-0017). The first run is empty: open
the UI and the first-run setup screen creates the first account, or seed it from the environment.

- **First-run bootstrap.** Set `LABELER_INIT_USER` and `LABELER_INIT_PASSWORD` to create the first user
  at startup when no users exist (a convenience for headless deploys). The password is read from the
  environment and never logged. Both must be non-empty; the seed runs only while zero users exist, so
  rotating these later has no effect. Prefer Docker secrets or an out-of-band `.env` over committing the
  password.

  ```env
  LABELER_INIT_USER=admin
  LABELER_INIT_PASSWORD=change-me-now
  ```

- **Secure cookie behind a proxy.** The session cookie is marked `Secure` only when the effective scheme
  is https. If you terminate TLS at a reverse proxy and forward plain http to the container, set
  `LABELER_TRUST_PROXY=true` so the service honors `X-Forwarded-Proto` and still issues a `Secure`
  cookie. Leave it unset on a plain-http LAN (the cookie is then non-Secure, acceptable under LAN-trust).
  Do not enable it unless a trusted proxy actually sets `X-Forwarded-Proto`, or a LAN client could spoof
  the header. When `LABELER_TRUST_PROXY=true`, the proxy should also forward `X-Forwarded-Host` (the
  original browser host); the CSRF origin check uses it so cookie-authenticated writes are not rejected
  when the proxy rewrites `Host` to an internal value.

- **No-auth mode (LAN-trust homelab).** `LABELER_NO_AUTH=true` removes the login wall for single-user
  LAN-trust use. The data API is open to anyone on the network (including the stored Homebox API key);
  the credential-management endpoints (`/auth/setup`, `/auth/login`, `/auth/logout`, `/auth/password`,
  `/users`, `/tokens`) return `403` so no durable credential can be created while this is set, and a
  relaxed origin check still rejects browser drive-by writes with a mismatched `Origin`. Leave unset
  (the default) to require login. Deliberate opt-in only.

  ```env
  LABELER_NO_AUTH=true
  ```

- **Automation uses API tokens.** Non-browser callers (scripts, the CSV importer, integrations) must
  send `Authorization: Bearer $LABELER_API_TOKEN`. Create a token in the UI (Settings), store it as
  `LABELER_API_TOKEN` in the caller's environment, and pass it on every request, e.g.
  `curl -H "Authorization: Bearer $LABELER_API_TOKEN" .../api/templates`. The bundled `scripts/*.sh`
  read `LABELER_API_TOKEN` from the environment. The Docker healthcheck stays on the exempt
  `/api/health`, so it needs no token.

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

## Architectures

Images are published as a multi-arch manifest list covering `linux/amd64` and `linux/arm64`, so
`docker pull ghcr.io/pfa230/labeler:<tag>` resolves the right variant automatically on x86 servers and
on arm64 boards (Raspberry Pi 4/5, Apple-silicon dev boxes). No arch-specific tag or flag is needed.
