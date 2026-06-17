# 19. CI and image publishing

Date: 2026-06-17

## Status

Accepted. Builds on (does not supersede) [ADR-0016](0016-deployment-and-packaging.md).

## Context

M6 produced a single-image multi-stage build that was only ever built locally (ADR-0016). There was no
registry, no published artifact, and CI only ran the Rust gates: the frontend and the Docker image were
never built or tested in CI. Issues #37 (registry/publish pipeline, pin base digests) and #36
(multi-arch) tracked the gap. The repo is private.

## Decision

One GitHub Actions workflow (`ci.yml`) with three jobs: `rust` (fmt/clippy/test), `ui`
(npm lint/test/build), and `image` (`needs: [rust, ui]`).

- **Registry:** GHCR only, `ghcr.io/pfa230/labeler`, authenticated with the built-in `GITHUB_TOKEN`
  (`packages: write`); no configured secrets. Images inherit the private repo (private by default).
- **Publish model:** push to `main` publishes `:edge` + `:sha-<short>`; a `vX.Y.Z` tag publishes
  `:X.Y.Z`, `:X.Y`, and `:latest`. Tags are computed by `docker/metadata-action` with `flavor:
  latest=false`, a `type=raw` `latest` gated on `refs/tags/`, and `type=sha` gated on `refs/heads/` so a
  release tag does not also get a `:sha-` tag. The workflow's tag trigger is the narrow `v*.*.*` so only
  semver tags release.
- **Build/test/publish ordering:** build amd64 once with `load: true` (tagged with the GHCR tags and a
  local `labeler:test`), smoke-test the loaded image against `/api/health`, then `docker push` the
  loaded GHCR tags. The tested bytes are the shipped bytes; no rebuild between test and publish.
- **Reproducibility:** the three Dockerfile bases are pinned to manifest-list `@sha256` digests;
  Dependabot (`docker` + `github-actions`) opens bump PRs. The GHA build cache (`cache-to`) is enabled
  only on non-PR events so read-only fork/Dependabot PR tokens do not fail the cache export.
- **Architecture:** amd64 only. Multi-arch (#36) stays deferred; the manifest-list digest pins keep the
  door open and `docker buildx --platform linux/arm64` remains the documented local path.

## Consequences

Every push runs the full gate (Rust + UI + an image build + a container smoke test) before anything
publishes; a broken frontend or Dockerfile is caught pre-merge. Operators can run a published image
instead of building. Cost: the image build/smoke adds CI minutes (billed on a private repo), and arm64
users still build locally until #36. Signing (cosign/SBOM/provenance) and a leaner non-`:debug` runtime
image are not addressed here.
