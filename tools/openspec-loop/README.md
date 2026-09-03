# openspec-loop

The gated adversarial-review workflow extracted from
[labeler](https://github.com/pfa230/labeler), so other projects can run the same loop.

One accepted issue goes in; a planned, adversarially reviewed, implemented, adversarially
re-reviewed, archived and gated commit comes out, on a green branch run, with the merge left to a
person. Nobody reviews their own plan and nobody reviews their own code, and that is enforced by
the launcher and again by a commit-time gate rather than by anyone remembering.

## Status

**Early. Labeler is the only consumer, and the gate stage is Rust-only.**

The kit ships `workflow/gates.sh` unchanged, which parses `cargo test` output and runs a baseline
suite at the fork point to tell a failure this change caused from one that was already there.
Generalizing that behind an adapter interface is deliberately deferred: it was designed once from
a single real case and failed review three times. A second consumer with genuinely different
gates is what triggers that design, informed by two real cases instead of one imagined one.

Everything else in the loop is language-neutral: the plan review, the diff review, both landing
gates, the question protocol, and the worktree and issue discipline.

## Two halves, two delivery mechanisms

The shell runtime is consumed as a **git subtree** at `tools/openspec-loop/`, because the gates
must be present in a bare `git clone` and in every `git worktree add` (the driver creates one per
change), and because a subtree gives a consumer's local divergence an ordinary three-way merge
instead of a fork.

The Claude Code surface is a **plugin**: `/change`, `/apply`, and an edit-time review-gate hook
that calls the repository's own vendored gate, so its rules cannot drift from the ones the commit
hook and CI enforce.

## Layout

    workflow/     the runtime and its three self-test suites
    .githooks/    pre-commit, pre-merge-commit, pre-push
    schema/       the OpenSpec schema fork and its templates
    commands/     /change, /apply
    hooks/        the edit-time gate and its plugin registration
    skills/       the loop, for Claude
    templates/    per-project config to be filled in

`.githooks/` keeps its dot: the suites resolve it as `$here/../.githooks`, and renaming it breaks
eight tests with exit 127.

## Tests

    workflow/gate-tests.sh      # the two commit gates, mostly the refusals
    workflow/apply-tests.sh     # apply.sh change resolution, through --dry-run
    workflow/change-tests.sh    # the driver, its stages, and the question protocol

A gate that stops firing looks exactly like a gate that passes. These suites exist because both
of them did that once during development.

## Provenance

Forked from labeler at `e57d5ef`. `schema/` is itself a fork of OpenSpec CLI 1.9.0's built-in
`spec-driven` schema, with the `review` artifact adapted from the `anvil` community schema by
@jikkujoyce. `workflow/apply-with-agy.sh` was deliberately not extracted: it records no verdict.

`--filter <substring>` runs only the cases whose label contains it, which turns a fixture
fix from an eight-minute pass into seconds. A filter matching nothing exits 2 rather than
reporting a clean zero.
