TREE_SHA256: b2e46c44312dcad6c5d7ea4ff1c3fedd0bce39c2ec849d846ffe1d247f55e4b9

## Diff review — issue-319 (repetition)

**Gates I ran** `[verified]`: `cargo fmt --check` exit 0; `cargo clippy --all-targets --all-features` exit 0; `cargo test` 834 passed / 0 failed / 2 ignored, plus 2+1 in the integration binaries; `npm --prefix ui run lint` / `test` (443 passed) / `build` all exit 0. `SPECS_SHA256` in `review.md` still matches `.workflow/specs-digest.sh` output, so the approving plan verdict is intact.

**Implementation** `[verified by reading]`: correct. The eight refusals fire where the spec says, in the order it requires (`src/convert.rs:320-337` during conversion, `src/convert.rs:754` last in `TryFrom`, so no existing message moves). `expand_items` (`src/render/mod.rs:1465`) is the single expansion both the pre-pass (`src/render/mod.rs:1581`, `:1901`) and the render walk (`src/render/mod.rs:2015`) call, evaluating `when:` in the enclosing scope before binding, so the gate is evaluated once and the two counts cannot diverge. Instance paths carry `#<idx>` on every render-time failure site and nested items extend them through `render_container_item` (`src/render/mod.rs:2483-2489`). Scope binding, nesting, `MissingField` on an absent list, and the input derivation (`src/templates.rs:376-403`) all match the deltas. The prior round's blocking findings 1, 2, 3, 5 and 7 are genuinely fixed: `src/lib.rs:9614` now exercises all eight refusals over HTTP with both the byte-identical and create-only clauses, `src/templates.rs:2966` proves quarantine for all eight while the valid sibling is still served, and `src/render/mod.rs:10841` asserts real per-instance widths.

What remains is one pattern, and it is the same one that blocked last round.

## Blocking

**1. Five checked tasks assert that the render succeeded, not what it drew.** Each of these passes against an implementation producing the wrong number of instances or the wrong binding, which is exactly the failure the task exists to catch. The tool to fix them is already in this diff: `src/render/mod.rs:11016` and `:10841` build the Typst source and assert on it.

- `src/lib.rs:9044` — task 4.7 "`[]` … drawing the strip with no instances", spec scenario "An empty list draws the strip and no pills". Asserts `200` and `image/png`. An expansion that drew one blank pill for `[]` passes.
- `src/lib.rs:9142` — same task, the `default: []` half. Same assertion, same gap.
- `src/lib.rs:9093` — task 4.7 "a declared `default:` supplying the elements", scenario "two pills are drawn, reading `CONSUMABLE` and `KIDS`". Asserts `200` only, so it proves the default did not raise `MissingField` and nothing about the two instances.
- `src/lib.rs:9485` — task 4.9 "the same container under `overflow: trim` draws the first two and succeeds", scenario identical. Asserts `200` only; a trim that dropped every instance passes.
- `src/render/mod.rs:11104-11112` — task 4.11 "nested repeats over two lists draw the four combinations in order", scenario "four texts … reading `A-1`, `A-2`, `B-1` and `B-2`, in that order". It locates the first and last `Apple`/`Broccoli` and asserts their order. An implementation that leaked the outer binding, so every instance read `Fruit:`, produces `Fruit: Apple`, `Fruit: Broccoli`, `Fruit: Apple`, `Fruit: Broccoli` and satisfies every one of those five positions. The `src.contains("Fruit+Veg")` assertion above it comes from the `{cats:join('+')}` token outside the strip, so it does not cover the binding either. Asserting `"Fruit: Apple"`, `"Fruit: Broccoli"`, `"Veg: Apple"`, `"Veg: Broccoli"` in ascending position closes it.

**2. Task 3.3 is checked; the sibling case it names is untested.** The task requires the `when:` refusal to be kept "on the repeating container's own `when:`, on a sibling, and on every item outside a repeat scope", and `conditional-visibility/spec.md:122` carries a scenario for the sibling. `src/templates.rs:8010` covers the container's own `when:` (`:8908`) and the outside case (`:8885`), and nothing covers a sibling of the repeating container gating on the repeated list. The code is right (`src/templates.rs:1684-1690` extends `child_repeated` only into that container's `items`), so this is one template and one `unwrap_err`.

## Non-blocking

**3. A present non-array value silently produces zero instances.** `src/render/mod.rs:1483` uses `if let Some(elements) = val.as_array()` with no `else`, so a repeating container whose parameter holds a non-array contributes nothing and the label renders as if the list were empty. `src/templates.rs:383` does the same in the derivation. It is unreachable today `[verified]`: the render path resolves strictly, and `src/render/mod.rs:303-345` either coerces the value to an array of strings, falls back to a coerced default, or removes the key entirely (`src/render/mod.rs:652-654`), which then reaches the `MissingField` branch. So this is a dead branch rather than a live bug. It is still the shape AGENTS.md forbids ("never substitute an approximation and carry on"), and the previous round's version of this branch at least failed loudly. An `else` returning an error costs two lines and removes the question.

**4. `src/lib.rs:9161` (`render_label_repetition_auto_sizing`) is dead weight.** Task 4.8 is satisfied by `src/render/mod.rs:10841`, which asserts three distinct measured widths and finds each in the emitted markup. The HTTP test beside it renders the same three elements and asserts `200` and `image/png`, which is what 4.8 says not to do. Harmless as a smoke test; just do not read it as the 4.8 evidence.

VERDICT: REVISE
