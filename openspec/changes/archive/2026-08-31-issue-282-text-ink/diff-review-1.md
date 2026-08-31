Reviewed the full diff (12 files, +1575/-38), the four planning artifacts, ADR-0091, and re-ran the gates myself.

## Gates (re-run in this worktree, not taken on trust)

`cargo fmt --check` exit 0, `cargo clippy --all-targets --all-features` exit 0, `cargo test` exit 0 [verified]. `.workflow/review-gate-check.sh "$PWD" src/models.rs src/render/mod.rs` exit 0, and `.workflow/specs-digest.sh` recomputes `2c98ded1…56fa3`, matching `review.md:71` [verified].

## Round-3 blocking findings: genuinely fixed

- 7.1: `DynamicValue::visit_str` (`src/models.rs:322-343`) is byte-identical to `main`; the shared `mm`/`in` fallback is restored. Pinned in both directions by `src/models.rs:1382-1397` (`"80mm"` → 80.0, `"infmm"` → `INFINITY`).
- 7.2: `deserialize_dynamic_ink` (`src/raw.rs:105-152`) resolves `{name}` then parses through `Ink::from_str` only, so `redmm` and `"#ff0000in"` stay refused (`src/raw.rs:455-467`).
- 8.1: ADR citation corrected. `PrinterCapabilities::from_parts` is at `src/driver.rs:440` and the bi-level parse at `src/driver.rs:447` [verified]; `binarize_rgba` is at `src/render/helpers.rs:18` [verified]. `design.md` carries the same corrected line.
- 8.2: the integer case now runs through the path a YAML integer takes (`src/raw.rs:462` via `deserialize_any`) and through `parse_and_validate` (`src/templates.rs:5514`), not `Ink::deserialize` alone.

## Correctness checks I ran independently

`validate_item_references` recurses into containers (`src/templates.rs:1558-1560`), so a nested bad ink reference is caught. `derive_inputs_internal`'s walk records the ink ref inside the `is_active` guard (`src/templates.rs:299-301`), so gating is inherited rather than reimplemented. `instantiate_item_defaults` cloning the ink (`src/templates.rs:1719`) is right, because that output feeds only `validate`'s geometry pass and ink changes no metric. `convert.rs:245` is the only production constructor of `LayoutItem::Text` from raw. `/api/print` reaches the batch envelope through `run_batch` (`src/api.rs:2504`) [verified], so the missing print-specific test is not a gap in the contract. Hex widening cannot overflow `u8` in any of the four digit counts (`15 * 17 = 255`, `15 << 4 = 240`). Only four integers this code produced reach the generated source (`src/render/mod.rs:1869-1875`). The PNG assertions are real and would fail if the fill were dropped: `(255,65,54)` glyphs, `(128,128,128)` for `#00000080` over white, and 0 black pixels for a yellow ink under `color_mode=bilevel` (`src/lib.rs:9500-9573`).

## Findings (all non-blocking)

1. **`ink` is documented nowhere an author reads.** `docs/SPEC.md` is frozen, and `docs/AUTHORING.md` is untouched. The precedent cuts the other way: `wrap` earned its own section (`docs/AUTHORING.md:391-397`) in commit `1e0513c`, and `flow` did too (`docs/AUTHORING.md:586`). `AGENTS.md` mandates only the ADR and its README row, both present, so this does not block, but the field ships discoverable only from `openspec/specs/text-ink/spec.md`.

2. **`DynamicValue<Ink>`'s generic `Deserialize` remains reachable and lenient.** `"redmm"` still parses to `Literal(red)` through the shared suffix branch (`src/models.rs:334-341`). Only the `deserialize_with` attribute on `src/raw.rs:231` keeps that path unused, and no test pins that a `DynamicValue<Ink>` never takes it. A second item type gaining an ink and forgetting the attribute would silently reintroduce the rewrite the `spelling` field exists to prevent.

3. **ADR number 0091 is contended.** The concurrent worktree `.worktrees/issue-280` plans `docs/adr/0091-*.md` (`issue-280-shape-paint-model/proposal.md:122`). It has not written the file, and this change has, so first-to-merge wins; whoever merges second must renumber and re-row `docs/adr/README.md`.

4. **No `diff-review.md` exists yet.** The folder holds `diff-review-1/2/3.md`, all ending `VERDICT: REVISE`. The landing gate reads `diff-review.md` with a passing verdict and differing `AUTHOR:`/`REVIEWER:`, so this review has to be recorded there before the commit that archives the change will pass `.workflow/review-gate-check.sh`.

Also outstanding by workflow order, not by defect: step 5 (`/opsx:archive` with the delta synced into `openspec/specs/text-ink/`) has not run. `openspec/specs/` currently holds no `text-ink` capability.

No blocking defect found. Every task checkbox corresponds to work I could locate, the delta is `ADDED`-only, `docs/SPEC.md` is unmodified, and the acceptance-evidence loop is correctly kept out of `tasks.md` per #220.

VERDICT: APPROVE
