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
