# Deployment

Labeler ships as one Docker image that serves the REST API and the web UI on port 8080. This guide covers running it; what the service does is specified under [`openspec/specs/`](../openspec/specs/).

## Run it

Docker Engine 20.10+ and Docker Compose v2 are required (the compose file uses `host-gateway` and `pull_policy: build`).

```bash
cp .env.sample .env        # optional: HOST_PORT, RUST_LOG, PUID, PGID
docker compose up -d --build
# open http://localhost:8080
```

The bundled `docker-compose.yml` builds the image locally. To run the published image instead, replace the `x-labeler-image` anchor with the line below and drop `--build`:

```yaml
x-labeler-image: &labeler-image
  image: ghcr.io/pfa230/labeler:latest
```

Without Compose:

```bash
docker run -d -p 8080:8080 -v labeler-config:/config ghcr.io/pfa230/labeler:latest
```

The container starts as root, creates `/config/templates` and `/config/assets`, chowns `/config` to `PUID:PGID`, then runs `/app/labeler` as that user through `gosu`. The image healthcheck runs `/app/labeler healthcheck`, which probes `GET /api/health` on the local port.

## Published images

CI publishes `ghcr.io/pfa230/labeler` as a multi-arch manifest for `linux/amd64` and `linux/arm64`; Docker picks the right variant.

| Trigger | Tags |
| --- | --- |
| push to `main` | `edge`, `sha-<short>` |
| tag `vX.Y.Z` | `X.Y.Z`, `X.Y`, `latest` |

## Environment variables

Set these in `.env` for Compose, or with `-e` for `docker run`. Nothing is required to start.

| Name | Default | Meaning |
| --- | --- | --- |
| `HOST_PORT` | `8080` | Compose only: host port published to the container's port 8080. |
| `PORT` | `8080` | Listen port. Leave it: the compose file publishes container port 8080. Remap with `HOST_PORT`. |
| `RUST_LOG` | `labeler=info,tower_http=info` | Log filter, tracing `EnvFilter` syntax. |
| `PUID` / `PGID` | `1000` / `1000` | User and group the service runs as and `/config` is chowned to. Set to `id -u` / `id -g` of the volume's owner. |
| `LABELER_CONFIG_DIR` | `/config` | All persistent state. In the container, mount a volume here rather than changing it. |
| `LABELER_FONTS_DIR` | `fonts`, relative to the working directory (`/app` in the image) | Font directory; must contain `InterVariable.ttf`. Baked into the image. |
| `LABELER_UI_DIR` | `ui/dist`; image sets `/app/ui/dist` | Built web UI. Baked into the image. |
| `LABELER_TRUST_PROXY` | `false` | `true` behind a TLS-terminating reverse proxy that sets `X-Forwarded-Proto` and `X-Forwarded-Host`. |
| `LABELER_NO_AUTH` | `false` | `true` removes the login wall, for single-user trusted-LAN use. |

The two boolean variables are on only for the exact value `true`. Their effects are specified in [auth](../openspec/specs/auth/spec.md) (Origin check, No-auth mode). Do not set `LABELER_TRUST_PROXY` unless a trusted proxy really sets those headers, or a LAN client can spoof them.

## First run

1. Open the UI. With no users, it shows the setup screen that creates the first account. For a scripted install, create it with `curl -X POST https://<host>/api/auth/setup -H 'Origin: https://<host>' -H 'Content-Type: application/json' -d '{"username":"admin","password":"<password>"}'`; setup without `Authorization` needs an `Origin` matching the host.
2. Install templates. The config starts empty; the Labels screen offers the catalog and a **Paste YAML** option. The catalog lives in this repo under `catalog/` (`catalog/index.json` lists it), and only your browser fetches it.
3. Add a printer under Settings → Printers (see [Printing](#printing-cups--ipp)).
4. For scripts and integrations, create an API token under Settings → API tokens and send it as `Authorization: Bearer $LABELER_API_TOKEN`.
5. If your templates use `{vars.qr_base_url}`, set it under Settings → Variables.

To install a catalog template without the UI, for example on an air-gapped host, POST its YAML under its id:

```bash
curl -fsSL https://raw.githubusercontent.com/pfa230/labeler/main/catalog/tape/brother/brother_12mm.yaml \
  | curl -fsS -X POST http://localhost:8080/api/templates/brother_12mm \
      -H "Authorization: Bearer $LABELER_API_TOKEN" --data-binary @-
```

Templates that reference images by `image.src` read them from `/config/assets/`. Copy files there, or bind-mount a host directory over it (`- ./assets:/config/assets:ro`).

## Data and backups

All state lives in `/config`, the `labeler-config` named volume: `labeler.db` (SQLite: users, tokens, printers, settings, variables, job log), `templates/` and `assets/`.

`docker compose down -v` and `docker volume rm labeler-config` delete it. A plain `docker compose down` keeps it.

Back up with the service stopped, since copying a live SQLite file can produce an inconsistent copy:

```bash
docker compose stop labeler
docker run --rm -v labeler-config:/d -v "$PWD":/b busybox tar czf /b/labeler-config.tgz -C /d .
docker compose start labeler
```

Restore with the service stopped:

```bash
docker run --rm -v labeler-config:/d -v "$PWD":/b busybox tar xzf /b/labeler-config.tgz -C /d
```

To use a host directory instead of the named volume, mount it at `/config` (`./config:/config` is commented out in `docker-compose.yml`) and set `PUID`/`PGID` to its owner.

## Upgrades

1. Back up `/config`.
2. Pull and recreate: `docker compose pull && docker compose up -d` for the published image, or `docker compose up -d --build` after updating the checkout.
3. Check the log. The database schema migrates on startup. A template the new version cannot load is quarantined and logged as `template failed to load`; the service still starts. Fix the file and `POST /api/templates/reload`, or edit it in the UI.

There is no downgrade path. An older binary refuses a database migrated by a newer one and exits with `failed to open store`. Restore the backup taken before the upgrade.

## Printing (CUPS / IPP)

Labeler is an IPP client. Each printer's URI must start with `ipp://` or `ipps://` and be reachable from the container network; no host socket, host networking or privileged mode is needed. The printer fields (credentials, CA certificate, `insecure`, render overrides) are specified in [printing](../openspec/specs/printing/spec.md).

| Printer | URI |
| --- | --- |
| Network printer with IPP Everywhere | `ipp://printer.lan:631/ipp/print` |
| CUPS server on the LAN | `ipp://cups-host:631/printers/<queue>` |
| CUPS on the Docker host | `ipp://host.docker.internal:631/printers/<queue>` |

`localhost` is the Labeler container itself, so it reaches CUPS on the Docker host only under host networking; otherwise use `host.docker.internal`. The compose file maps `host.docker.internal` to the host gateway; on Docker Desktop, which provides its own mapping, remove the `extra_hosts` line if resolution misbehaves.

Host CUPS listens only on `localhost` by default. To reach it from the container:

1. In `cupsd.conf`, `Listen` on a non-loopback interface (or `Port 631`).
2. Allow the Docker bridge subnet in the `<Location>` access policy.
3. Share the target queue.
4. Open port 631 to the bridge in the host firewall.

Test reachability from inside the container. Any HTTP response, including `401`, `403` or `405`, means the port is reachable; connection refused or a timeout means it is not:

```bash
docker compose exec labeler sh -c 'apt-get update >/dev/null && apt-get install -y --no-install-recommends curl >/dev/null && curl -sS -o /dev/null -w "%{http_code}\n" http://host.docker.internal:631/'
```

Then add the printer under Settings → Printers and print a label. For `ipps://` with a self-signed or private-CA certificate, set the printer's `ca_cert` to the CA certificate; the image trusts the public CA bundle otherwise.

## Debugging

```bash
docker compose logs -f labeler                                   # service log
docker inspect --format '{{.State.Health.Status}}' "$(docker compose ps -q labeler)"
docker compose exec labeler sh                                   # shell, as root
```

Raise verbosity with `RUST_LOG=labeler=debug,tower_http=debug` in `.env` and `docker compose up -d`. The runtime image is `debian:trixie-slim` without `curl` or `wget`; install tools ad hoc with `apt-get update && apt-get install -y --no-install-recommends curl`, which lasts until the container is recreated.

Startup logs `templates loaded` with the count of loaded and broken templates, then one warning per broken file with its path and error. A fatal startup error (unwritable `/config`, unopenable database, invalid `PORT`) logs one error line and exits with status 1.
