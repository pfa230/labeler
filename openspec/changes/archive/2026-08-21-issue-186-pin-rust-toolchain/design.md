## Context

See `proposal.md` - Why. The relevant current state:

- `.github/workflows/ci.yml:24` sets the toolchain with `dtolnay/rust-toolchain@stable`, which
  resolves at job start to whatever `stable` is that day (now 1.98.0, released 2026-08-20).
- `.github/workflows/ci.yml:43` runs `cargo clippy --all-targets --all-features -- -D warnings`. That
  step is the **only** place the repo denies warnings; `cargo test` (`:45-46`) and
  `cargo run --bin catalog-index` (`:52-54`) deliberately run without it.
- `Cargo.toml` declares `edition = "2021"` and **no** `rust-version`, so the repo has never stated an
  MSRV and nothing constrains which compiler is acceptable.
- `Dockerfile:8` builds on `rust:1-trixie` pinned by digest, and `Dockerfile:10-11` copies only
  `Cargo.toml`, `Cargo.lock` and `src/` into the build stage.
- The `build` job declares `needs: [rust, ui]` (`.github/workflows/ci.yml:114-115`), so the image is
  compiled in CI on every push once the `rust` job passes.
- Grep across `src/` and `tests/` finds exactly two `chunks_exact` uses, both in
  `src/render/helpers.rs`.

**No ADR.** This change is behavior-neutral: it alters no externally observable behavior of the
service. ADR-0057 narrowed the ADR trigger from ADR-0001's "every major decision" to every *behavior
change* (`docs/adr/0057-openspec-adoption.md:3-4`), and `openspec/config.yaml` `rules.design` asks a
behavior-neutral refactor, docs or tooling change to say so and state why none is required. This is
tooling. The durable "why is this pinned, and how do I move it" reasoning goes where a reader will
actually hit it: a header comment in `rust-toolchain.toml` itself, and the gate section of
`AGENTS.md`. `docs/adr/` and `docs/adr/README.md` are untouched.

## Goals / Non-Goals

**Goals:**

- One committed file decides the compiler version, for CI and for every checkout.
- A local `cargo clippy` pass and a CI `cargo clippy` pass mean the same thing.
- A toolchain bump is a reviewable commit with the lint fallout attached, not a silent Tuesday.

**Non-Goals:**

- Declaring an MSRV. `rust-version` in `Cargo.toml` answers "which compilers can consume this crate",
  a different question from "which compiler do we develop and gate against". Adding one is a separate
  decision; this change does not sneak it in.
- Pinning the release image's compiler. Covered under Decisions.
- Changing which steps deny warnings, which gates run, or any other CI job. The `rust` job's
  toolchain step is the only step touched, and decision 3 covers what it takes to keep that true.
- Automating the bump. Nothing in this change reminds anyone to move the pin forward; see Risks.

## Decisions

### 1. Rewrite the call sites, do not silence the lint

`AGENTS.md` forbids `#[allow(clippy::...)]`, and the lint is right on the merits: `as_chunks::<4>()`
yields `&[[u8; 4]]`, an indexable slice of fixed-size arrays, where `chunks_exact(4)` yields an
iterator of length-checked slices.

- `src/render/helpers.rs:17` `binarize_rgba`: `data.chunks_exact_mut(4)` becomes
  `data.as_chunks_mut::<4>().0.iter_mut()`.
- `src/render/helpers.rs:960` (unit test): `data.chunks_exact(4).enumerate()` becomes
  `data.as_chunks::<4>().0.iter().enumerate()`.

Semantics are identical, including the tail: `as_chunks` returns `(&[[T; N]], &[T])` and taking `.0`
discards the same trailing remainder that `chunks_exact` skips. `binarize_rgba` is called on RGBA
buffers whose length is a multiple of 4, so the remainder is empty either way; the rewrite does not
change what happens if it ever is not.

`as_chunks` / `as_chunks_mut` are stable since Rust 1.88, comfortably below the pin, so the fix does
not depend on decision 2 and would compile on the 1.96 toolchain this machine currently has.

*Alternative rejected:* `#[allow(clippy::chunks_exact_to_as_chunks)]`. Forbidden by `AGENTS.md`, and
it would carry the debt forward on every future call site.

### 2. Pin to 1.98.0 in `rust-toolchain.toml`

```toml
# Why this exists and how to move it: see AGENTS.md, next to the gate commands.
[toolchain]
channel = "1.98.0"
components = ["rustfmt", "clippy"]
```

Pin to the **current** stable, not to the 1.96 that predates the lint. Pinning backwards would make
CI green by hiding the lint, leaving the code stale and the fix unverified.

`components` is listed explicitly so the file alone guarantees the two gate tools, no matter how the
toolchain gets installed.

`profile` is deliberately omitted, so rustup's `default` profile applies. That profile already
includes `rustfmt`, `clippy` and `rust-docs`, which keeps a fresh local checkout usable for editing
and doc lookup.

*Alternative rejected:* `profile = "minimal"`. It saves a modest download in CI, but it degrades the
local experience for anyone whose first contact with the repo is this pin, and CI download time is
not a problem this change is trying to solve.

*Alternative rejected:* `channel = "stable"` in the file. It is a pin in name only and reintroduces
exactly the drift being removed.

### 3. Swap the CI toolchain action for one that reads the file, and neutralize its defaults

Replace `dtolnay/rust-toolchain@stable` with `actions-rust-lang/setup-rust-toolchain@v1`, passing no
`toolchain` input. That action documents: if a `rust-toolchain` or `rust-toolchain.toml` exists at the
repository root and no `toolchain` value is given, everything the file specifies is installed;
supplying a `toolchain` value makes it ignore the file instead. The existing
`with: components: rustfmt, clippy` block goes away, since the file now declares them.

Two of the action's defaults must be overridden, or the step quietly does more than set a toolchain:

- **`rustflags: ""`.** The action sets `RUSTFLAGS` to `-D warnings` by default, for the whole job.
  Today only the Clippy step denies warnings (`.github/workflows/ci.yml:43`); `cargo test` and
  `cargo run --bin catalog-index` do not. Accepting the default would silently promote every rustc
  warning in those two steps to an error, which is a lint-policy change smuggled inside a toolchain
  change. The empty string leaves `RUSTFLAGS` unset. If the project ever wants job-wide
  deny-warnings, that is its own issue and its own decision.
- **`cache: false`.** The action enables `Swatinem/rust-cache` by default, and the `rust` job already
  has an explicit `actions/cache@v6` step keyed on `Cargo.lock` (`.github/workflows/ci.yml:27-36`).
  Leaving both on would layer two cache implementations over the same directories; keeping the
  existing, reviewed one is the smaller change.

Pin the action at the `@v1` major tag, matching how the repo pins `actions/checkout@v7`,
`actions/cache@v6` and `actions/setup-node@v7`. (Docker bases and the OpenSpec CLI are pinned harder,
by digest and exact version respectively, because those decide what ships or what generates committed
files. A setup action decides neither.)

*Alternative rejected:* keep `dtolnay/rust-toolchain` and pin it at `@1.98.0`. It does not read
toolchain files, so the version would live in two files that must be bumped together. Two sources of
truth for one version is the failure mode this change exists to remove.

*Alternative rejected:* keep `dtolnay/rust-toolchain@stable` and let the file win anyway. rustup does
honour `rust-toolchain.toml` over an action-installed default, so CI would end up on 1.98.0, but only
after installing a toolchain it then ignores, and the workflow would read as though CI tracks stable
when it does not. Correct by accident, misleading on the page.

*Alternative rejected:* drop the setup action and rely on the runner's preinstalled rustup
auto-installing from the file on first `cargo` call. Fewest moving parts, no `RUSTFLAGS` default to
neutralize, but the install then happens silently inside the `Format` step, so a toolchain download
failure is reported as a formatting failure. Keeping an explicit, named step is worth one action
dependency; this stays the fallback if the action misbehaves.

### 4. Leave the release image on its digest-pinned base

`rust-toolchain.toml` stays out of the `Dockerfile` `COPY` list, so the image build keeps using the
compiler inside `rust:1-trixie@sha256:3382bd…`. The image build is already reproducible by digest,
and dependabot bumps that digest. Copying the file in would make every image build download a second
toolchain over the network into an image that already has one, trading build time and a network
dependency for an exactness the digest pin already provides in its own way.

The accepted consequence is that the gate compiler and the ship compiler can differ. It is bounded:
`build` runs on every push behind `needs: [rust, ui]`, so any real incompatibility surfaces as a red
image build in the same run, not as a bad artifact.

*Alternative rejected:* `COPY rust-toolchain.toml ./` in the build stage. Exact agreement, at the cost
of a toolchain download per image build and a build that fails when static.rust-lang.org is
unreachable. Revisit if the gate/ship divergence ever actually bites.

### 5. Document the pin where the gates are documented

`AGENTS.md` gains, next to the `cargo fmt` / `cargo clippy` / `cargo test` gate paragraph (around
`AGENTS.md:131-137`), a short note stating that `rust-toolchain.toml` pins the toolchain, that rustup
installs it automatically on first use, that a per-directory `rustup override` silently beats it, and
how to bump it: edit `channel`, run the three gates, fix the lint fallout, commit the bump on its own.
`CLAUDE.md` is a symlink to `AGENTS.md`, so this reaches human and agent readers at once.

With no ADR in scope (see Context), this note plus the `rust-toolchain.toml` header comment is where
the decision's "why" lives.

## Risks / Trade-offs

- **A pin goes stale, and nothing here bumps it.** Dependabot has no updater for
  `rust-toolchain.toml`, so the version moves only when a person decides to move it. → Accepted, and
  named as such in the `AGENTS.md` note. The trade is deliberate: freshness for reproducibility. Lint
  debt now accumulates visibly in one bump commit instead of arriving as an unrelated red build.
- **A per-directory `rustup override set` silently beats the file.** A contributor who ran that in
  this directory keeps their own toolchain, and their local gate results stop matching CI, which is
  the exact confusion this change removes for everyone else. → Named in the `AGENTS.md` note;
  `rustup override unset` in the repo root is the fix. Not detectable from CI.
- **The action swap is the only untested piece.** `actions-rust-lang/setup-rust-toolchain` reading the
  file, and honouring `rustflags: ""`, are documented behaviors, not behaviors verified in this repo.
  → Verified by the first CI run of the change's branch: the job log names the installed version, and
  the same run's `Clippy` and `Test` steps are the assertion. If the action misbehaves, decision 3's
  last alternative (drop the action entirely) is a one-step fallback.
- **Contributors pay a one-time toolchain download.** First `cargo` call after this lands fetches
  1.98.0 if absent. → Unavoidable and small; it is the cost of the guarantee.
- **Someone reads the pin as an MSRV.** `rust-toolchain.toml` says "build with this", not "this is the
  oldest supported compiler". → The `AGENTS.md` note states the distinction, since `Cargo.toml`
  carries no `rust-version` to contradict it.

## Migration Plan

No deploy step, no data migration, no runtime change. The commit is the migration: after it lands,
the next `cargo` invocation in any checkout resolves 1.98.0 through rustup.

Rollback is `git revert` of the single commit. Reverting restores the floating `@stable` step and the
`chunks_exact` call sites together, which returns `main` to red; the useful partial rollback, if the
action swap alone proves wrong, is to keep the fix and the pin and replace only the CI step per
decision 3's fallback.
