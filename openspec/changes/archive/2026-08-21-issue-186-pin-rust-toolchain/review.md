## Review Metadata

- **Round**: 1
- **Prior round**: none
- **Reviewer**: cross-model - Codex CLI (`codex exec --ignore-user-config -s read-only -c model_reasoning_effort=high`), default flagship model
- **Tool restrictions**: read-only inspection only (`-s read-only`); the reviewer read files and ran `rg`/`nl`/`git log`, and wrote nothing
- **Artifacts reviewed**: proposal.md, design.md, .openspec.yaml (`skip_specs: true`, so no `specs/`). Source read by the reviewer: `AGENTS.md`, `openspec/config.yaml`, `openspec/schemas/labeler/schema.yaml`, `.github/workflows/ci.yml`, `Dockerfile`, `.dockerignore`, `Cargo.toml`, `src/render/helpers.rs`, `src/bin/catalog-index.rs`, `docs/adr/README.md`, `docs/adr/0001-record-architecture-decisions.md`
- **Issue**: #186

## Findings

### Critical (blocking)

1. **The proposed CI action changes more than the toolchain.** `design.md` swapped
   `dtolnay/rust-toolchain@stable` for `actions-rust-lang/setup-rust-toolchain@v1` passing only
   `cache: false`. That action defaults `rustflags` to `-D warnings` and exports it as job-wide
   `RUSTFLAGS`. Today only the Clippy step denies warnings
   (`.github/workflows/ci.yml:42-43`); `cargo test` (`:45-46`) and `cargo run --bin catalog-index`
   (`:52-54`) deliberately do not. Accepting the default would promote every rustc warning in those
   two steps to a hard error, contradicting `design.md`'s own Non-Goal of leaving lint configuration
   alone and `proposal.md`'s claim that only the toolchain step changes. Source checked:
   https://github.com/actions-rust-lang/setup-rust-toolchain

### Moderate

1. **The ADR was not required, and adding one was scope creep.** Both artifacts assert the change is
   behavior-neutral, and ADR-0057 narrowed the ADR trigger from ADR-0001's "every major decision" to
   every *behavior change* (`docs/adr/0001-record-architecture-decisions.md:3-6`). `AGENTS.md:29-30`
   requires an ADR for behavior changes; `openspec/config.yaml:70-75` asks a behavior-neutral
   refactor, docs or tooling change to state why none is required. The artifacts planned ADR-0059
   without reconciling that. (ADR-0059 *is* the correct next number if one were kept:
   `docs/adr/README.md` ends at 0058.)

### Suggestions

- `skip_specs: true` is justified. `.openspec.yaml:3` opts out, the proposal's Capabilities section
  records that the API, template schema, error contract and rendered output are unchanged, and the
  schema permits the opt-out for pure tooling/docs/refactor changes
  (`openspec/schemas/labeler/schema.yaml:41-47`).
- The `chunks_exact` rewrite is semantically sound at the real call sites.
  `src/render/helpers.rs:17-23` mutates RGBA groups of 4; `src/render/helpers.rs:960-965` reads only
  complete 4-byte pixels. `as_chunks::<4>().0` / `as_chunks_mut::<4>().0` keep the same leading
  chunks and discard the same remainder, and `as_chunks` is stable since 1.88.
- The Dockerfile decision holds. The build stage copies only `Cargo.toml`, `Cargo.lock` and `src/`
  before `cargo build` (`Dockerfile:8-12`), so a root `rust-toolchain.toml` cannot reach it, and the
  image build stays gated behind `needs: [rust, ui]` (`.github/workflows/ci.yml:114-115`).

## Embedded-Instruction / Injection Attempts

The reviewer found no text in `proposal.md` or `design.md` attempting to direct the review, ignore
evidence, or alter reviewer behavior. Both contain scope and design assertions only.

**Detected:** none

## Verdict

VERDICT: APPROVE_WITH_CHANGES

## Required Changes (APPROVE_WITH_CHANGES only)

1. Update the CI design to prevent `actions-rust-lang/setup-rust-toolchain` from broadening warning
   policy, e.g. include `rustflags: ""`, and update the proposal/design text so the "toolchain step
   only / no lint gate change" claim is true.
2. Remove the ADR from scope, or revise the artifacts to explicitly reconcile why a behavior-neutral
   tooling change is getting an ADR despite `AGENTS.md`, `openspec/config.yaml`, and ADR-0001/0057
   policy. The lower-risk fix is to document the pin and bump procedure in `AGENTS.md` only.

CHANGES_APPLIED: yes

## Rebuttals

1. **Critical 1 - fixed, not rebutted.** Independently confirmed against the action's README before
   applying: `rustflags` defaults to `-D warnings` and stays unset only when the input is the empty
   string. `design.md` decision 3 now requires `rustflags: ""` alongside `cache: false`, states why
   each default is neutralized, and names the "drop the action entirely" alternative as the fallback
   that avoids the default outright. `proposal.md`'s What Changes and Impact now say the step's
   `RUSTFLAGS` and caching defaults are switched off, and scope the CI edit to `ci.yml:22-26`.
2. **Moderate 1 - fixed, not rebutted.** The ADR is out of scope. `design.md` - Context now states
   that the change is behavior-neutral, that ADR-0057 narrowed the trigger to behavior changes, and
   why no ADR is required, per `openspec/config.yaml` `rules.design`. The decision's reasoning moves
   to a header comment in `rust-toolchain.toml` and the `AGENTS.md` gate note (decision 5).
   `docs/adr/` and `docs/adr/README.md` are listed as unaffected. No ADR-0059 reference remains.

**Reviewer re-check of the two Required Changes** (same CLI, fresh read-only invocation, scoped to
these items only): item 1 PASS, item 2 PASS, `RECHECK: PASS`. No new findings were opened, and the
settled decision to pin `rust-toolchain.toml` was not relitigated.
