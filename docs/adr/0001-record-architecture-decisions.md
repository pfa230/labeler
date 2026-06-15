# 1. Record architecture decisions

**Status:** Accepted

## Context

Labeler is evolving quickly and several non-obvious design choices (parsing strategy, coordinate
system, rendering backend) are already baked into the code. Without a written trail, the rationale for
these choices lives only in commit messages and contributors' heads, and future changes risk
re-litigating settled questions or silently violating an existing constraint.

## Decision

Keep a log of Architecture Decision Records in `docs/adr/`, using Nygard's format (Context, Decision,
Consequences). Pair it with a living specification in `docs/SPEC.md`. Every major decision adds or
supersedes an ADR and updates the spec in the same change.

## Consequences

- New contributors can read the "why", not just the "what".
- A small amount of process overhead per significant change.
- ADRs are append-only; reversing a decision means writing a superseding ADR rather than editing
  history.
