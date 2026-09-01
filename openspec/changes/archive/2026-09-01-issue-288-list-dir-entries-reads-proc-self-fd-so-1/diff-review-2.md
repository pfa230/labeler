## Review: `issue-288-list-dir-entries-reads-proc-self-fd-so-1` (round 2)

### What I verified independently

- **Gates, all on this macOS host:** `cargo fmt --check` clean; `cargo clippy --all-targets --all-features -- -D warnings` exit 0; `cargo test` **759 passed, 0 failed, 2 ignored**. Both ignores are pre-existing (`src/driver.rs`, `src/render/mod.rs`) — the diff adds no `#[ignore]`, no `#[cfg(target_os)]`, no `#[allow(clippy::…)]`. Residue is zero, as `proposal.md` Acceptance requires. [verified]
- **The folding branches really execute here.** `mkdir A; mkdir a` under `std::env::temp_dir()` (`/var/folders/…`) fails → case-folding. So `template_group_case_sibling_created_on_case_sensitive_fs` cannot have passed via its `201`/two-groups branch, and `exact_reuse_and_sibling_creation` cannot have passed via its two-entry branch. The new `422 template_group_case_conflict` and stored-spelling assertions are live coverage, not dead code. [verified]
- **`--nocapture` run** of the five touched tests: all pass, and `load_from_dir_handles_non_utf8_paths` prints `capability: non_utf8_paths, errno: 92 EILSEQ` before returning. It is the only early return in the suite. [verified]
- **Linux CI will compile.** `Errno::ILSEQ` exists in both backends (`rustix-1.1.4/src/backend/linux_raw/io/errno.rs:349`, `libc/io/errno.rs:296`), `raw_os_error()` is `const` on both, and `Dir::read_from` is present in both (`linux_raw/fs/dir.rs:72`, `libc/fs/dir.rs:102`). [verified]
- **No `/proc/self/fd` or `/dev/fd` remains** anywhere under `src/` or `ui/src`. Five callers of `list_dir_entries` unchanged. The `.`/`..` filter and the `CStr::to_str()` drop match delta "Two kinds of entry SHALL be omitted … and no others". [verified]
- **Gate and plan artifacts:** `review-gate-check.sh --plan-only` exit 0, `specs-digest.sh` reproduces `02de3182…` matching `review.md:SPECS_SHA256`, `openspec validate` passes, and the `ADDED` requirement name collides with none of the 12 already in `openspec/specs/template-registry/spec.md`. Nothing touches `.workflow/`, hooks, `AGENTS.md` or `openspec/config.yaml`. [verified]
- **Round-1 findings all closed:** F1 — `tasks.md:59-63` and `design.md:176` now say `200`; F2 — `src/lib.rs:5278` is now `assert_eq!(resp2.status(), StatusCode::OK)`; F3 — `println!`→`eprintln!` and `design.md` Verification now qualifies visibility as `--nocapture`-only; F4 — the probe uses a dedicated `.probe_case_sensitive` subdirectory with a doc comment (`src/fs_safe.rs:863-878`). [verified]

The production change is correct and matches the delta.

---

### Finding 1 — MINOR: the folding branch of `template_group_rename_recasing` cannot fail on status

`src/lib.rs:5245` branches on the *observed* response (`if resp.status() == StatusCode::OK`) with a `409` arm at `:5284`, so on a case-folding volume the test passes whichever status comes back. `design.md:176` says "the folding branch asserts `200`", which the code does not do; the following sentence ("the `409` else-branch remains in the test as documentation") describes the shape that is actually there, so the two sentences pull against each other.

Not blocking: `openspec/specs/template-groups/spec.md:889-893` genuinely permits both outcomes depending on what the filesystem reports, and each arm asserts substantive post-conditions (exactly one entry, the expected spelling, nothing renamed). But a regression from `200` to a spurious `409` would slide into the else arm unnoticed on the only host that runs this branch. Either assert `200` outright (the volume already proved it performs `renamex_np` recasing when the first PUT returned `200`), or reword `design.md:176` so it stops claiming an assertion the code does not make.

### Finding 2 — INFORMATIONAL: `proposal.md:83` Impact is now narrower than the diff

It states "**Code**: `src/fs_safe.rs:list_dir_entries` (lines 39-68) only." The diff also adds `#[derive(Debug)]` to the public `ResolvedGroup` (`src/fs_safe.rs:14`, required by `unwrap_err()` in the new folding assertion) and the `#[cfg(test)] pub(crate) fn probe_is_case_sensitive` helper (`src/fs_safe.rs:868`). Both are benign — the derive is additive and behaviour-free, the helper is test-only — but the Impact line no longer describes the diff. `proposal.md` is not digest-covered, so correcting it is free.

### Finding 3 — INFORMATIONAL: two carry-overs from round 1, deliberately left

- `http_tests::template_group_case_sibling_created_on_case_sensitive_fs` (`src/lib.rs:4855`) now asserts on both kinds of volume, so its name misdescribes it. Renaming is churn; noting rather than requesting.
- `tasks.md:3.1` is checked, but the residue measurement exists in no artifact — it is routed to the commit message by `5.4`, correctly still unchecked, and apply ends before the commit. I measured the "after" state myself (759/0/2). The "16 failures before" figure is consistent with the counts (760 tests before, one test added → 761 = 759 + 2 ignored) but is not reproducible from this worktree. `5.4` must actually carry the before-and-after, since Linux CI cannot evidence the platform fix.

VERDICT: APPROVE
