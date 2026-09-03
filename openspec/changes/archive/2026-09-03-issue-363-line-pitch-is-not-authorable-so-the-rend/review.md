# Plan review

AUTHOR: opencode
REVIEWER: codex
VERDICT: APPROVE_WITH_CHANGES
ROUNDS: 2

## Findings

1. The `MODIFIED` delta renames two existing scenarios, so strict OpenSpec validation fails and treats them as omitted. The published names are `A centred item with headroom is unaffected` and `Aligned edges are unchanged` (`openspec/specs/layout-sizing/spec.md:1189`, `:1225`), while the delta changes them to different identities (`specs/layout-sizing/spec.md:87`, `:124`). A `MODIFIED` requirement must retain existing scenario identities.

2. The first-touch field list drops the existing parameter-reference form of `font_weight`. It describes only a weight value (`specs/text-line-spacing/spec.md:9`) and then supersedes the frozen text bullet in full (`:21`), but that bullet permits either a literal or `"{param}"` reference resolving from an integer parameter (`docs/SPEC.md:489-493`). No listed sibling capability owns that complete rule. This violates the complete-post-change-contract requirement (`AGENTS.md:23-25`; `openspec/config.yaml:65-68`).

3. The claimed universal `at: [0, 0]` default is false for packed text children (`specs/text-line-spacing/spec.md:9`). A packed child must carry neither `at` nor `to`, remains anchorless on read-back, and is positioned by its flow container (`openspec/specs/flow-layout/spec.md:151-160`, `:212-220`). The complete field contract must preserve that contextual exception.

## Required changes

- Restore these exact scenario headings in `specs/layout-sizing/spec.md`, retaining the narrowed single-line bodies:
  - `A centred item with headroom is unaffected`
  - `Aligned edges are unchanged`
- Amend the `font_weight` clause to preserve both accepted forms: a literal multiple of 100 from 100 through 900, or a `"{param}"` reference to an integer parameter resolving to such a value; omission defaults to 400.
- Qualify placement so non-packed text items default `at` to `[0, 0]`, while packed children accept neither `at` nor `to` and are positioned under `flow-layout`.
- Re-run strict OpenSpec validation and require it to pass.

The author applies these edits; under `APPROVE_WITH_CHANGES`, NO further review follows.

CHANGES_APPLIED: yes
SPECS_SHA256: 274d82f4498e37a65d580491c63391a488a7e0fb206fc21a6d900012ac0ac8a6
