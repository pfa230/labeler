## Context

See `proposal.md` for motivation. The constraints that shape the approach:

`src/fs_safe.rs` exists to make every resolution of and write to a group directory fd-relative
(`openat`, `renameat`, `statat`), so a path cannot be swapped underneath the service between the
check and the use. `list_dir_entries` is the one hole in that: it re-opens the directory with
`openat`, then throws the descriptor away by formatting it into `/proc/self/fd/<n>` and calling
`std::fs::read_dir` on the resulting path. The `/proc` indirection was the mechanism for turning a
descriptor back into something `std::fs` could read; it was never the goal.

The function has five callers, all inside `src/fs_safe.rs`: `check_sibling_name:76`,
`classify_eexist:137`, `resolve_group_for_delete:414`, `resolve_group_for_rename:507`, and
`collect_subgroup_rel_paths_fd:567`. Each consumes a `Vec<String>` of entry names. None changes.

The registry's own tree walks are a separate mechanism and are not touched:
`templates.rs:collect_dir_entries:592` and `collect_group_paths:694` enumerate by path with
`std::fs::read_dir`, and `GET /api/template-groups` reaches the second of them through
`api.rs:435`. They never used `/proc`, so they neither fail on macOS today nor change here. That
they are not descriptor-relative is a pre-existing property of read-only walks, out of scope for
#288, and the delta says so rather than claiming a guarantee the code does not provide.

The second half of the change touches no service code at all. Four tests assert behavior that
depends on a filesystem capability rather than on the service: three need a case-sensitive volume,
one needs a filesystem that accepts a non-UTF-8 filename. They are in this change because
`.workflow/run-change.sh:516` runs `cargo test` locally and neither half can reach a green suite
alone. The constraint that shapes their repair is that `openspec/specs/template-groups/spec.md`
already specifies what a case-folding filesystem must produce, so the branches being added are
transcribed from it rather than invented, and no spec delta is owed.

## Goals / Non-Goals

**Goals:**

- One enumeration path for group resolution that works wherever the crate builds, with no platform
  branch.
- Keep the descriptor-relative property `openat` was introduced for.
- Keep every caller's input identical, so the change is confined to one function body.
- A `cargo test` that is green on a default macOS volume and on Linux CI, with no test passing by
  not running.

**Non-Goals:**

- Detecting or reporting an unsupported platform. Deleting the dependency removes the case.
- Changing how `list_dir_entries` handles a non-UTF-8 entry name (see Decision 4). This is a
  different question from the test at Decision 10, which is about a filename the OS refuses to create.
- Making the registry's path-based tree walks descriptor-relative.
- Gating, `#[ignore]`-ing or `#[cfg(target_os)]`-ing any test to reach a green suite. Adapting the
  capability-dependent tests so they assert the real contract on the volume they run on *is* the
  work (Decisions 8 to 11); making one pass by not running is not. Exactly one test may return early
  without asserting, and Decision 10 states which and why.

## Decisions

### 1. Read entries from the descriptor with `rustix::fs::Dir::read_from`

`Dir::read_from(fd)` enumerates a directory straight from an open descriptor, which is what the
`/proc/self/fd/<n>` path was faking. Verified present in `rustix` 1.1.4 in both backends the crate
selects between: `src/backend/libc/fs/dir.rs:102` (macOS) and `src/backend/linux_raw/fs/dir.rs:72`
(Linux). It takes `Fd: AsFd`, so it accepts the existing `BorrowedFd<'_>` parameter unchanged, and
`Dir` is an `Iterator<Item = io::Result<DirEntry>>`.

Alternatives considered:

- **A `cfg`-picked `/dev/fd/<n>` prefix on macOS.** Rejected in #288 and the reasoning holds: it is
  a second code path for one behavior, a third platform needs a third arm, and whether `/dev/fd/<n>`
  resolves identically for a *directory* on macOS is the assumption under test. Deleting an
  assumption beats adding one.
- **`std::fs::read_dir` on the absolute path.** Reopens the TOCTOU window the descriptor closes: the
  path could name a different directory by the time it is read. This is what the registry's
  read-only walks do, and it is why they are excluded from the delta rather than swept into it.
- **`openat` plus raw `getdents64`.** Platform-specific syscall plumbing, which is what depending on
  `rustix` already buys.

### 2. Drop the manual `openat(dir_fd, ".")`

The current code dups the descriptor before building the `/proc` path. `Dir::read_from` performs its
own independent re-open internally (`fcntl_getfl`, then `openat(fd, ".", flags | CLOEXEC)`),
deliberately, so that `fdopendir` does not share file-description state with a descriptor the caller
may still hold. Keeping our dup would make two re-opens where one is needed. It goes, and with it the
"failed to re-open directory for listing" error string.

### 3. Filter `.` and `..` explicitly, in the code and not in the spec

`std::fs::read_dir` omits the traversal aliases. `rustix::fs::Dir` yields raw `readdir` results and
omits nothing; neither backend filters them. Without an explicit filter, the function's contract
changes under a change advertised as behavior-preserving.

The delta states the omission, as one of the two the listing makes, so that "complete" cannot be
read as "every name the directory holds". It has no externally observable consequence today: every
segment compared against the listing has passed `validate_group_segment`, which rejects `.` and `..`
at `src/templates.rs:1265`, and `collect_subgroup_rel_paths_fd` drops dot-prefixed names at
`src/fs_safe.rs:568`. It is a latent trap for the next caller rather than a live bug, which is why
it is also guarded by a test (see Verification) rather than left to the contract alone.

### 4. Keep omitting entry names that are not valid UTF-8

`std::fs::DirEntry::file_name()` returns an `OsString` and the current code drops the entry when
`into_string()` fails. `rustix::fs::DirEntry::file_name()` returns a `&CStr`; dropping the entry when
`CStr::to_str()` fails preserves that exactly.

The delta states this omission too, as the second of the two, for the same reason as Decision 3: it
bounds what completeness claims. It is unobservable through these five callers, since a group name
arrives over HTTP as a decoded `String`, so no request can name an entry whose stored name is not
valid UTF-8 and no listed comparison can turn on one. It is in the contract to bound completeness,
not because a caller can see it.

Changing it was considered and rejected as out of scope: reporting such an entry rather than
omitting it would alter production behavior on Linux, which #288 does not ask to touch. Worth
recording for whoever does touch it: the two enumeration mechanisms already disagree, since
`templates.rs:collect_dir_entries` reports a non-UTF-8 *file* name as `broken` with "not valid
UTF-8" while this listing silently omits it. That asymmetry predates this change.

### 5. Propagate a read error instead of dropping it

The current loop is `read_dir.flatten()`, which discards a per-entry error and keeps going.
`Dir` cannot keep going: `read()` sets an internal `any_errors` flag and returns `None` on every
later call (`src/backend/libc/fs/dir.rs:120-124`), so ignoring an error would hand the caller a
silently truncated listing. A caller cannot distinguish a name missing from the directory from a
name missing from a truncated listing, and answers "no such group" or "no case conflict" on that
evidence.

So an error from the iterator returns `template_registry_io`. Unlike Decisions 3 and 4 this *is*
observable, on Linux and in production, which is why it is in the delta and called out in the
proposal's Impact rather than filed as an implementation detail. It makes a silent failure loud, and
the case is rare: it needs the operating system to fail part way through reading a directory the
service has already opened.

### 6. No new `Reason` variant

The unsupported-platform error #288 originally contemplated has nothing left to report once the
dependency is gone. A `Reason` no code path can produce is dead contract.

### 7. Add `"alloc"` to the `rustix` features even though it is already enabled

#288 states `Cargo.toml:49` must go from `features = ["fs"]` to `features = ["fs", "alloc"]` because
`Dir` is gated on `alloc` (`rustix/src/fs/mod.rs:8`, `:73`). The gate is real, but the premise that
the build needs the edit is not: `rustix`'s `std` feature enables `alloc`
(`rustix/Cargo.toml:130-135`), `default = ["std"]`, and `Cargo.toml:49` does not set
`default-features = false`. `cargo tree -e features -i rustix` on this branch shows `alloc` already
enabled through `default → std`. `Dir` compiles today with no manifest change.

Make the edit anyway. It declares a feature this crate's code now genuinely requires rather than
inheriting it by accident from `default`, and because features are additive and `alloc` is already
on, it compiles nothing new. Recorded here so that neither the implementer nor the reviewer treats
the edit as load-bearing, or "corrects" the manifest by removing it.

### 8. Ask the volume, do not predict it

The three case-dependent tests hard-code the answer a case-sensitive filesystem gives. That is the
thing `openspec/specs/template-groups/spec.md:321` forbids the *service* from doing: the case answer
"SHALL be answered by the filesystem holding that directory, and SHALL NOT be predicted". A test that
predicts it asserts a contract the spec does not make.

Each of the three probes the temporary directory it already creates - `mkdir A`, then `mkdir a`, and
whether the second succeeds - and asserts the branch for what it found. Both branches assert; neither
returns early. The probe is one `#[cfg(test)] pub(crate) fn` reachable from both test modules rather
than three copies, because three copies of a filesystem probe drift.

The case-folding branch is not invented here. Every answer it asserts is already normative:

- **Creating a case-only sibling** (`http_tests::template_group_case_sibling_created_on_case_sensitive_fs:4855`,
  and `resolve_or_create_group` inside `fs_safe::tests::exact_reuse_and_sibling_creation:889`).
  Case-sensitive: `201`, two groups, as today. Case-folding: the state machine at `spec.md:321` runs
  list, exclusive create, `EEXIST`, re-list, no-follow resolve, finds a directory alias, so
  `422` with `details.reason` `template_group_case_conflict` naming the existing group by its stored
  spelling `Warehouse`.
- **Recasing a group** (`http_tests::template_group_rename_recasing:5169`). Case-sensitive: `200`,
  as today. Case-folding: `spec.md:890-893` permits both outcomes and APFS takes the `200` path
  ("If a filesystem can perform the no-replace call and the post-rename confirmation observes the
  requested spelling, the ordinary `200` applies"), which the implementation does via
  `renamex_np(…, RENAME_EXCL)` returning 0 for `shipping→Shipping` and for the reverse. The
  alternative `409` path ("If a case-folding filesystem aliases the two spellings and reports the
  destination as existing, the response SHALL be `409` and nothing SHALL be renamed") is not
  exercised on APFS (which succeeds) nor on Linux (which takes the case-sensitive outer branch), so
  the folding branch asserts `200` and that the listing still holds exactly one entry with the new
  spelling. The `409` else-branch remains in the test as documentation of that spec path but is not
  covered on either host.

One trap to avoid in the unit test: `src/fs_safe.rs:906-907` asserts both spellings with
`Path::is_dir()`, which is true for both on a case-folding volume because the path resolves through
the alias. It cannot tell the branches apart. Both branches must assert through `list_dir_entries`,
which reports stored spellings: two entries on a case-sensitive volume, one on a case-folding one.

### 9. Three tests need this, and two near-misses do not

Determined by scanning every one of the sixteen failing tests for path strings that are equal under
`to_lowercase` but not equal, then reading each hit. Three are real:
`fs_safe::tests::exact_reuse_and_sibling_creation`,
`http_tests::template_group_case_sibling_created_on_case_sensitive_fs`,
`http_tests::template_group_rename_recasing`.

Two hits are false positives, recorded so they are not raised again:

- `http_tests::template_group_rename_success_paths` pairs `Euro` with `euro`, but `Euro` is a group
  directory and `euro.yaml` is a template file inside it (`src/lib.rs:4891`, `:4968`). Different
  names, not two spellings of one, so they coexist on a case-folding volume.
- `http_tests::template_groups_list_and_delete_endpoint` pairs `/api/template-groups/Warehouse` with
  `/api/template-groups/warehouse`, but the lowercase one is a `DELETE` expecting `404`
  (`src/lib.rs:3733-3745`). `spec.md:1088` requires exactly that on both kinds of volume: a segment
  matching no entry byte for byte is `404` "even where the filesystem would have opened a
  case-variant". The test already asserts the folding-safe answer.

The remaining eleven carry no case-only pair at all and are pure `/proc`. This analysis is static, so
it is a prediction: the acceptance is a measured zero residue, and anything else that surfaces after
the `/proc` fix gets the Decision 8 treatment or stops the run.

### 10. The one test that may return early, and why it is not a `cfg`

`templates::tests::load_from_dir_handles_non_utf8_paths:4541` writes a file named
`non_utf8_\xff.yaml`. APFS refuses the name with `EILSEQ` on every volume it offers, case-sensitive
included, so the state under test cannot be constructed and there is no macOS behavior to assert.
Verified directly: creating such a name under `/tmp` fails with errno 92, and the test panics in
`std::fs::write` at `:4547` before reaching any listing code.

It attempts the name and returns early only when creation fails with `EILSEQ`, printing the
capability and the errno that stopped it, with the proof in a comment beside the early return. Not
`#[cfg(target_os = "macos")]`, which would also skip it on a case-sensitive macOS volume where the
name is equally impossible, and would keep skipping it if APFS ever accepted one. Not `#[ignore]`,
which would stop it running on Linux CI, the only place it can run at all.

The errno comparison is `rustix::io::Errno::ILSEQ` against `raw_os_error()`. `rustix` is already a
dependency and defines `ILSEQ` in both backends, so no crate is added and no constant is hard-coded:
`EILSEQ` is 92 on macOS and 84 on Linux, and a literal would be wrong on one of them.

### 11. A contradiction between the code and the spec is a bug, not a fact to assert

The case-folding branches above are written from `openspec/specs/template-groups/spec.md`, which is
normative, not from whatever macOS currently returns. If the observed behavior disagrees with
`:321`, `:832` or `:889-893`, the code is wrong and the test must not be written to match it: a test
that asserts a spec violation makes the violation permanent and invisible.

Fixing such a defect is inside this change, because the acceptance is a zero residue and the tests
that would have caught it are the ones being repaired. If the fix is not small, or if the spec itself
turns out to be wrong, the run stops and asks rather than encoding either answer.

## Verification

What each newly specified or newly constrained behavior is checked by, and where nothing checks it.

- **Listing without a pseudo-filesystem** (delta scenarios 1 and 2). On macOS the twelve tests that
  fail today solely on `/proc` are the regression evidence, and the implementer records the measured
  before-and-after in the commit message. On Linux the behavior is identical before and after, so CI
  cannot observe this at all. No new test: a test cannot take `/proc` away from the host it runs on.
- **Stored spelling drives the case decision** (delta scenario 3). Covered by
  `fs_safe::tests::exact_reuse_and_sibling_creation` and
  `http_tests::template_group_case_sibling_created_on_case_sensitive_fs`, both of which gain a
  case-folding branch under Decision 8, so each asserts on both kinds of volume instead of on one.
- **The already-normative case-folding answers** (`template-groups/spec.md:321`, `:832`,
  `:889-893`). Asserted for the first time, by those two tests and by
  `http_tests::template_group_rename_recasing`. Linux CI keeps exercising only the case-sensitive
  branch; the macOS developer volume is the only place the folding branch runs, which is the point of
  probing rather than `cfg`-ing.
- **A non-UTF-8 filename is refused as broken** (`templates::tests::load_from_dir_handles_non_utf8_paths`).
  Runs and asserts on Linux CI exactly as today. On APFS it returns early after printing the
  capability and errno to stderr, per Decision 10. This is the one place in the suite where a pass
  does not mean an assertion ran, and it prints why when run with `--nocapture` (the harness
  captures stdout/stderr for passing tests, so the message is invisible in a default `cargo test`
  run).
- **Traversal aliases are filtered** (Decision 3). **New unit test** in `src/fs_safe.rs`:
  `list_dir_entries` over a temporary directory holding known entries returns exactly those names and
  contains neither `.` nor `..`. Deterministic and portable, and it fails against a `Dir`-based
  implementation that forgets the filter, which is the regression worth catching.
- **A truncated listing is never answered from** (delta scenarios 4 and 5, Decision 5). The
  read-at-all failure is reachable and is covered by making the parent unreadable. The *mid-iteration*
  failure has **no test**: there is no portable, deterministic way to make `readdir` fail part way
  through a directory without a fault-injection layer this repository does not have, and adding one
  for a single branch is not worth its weight. Recorded as an honest gap rather than claimed. The
  branch is three lines and is exercised by inspection.
- **Non-UTF-8 names stay omitted from *this* listing** (Decision 4). **No new test**, and note it is
  a different thing from the bullet above: `load_from_dir_handles_non_utf8_paths` exercises the
  registry's path-based walk, not `list_dir_entries`. The name cannot be created on macOS at all, and
  behavior is preserved by construction, since `CStr::to_str()` failing drops the entry exactly as
  `OsString::into_string()` failing did.

## Risks / Trade-offs

- **A directory removed between `openat` and enumeration answers differently per platform.** The
  libc backend treats `openat(".")` returning `ENOENT` as an empty directory
  (`src/backend/libc/fs/dir.rs:117-121`); the linux_raw backend propagates the error
  (`src/backend/linux_raw/fs/dir.rs:79`). So on Linux this stays a `500 template_registry_io`,
  matching today's `/proc` behavior, and on macOS it becomes an empty listing, which a caller turns
  into `404`. → Accept. Linux is production and its behavior is unchanged; the divergence needs a
  concurrent `rmdir` inside the window, and `404` for a directory that has genuinely gone is not a
  wrong answer. Recorded rather than papered over.
- **The change is invisible to the gate on the platform it fixes.** CI is ubuntu, where the
  before-and-after behavior is identical, so a green CI run is not evidence the macOS failure is
  gone. → The implementer runs `cargo test` on macOS and records the measured residue, which the
  acceptance requires to be zero, in the commit message.
- **The static analysis behind Decision 9 could be wrong, and a thirteenth `/proc` test could turn
  out to need a case branch.** → The acceptance is a measured zero residue, not a predicted one. The
  implementer applies the `/proc` fix, runs `cargo test`, and treats whatever remains under
  Decision 8. Nothing in the plan depends on the count being twelve.
- **A probing test can assert the wrong branch on a volume that behaves unexpectedly**, for instance
  a case-preserving volume that folds only some names. → Accept. The probe asks the specific
  directory the test is about to use, which is the same directory the service will ask, so the test
  and the service reach the same conclusion by the same means. That is exactly what
  `spec.md:321` requires of the service.
- **The case-folding branches run only on a developer machine.** Linux CI takes the case-sensitive
  branch every time, so a regression in the folding branch is caught only by whoever runs the suite
  on macOS. → Accept, and it is still strictly better than today, where the folding branch is not
  asserted anywhere at all and the test simply fails.

## Migration Plan

No data, no configuration, no API surface, no deployment step. Rollback is reverting the commit.

Deployment carries one behavior change, per the proposal's Impact: a request that encounters a
mid-iteration read failure while listing a group's parent directory may now return
`500 template_registry_io` where it previously returned an answer derived from a truncated listing.
That is a request which succeeds today and fails after this change, so the change is visible in
production even though it needs an OS-level read failure to reach. No operator action is needed. The
platform half of the change is invisible in production, which runs on Debian and has `/proc`.
