# CI and Image Publishing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend CI to build/test the frontend and the Docker image on every change, and publish reproducible amd64 images to GHCR on `main` (edge) and release tags (semver).

**Architecture:** One workflow (`.github/workflows/ci.yml`) with three jobs: `rust` (unchanged), `ui` (npm lint/test/build), and `image` (`needs: [rust, ui]`) that builds amd64 with `load: true`, smoke-tests `/api/health` against the loaded image, then `docker push`es that same image to GHCR on non-PR events. Base images are pinned to manifest-list digests; Dependabot keeps them and the Actions current.

**Tech Stack:** GitHub Actions, `docker/{setup-buildx,metadata,login,build-push}-action`, `actions/setup-node`, BuildKit GHA cache, Dependabot. No application code changes.

**Spec:** `docs/superpowers/specs/2026-06-17-ci-packaging-design.md`.

## Global Constraints
- Repo is **private**; default branch `main`; image is `ghcr.io/pfa230/labeler` (lowercase, hardcoded). Auth via the built-in `GITHUB_TOKEN` (no configured secrets).
- amd64 only. arm64/multi-arch is out of scope (#36 stays open).
- No em dashes in docs. Files end with a newline. Do not change application code, the Docker build layout (beyond the `FROM` digest pins), or `docker-compose.yml` (the published-image path is documented, not the default).
- Verification reality: workflow/Dependabot YAML has no unit test. The gate is `actionlint` (+ a local `docker build`/smoke run when a Docker daemon is available) and the **real Actions run on the PR** (Task 5). If `actionlint` is missing, install it: `brew install actionlint` (macOS) or `go install github.com/rhysd/actionlint/cmd/actionlint@latest`.
- This is config/docs, so tasks are "write file → lint → commit," not failing-test-first.
- Work on a short-lived branch `ci-packaging`; Task 5 merges to `main`. **Never** force-push; nothing auto-pushes mid-plan.

## File structure
- `Dockerfile` (modify) — pin the three `FROM` bases to digests. Task 1.
- `.github/dependabot.yml` (create) — docker + github-actions update PRs. Task 1.
- `.github/workflows/ci.yml` (rewrite) — triggers/concurrency/permissions + `ui` job (Task 2), then the `image` job (Task 3).
- `docs/adr/0019-ci-and-image-publishing.md` (create), `docs/adr/README.md` (modify), `docs/DEPLOY.md` (modify), `docs/SPEC.md` (modify, changelog). Task 4.

---

### Task 1: Pin base-image digests + add Dependabot

**Files:** Modify `Dockerfile` (the three `FROM` lines only); Create `.github/dependabot.yml`.

- [ ] **Step 1: Pin the `FROM` digests.** In `Dockerfile`, change only the three `FROM` lines to append the manifest-list (index) digest, keeping the tag. Use these digests (resolved 2026-06-17 via `docker buildx imagetools inspect <ref>`; they are immutable, so they stay valid; re-resolve with that command only if you deliberately want newer bases):
```dockerfile
FROM node:22-bookworm-slim@sha256:e21fc383b50d5347dc7a9f1cae45b8f4e2f0d39f7ade28e4eef7d2934522b752 AS ui
```
```dockerfile
FROM rust:1-bookworm@sha256:19817ead3289c8c631c73df281e18b59b172f6a31f4f563290f69cddd06c30e9 AS build
```
```dockerfile
FROM gcr.io/distroless/cc-debian12:debug@sha256:83cd6a79595b063961fc4c21814a7961e3f2741e94bfea8bde420898719e80c5 AS runtime
```
Leave the rest of the Dockerfile exactly as-is. Pinning the **index** digest is deliberate: BuildKit resolves the amd64 child from it, and a future arm64 build would resolve its own child, so this does not block #36.

- [ ] **Step 2: Create `.github/dependabot.yml`:**
```yaml
version: 2
updates:
  - package-ecosystem: docker
    directory: "/"
    schedule:
      interval: weekly
  - package-ecosystem: github-actions
    directory: "/"
    schedule:
      interval: weekly
```

- [ ] **Step 3: Verify.** Confirm the YAML parses: `python3 -c "import yaml,sys; yaml.safe_load(open('.github/dependabot.yml'))"` (expected: no output / exit 0). If a local Docker daemon is running, confirm the pinned Dockerfile still builds: `docker build -t labeler:pintest .` (expected: build succeeds; this can take several minutes). If no daemon is available, skip the build here — Task 5's CI run is the real check.

- [ ] **Step 4: Commit:**
```bash
git checkout -b ci-packaging
git add Dockerfile .github/dependabot.yml
git commit -m "build: pin base-image digests + add Dependabot (docker, github-actions)"
```

---

### Task 2: Workflow scaffold + UI job

**Files:** Rewrite `.github/workflows/ci.yml` (header + `rust` kept + new `ui` job; the `image` job comes in Task 3).

**Interfaces:**
- Produces: the `rust` and `ui` jobs; triggers (`pull_request`, push `main` + tags `v*.*.*`); the concurrency block; top-level `permissions: contents: read`. Task 3 adds the `image` job that `needs: [rust, ui]`.

- [ ] **Step 1: Write the workflow** (replace the whole file with this; the `image` job is appended in Task 3):
```yaml
name: CI

on:
  pull_request:
  push:
    branches: [main]
    tags: ['v*.*.*']

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: ${{ github.event_name == 'pull_request' }}

permissions:
  contents: read

jobs:
  rust:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Set up Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-

      - name: Format
        run: cargo fmt --check

      - name: Clippy
        run: cargo clippy --all-targets --all-features -- -D warnings

      - name: Test
        run: cargo test

  ui:
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: ui
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Set up Node
        uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: npm
          cache-dependency-path: ui/package-lock.json

      - name: Install
        run: npm ci

      - name: Lint
        run: npm run lint

      - name: Test
        run: npm run test

      - name: Build
        run: npm run build
```

- [ ] **Step 2: Lint.** Run `actionlint .github/workflows/ci.yml` (expected: no output / exit 0). Fix any reported issue at the source.

- [ ] **Step 3: Commit:**
```bash
git add .github/workflows/ci.yml
git commit -m "ci: add UI lint/test/build job; semver-tag + main triggers, scoped concurrency"
```

---

### Task 3: Image build + smoke test + GHCR publish

**Files:** Modify `.github/workflows/ci.yml` (append the `image` job).

**Interfaces:**
- Consumes: the `rust` and `ui` jobs from Task 2 (`needs: [rust, ui]`).

- [ ] **Step 1: Append the `image` job** to the end of `jobs:` in `.github/workflows/ci.yml`:
```yaml
  image:
    needs: [rust, ui]
    runs-on: ubuntu-latest
    permissions:
      contents: read
      packages: write
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Set up Buildx
        uses: docker/setup-buildx-action@v3

      - name: Image metadata
        id: meta
        uses: docker/metadata-action@v5
        with:
          images: ghcr.io/pfa230/labeler
          flavor: latest=false
          tags: |
            type=edge,branch=main
            type=sha,prefix=sha-
            type=semver,pattern={{version}}
            type=semver,pattern={{major}}.{{minor}}
            type=raw,value=latest,enable=${{ startsWith(github.ref, 'refs/tags/') }}

      - name: Log in to GHCR
        if: github.event_name != 'pull_request'
        uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Build (amd64, load locally)
        uses: docker/build-push-action@v6
        with:
          context: .
          platforms: linux/amd64
          push: false
          load: true
          tags: |
            ${{ steps.meta.outputs.tags }}
            labeler:test
          labels: ${{ steps.meta.outputs.labels }}
          cache-from: type=gha
          cache-to: ${{ github.event_name != 'pull_request' && 'type=gha,mode=max' || '' }}

      - name: Smoke test (/api/health)
        run: |
          docker run -d -p 8080:8080 --name smoke labeler:test
          ok=
          for i in $(seq 1 30); do
            if curl -fsS http://127.0.0.1:8080/api/health >/dev/null 2>&1; then ok=1; break; fi
            sleep 2
          done
          if [ -z "$ok" ]; then
            echo "smoke test failed: /api/health never became healthy"
            docker logs smoke || true
            docker ps -a || true
            docker rm -f smoke || true
            exit 1
          fi
          docker rm -f smoke

      - name: Push to GHCR
        if: github.event_name != 'pull_request'
        run: |
          echo "${{ steps.meta.outputs.tags }}" | while read -r tag; do
            [ -n "$tag" ] && docker push "$tag"
          done
```
Why this shape (do not "optimize" it away):
- `push: false` + `load: true` builds **once** into the local daemon, tagged with both the GHCR tags and `labeler:test`. The smoke test runs against `labeler:test`, and the **same loaded images** are what `docker push` ships, so the tested bytes are the shipped bytes. Using `push: true` would publish during build, before the smoke test.
- `cache-to` is gated to non-PR events because fork PRs and **Dependabot PRs** get a read-only `GITHUB_TOKEN`, and the GHA cache exporter needs write; an unconditional `cache-to` would fail those PRs. `cache-from` stays on for everyone.
- `labels: ${{ steps.meta.outputs.labels }}` puts `org.opencontainers.image.source` on the image, which is what makes GHCR auto-link the package to this repo on first publish (avoids a token-denied first push).
- `flavor: latest=false` + the `type=raw` latest gated on `refs/tags/` means `main` pushes never produce `:latest`; only a `v*.*.*` tag does.

- [ ] **Step 2: Lint.** `actionlint .github/workflows/ci.yml` (expected: clean). Note: `actionlint` runs `shellcheck` on `run:` blocks if it is installed; address any SC warning on the smoke/push scripts (the loops above are written to pass: quoted vars, no unused `i`). If shellcheck flags `i` as unused in the `for i in $(seq ...)` loop (SC2034), that is acceptable for a counter loop; if it errors, replace with `for _ in $(seq 1 30)`.

- [ ] **Step 3: Commit:**
```bash
git add .github/workflows/ci.yml
git commit -m "ci: build amd64 image, smoke-test /api/health, publish edge+semver to GHCR"
```

---

### Task 4: Docs (ADR-0019, DEPLOY, SPEC)

**Files:** Create `docs/adr/0019-ci-and-image-publishing.md`; Modify `docs/adr/README.md`, `docs/DEPLOY.md`, `docs/SPEC.md`.

- [ ] **Step 1: Create `docs/adr/0019-ci-and-image-publishing.md`** (match the Nygard format of `docs/adr/0018-api-integration-spine.md`):
```markdown
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
  latest=false` and a `type=raw` `latest` gated on `refs/tags/`, so `main` never moves `:latest`. The
  workflow's tag trigger is the narrow `v*.*.*` so only semver tags release.
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
```

- [ ] **Step 2: Add the index row to `docs/adr/README.md`.** Add a row for ADR-0019 to the index table, matching the existing rows' columns (number, title linked to the file, status `Accepted`). Place it after the 0018 row.

- [ ] **Step 3: Add a "Run a published image" section to `docs/DEPLOY.md`** (after the Quick start section). Use this content:
```markdown
## Run a published image (GHCR)

CI publishes images to `ghcr.io/pfa230/labeler`:

- `:edge` and `:sha-<short>` on every push to `main`.
- `:X.Y.Z`, `:X.Y`, and `:latest` on a `vX.Y.Z` release tag.

The repo is private, so the package is **private**: you must authenticate to pull. The pulling
user/token needs **read access to the package** (for a personal-account private package, access is
granted per user in the package settings; a classic PAT additionally needs the `read:packages` scope).

```bash
echo "$GITHUB_TOKEN" | docker login ghcr.io -u <your-github-username> --password-stdin
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
```
(Note: the triple-backtick `bash`/`yaml` fences above are literal content to paste into DEPLOY.md.)

- [ ] **Step 4: Add a `docs/SPEC.md` changelog entry** at the top of the Changelog list (matching the existing dated-entry format):
```markdown
- **2026-06-17**: CI and image publishing (ADR-0019, #37). CI now also builds/tests the UI and builds +
  smoke-tests the Docker image; images publish to `ghcr.io/pfa230/labeler` (`:edge` + `:sha-` on `main`,
  `:X.Y.Z`/`:X.Y`/`:latest` on a `vX.Y.Z` tag) via the built-in `GITHUB_TOKEN`. Base images are pinned to
  digests with Dependabot bumps. amd64 only (arm64 deferred, #36). No API change.
```

- [ ] **Step 5: Commit:**
```bash
git add docs/adr/0019-ci-and-image-publishing.md docs/adr/README.md docs/DEPLOY.md docs/SPEC.md
git commit -m "docs: ADR-0019 CI/image-publishing + DEPLOY pull section + SPEC changelog"
```

---

### Task 5: Integrate (real CI run, then merge)

**Files:** none (verification + merge).

- [ ] **Step 1: Push the branch and open the real run.** `git push -u origin ci-packaging`. This is a `push` to a non-`main` branch, so it triggers `pull_request`-equivalent gating only if a PR is opened; open a PR (`gh pr create --fill --base main`) so the `pull_request` event runs `rust` + `ui` + the `image` build+smoke (no push, no login). 

- [ ] **Step 2: Confirm the PR run is green.** `gh pr checks` (or watch the Actions tab). Expected: `rust`, `ui`, and `image` all pass; the `image` job builds amd64, runs the smoke test (container reaches `/api/health`), and does **not** push (PR event). If the smoke test fails, read the dumped `docker logs smoke` in the job output and fix the Dockerfile/health path; do not merge on a red run.

- [ ] **Step 3: Merge to main and confirm the publish.**
```bash
git checkout main && git merge --no-ff ci-packaging -m "Merge ci-packaging: CI UI gate + GHCR image publishing (Fixes #37)" && git push
```
The push to `main` triggers the workflow again; this time the `image` job logs in and pushes `:edge` + `:sha-<short>`. Confirm: the package appears at `ghcr.io/pfa230/labeler`, is linked to the repo, and `gh run list` shows the main run green. `Fixes #37` in the merge commit closes the issue on push.

- [ ] **Step 4 (optional): verify the release path without cutting a junk release.** Do NOT push a throwaway tag to a real release line. Either review the tag logic by reading the metadata-action config, or, if you want a live check, push `v0.0.1` only when you genuinely intend it as the first release (it will publish `:0.0.1`, `:0.0`, `:latest`).

---

## Self-Review

**1. Spec coverage:** UI gate + amd64 build/smoke/publish workflow -> Tasks 2, 3. edge/semver tags via metadata-action (`latest=false` + raw-latest-on-tag) -> Task 3. build->smoke(`load:true`)->`docker push` byte-identical ordering -> Task 3. conditional `cache-to` (fork/Dependabot PR safety) -> Task 3. conditional `cancel-in-progress` + `github.workflow` in the group -> Task 2. base-digest pins (manifest-list) -> Task 1. Dependabot (docker + github-actions) -> Task 1. OCI source label for GHCR linking -> Task 3. GHCR private-pull auth wording (per-user package read access) -> Task 4. ADR-0019 + DEPLOY + SPEC changelog -> Task 4. `v*.*.*` trigger -> Task 2. amd64-only / #36 deferred -> noted throughout. Closes #37 -> Task 5. Verification (actionlint + local build/smoke + real run) -> per-task + Task 5.

**2. Placeholder scan:** no TBD/TODO. The base digests are real values (resolved 2026-06-17), not placeholders. `<your-github-username>` and `$GITHUB_TOKEN` in the DEPLOY snippet are deliberate user-supplied values in operator-facing copy. The ADR README row is described by "match the existing columns" because the table format is obvious from the file; no invented schema.

**3. Consistency:** the `image` job `needs: [rust, ui]` matches the job names defined in Task 2. `labeler:test` is the local tag built in Task 3 and the same tag the smoke test runs. `ghcr.io/pfa230/labeler` is identical across the workflow, ADR, DEPLOY, and SPEC. The tag trigger `v*.*.*` (Task 2) and the `refs/tags/` raw-latest gate (Task 3) are consistent (only semver tags fire the workflow). `cache-to` and login and push are all gated on the same `github.event_name != 'pull_request'` condition.

**Known caveat to watch during execution:** Dependabot's docker-ecosystem digest refresh is best-effort per GitHub's docs; after merge, confirm a Dependabot PR actually appears for the pinned bases. If a local Docker daemon is unavailable for Task 1/Task 3 local checks, the PR CI run (Task 5) is the authoritative verification.
