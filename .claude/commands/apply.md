---
description: Implement a change on one agent and review it on another until it passes
argument-hint: [<implementer> <reviewer>] [change]
allowed-tools: Bash
---

Run `.workflow/loop apply $ARGUMENTS` in the background; it takes many minutes.

The pair is named first because that is the guarantee: the model writing the code is never the model
judging it. It is also optional: left out, both roles come from the gitignored
`.workflow/roles.local`, which records which CLIs work on this machine. Name both or name neither,
and note that with neither named a lone argument is the *change*, not the implementer. The change
comes last and is optional. Left out, the script resolves the one in flight,
from the worktree you are in or from the single one across `.worktrees/`, and prints what it picked
before doing anything; with several in flight it refuses and lists them rather than guessing. Pass
`--rounds N` to change the three-round cap and `--dry-run` to see what it would do.

That script owns the loop: implement, review, send findings back to the implementer, re-review, up to
three rounds. Do not reimplement any of it here, and do not step in between the rounds.

Report only what it prints: the per-stage status lines, the files-touched counts and the tails. Do NOT
read the agent logs in full. They run to thousands of lines and keeping them out of this context is
why the script writes them to files. Read a targeted range only to diagnose a failure.

Exit codes worth naming when you report them:

- **1** a stage failed
- **3** implement exited cleanly having changed nothing, so it did not run
- **4** no readable verdict, so the script refused to guess whether the review passed
- **5** the reviewer edited files, which invalidates its verdict
- **7** the reviewer returned no structured result, so its log is a transcript rather than a review
- **6** still REVISE after the last round; the findings want a human, not another round
- **10** the fix round left the tree byte-identical to the one the previous round judged, so the next
  review would be a second verdict on bytes already judged. Nothing was launched. Read the round file
  it names: either every finding was answered in prose, which a person accepts or rejects, or none was
  acted on, which is a fix round to re-run.
- **2** bad arguments, or the change could not be resolved; nothing was launched

On success it has committed nothing. Archive, merge and push are separate, explicitly requested steps.
It records the verdict as `openspec/changes/<change>/diff-review.md`, which the commit gate reads, and
keeps each round beside it as `diff-review-<n>.md`. That record names `AUTHORS:`, every agent whose
implement or gate-fix stage actually changed the worktree, and `TREE_SHA256:`, the tree the approving
review was handed.

If you find yourself reviewing the diff yourself, the pairing has collapsed into self-review. Say so
instead.
