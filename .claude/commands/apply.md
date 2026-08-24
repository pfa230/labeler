---
description: Implement a change on one agent and review it on another
argument-hint: <implementer> <reviewer> [change-name]
allowed-tools: Bash
---

Run the implement/review loop for `$ARGUMENTS`, read as `<implementer> <reviewer> [change]`.
If the change is omitted, infer it from the single active change, or ask.

**Refuse if the implementer and the reviewer are the same agent.** Nobody reviews their own work,
and that is the entire reason this command takes two names.

1. Implement:

       .workflow/run-stage.sh implement <implementer> <change>

   Run it in the background; it takes many minutes. Report only the status line, the files-touched
   count and the tail it prints. Do NOT read the log in full: it runs to thousands of lines and
   keeping it out of this context is why the script writes it to a file. Read a targeted range only
   to diagnose a failure.

   A non-zero exit means it did not do the work. Exit 3 in particular means a clean exit that
   changed nothing, which is a no-op, not a success. Stop and report; do not proceed to review.

2. Review:

       .workflow/run-stage.sh review <reviewer> <change>

   Same output discipline. Exit 5 means the reviewer edited files, which invalidates the review;
   report it rather than accepting the result.

3. Loop. If the review raises meaningful findings, send them back to the **implementer**, resuming
   its session so it keeps what it built:

       .workflow/run-stage.sh implement <implementer> <change> --resume "<the findings>"

   Then re-review. Repeat until a pass surfaces nothing meaningful. The reviewer never fixes what it
   found: that would produce a delta nobody reviewed, and the loop would end only by silently
   accepting unreviewed work.

4. Stop there and report. Do not commit, archive, sync specs, or merge. Those are separate,
   explicitly requested steps, and the apply lock makes git refuse them mid-run anyway.

Neither agent is this session. If you find yourself reviewing the diff yourself, the pairing has
collapsed into self-review; say so instead.
