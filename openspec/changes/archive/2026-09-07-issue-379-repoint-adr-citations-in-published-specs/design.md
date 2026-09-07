## Context

See proposal.md for motivation. Two constraints shape everything below.

`openspec/specs/` is written by archive and never by hand, so a correction to a published spec arrives
as a delta, and a delta of an existing requirement is a `MODIFIED` block reproducing that requirement
whole. The published citations sit inside five requirements, two of which run to hundreds of lines, so
the deliverable is roughly 640 lines of reproduction carrying six word-level edits.

The five decisions cited all have a change folder under `openspec/changes/archive/`, and each folder
was verified to carry the rationale the citation reaches for: the `text` `overflow` policy in
`2026-08-27-issue-226-unify-size-resolution`, the alignment-slot box in
`2026-08-21-issue-180-auto-length-text-alignment`, the cap-height metric model and its ink reservation
in `2026-08-28-issue-245-center-ink-reserve`, the quarantine of a duplicate id in
`2026-08-21-issue-181-duplicate-id-not-fatal`, and the two-`red` divergence in
`2026-08-31-issue-280-shape-paint-model`. Each ADR's own header names the same issue number as its
folder, which is what pairs them.

## Goals / Non-Goals

**Goals:**

- Leave no `openspec/specs/**` reference to an ADR above 0057, so #378 deletes nothing that is still
  pointed at.
- Change the six pointers and no other word. The delta is provable against the published spec by a
  word-level diff yielding exactly six substitutions.

**Non-Goals:**

- Repointing ADR-0045 or ADR-0050. Both survive #378 and are left as written.
- Repointing the citations at or below ADR-0057 elsewhere in `openspec/specs/` (0010, 0014, 0022,
  0024, 0028, 0033, 0036, 0051, 0052, 0055). Those records survive, and the frozen `docs/adr/` set
  through 0057 stays the right address for them.
- Touching `docs/adr/`, which is frozen, or deleting anything. That is #378.

## Decisions

**The pointer is the folder, never a file inside it.** Each folder's rationale is split across its
`proposal.md` and its `design.md`, and which of the two carries a given sentence is not predictable
from the citation. Naming the folder makes the address stable under a folder whose file set later
grows, and spares the reader a guess. The alternative, a `folder/design.md#anchor`-style pointer,
buys precision that only holds until someone moves a paragraph between the two files.

**A pointer swap, not a rewording.** At `layout-sizing:743` and `colour-vocabulary:164` the sentence
reads "This supersedes X". Substituting a folder path for an ADR number leaves the grammar intact,
because both spellings name the same thing: a decision. The considered alternative was to repair the
grammar to "the decision recorded in <folder>", which reads slightly better and costs the guarantee
that a word-level diff shows nothing but the pointers. The guarantee is worth more than the reading:
it is what lets a reviewer confirm no requirement changed meaning without re-reading 640 lines.

**Prose is re-wrapped where a pointer overflows the line.** The paths run 63 to 73 characters against
an 8-character ADR number, so four of the six sites push their line past the ~100-column wrap these
files keep. Re-wrapping the affected paragraph is whitespace, and the word-level diff above is what
proves it changed nothing else. The alternative, leaving 160-column lines behind, would make every
later diff of those paragraphs unreadable.

**`layout-sizing:781` keeps two ADR numbers alongside one folder path.** The metric model is defined
by ADR-0045, ADR-0050 and ADR-0084 together; only ADR-0084 is deleted. ADR-0045 and ADR-0050 predate
OpenSpec and have no change folder, so an ADR number is the only address they have. The mixture is
correct and is deliberately not smoothed over: making all three uniform would mean either inventing
folders that do not exist or leaving a pointer #378 breaks.

**`layout-sizing:993` points at the archive folder, not at the live `template-registry` capability.**
The clause is normative ("SHALL be quarantined per ADR-0058"), and the quarantine rule is published
today in `openspec/specs/template-registry/spec.md`, which would make a stronger normative reference
than an archived proposal. It is not taken here, for two reasons. The issue's acceptance criteria
require every replaced pointer to name a path under `openspec/changes/archive/`, and swapping the
reference to a live capability would change what the sentence normatively binds to, which is a change
of meaning and outside "only the pointer moves". If that reference is wanted, it is its own issue.

**No code, no tests.** `DELIVERABLE: spec-only`. Nothing under `src/`, `ui/` or `tests/` reads an ADR
number, so no gate can observe this change; the delta is the whole deliverable and the acceptance
checks below are what stand in for a test.

## Risks / Trade-offs

**A `MODIFIED` block reproducing 640 lines can silently drop or alter a line, and archive would write
the damaged version into the published spec.** → The delta was generated by extracting the exact
published line ranges and applying six scripted substitutions, each asserted to match exactly once.
The check that stands is a word-level diff of the delta against the published requirement, which must
show exactly the six pointer substitutions and nothing else.

**The `§6` anchor at `colour-vocabulary:164` is lost.** ADR-0092 §6 recorded the two-`red` divergence
in one numbered section; a folder has no section numbering, so the pointer now names the folder and
the reader locates the divergence by reading it. → Accepted. The surrounding sentence already states
what the divergence was ("a text item's `red` was `#ff4136` and a shape's `red` was `#ff0000`"), so
the pointer is corroboration rather than the only account, and #378 deletes the anchored text anyway.

**A future folder rename or archive reorganisation breaks all six pointers at once.** → Accepted, and
strictly better than today: an ADR number pointed at a file that #378 deletes, whereas these folders
are kept permanently by policy. The acceptance check below is cheap to re-run.

**Archive rewrites three published specs from this delta, so a mistake lands in `openspec/specs/`
rather than in a reviewed diff.** → The delta is reviewed before archive, and after archive the same
word-level diff can be re-run against the pre-change files from git history.

## Migration Plan

None. No stored data, no template, no request and no response changes. Rollback is `git revert` of the
single commit.

## Acceptance

Three checks, all runnable from the worktree root:

1. `grep -rnoE "ADR-0(05[89]|0[6-9][0-9]|[1-9][0-9]{2})" openspec/specs/` returns nothing. The `05[89]`
   alternative is load-bearing: without it the predicate starts at ADR-0060 and silently passes while
   ADR-0058 and ADR-0059 are still cited. Run against the published specs before the change it must
   report exactly the six repointed sites (ADR-0058, ADR-0059, ADR-0082 twice, ADR-0084, ADR-0092),
   and it must report none of the citations that intentionally remain (ADR-0010, 0014, 0022, 0024,
   0028, 0033, 0036, 0045, 0050, 0051, 0052, 0055). A run that finds nothing on both trees has proved
   nothing, so check the predicate against the pre-change tree first.
2. Every `openspec/changes/archive/<folder>` named in the changed specs is an existing directory.
3. `grep -n "ADR-0045\|ADR-0050" openspec/specs/layout-sizing/spec.md` still reports both at the
   metric-model clause and at the two other sites that name them.
