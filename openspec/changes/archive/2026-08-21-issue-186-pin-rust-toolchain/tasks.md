## 1. Reproduce on the toolchain being pinned

- [x] 1.1 `rustup toolchain install 1.98.0` (this machine is on 1.96.0, which predates the lint, so
      nothing below proves anything until 1.98.0 is present)
- [x] 1.2 Run `cargo +1.98.0 clippy --all-targets --all-features -- -D warnings` and confirm it fails
      with `chunks_exact_to_as_chunks` at `src/render/helpers.rs:17` and `:960`, and at no other site.
      Red before green: if this passes, the premise of issue #186 is wrong and the change stops here.
- [x] 1.3 Capture a baseline bi-level PNG for the byte-comparison in 5.2: start the server with
      `LABELER_CONFIG_DIR=./config-dev LABELER_NO_AUTH=true cargo run`, `POST /api/render/label?format=png`
      with `color_mode` set to bi-level, and save the bytes. Built from `main`'s code, so it captures
      the pre-change output.

## 2. Fix the lint

- [x] 2.1 `src/render/helpers.rs:17` - rewrite `binarize_rgba`'s loop to
      `for px in data.as_chunks_mut::<4>().0.iter_mut()`. No `#[allow]`.
- [x] 2.2 `src/render/helpers.rs:960` - rewrite the test's loop to
      `for (i, px) in data.as_chunks::<4>().0.iter().enumerate()`. Keep the existing assertions
      unchanged, including the `data[8..11]` / `data[12..15]` index checks that pin the 0.5 split.
- [x] 2.3 `cargo +1.98.0 clippy --all-targets --all-features -- -D warnings` now passes. Same command
      as 1.2, opposite result.

## 3. Pin the toolchain

- [x] 3.1 Create `rust-toolchain.toml` at the repo root per design decision 2: `channel = "1.98.0"`,
      `components = ["rustfmt", "clippy"]`, no `profile` key, and a one-line header comment pointing
      at the `AGENTS.md` note.
- [x] 3.2 `.github/workflows/ci.yml:22-26` - replace the `dtolnay/rust-toolchain@stable` step with
      `actions-rust-lang/setup-rust-toolchain@v1`, no `toolchain` input, `rustflags: ""` and
      `cache: false`. Drop the now-redundant `components:` block. Comment the step with why both
      defaults are off (design decision 3), since neither is obvious from the YAML.
- [x] 3.3 Confirm nothing else in `ci.yml` changed: the `actions/cache@v6` step, the three gate steps
      and the catalog-index step are untouched, and the `ui`, `openspec`, `build` and `gitleaks` jobs
      are byte-identical. `git diff .github/` is the check.
- [x] 3.4 Confirm `Dockerfile` is unmodified, so the new file cannot reach the image build
      (design decision 4).

## 4. Document the pin

- [x] 4.1 `AGENTS.md`, in the Commands section next to the gate paragraph (around `:131-137`): state
      that `rust-toolchain.toml` pins the toolchain and rustup installs it on first use; that a
      per-directory `rustup override` silently beats it and `rustup override unset` is the fix; that
      it is not an MSRV; and the bump procedure - edit `channel`, run the three gates, fix the lint
      fallout, commit the bump alone.
- [x] 4.2 Confirm the wording holds for both readers, since `CLAUDE.md` is a symlink to `AGENTS.md`.

## 5. Verify

- [x] 5.1 From a shell with no toolchain override, run `cargo fmt`, then
      `cargo clippy --all-targets --all-features`, then `cargo test` - with no `+toolchain` argument,
      so the run proves `rust-toolchain.toml` is what selects 1.98.0. `rustc --version` in that
      directory must report 1.98.0.
- [x] 5.2 Re-render the 1.3 label with the same request and diff the PNG bytes against the baseline.
      They must be byte-identical: `binarize_rgba` is on the bi-level render path, and this change
      claims to alter no output.
- [x] 5.3 Run the adversarial code-review loop over the diff (`AGENTS.md` - Reviewing before you call
      it done), separate from this change's plan review. One codex pass, read-only: no MAJOR issues.
      Its single MINOR is a pre-existing test gap outside this issue's scope, filed rather than
      absorbed (5.4). Everything it verified is listed in its report: call-site equivalence including
      the trailing remainder, the toolchain file's syntax against rustup's docs, the action's
      documented `toolchain` / `rustflags` / `cache` behavior, and that no other CI job depended on
      the old step.
- [x] 5.4 File the out-of-scope findings as issues rather than widening this change:
      [#192](https://github.com/pfa230/labeler/issues/192) (the `binarize_rgba` test asserts an alpha
      it cannot observe changing) and [#193](https://github.com/pfa230/labeler/issues/193) (the
      review-gate hook denies any Bash command whose text merely mentions `src/`, which blocked both
      writing `proposal.md` and invoking the reviewer during this change).

Archive, the single commit, the push, the CI-green check and the merge are workflow steps
(`AGENTS.md` - OpenSpec workflow, steps 5-7), not tasks: the archive validates this file, so it
cannot contain the steps that follow it. The archived `issue-181` change sets the same precedent.
Design Risks names the first CI run as the only real test of the action swap; that check happens on
the pushed branch before the merge.
