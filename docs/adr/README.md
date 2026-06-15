# Architecture Decision Records

This directory records the significant architectural decisions for Labeler, using
[Michael Nygard's ADR format](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions).

An ADR captures a single decision: its context, the choice made, and the consequences. ADRs are
immutable once **Accepted** — to change a decision, add a new ADR that supersedes the old one (mark the
old one `Superseded by ADR-NNNN` and the new one `Supersedes ADR-NNNN`).

## Index

| ADR | Title | Status |
| --- | --- | --- |
| [0001](0001-record-architecture-decisions.md) | Record architecture decisions | Accepted |
| [0002](0002-two-stage-template-parsing.md) | Two-stage template parsing | Accepted |
| [0003](0003-typst-rendering-engine.md) | Typst as the rendering engine | Accepted |
| [0004](0004-bottom-left-coordinate-system.md) | Bottom-left coordinate system | Accepted |
| [0005](0005-recursive-containers-with-option-gating.md) | Recursive containers with option gating | Accepted |
| [0006](0006-template-edit-ownership.md) | Template edit ownership: manual vs GUI | Accepted |
| 0007 | Printer architecture and transport model (reserved — issue #1) | Proposed |
| 0008 | Web UI delivery (reserved — issue #2) | Proposed |
| [0009](0009-image-source-model.md) | Image source model | Accepted |

## Adding an ADR

1. Copy the structure of an existing record. Use the next zero-padded number.
2. Set `Status: Proposed`, fill in Context / Decision / Consequences.
3. Add a row to the index above.
4. On acceptance, set `Status: Accepted` and update [`../SPEC.md`](../SPEC.md) to reflect the new
   behavior (and its changelog).
