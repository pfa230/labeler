# 29. Runtime base image: debian-slim, not distroless

Date: 2026-06-25

## Status

Proposed. Supersedes the runtime-base-image decision in
[ADR-0016](0016-deployment-and-packaging.md) (the rest of ADR-0016 stands). Implementation and the
clean-boot verification are tracked in issue #91. The base-image `@sha256` pins in
[ADR-0019](0019-ci-and-image-publishing.md) and [ADR-0027](0027-multi-arch-image-publishing.md) change
to the debian-slim digest when this is implemented; Dependabot continues to bump it.

## Context

ADR-0016 chose `gcr.io/distroless/cc-debian12:debug` for the runtime stage. The `:debug` variant ships a
BusyBox shell, and that shell is the *only* thing powering the `wget` HEALTHCHECK and the `labeler-init`
volume-chown container. So we kept distroless's costs (no package manager, shell-less
entrypoints, and only a stripped BusyBox shell for `docker exec` debugging) while forfeiting its one real
benefit, the no-shell minimal attack surface, because `:debug` puts a shell back. ADR-0016 itself flagged
this as a compromise and deferred an app-native healthcheck. (debian-slim is not the first shell-debuggable
runtime here; it upgrades the awkward BusyBox to a normal `/bin/sh` + `apt`.)

Distroless is a poor fit for what labeler is: a LAN-trust, self-hosted homelab label renderer (it ships a
`LABELER_NO_AUTH` homelab mode and integrates with Homebox). Its threat model does not justify the
minimal-CVE-surface trade, and the homelab ecosystem agrees in practice. None of the comparable
self-hosted apps use distroless: the linuxserver.io `*arr` stack and Jellyfin use an Alpine base with
s6-overlay and PUID/PGID; Vaultwarden, Plex, and Homebox use conventional shelled images. The recurring
reasons map exactly onto labeler:

- **Shell entrypoints for volume ownership.** The standard homelab answer to "a named volume is
  root-owned but the app runs non-root" is a shell entrypoint that adjusts ownership (PUID/PGID).
  Distroless cannot run one, which is precisely why labeler bolted on a separate root `labeler-init`
  container to `chown` its volumes.
- **Debuggability.** Self-hosters expect to `docker exec` a shell to inspect files and test connectivity.
- **Convenience over minimalism** for an app that needs filesystem permissions and runtime config.

Even Vaultwarden, far more security-sensitive than a sticker printer, ships debian-slim rather than
distroless. A label renderer has strictly less reason to.

## Decision

- **Runtime base is `debian:trixie-slim` (pinned `@sha256`), not distroless.** It keeps glibc (so no musl
  migration), provides `/bin/sh` + coreutils for `docker exec` troubleshooting and for shell entrypoints,
  and uses standard `apt` for any future extension. **Trixie (Debian 13), the current stable**, not
  bookworm (Debian 12 / oldstable) which the repo had been sitting on by inheritance.
- **The build stage moves to `rust:1-trixie` in the same change**, so the binary's glibc baseline matches
  the runtime exactly. (A bookworm-built binary would also run on a trixie runtime, since glibc is
  backward compatible, but bumping both keeps the stack on one current release and avoids drift.)
- **Not Alpine.** Alpine's musl can surprise native dependencies and would change the build target;
  debian-slim keeps glibc parity with the build stage and matches the Vaultwarden precedent.
- **Non-root stays:** `USER 65532:65532` (numeric; the image already `COPY --chown=65532:65532`s
  `/app/data` and `/app/templates`). debian-slim has no predefined `nonroot` user, so the numeric UID is
  used directly.
- **Healthcheck becomes app-native:** a `labeler healthcheck` subcommand (GET `127.0.0.1:8080/api/health`,
  exit 0/1; reuses the existing `reqwest`) replaces the BusyBox `wget`. This is base-image-agnostic, needs
  no extra package, and is the option ADR-0016 deferred. (debian-slim ships no `wget`/`curl`, so an
  app-native check is also the cleanest way to avoid an `apt` layer purely for the probe.)
- **Drop `labeler-init`.** For a **fresh, empty, Docker-managed named volume** (the default compose path),
  Docker seeds it from the image's content *and ownership* on first mount, and the image already
  `--chown=65532:65532`s `/app/data` and `/app/templates`, so the non-root app can write (SQLite at
  `/app/data`, template temp-then-rename at `/app/templates`). The separate root chown container is
  redundant (and was only possible because `:debug` provided a shell). Removing it also removes the
  compose `depends_on` on it. **Caveat:** this seeding does NOT apply to a pre-existing non-empty volume,
  an `external:`/restored volume, or a host bind mount; a root-owned existing mount would fail at runtime.
  Document that operators bind-mounting a host dir must make it writable by UID 65532 (this is the gap
  PUID/PGID would close, see below).
- **Install `ca-certificates`.** distroless/cc bundled it; `debian:trixie-slim` does not. The app's own
  `reqwest 0.12` uses `rustls-tls` with bundled `webpki-roots` (connector HTTPS needs no system store),
  but the `ipp` printing crate pulls `reqwest 0.13` -> `rustls-platform-verifier`, which uses the **system
  CA store** on Linux. Since the app supports `ipps://` printers (`src/driver.rs`) and DEPLOY promises
  public-CA `ipps://` works, the runtime stage must `apt-get install -y --no-install-recommends
  ca-certificates && rm -rf /var/lib/apt/lists/*`. (Not optional, as the first draft implied.)
- **PUID/PGID host-UID mapping is NOT adopted now.** The fixed-`65532` + seeded-volume model works and is
  simpler. A shell entrypoint is now available, so PUID/PGID can be added later if host-UID matching is
  requested; recorded as a possible future enhancement, not part of this decision.

## Consequences

- We give up distroless's minimal-CVE-surface posture, which was marginal for this threat model and
  already forfeited by `:debug`. In return: `docker exec` shell debugging for self-hosters, a base that is
  trivial to extend, a self-contained healthcheck, and removal of the `labeler-init` jank.
- The runtime base is slightly larger (debian-slim is on the order of ~30 MB vs distroless/cc ~24 MB),
  negligible against the typst-laden Rust binary + bundled fonts + `ui/dist` that dominate the image.
- The whole stack moves off oldstable (Debian 12) onto current stable (Debian 13/trixie): runtime
  `debian:trixie-slim` and build `rust:1-trixie`, both pinned by digest.
- The binary gains a tiny CLI surface (a `healthcheck` subcommand) where it had none; `main.rs` must parse
  that one argument before starting the server.
- The `@sha256` base pins in CI and multi-arch publishing move to the debian-slim digest.
- `docs/DEPLOY.md` must be updated in the same change: it still documents the `labeler-init` startup, the
  BusyBox healthcheck probe, exec'ing the `:debug` BusyBox shell, and distroless CA-bundle extension, all
  of which change here.
- Verification is a clean `docker compose up` on **fresh** volumes: the non-root app reads/writes
  `data` and `templates`, the healthcheck reports healthy, connector HTTPS works, and an `ipps://` print
  path still validates a public CA (proving `ca-certificates` is present). Tracked in #91.

## Alternatives considered

- **Keep `gcr.io/distroless/cc-debian12:debug` (status quo).** Incoherent (a shell is present anyway), the
  worst of both worlds. Rejected; it is the thing this ADR exists to fix.
- **Non-debug distroless + app-native healthcheck** (ADR-0016's deferred option). Gets the genuine
  no-shell benefit, but still cannot run a shell entrypoint (so PUID/PGID stays impossible) and still
  offers no `docker exec` debugging. The homelab analysis concludes the no-shell benefit is not worth
  these costs for this app. Rejected in favor of debian-slim.
- **Alpine (+ optional s6-overlay).** Smaller and shelled, the linuxserver model, but musl/native-lib
  surprises and a build-target change raise risk for no benefit debian-slim does not already provide.
  Rejected.
- **`scratch` / `distroless/static` with a musl-static binary.** Smallest possible, but maximal
  minimalism is the opposite of what a debuggable homelab image wants. Rejected.
