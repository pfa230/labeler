## Why

`main` has been red since 02b374f (2026-08-21). This change implements issue #186.

Rust 1.98.0 shipped on 2026-08-20 with a new clippy lint, `chunks_exact_to_as_chunks`. CI resolves
its toolchain with `dtolnay/rust-toolchain@stable` (`.github/workflows/ci.yml:24`) and runs
`cargo clippy --all-targets --all-features -- -D warnings` (`.github/workflows/ci.yml:43`), so the
new lint is a hard error against two call sites in `src/render/helpers.rs` that nobody touched. No
commit caused this; the runner's `stable` moved underneath the repo.

The recurring half of the problem is the floating resolution itself. With `@stable` in CI and
whatever rustup last installed locally, "clippy is clean here" proves nothing about clippy in CI, and
the break repeats on the next lint-adding release. This change fixes the two call sites and makes the
compiler version a committed, deliberate choice.

## What Changes

- Rewrite the two `chunks_exact` call sites in `src/render/helpers.rs` to `as_chunks` /
  `as_chunks_mut` (stable since Rust 1.88, so the code still builds on older toolchains). No
  `#[allow]`.
- Add a repo-root `rust-toolchain.toml` pinning the channel to `1.98.0` and declaring the `rustfmt`
  and `clippy` components, so one committed file is the source of truth for the toolchain in CI and
  in every local checkout.
- Replace the CI `dtolnay/rust-toolchain@stable` step with an action that reads that file, so the pin
  is honoured rather than raced against and CI never installs a compiler the file does not name. The
  replacement's job-wide `RUSTFLAGS` and caching defaults are switched off so the step sets a
  toolchain and nothing else; `design.md` decision 3 has the detail.
- Record the pin and its bump procedure in `AGENTS.md`, next to the gate commands.

**Deliberately out of scope: the release image.** `Dockerfile` copies only `Cargo.toml`,
`Cargo.lock` and `src/`, so a root `rust-toolchain.toml` never reaches the build stage. The image
keeps resolving its compiler from the digest-pinned `rust:1-trixie` base that dependabot bumps.
`design.md` decision 4 records why that divergence is accepted rather than closed.

**Deliberately out of scope: an ADR.** ADR-0057 narrowed the trigger to behavior changes, and this
change has none. `openspec/config.yaml` `rules.design` asks a behavior-neutral tooling change to say
so and state why no ADR is required; `design.md` - Context does. The decision's reasoning lives in a
header comment in `rust-toolchain.toml` and in the `AGENTS.md` note instead.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

None.

This change alters no externally observable behavior: the binarized PNG bytes, the API surface, the
template schema and the error contract are all unchanged. `as_chunks_mut` walks the same 4-byte
pixels in the same order and writes the same values. It is a lint fix plus build tooling, so
`.openspec.yaml` sets `skip_specs: true` rather than inventing a requirement to satisfy validation.

## Impact

- `src/render/helpers.rs` - two call sites (`binarize_rgba` at `:17`, its unit test at `:960`). Grep
  confirms these are the only `chunks_exact` uses in the tree.
- `rust-toolchain.toml` - new file at the repo root.
- `.github/workflows/ci.yml` - the `rust` job's toolchain step (`:22-26`) only. No other step in that
  job changes, and the `ui`, `openspec`, `build` and `gitleaks` jobs are untouched.
- `AGENTS.md` - the gate-commands section gains the pin and its bump procedure.
- Contributors and agents: the first `cargo` invocation after this lands downloads the pinned
  toolchain through rustup if it is absent. Anyone who set a per-directory `rustup override` keeps
  that override and will not see the pin; the `AGENTS.md` note names this escape hatch.
- Unaffected: `Dockerfile`, the published image, `Cargo.toml`, `Cargo.lock`, `docs/adr/`, `ui/`,
  `catalog/`.
