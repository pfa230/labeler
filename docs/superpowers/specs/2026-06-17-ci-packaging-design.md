# CI and Image Publishing — Design

**Status:** Approved design, ready for an implementation plan. Closes [#37](https://github.com/pfa230/labeler/issues/37); [#36](https://github.com/pfa230/labeler/issues/36) (arm64) stays deferred.

## Goal

Turn the locally-built image (M6, ADR-0016) into a published, reproducible artifact, and close the
gaps in CI: build/test the frontend and the Docker image on every change, and publish tagged images to
GHCR on `main` (edge) and on release tags (semver). One workflow, gated so a publish only happens when
the tests pass.

## Context (current state)

- `.github/workflows/ci.yml`: one `ubuntu-latest` job on `push` + `pull_request` running `cargo fmt
  --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, with
  `actions/cache` for cargo. It does **not** build the UI, run UI tests, build the image, or publish.
- `Dockerfile`: multi-stage. `node:22-bookworm-slim` builds `ui/`; `rust:1-bookworm` `cargo build
  --release --locked`; `gcr.io/distroless/cc-debian12:debug` runtime (`USER nonroot` 65532, `EXPOSE
  8080`, `HEALTHCHECK` via `/busybox/wget` GET `/api/health`). All `FROM` tags float (no digest pins).
- `/api/health` is auth-exempt (`src/middleware.rs`), so an unauthenticated smoke test can poll it.
- Repo is **private** (`github.com/pfa230/labeler`); default branch `main`. UI tests/lint/build are
  `npm run test|lint|build` in `ui/` (Vitest, eslint, `tsc -b && vite build`).

## Decisions (settled with the user)

1. **Publish model:** edge + semver. Push to `main` publishes a moving `:edge` plus `:sha-<short>`; a
   `vX.Y.Z` git tag publishes `:X.Y.Z`, `:X.Y`, and `:latest`. Users can track edge or pin a release.
2. **Registry:** GHCR only, `ghcr.io/pfa230/labeler`, authenticated with the built-in `GITHUB_TOKEN`
   (no configured secrets). Images are private (inherit the private repo); pulling needs auth.
3. **CI scope:** close the gaps. Add a UI job (lint + test + build) and an amd64 Docker build + smoke
   test to CI, alongside the publish pipeline.
4. **Reproducibility:** pin all three base images to manifest-list `@sha256:` digests (keep the tag);
   add Dependabot (`docker` + `github-actions`) to auto-PR digest and Action bumps.
5. **Architecture:** amd64 only. arm64 / multi-arch is deferred (#36) under the existing documented
   local `docker buildx --platform linux/arm64` path; not worth QEMU's slow emulated Rust builds or
   billed arm64 runners on a private repo right now.

## Architecture

One workflow, `.github/workflows/ci.yml`, with three jobs. Triggers: `pull_request`; `push` to
`main`; `push` tags matching `v*.*.*` (semver only — a narrow glob so a non-semver tag like `vtest`
cannot fire a release).

```
on:
  pull_request:
  push:
    branches: [main]
    tags: ['v*.*.*']
```

**Concurrency** (group includes the workflow name to avoid cross-workflow cancellation; publishes are
never cancelled mid-push):

```
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: ${{ github.event_name == 'pull_request' }}
```

### Job `rust` (unchanged)
fmt + clippy + test, exactly as today.

### Job `ui` (new)
`ubuntu-latest`. `actions/setup-node@<pinned>` with `node-version: 22`, `cache: npm`,
`cache-dependency-path: ui/package-lock.json`. Steps: `npm ci`, `npm run lint`, `npm run test`,
`npm run build`, all run in `ui/` (`working-directory: ui`).

### Job `image` (new) — `needs: [rust, ui]`
Builds the amd64 image once, smoke-tests the exact built artifact, then pushes that artifact (no
rebuild). The byte-identical guarantee is the reason push is a plain `docker push` of the loaded image,
**not** `push: true` on the build step (which would publish during build, before the smoke test).

Steps:
1. `actions/checkout`.
2. `docker/setup-buildx-action`.
3. `docker/metadata-action` (id `meta`) computing tags + labels (see below).
4. `docker/login-action` to `ghcr.io` with `${{ github.actor }}` / `${{ secrets.GITHUB_TOKEN }}` —
   **only** on non-PR events (`if: github.event_name != 'pull_request'`). PRs build but never push, so
   they need no login (and fork/Dependabot PRs get a read-only token anyway).
5. `docker/build-push-action`:
   - `context: .`, `platforms: linux/amd64`, `push: false`, `load: true`.
   - `tags:` = `${{ steps.meta.outputs.tags }}` **plus** a stable local `labeler:test`.
   - `labels: ${{ steps.meta.outputs.labels }}` (so `org.opencontainers.image.source` is on the image,
     which is what makes GHCR auto-link the package to this repo on first publish).
   - `cache-from: type=gha`.
   - `cache-to:` set to `type=gha,mode=max` **only** on non-PR events, empty otherwise. The GHA cache
     *exporter* needs a writable token; fork PRs and Dependabot PRs get a read-only `GITHUB_TOKEN`, so
     an unconditional `cache-to` would fail exactly those PRs. `cache-from` is always on.
6. **Smoke test** (all events, against the loaded `labeler:test`):
   `docker run -d -p 8080:8080 --name smoke labeler:test`, then poll `http://127.0.0.1:8080/api/health`
   from the host until it returns 200 (bounded retry loop, e.g. 30 tries × 2s). On timeout, dump
   `docker logs smoke` and `docker ps -a` and exit non-zero. Always `docker rm -f smoke` at the end.
7. **Publish** (`if: github.event_name != 'pull_request'`): `docker push` each tag in
   `${{ steps.meta.outputs.tags }}` (the images are already loaded locally from step 5). A small shell
   loop over the multiline tags output, or `docker push --all-tags ghcr.io/pfa230/labeler`.

**Permissions** (workflow or `image` job): `contents: read`, `packages: write`.

### `metadata-action` tags + labels

```
images: ghcr.io/pfa230/labeler
flavor: latest=false          # we control :latest explicitly
tags: |
  type=edge,branch=main
  type=sha,prefix=sha-
  type=semver,pattern={{version}}
  type=semver,pattern={{major}}.{{minor}}
  type=raw,value=latest,enable=${{ startsWith(github.ref, 'refs/tags/') }}
```

- Push to `main` → `:edge` + `:sha-<short>`. No `:latest` (flavor disables auto-latest; the raw rule is
  tag-only).
- Tag `vX.Y.Z` → `:X.Y.Z`, `:X.Y`, `:latest`. (Bare `{{major}}` is intentionally **not** emitted while
  the project is `0.x` — metadata-action's major-zero caveat — and we publish `{{major}}.{{minor}}`
  instead.)
- The `v*.*.*` trigger guarantees only semver tags reach this workflow, so the `refs/tags/` raw-latest
  condition is safe.

### Dockerfile changes (reproducibility only)

Pin each `FROM` to its manifest-list (index) digest while keeping the tag, so BuildKit still resolves
the amd64 child and a future arm64 build would resolve its own child:

```
FROM node:22-bookworm-slim@sha256:<index-digest> AS ui
FROM rust:1-bookworm@sha256:<index-digest> AS build
FROM gcr.io/distroless/cc-debian12:debug@sha256:<index-digest> AS runtime
```

The implementation task resolves the current digests with `docker buildx imagetools inspect <ref>`
(the top-level `Digest:` of the index) and records them. No other Dockerfile change is in scope (no
build-layer reordering / dependency-cache "skeleton" — that is a separate optimization, out of scope
here).

### Dependabot (`.github/dependabot.yml`)

Two ecosystems:
- `docker` (directory `/`) — bumps the pinned `@sha256` base digests.
- `github-actions` (directory `/`) — bumps the pinned Action versions.

Pin Actions by major tag at minimum (`actions/checkout@v4`, `docker/build-push-action@v6`, etc.);
Dependabot annotates bumps with the version. (SHA-pinning the Actions is a stronger supply-chain
posture and is compatible with Dependabot, but tag-pinning is acceptable for a private single-maintainer
repo.) Note: GitHub's docs do not firmly promise same-tag digest refresh, so treat the docker-ecosystem
digest bumps as best-effort and verify a Dependabot PR actually appears.

## Docs

- **ADR-0019 "CI and image publishing"** (append-only; does not supersede ADR-0016): records the
  registry choice (GHCR + `GITHUB_TOKEN`), the edge/semver publish model, amd64-only-for-now (#36
  deferred), digest pinning + Dependabot, and the build→smoke→`docker push` ordering for a
  byte-identical tested artifact. Add the row to `docs/adr/README.md`.
- **DEPLOY.md**: add a "Run a pre-built image" section. Document the tag scheme (`:edge`, `:X.Y.Z`,
  `:latest`) and that the image is **private**: pulling needs `docker login ghcr.io` with a token that
  has **package read access** (for a personal-account private package, access is granted per user, not
  via `read:packages` scope alone), and note the package can be made public for anonymous pulls. Show
  the `docker-compose.yml` change to pull `image: ghcr.io/pfa230/labeler:latest` instead of building.
- **SPEC.md**: a changelog line only (no API change).

## Verification / testing

Workflow YAML is not unit-testable, so the plan verifies by:
1. `actionlint` on the workflow (and a YAML lint on `dependabot.yml`).
2. A local `docker build` of the digest-pinned Dockerfile to confirm it still builds, plus a local run
   of the smoke-test command (`docker run` + poll `/api/health`) to confirm the container goes healthy.
3. The real Actions run on the feature branch: the PR triggers `rust` + `ui` + the `image`
   build+smoke (no push); confirm green before merge. After merge to `main`, confirm `:edge` +
   `:sha-` appear in GHCR and the package is linked to the repo. A throwaway `v0.0.0-test`-style check
   for the release path is optional (and would publish a real tag, so prefer reviewing the tag logic
   over cutting a junk release).

## Scope

**In:** the three-job workflow (UI gate + amd64 build/smoke/publish), GHCR edge/semver publishing,
base-digest pinning, Dependabot, ADR-0019, DEPLOY/SPEC docs.

**Explicitly out:** arm64 / multi-arch (#36); cosign/SBOM/provenance signing; a leaner non-`:debug`
runtime image; Docker Hub mirroring; Dockerfile build-cache restructuring (skeleton dependency layer).
