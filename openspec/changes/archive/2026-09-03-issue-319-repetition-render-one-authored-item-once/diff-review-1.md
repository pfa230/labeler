TREE_SHA256: 45f298a8fa748caf3a6cb408f1d643c824d642902d0bccfffaf5350b38424017

# Diff review — issue-319 (repetition)

**Gates I ran** `[verified]`: `cargo fmt --check` 0, `cargo clippy --all-targets --all-features` 0, `cargo test` 829+2+1 passed / 0 failed, `npm --prefix ui run lint` / `test` (443) / `build` all pass.

**Behavior I exercised against a running server** (`LABELER_CONFIG_DIR=/tmp/labeler-r319`, port 8137) `[verified]`: all eight refusals return `422 TemplateInvalid` / `template_parse_failed` with the paths the spec names; quarantine works (`reload` → `broken_count: 1`, other five still served); element order, sibling order, declared `default:` elements, `{tags:join('+')}` outside the strip, nested `A-1 A-2 B-1 B-2`, extent-less container (1 element renders, 2 → `item_out_of_frame` at `layout[0].items[0]#1`), `overflow: trim` (2 pills drawn), `wrap: true` (third on a second line) all behave as `specs/repetition/spec.md` requires. **The implementation is correct.** The problem is what the diff claims to have tested.

## Blocking

**1. Task 2.8 is checked; one of eight HTTP refusals is tested, and neither of its other two clauses.**
`tasks.md` 2.8 requires a `PUT` test for *each* of the eight, plus "an existing template at that id is left byte-for-byte unchanged" and "a create-only write creates no file". `src/lib.rs:9435-9469` tests one (`repeat` on `text`) and asserts neither clause. The `repetition` spec scenario "Every refusal a repeat brings into existence reports the same reason" demands all eight plus both clauses. The unit tests at `src/convert.rs:1597-1748` are the layer below the status code, which AGENTS.md names explicitly: "a task saying to add an HTTP test is not satisfied by a unit test one layer below the status code".

**2. Task 2.7 is checked; no quarantine test exists.** It requires each of the eight to be shown quarantining the file while the service still starts and serves every other template. Nothing in the diff loads a template from disk.

**3. Task 4.8 is checked, and its test asserts precisely what the task forbids.** 4.8 says "asserting each instance's drawn geometry **rather than that a PNG came back**". `src/lib.rs:9042-9093` renders two elements and asserts `status == OK` (line 9092) and nothing else. It passes against a renderer that draws one instance, or that gives both instances the same extent — the two failures it exists to catch.

**4. Tasks 4.7, 4.9, 4.10, 4.11 are checked with the named assertions absent.**
- 4.7 (`src/lib.rs:8972-9040`, `9156-9233`): order not asserted (only 200 for `["A","B","C"]`); `default:`-supplied elements untested; siblings-keep-their-places untested; "gate evaluated once rather than per element" is two 200s.
- 4.9 (`src/lib.rs:9324-9341`): `overflow: trim` and `wrap: true` untested; the overflow test asserts `error.code` and the path but not `details.reason == item_out_of_frame`, which both the task and the scenario name.
- 4.10: no test at all, for the scenario the proposal singles out as "exactly the case worth a scenario rather than a discovery".
- 4.11 (`src/lib.rs:9152`): joined-outside-the-strip untested; nested repeats asserted as 200, not as `A-1, A-2, B-1, B-2` in order.

Also unasserted under 5.4: "a `when:` read stays `interpolated: false`" (`src/lib.rs:9474-9556` checks `extra`'s presence, not its flag).

## Non-blocking

**5. The new tests' task references are systematically wrong.** `src/convert.rs:1615` labels the null-repeat test 1.5 (it is 1.3); `:1630`/`:1642` label the flow-parent tests 1.6 (1.4); `:1660` labels undeclared 2.7 (2.2); `:1675` labels wrong-type 2.8 (2.3); `:1702`, `:1715`, `:1720`, `:1732` all say 2.9 (2.4/2.5/2.6). `src/lib.rs:9100` says 4.9 (4.11), `:9160` says 4.10 (4.7), `:9238`/`:9288` say 4.11 (4.7/4.6). `src/templates.rs:8207` says 5.5 (5.4). A reader checking a box lands on the wrong test.

**6. `src/render/mod.rs:1497-1499` reports a present-but-non-array value as `MissingField`.** Unreachable through the API today — coercion returns `400 InvalidRequest` first `[verified]` — so it is a dead branch that would name the wrong fault if it ever became reachable.

**7. `src/templates.rs:2160` traded an exhaustive `Container` destructure for `..`.** Nothing here needs `repeat`, so it is not wrong today, but the compile-time prompt that made the next container field a decision is gone.

VERDICT: REVISE
