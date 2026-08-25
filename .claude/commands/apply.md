---
description: Implement a change on one agent and review it on another until it passes
argument-hint: <change> <implementer> <reviewer>
allowed-tools: Bash
---

Run `.workflow/apply.sh $ARGUMENTS` in the background; it takes many minutes.

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

On success it has committed nothing. Archive, merge and push are separate, explicitly requested steps.

If you find yourself reviewing the diff yourself, the pairing has collapsed into self-review. Say so
instead.
