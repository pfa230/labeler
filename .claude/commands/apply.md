---
description: Implement a change on one agent and review it on another until it passes
argument-hint: <implementer> <reviewer> [change]
allowed-tools: Bash
---

Run `.workflow/apply.sh $ARGUMENTS` in the background; it takes many minutes.

The pair is named first because that is the guarantee: the model writing the code is never the model
judging it. The change comes last and is optional. Left out, the script resolves the one in flight,
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
- **6** still REVISE after the last round; the findings want a human, not another round
- **2** bad arguments, or the change could not be resolved; nothing was launched

On success it has committed nothing. Archive, merge and push are separate, explicitly requested steps.
It records the verdict as `openspec/changes/<change>/diff-review.md`, which the commit gate reads, and
keeps each round beside it as `diff-review-<n>.md`.

If you find yourself reviewing the diff yourself, the pairing has collapsed into self-review. Say so
instead.
