# 19. CI/packaging design

**Status:** Accepted

## Context

To support continuous integration and production deployment, we need an automated CI and packaging pipeline for the private repository `pfa230/labeler`. The pipeline must verify Rust and UI (React/Vite) code health, build a single Docker image containing both components, smoke-test the built image locally to guarantee runtime correctness, and publish it to GitHub Container Registry (GHCR) under structured tagging conventions. Since the repository is private, security (such as access control and fork PR behavior) and reproducibility are critical.

## Decision

- **Single-workflow orchestrator.** We consolidate all CI/CD operations into a single GitHub Actions workflow (`.github/workflows/ci.yml`) triggered on `pull_request`, `push` to `main`, and pushes to tag `v*`.
- **Sequential job dependency.** The workflow defines three jobs: `rust` (lint/test/format), `ui` (npm ci/lint/test/build), and `image` (Docker build and publish). The `image` job depends on both `rust` and `ui` (`needs: [rust, ui]`) to prevent wasting builder resources on broken code.
- **Build-and-load smoke test.** To guarantee the published image is correct and functional, the `image` job builds the image using the `local` docker driver (`load: true`) and runs it locally as a container named `smoke-test` (mapping host port 8080). It polls `/api/health` for a successful response.
- **Diagnostic logging.** If the smoke test fails or times out (capped at 60s), the workflow dumps `docker logs smoke-test` and `docker ps -a` before exiting.
- **Registry push verification.** Only after the smoke test succeeds, and only for non-PR events (commits to `main` and release tags `v*`), the image is pushed directly from the runner's Docker daemon. This ensures that the exact same bits that passed the smoke test are what gets published.
- **Digest-pinned multi-arch base images.** We pin all base images (`node:22-bookworm-slim`, `rust:1-bookworm`, `gcr.io/distroless/cc-debian12:debug`) to their manifest-list digests using the `@sha256:` suffix while retaining their tags. This ensures cryptographic reproducibility, prevents upstream tag mutation, and allows future multi-arch support (issue #36) since it pins the platform-agnostic manifest list rather than a platform-specific digest.
- **Dependabot versioning.** We add `.github/dependabot.yml` tracking both `docker` (Dockerfile base images) and `github-actions` ecosystems. Dependabot will automatically manage updates for pinned tags and digests.
- **Precise tag policies.** We configure `docker/metadata-action` with global `flavor: latest=false` to prevent branch pushes to `main` from overwriting the stable `latest` release tag. Tags are generated as follows:
  - Commit pushes to `main` tag as `:edge`.
  - All pushes (including branches/PRs/tags) generate a unique `:sha-<short-sha>` tag.
  - Stable release tags `v*` generate `:X.Y.Z`, `:X.Y`, and `:latest` (using a raw conditional tag).
- **Release-safe concurrency.** To prevent a subsequent push from aborting an active release build or leaving a partially-pushed image in GHCR, we configure the concurrency group to cancel in-progress runs on `pull_request` events only, allowing branch and tag publishes to run to completion.

## Consequences

- **Secure by default.** Pull request runs from branches or forks build and smoke-test the image but skip the `login` and `push` steps, preventing unauthorized modifications to GHCR.
- **Reproducible builds.** Base image digests are pinned, avoiding drift. Automated weekly Dependabot PRs keep dependencies and base image patches up to date.
- **Faster iterations.** Caching is enabled for Cargo (`target` and registry folders) and Node (`npm` cache) on the host runner, and Docker layers are cached via the `gha` cache exporter.
- **Private registry authentication.** Because the repository is private, GHCR packages are private by default. Production servers running `docker compose` must run `docker login ghcr.io` using a Personal Access Token (PAT) with `read:packages` scope to pull the image. This has been documented in `docs/DEPLOY.md`.
