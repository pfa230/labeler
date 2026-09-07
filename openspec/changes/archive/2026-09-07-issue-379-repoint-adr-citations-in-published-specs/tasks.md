`DELIVERABLE: spec-only`. The deliverable is the delta under `specs/`, which is already written; the
published specs under `openspec/specs/` are rewritten by archive and MUST NOT be edited by hand here.
No task below writes code, and none writes a file. Every task is a check with a command whose output
decides whether it is done.

## 1. The delta reproduces the published requirements exactly

- [x] 1.1 Extract the published source blocks the delta was built from: `flow-layout/spec.md` lines
      275-431 and 529-651, `layout-sizing/spec.md` lines 689-984 and 986-1020, and
      `colour-vocabulary/spec.md` lines 149-179. Confirm each range starts at a `### Requirement:`
      line and ends at the last line before the next `### Requirement:` (or at EOF, for
      `flow-layout` 529-651).
- [x] 1.2 Word-level diff each capability's delta body (the file minus its `## MODIFIED Requirements`
      header) against the concatenated source blocks from 1.1, e.g.
      `git diff --no-index --word-diff=porcelain --word-diff-regex='[^[:space:]]+' <src> <delta>`.
      The output must show exactly six substitutions and nothing else: `(ADR-0082)` twice,
      `ADR-0059,`, `ADR-0084`, `ADR-0058`, and `ADR-0092 §6`, each replaced by its backticked archive
      path. Any seventh change, in either direction, fails this task.
- [x] 1.3 Confirm the five `### Requirement:` headers in the delta are byte-identical to the headers
      in `openspec/specs/`, since archive resolves a MODIFIED delta by matching that header:
      `flow-layout` "Packing places the children that take up room along the primary axis" and
      "Packing past the padded inner box fails where it lands"; `layout-sizing` "Text is laid out
      against the box it will get, and what does not fit is authored" and "The size vocabulary is a
      number, `content`, or `fill`"; `colour-vocabulary` "A name denotes one colour on every field
      that takes one".

## 2. The six pointers, checked as design.md Acceptance specifies

- [x] 2.1 Run `grep -rnoE "ADR-0(05[89]|0[6-9][0-9]|[1-9][0-9]{2})" openspec/specs/` against the
      pre-change published specs. It must report exactly six sites: `colour-vocabulary:164` ADR-0092,
      `flow-layout:339` and `:559` ADR-0082, `layout-sizing:743` ADR-0059, `:781` ADR-0084, `:993`
      ADR-0058. This run is what proves the predicate can fire; a predicate that finds nothing on
      both trees has proved nothing.
- [x] 2.2 Run the same predicate against `specs/` in the change folder. It must report nothing.
- [x] 2.3 Confirm the same run in 2.1 reports none of the citations that intentionally remain:
      ADR-0010, 0014, 0022, 0024, 0028, 0033, 0036, 0045, 0050, 0051, 0052, 0055.
- [x] 2.4 Confirm every `openspec/changes/archive/<folder>` named in the delta is an existing
      directory: `2026-08-27-issue-226-unify-size-resolution` (twice),
      `2026-08-21-issue-180-auto-length-text-alignment`, `2026-08-28-issue-245-center-ink-reserve`,
      `2026-08-21-issue-181-duplicate-id-not-fatal`, `2026-08-31-issue-280-shape-paint-model`.
- [x] 2.5 Confirm ADR-0045 and ADR-0050 survive in the `layout-sizing` delta at all three sites that
      name them, including the metric-model clause, which after the swap names two ADRs and one
      archive folder.

## 3. The change writes nothing it must not

- [x] 3.1 Confirm `git status --porcelain` shows no modification under `openspec/specs/`, `src/`,
      `ui/`, `tests/`, `docs/SPEC.md` or `docs/adr/`: the only entry is the untracked change folder.
- [x] 3.2 Run `openspec validate issue-379-repoint-adr-citations-in-published-specs --strict` and
      confirm it reports the change valid.

## 4. Gates

- [x] 4.1 `cargo fmt --check`
- [x] 4.2 `cargo clippy --all-targets --all-features`
- [x] 4.3 `cargo test`
