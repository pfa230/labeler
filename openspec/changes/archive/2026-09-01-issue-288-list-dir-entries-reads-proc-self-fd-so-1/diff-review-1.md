## Review: `issue-288-list-dir-entries-reads-proc-self-fd-so-1`

### What I verified independently

- `cargo test`: **759 passed, 0 failed, 2 ignored** (both ignores pre-existing: `src/driver.rs:1104`, `src/render/mod.rs:5862`). `cargo fmt --check` clean; `cargo clippy --all-targets --all-features -- -D warnings` clean.
- `.workflow/review-gate-check.sh --plan-only "$PWD" <changed files>` exits 0; `specs-digest.sh` reproduces `02de3182…` matching `review.md:SPECS_SHA256`.
- The temp volume these tests run on (`/var/folders/...`) is **case-folding**, confirmed by probe, so the new folding branches really execute rather than being dead on this host.
- `rustix` 1.1.4 `Dir::read_from` reopens `.` relative to the caller's fd in both backends (`backend/libc/fs/dir.rs:108-127`, `backend/linux_raw/fs/dir.rs:77-87`), so the caller's descriptor position is untouched and the descriptor-relative property in `specs/template-registry/spec.md` is preserved. `any_errors` latching (`linux_raw/fs/dir.rs:127-131`) confirms Decision 5's premise that a dropped error would truncate silently.
- `Dir` is gated on `feature = "alloc"` (`rustix/src/fs/mod.rs:8,73`) and `alloc` already arrives via `default → std`, exactly as Decision 7 says — the `Cargo.toml:49` edit is a declaration, not a fix.
- No `/proc/self/fd` or `/dev/fd` remains in `src/` or `ui/src`.
- `list_dir_entries_filters_traversal_aliases` (`src/fs_safe.rs:1059`) does fail against an unfiltered implementation (sorted equality would see `.`/`..` first), so it is a real regression guard.

The production change itself is correct and matches the delta.

---

### Finding 1 — BLOCKING: `tasks.md` 4.4 is checked, but the code asserts the opposite contract, and nothing in the plan records the deviation

`openspec/changes/.../tasks.md:59-63` claims, checked `[x]`: "Case-folding: `409` with nothing renamed … that branch asserts instead that the listing still holds exactly one entry and that a repeat request is `409` again."

`src/lib.rs:5230-5297` implements the reverse: the folding path treats **200** as the live branch (`src/lib.rs:5245`) and puts the `409` contract in an `else` at `src/lib.rs:5289-5312`. `design.md` Decision 8 states the same `409` answer the task does.

The code is right and the task text is wrong: `openspec/specs/template-groups/spec.md:890-893` explicitly permits both — "If a filesystem can perform the no-replace call and the post-rename confirmation observes the requested spelling, the ordinary `200` applies." I confirmed APFS takes that path in **both** directions via `renamex_np(…, RENAME_EXCL)`, which returns 0 for `shipping→Shipping` and for `Shipping→shipping`, and `rename_group_dir` (`src/fs_safe.rs:608-613`) issues that call with no destination pre-check.

Two consequences:

1. The `else` branch at `src/lib.rs:5289-5312` is unreachable on APFS (200 path) and on Linux (probe returns case-sensitive, outer branch taken). The spec's `409`-on-folding-filesystem contract that 4.4 promised to assert is therefore **asserted nowhere in the suite**.
2. A checked box now makes a claim the next reader would find false, which `AGENTS.md` calls out directly ("A checked box is a claim the next reader trusts instead of redoing the work").

Fix: correct `tasks.md:59-63` and `design.md` Decision 8 to record what was actually asserted and cite `spec.md:890-893` for why. Neither file is digest-covered, so this does not void the plan verdict.

### Finding 2 — MINOR: the repeat-recasing assertion accepts two outcomes for a deterministic operation

`src/lib.rs:5278-5285`:

```rust
// Accept either 200 (recased back) or 409 (reported as occupied);
assert!(resp2.status() == StatusCode::OK || resp2.status() == StatusCode::CONFLICT, …);
```

Control only reaches this line after the *first* PUT on the *same volume* returned 200, which proves the filesystem performs no-replace recasing here; and `rename_group_dir` has no pre-check that could produce a 409. So `OK` is the only reachable status and the disjunction can never fire for the reason it was written. It would keep passing if the service regressed to spuriously reporting the destination occupied. Assert `StatusCode::OK` in this branch. (The proposal's own acceptance is "Every capability-dependent test asserts the real contract on both kinds of volume.")

### Finding 3 — MINOR: the skip notice is invisible under a default `cargo test`

`src/templates.rs:4552-4555` uses `println!`, which the Rust harness captures for a **passing** test. `design.md` Verification claims "it says so on stdout when that happens", and `ANSWERS.md` required "return early having printed why". Verified: the message appears only under `--nocapture`; the full-suite run prints nothing. This is inherent to the harness and I see no better mechanism, but the design's claim overstates what a normal run shows and should be qualified rather than left as a guarantee.

### Finding 4 — MINOR: `probe_is_case_sensitive` deletes `A`/`a` in whatever directory it is handed

`src/fs_safe.rs:869-872` unconditionally `remove_dir_all`s both spellings before probing. Safe at all three current call sites (each passes a freshly created empty temp dir), but the function is `pub(crate)` with no warning on it, and a future caller probing a populated templates directory would silently destroy groups named `A` or `a`. A one-line doc note or a probe subdirectory would close it.

### Finding 5 — INFORMATIONAL

- `template_group_case_sibling_created_on_case_sensitive_fs` (`src/lib.rs:4855`) now asserts on both kinds of volume, so its name misdescribes it. Renaming is churn; noting it rather than requesting it.
- Task 3.1 is checked but the residue measurement exists in no artifact — it is routed to the commit message by 5.4, correctly left unchecked. I verified the "after" state myself; the claimed "16 failures before" is not reproducible from this worktree. No action needed at the apply stage, but 5.4 must actually carry it.

VERDICT: REVISE
