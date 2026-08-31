# Diff review — issue-282-text-ink (round 2)

**Scope:** full working-tree diff (12 files, +1306/−38), `docs/adr/0091-text-ink-is-a-full-colour.md`, against `proposal.md` / `specs/text-ink/spec.md` / `design.md` / `tasks.md` and `AGENTS.md`.

**Gates, re-run here [verified]:** `cargo fmt --check` exit 0, `cargo clippy --all-targets --all-features` exit 0 with 0 warnings, `cargo test` 719 passed / 0 failed / 2 ignored.

**Round-1 findings, re-checked:** B1 fixed (hand-written `PartialSchema` at `src/models.rs:836-845`, `Ink` registered at `src/openapi.rs:132`, asserted at `src/lib.rs:6221-6237` including the absence of the squatted `String` component). B3 fixed (`docs/adr/0091...:29` now attributes thresholding to `binarize_rgba` at `src/render/helpers.rs:18` and drops the false CSS-provenance claim; the table is stated as pinned to Typst's values). N1 fixed (assertions now `contains("layout[0]") && contains("invalid ink 'chartreuse'")`, `src/templates.rs:5527`, `src/lib.rs:2775`). N2 fixed (`src/lib.rs:9397-9432` asserts `(255,65,54)` glyph pixels and the `(128,128,128)` alpha composite). N3 fixed (`src/render/helpers.rs:230` now says "parameter was not supplied"). **B2 is half fixed:** the bilevel assertion is now real, the sheet assertion is not.

---

## Blocking

### B1. The "every slot" and "survives to PDF" claims of task 5.5 are still asserted by nothing

`tasks.md:72` is checked: "Test that an ink survives to PDF and to every slot of a multi-slot sheet". Two tests exist for it and both assert only the PDF magic bytes:

- `src/lib.rs:9439` `ink_multi_slot_sheet_and_bilevel_rendering`, sheet half: builds a two-slot sheet with `ink: "{color}"` and labels `red` / `navy`, then `assert!(pdf.starts_with(b"%PDF"))` (`src/lib.rs:9483`) and nothing more.
- `src/render/mod.rs:8430` `sheet_multi_slot_ink_rendering`: the same template, the same single assertion (`src/render/mod.rs:8468`).

Neither can distinguish "both slots carry their own colour" from "the fill was dropped", from "both slots got slot 1's data". That last one is a live failure mode, not a hypothetical: `render_sheet_pages` resolves parameters and calls `render_items` once per label inside the loop at `src/render/mod.rs:812-872`, so a per-label ink is exactly the kind of thing a shared-context bug would flatten, and the spec scenario "A batched sheet carries the colour in every slot" (`specs/text-ink/spec.md:248`) exists to catch it. Likewise the scenario "The colour survives both formats" (`specs/text-ink/spec.md:245`): the PNG half is now asserted at `src/lib.rs:9410`, the PDF half is asserted nowhere (`src/lib.rs:9357`, the white-ink test, is magic-bytes only, which is correct for task 5.4's weaker claim).

Round 1 raised this as B2 and reported the rasterised evidence that the assertion is available `((255,65,54)` in slot 1, `(0,31,63)` in slot 2). Nothing in the change folder records a justification for leaving it, which `AGENTS.md` requires ("addresses every meaningful finding, or justifies with file:line evidence why it is not one"), and `AGENTS.md` is explicit that a checked box is a claim the next reader trusts instead of redoing the work.

The cheap fix stays inside the repo's conventions: the sheet path routes through `RenderContext::render_items`, so a per-label source assertion in the style of `emitted_typst_source_ink_fill_and_omission` (`src/render/mod.rs:8290`) pins both slots without touching the PDF bytes. The honest alternative is to uncheck the sheet and PDF halves of 5.5 and drop the corresponding claim.

Note the mitigation, so the severity is read right: this is a coverage and claim-integrity defect, not a demonstrated functional bug. `render_text_item` is shared by every path and its `fill_arg` emission is asserted at `src/render/mod.rs:8298`, so an unconditionally dropped fill would still be caught.

---

## Non-blocking

### N1. `ink: redmm` and `ink: "#ff0000in"` load, against the closed vocabulary

`specs/text-ink/spec.md:65` says "Anything else SHALL be refused: an unrecognised name, a hex string without its `#`, a hex string of any other digit count, a non-string YAML value, and the empty string alike." `DynamicValue`'s shared visitor defeats that for a suffixed string: `src/models.rs:321-340` tries `trimmed.parse::<T>()`, and on failure strips a trailing `mm` or `in` and parses again. So `redmm` fails `Ink::from_str` (`src/models.rs:858`), then parses as `red` and loads; `#0074d9in` does the same. Worse for the design's own goal at `design.md:80-84`, the stored `spelling` is the *stripped* string, so `GET /templates/{id}` reports `red` for a template that says `redmm` — the silent rewrite the spelling field exists to prevent.

This is inherited machinery that `font_weight` and the size vocabulary already live with, and the design explicitly chose to reuse it (`design.md:103-108`), so I am not calling it a blocker. It is worth either a narrow `Deserialize` for `Option<DynamicValue<Ink>>` that does not take the length-unit branch, or one sentence in the spec admitting the leniency, per the exceptions rule in `AGENTS.md` that a surviving exception lives next to the rule it bends.

### N2. Two tests, one assertion, no added coverage

`src/render/mod.rs:8430` and the sheet half of `src/lib.rs:9439` build the same template, render the same two labels and assert the same `%PDF` prefix at two layers. Whichever assertion B1 gets, one of these should carry it and the other should go; the current pair reads as coverage it is not.

### N3. The read-back requirement has no test at its own layer

`specs/text-ink/spec.md:23` requires that a template read back through the template API reports the declared `ink` and omits the key when absent. The mechanism is right (`src/models.rs:978-979`, `skip_serializing_if`), and `src/models.rs:1332` round-trips `Ink` through serde, but nothing exercises `GET /api/templates/{id}` for it, which is the layer the requirement names and the one the UI reads.

---

## Confirmed correct, for the record

The name table matches `design.md:95-98` byte for byte and is asserted independently (`src/models.rs:1296-1325`); hex widening is arithmetically right in all four digit counts and cannot overflow `u8`. `instantiate_item_defaults` cloning `ink` rather than resolving a default (`src/templates.rs:1719`) is correct, since that function only feeds `validate`'s geometry pass and ink changes no metric. Gated-off items never reach ink resolution, because `render_items` iterates `active_items` (`src/render/mod.rs:1719`), so a `when`-gated ink reference cannot provoke a spurious `400` for a parameter `derive_inputs` never asked for. `check_param_ref(..., &["string","enum"])` (`src/templates.rs:1465`), `record_ref(r, false, false, false)` (`src/templates.rs:300`) and the interpolated-wins merge are the correct reuse, each with a test that fails for the right reason. The `/api/print` path needs no separate test: it routes through `run_batch` (`src/api.rs:2504`). The delta is `ADDED`-only, `docs/SPEC.md` is untouched, `SPECS_SHA256` in `review.md:71` still matches `specs/` [verified], ADR-0091 is free on `origin/main`, and no `ui/src/` change is owed since the UI names no layout field.

VERDICT: REVISE
