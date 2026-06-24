# 27. Multi-arch image publishing

Date: 2026-06-24

## Status

Accepted. Supersedes the "amd64 only" architecture decision in
[ADR-0019](0019-ci-and-image-publishing.md) (which otherwise stands).

## Context

ADR-0019 shipped a single linear `image` job that built amd64 only, loaded it, smoke-tested it, and
pushed the loaded tags. arm64 users (Raspberry Pi, Apple-silicon dev boxes) had to build locally
(issue #36). The repo is now public, and GitHub provides free, unlimited native `ubuntu-24.04-arm`
runners for public repositories, so arm64 can be built natively without QEMU emulation.

## Decision

Replace the single `image` job with a `build` matrix plus a publish-only `merge` job in `ci.yml`.

- **Native matrix.** `linux/amd64` builds on `ubuntu-latest`, `linux/arm64` on `ubuntu-24.04-arm`.
  `fail-fast: false` so one arch's failure still surfaces the other's result. Each leg carries
  `packages: write` because it pushes by digest itself.
- **Tested bytes == shipped bytes.** On publish events each leg pushes its single platform by digest
  (`outputs: type=image,push-by-digest=true,name-canonical=true,push=true`, no per-arch tag), then
  smoke-tests the pushed image by digest against `/api/health`. The arm image is validated natively on
  the arm runner. Pull requests build both arches with `load: true` and smoke-test locally, pushing
  nothing.
- **Digest merge.** Each leg uploads its digest as an artifact (`digest-<arch>`). The `merge` job
  downloads both, asserts exactly two are present, computes the tag set with `docker/metadata-action`
  (unchanged from ADR-0019), assembles one manifest list per tag with `docker buildx imagetools create`
  referencing `ghcr.io/pfa230/labeler@<digest>`, and verifies the published list contains both
  `linux/amd64` and `linux/arm64`. Pushing by digest (rather than throwaway `:sha-<arch>` tags) keeps
  GHCR free of variant tags and gives immutable references for the merge.
- **Caching.** Per-arch gha cache scope `labeler-<arch>` (the default `buildkit` scope would collide
  across legs); `cache-to` stays gated to non-PR events so read-only fork/Dependabot tokens do not fail
  the cache export.

## Consequences

`docker pull ghcr.io/pfa230/labeler:<tag>` resolves the right variant on amd64 and arm64 hosts; Pi users
run the published image instead of building. CI does two native builds per publish instead of one (free
on public runners). The published tag set, GHCR-only registry, `GITHUB_TOKEN` auth, pinned base digests,
and Dependabot are unchanged. Signing (cosign / SBOM / provenance) and a leaner non-`:debug` runtime
image remain out of scope.
