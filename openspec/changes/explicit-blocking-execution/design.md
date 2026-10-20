# Design

## Context

See `proposal.md` for motivation. Inspection at `3dddfb20` found synchronous `rusqlite::Connection` operations behind `std::sync::Mutex`, with no production `.await` inside Store methods. API handlers call those methods and render synchronously before awaiting printer I/O. The reload comment already describes a single-user, local-filesystem deployment. HTTP test token seeding currently starts a thread and runtime solely to call the async-shaped Store.

## Goals / Non-Goals

**Goal:** Express the existing blocking execution model directly in signatures and comments.

**Non-goal:** Change task scheduling or promise request latency under concurrency.

## Decisions

- Default: use ordinary synchronous Store methods. Remove awaits from Store call sites and make helpers synchronous when Store calls are their only async work. Keep genuinely asynchronous HTTP/server and printer operations asynchronous. Remove the token-seeding thread/runtime wrapper once its Store calls are synchronous. Keeping fake futures fails the issue's explicit blocking-contract requirement; an async driver or per-operation offloading adds scheduling and ownership machinery without an observed workload failure requiring it.
- Default: use one private connection accessor returning the existing mutex guard, retaining its current poisoning policy. A closure executor adds indirection without changing what callers need; direct repeated locking fails the issue's one-accessor requirement.
- Default: keep SQLite and all Typst compilation/rasterization inline, including thumbnails, single-label and batch rendering. Document that these calls occupy their calling thread, assume small local operator workloads, and do not provide latency isolation. No throughput failure is asserted by the issue, so this refactor does not introduce a pool, semaphore or offloading wrapper. Tokio documents `spawn_blocking` for isolating blocking work, but that would be a separate scheduling decision: https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html.
- Validation uses existing Store, auth and HTTP coverage plus the required Rust gates. No concurrency test is added because no scheduling behavior changes. Compilation detects forgotten Store awaits; existing behavioral tests can fail if call-site changes alter persistence or responses.

## Risks / Trade-offs

Inline work can occupy Tokio workers for its duration; this is the existing limitation, explicitly documented by this change. The local, small-workload premise is an assumption, not a measured throughput claim.

## Migration Plan

Land as an ordinary refactor under the repository workflow. No data migration or deployment sequencing is required; reverting the refactor restores the previous Rust signatures.
