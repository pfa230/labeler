---
description: Run the OpenSpec apply stage on agy for a change, then review its diff
argument-hint: <change-name>
allowed-tools: Bash
---

Run the apply stage for `$ARGUMENTS` on agy.

**Superseded by `/apply agy <reviewer> [change]`**, which pairs the implementer with a reviewer
instead of leaving the review to whoever remembers, and records the verdict as a gated artifact.
Prefer that.

1. Run `.workflow/apply-with-agy.sh $ARGUMENTS` in the background; it can take many minutes.
2. Report only the exit status and the tail it prints. Do NOT read
   `.worktrees/<issue>/.agy-apply.log` in full: it runs to thousands of lines and pulling it
   into this context is exactly what the script exists to avoid. Read a targeted range only if
   diagnosing a failure.
3. While it runs, and after it finishes, do not commit, merge, push, or archive. The script holds
   a lock that makes git refuse those anyway; do not attempt to remove the lock.
4. When it completes, review the resulting diff adversarially, per "Reviewing before you call it
   done" in AGENTS.md. agy wrote the code, so this session reviews it: different models by
   construction.
5. Stop there and report. Archiving and integration are separate, explicitly requested steps.
