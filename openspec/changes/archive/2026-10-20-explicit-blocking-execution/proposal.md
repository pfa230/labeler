# Proposal

## Why

Store methods expose futures despite doing synchronous SQLite work without yielding. Making that blocking contract explicit removes the misleading async surface without introducing a scheduling mechanism for a throughput problem nobody has reported.

Source issue: https://github.com/pfa230/labeler/issues/247

Fixes #247

## What Changes

- Make Store operations synchronous and update their callers.
- Acquire the Store mutex through one private accessor.
- Document inline SQLite and Typst execution and the small, local deployment assumption beside the implementation.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

None. This is a behavior-preserving refactor with `skip_specs: true`.

## Impact

Touches `src/store.rs`, Store callers in API, authentication, middleware, settings and tests, and comments at rendering entry points. No dependency or HTTP contract changes.

Non-goals: worker pools, async database drivers, render offloading, concurrency limits, schema changes and performance instrumentation. Implementation follows the repository's ordinary refactor workflow rather than a behavior-change loop run.
