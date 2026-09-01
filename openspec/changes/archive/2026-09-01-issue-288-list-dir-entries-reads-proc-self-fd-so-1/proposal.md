## Why

Implements [#288](https://github.com/pfa230/labeler/issues/288), which now also carries the work
first split out as #308. `cargo test` is red on macOS, 16 of 760 tests fail, and CI on ubuntu is
green on the same commits, so the suite is a gate only CI can satisfy. Two unrelated causes produce
those 16, and neither can be fixed alone: whichever landed first would still face the other's
failures at `.workflow/run-change.sh:516`, which runs `cargo test` locally and exits non-zero. They
are therefore one change.

**Cause 1, the `/proc` indirection.** `src/fs_safe.rs:list_dir_entries` enumerates a directory it
already holds an open descriptor for by formatting that descriptor into a `/proc/self/fd/<n>` path
and handing the path to `std::fs::read_dir`. `/proc` is a Linux pseudo-filesystem. On a host without
it the `openat` succeeds and the `read_dir` fails with `ENOENT`, reported as `500 RenderFailed` with
`details.reason` `template_registry_io`: a real IO failure claimed where the only fault is the
platform. Twelve of the sixteen failures are this.

**Cause 2, tests that predict the filesystem.** Four tests assert behavior that depends on a
capability the default macOS APFS volume does not have. Three require a case-sensitive volume; one
requires a filesystem that accepts a non-UTF-8 filename, which APFS refuses with `EILSEQ` on every
volume it offers.

Cause 2 is not only a platform problem: those three tests contradict a requirement that is already
normative. `openspec/specs/template-groups/spec.md:321` says whether a case-only sibling can be
created "SHALL be answered by the filesystem holding that directory, and SHALL NOT be predicted",
and the tests predict it.

## What Changes

- The listing behind group resolution, creation, rename and delete is read **from the open
  descriptor itself** rather than from a path that names the descriptor. The pseudo-filesystem
  dependency is deleted rather than detected: one code path, no platform branch, and no
  unsupported-platform case left to report.
- A listing that cannot be read to completion **fails** with `template_registry_io` instead of
  yielding a truncated list. Today a mid-iteration read error is silently dropped, and the caller
  then decides exact-name matches and case conflicts against a partial listing. This is a change to
  shipped behavior, rare and deliberate; see Impact.
- `template_registry_io` otherwise continues to mean what it meant: the operating system refused to
  read the directory. It is no longer reachable merely because a host lacks `/proc`.
- No new `Reason` variant. The unsupported-platform error #288 originally contemplated has nothing
  left to report, and a `Reason` for an unreachable case is dead contract.
- **The three case-dependent tests probe the volume they run on and assert the contract for what
  they find**, rather than assuming a case-sensitive one. Each gains the case-folding branch the
  `template-groups` requirements already specify, so this adds coverage instead of removing it, and
  no test passes by not running.
- **One test returns early without asserting**, and it is the only one: the state
  `templates::tests::load_from_dir_handles_non_utf8_paths` builds cannot be constructed on APFS at
  all. It is gated on the capability rather than the OS, prints the capability and the errno that
  stopped it, and carries its reason in a comment beside the early return.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `template-registry`: what the service guarantees about the directory listing that group
  resolution, creation, rename and delete rest on, and when `template_registry_io` may be returned
  for such a listing. Both are documented today only in frozen `docs/SPEC.md` §10.1, so under the
  first-touch rule this lands as an **ADDED** requirement carrying the complete post-change contract
  and naming the `docs/SPEC.md` §10.1 `template_registry_io` row it supersedes.

The requirement is scoped to that listing and says so. It does not govern the read-only enumerations
that build the registry at load and reload or answer `GET /api/template-groups`
(`src/templates.rs:collect_dir_entries:592`, `collect_group_paths:694`, reached from
`src/api.rs:435`): those walk the tree by path, never used `/proc`, and are untouched here.

**The test work carries no delta, and must not invent one.** Every case-folding answer those tests
will newly assert is already normative in `openspec/specs/template-groups/spec.md`: the
`422 template_group_case_conflict` for an aliased sibling at `:321`, the `404` for a path segment
that matches no entry byte for byte at `:832`, and `409` with nothing renamed where "a case-folding
filesystem aliases the two spellings and reports the destination as existing" at `:889-893`. The
tests are being brought into line with requirements that already exist and that they currently
contradict. Writing a delta here would restate the contract, not change it.

`template-groups` therefore needs no delta either. It cites `template_registry_io` at `:394`, `:396`,
`:505` and `:519` but defines neither the listing nor that reason, and this change alters none of the
status codes or reasons its requirements name.

## Impact

- **Code**: `src/fs_safe.rs:list_dir_entries` (lines 39-68) only. Its five callers are unchanged:
  `check_sibling_name:76`, `classify_eexist:137`, `resolve_group_for_delete:414`,
  `resolve_group_for_rename:507`, `collect_subgroup_rel_paths_fd:567`.
- **Tests**: four adapted and one added. `fs_safe::tests::exact_reuse_and_sibling_creation:889`,
  `http_tests::template_group_case_sibling_created_on_case_sensitive_fs:4855` and
  `http_tests::template_group_rename_recasing:5169` gain a volume probe and a second branch;
  `templates::tests::load_from_dir_handles_non_utf8_paths:4541` gains the capability early return;
  one new unit test covers the traversal-alias filter. Details in `design.md` - Decisions 8 to 11.
- **Dependencies**: `Cargo.toml:49` gains the `alloc` feature on `rustix`. See `design.md` -
  Decision 7: the feature is already enabled transitively, so this declares a dependency rather than
  enabling one. No new crate; the errno comparison uses `rustix::io::Errno::ILSEQ`.
- **API**: no status code, error `code`, `details.reason` or response shape changes, so
  `src/openapi.rs` is untouched.
- **Shipped behavior**: one change, in one rare case. Where the operating system fails part way
  through reading a group's parent directory, the request now returns `500 template_registry_io`
  where it previously answered from the entries read before the failure. That is a production
  behavior change on Linux, it is intended, and the delta requires it. Everything else about the
  shipped image is unchanged: production has `/proc`, so the platform half of this change is
  invisible there, and no test change reaches shipped code.

## Acceptance

- `cargo test` passes on a default macOS volume and stays green on Linux CI. The residue is zero.
- `cargo fmt` and `cargo clippy --all-targets --all-features -- -D warnings` pass, so
  `.workflow/run-change.sh:516` passes unmodified. Nothing edits the driver, the hooks, or the gate
  contract in `AGENTS.md` or `openspec/config.yaml`.
- Every capability-dependent test asserts the real contract on both kinds of volume, chosen by a
  runtime probe of the volume rather than by the OS.
- Exactly one test returns early without asserting, and it prints the capability and errno that
  stopped it.
- If a test resists this, the run stops and asks. Nothing is gated away, `#[ignore]`d, or
  `#[cfg(target_os)]`d to reach green.
