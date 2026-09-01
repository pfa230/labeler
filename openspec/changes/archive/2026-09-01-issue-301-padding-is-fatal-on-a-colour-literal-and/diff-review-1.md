## Diff review — issue-301 (padding on a colour literal)

**Scope reviewed:** working-tree diff (8 files, +310/−31) against `proposal.md`, `specs/colour-vocabulary/spec.md`, `design.md`, `tasks.md`, `.agent-runs/issue-301.md` and `AGENTS.md`.

### Verification performed

`cargo fmt --check`, `cargo clippy --all-targets --all-features` and `cargo test` all exit 0; 764 passed, 0 failed, 2 ignored [verified]. `.workflow/review-gate-check.sh --plan-only` exits 0 and `.workflow/specs-digest.sh` recomputes `6d93e7e8…8342238`, matching the `SPECS_SHA256:` in `review.md` [verified] — the delta was not edited after the `APPROVE_WITH_CHANGES` verdict.

The core change is correct and minimal. `Color::from_str` captures `spelling = s.to_owned()` before `value = s.trim()` (`src/models.rs:894-895`) and every judging step reads `value` — empty guard, `#` test, hex walk, name table and all three error messages — while both constructors use the captured `spelling` (`src/models.rs:953`, `:975`). That single edit covers all three convert call sites and `resolve_dynamic_value_color`, which is the only colour resolver in the tree (`grep` confirms one definition, four call sites at `src/render/mod.rs:1916,2181,2246,2254`) [verified]. `resolve_dynamic_value_color` trims at `src/render/helpers.rs:234`, so the chained-reference test, the parse and the `unrecognised colour '{s}'` message all read the trimmed binding, exactly as the design requires.

The `RawColor` trap is genuinely handled. `DynamicValueVisitor::visit_str` falls through to `T::deserialize(into_deserializer(v))` with the *untrimmed* `v` (`src/models.rs:361`), so the padding survives only because `RawColor::from_str` still fails unconditionally; the rewritten comment at `src/raw.rs:34-37` now states that surviving reason, and `template_detail_readback_preserves_padded_literal_and_canonical_reference` (`src/lib.rs:2907`) would fail if the hack were deleted (the fast path would then build `RawColor("red")` and the response would report `"red"`, not `" red "`) [verified by reading `visit_str`].

All three delta requirements are `MODIFIED` against names that exist verbatim in `openspec/specs/colour-vocabulary/spec.md` (lines 11, 140, 280), and no scenario from the originals was dropped — the nine new scenarios are purely additive, and each has a corresponding test [verified]. Every acceptance bullet in the issue has one. Nothing was committed, archived or synced into `openspec/specs/`, per the apply-ends-at-implementation rule.

### Findings (all minor, none blocking)

**1. `whitespace_only_color_template_is_quarantined` passes against the pre-change code — `src/templates.rs:6623`.** Before this change `Color::from_str("   ")` already failed (no `#`, no name match → `unknown colour '   '`), so the template was already quarantined with `layout[0]` and `color` in the error. The test therefore cannot distinguish the ordering that `design.md` and `tasks.md:1.3` single out, and that the issue calls out explicitly ("Check the order of the empty-string guard against the trim"): if the `is_empty()` guard were moved back above the trim, the code would still refuse `"   "` and this test would still pass. The implementation is correct (`src/models.rs:896-898` refuses with `colour cannot be empty`); only the pin is missing. Asserting `item.error.contains("colour cannot be empty")` would close it. Non-blocking: the test satisfies task 4.7 as written, and the acceptance criterion is quarantine plus file/path/field, which it does assert.

**2. Tautological disjunction in the source-endpoint assertion — `src/lib.rs:2973`.** `source_str.contains(r#""\u0062lue""#) || source_str.contains(r#"\u0062lue"#)` — the first operand implies the second, so the check reduces to the second and the `||` buys nothing. The escape's *decoding* is proved elsewhere in the same test (the response reports `"blue"` at `src/lib.rs:2969`, and an undecoded scalar would have quarantined the template and failed the earlier `StatusCode::OK`), so this line is redundant rather than wrong.

**3. `docs/AUTHORING.md:501` is now 141 characters** where the rest of that bullet wraps at 95-100. The clause is correct and in the right place; it just wasn't reflowed. The file has 126 lines over 100 chars (tables and code), so this is convention, not a rule.

Also noted, not a finding: `padded_color_reference_loads_and_renders` (`src/render/mod.rs:8991`) asserts only `res.is_ok()` for the render half. That is what task 4.3 asked for, and this path's behaviour is unchanged by the diff, so the weak assertion is appropriate for a regression pin.

VERDICT: APPROVE
