## 1. Read entries from the descriptor

- [x] 1.1 In `Cargo.toml:49`, change the `rustix` dependency from `features = ["fs"]` to
      `features = ["fs", "alloc"]`. Per design Decision 7 this declares a dependency the code now
      has rather than enabling one: `alloc` already arrives through `default → std`, so nothing new
      compiles. Do not remove it as redundant.
- [x] 1.2 Rewrite `list_dir_entries` (`src/fs_safe.rs:39-68`) to enumerate with
      `rustix::fs::Dir::read_from(dir_fd)`, which takes the existing `BorrowedFd<'_>` unchanged
      (design Decision 1). A failure to open the directory for reading returns
      `AppError::render_failed(Reason::TemplateRegistryIo, ...)`.
- [x] 1.3 Delete the manual `openat(dir_fd, ".")` dup and its "failed to re-open directory for
      listing" error string. `Dir::read_from` performs its own independent re-open internally, so
      keeping ours makes two where one is needed (design Decision 2).
- [x] 1.4 Filter the traversal aliases `.` and `..` explicitly. `std::fs::read_dir` omitted them and
      `rustix::fs::Dir` does not, so without this the function's contract changes silently (design
      Decision 3, delta "Two kinds of entry SHALL be omitted").
- [x] 1.5 Drop an entry whose name is not valid UTF-8, by discarding it when `CStr::to_str()` fails.
      This preserves exactly what `OsString::into_string()` failing did (design Decision 4, and the
      second of the delta's two omissions).
- [x] 1.6 Return `template_registry_io` when the iterator yields an error, instead of dropping it as
      `read_dir.flatten()` did. `Dir` stops iterating after its first error, so a dropped error
      would hand the caller a silently truncated listing (design Decision 5, delta "the listing SHALL
      be complete or the operation SHALL fail").
- [x] 1.7 Confirm the result against the delta and the issue: no `/proc` or `/dev/fd` path remains
      anywhere in `src/fs_safe.rs`, no new `Reason` variant was added (design Decision 6), and all
      five callers are untouched (`check_sibling_name:76`, `classify_eexist:137`,
      `resolve_group_for_delete:414`, `resolve_group_for_rename:507`,
      `collect_subgroup_rel_paths_fd:567`).

## 2. Cover the traversal-alias filter

- [x] 2.1 Add a unit test in `src/fs_safe.rs` asserting that `list_dir_entries` over a temporary
      directory holding known entries returns exactly those names and contains neither `.` nor `..`.
      It must fail against a `Dir`-based implementation that forgets the filter, which is the
      regression worth catching (design Verification).

## 3. Measure the residue

- [x] 3.1 Run `cargo test` and record, for every test that still fails, its name and the observed
      cause. Design Decision 9 predicts twelve tests repaired by group 1 and four left that depend on
      a filesystem capability, but that analysis is static: the measurement is authoritative and
      decides the scope of group 4.

## 4. Make the capability-dependent tests ask the volume

- [x] 4.1 Add one `#[cfg(test)]` volume probe reachable from both test modules, which creates `A` and
      then `a` in the temporary directory the test already made and reports whether the second
      succeeded. One implementation, not three copies, because three copies of a filesystem probe
      drift (design Decision 8).
- [x] 4.2 Give `fs_safe::tests::exact_reuse_and_sibling_creation:889` both branches. Case-sensitive:
      `resolve_or_create_group(root, Some("warehouse"), true)` creates a distinct sibling as today.
      Case-folding: it returns the case-conflict error. Assert through `list_dir_entries`, which
      reports stored spellings, and not through `Path::is_dir()`, which is true for both spellings on
      a case-folding volume and cannot tell the branches apart (design Decision 8).
- [x] 4.3 Give `http_tests::template_group_case_sibling_created_on_case_sensitive_fs:4855` both
      branches. Case-sensitive: `201` and two groups, as today. Case-folding: `422` with
      `details.reason` `template_group_case_conflict` naming the existing group by its stored
      spelling `Warehouse`, per `openspec/specs/template-groups/spec.md:321`.
- [x] 4.4 Give `http_tests::template_group_rename_recasing:5169` both branches. Case-sensitive: `200`,
      as today. Case-folding: `200` with the spelling updated, per
      `openspec/specs/template-groups/spec.md:890-893` ("If a filesystem can perform the no-replace
      call and the post-rename confirmation observes the requested spelling, the ordinary `200`
      applies"); since the test's second phase cannot create a second directory there, that branch
      asserts instead that the listing still holds exactly one entry with the new spelling. Both
      branches assert; neither returns early. The alternative `409` path in that spec is not
      exercised on APFS (which succeeds) nor on Linux (case-sensitive branch), and is documented but
      not covered.
- [x] 4.5 Change `templates::tests::load_from_dir_handles_non_utf8_paths:4541` to attempt the
      non-UTF-8 name and return early only when creation fails with `EILSEQ`, printing the capability
      and the errno that stopped it, with the reason in a comment beside the early return. Compare
      using `rustix::io::Errno::ILSEQ` against `raw_os_error()`, not a literal, because `EILSEQ` is 92
      on macOS and 84 on Linux (design Decision 10). Not `#[cfg(target_os)]` and not `#[ignore]`.
- [x] 4.6 Apply the same treatment to any further capability-dependent failure the 3.1 measurement
      surfaced. If observed behavior contradicts `openspec/specs/template-groups/spec.md:321`, `:832`
      or `:889-893`, the code is wrong and the test must not be written to match it: fix the defect,
      or stop and ask if the fix is not small or the spec itself looks wrong (design Decision 11).

## 5. Gates

- [x] 5.1 Run `cargo fmt`.
- [x] 5.2 Run `cargo clippy --all-targets --all-features -- -D warnings` and fix the root cause of
      anything it reports. Never silence a lint with `#[allow(clippy::...)]`.
- [x] 5.3 Run `cargo test` and confirm the residue is zero on a default macOS volume. Nothing is
      `#[ignore]`d, `#[cfg(target_os)]`d or otherwise gated away to reach green, and nothing edits
      `.workflow/run-change.sh`, the hooks, or the gate contract in `AGENTS.md` or
      `openspec/config.yaml`. If a test resists this, stop and ask.
