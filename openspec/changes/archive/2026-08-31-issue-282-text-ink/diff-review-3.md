Review complete. Gates re-run here [verified]: `cargo fmt --check` exit 0, `cargo clippy --all-targets --all-features` exit 0 with no warnings, `cargo test` 720 passed / 0 failed / 2 ignored.

## Blocking

### B1. The diff silently changes the shared `DynamicValue<T>` deserializer, and no artifact records it

`src/models.rs:337-345` adds a guard to the length-suffix fallback in `DynamicValue`'s `visit_str`: the `mm`/`in` strip now only re-parses when the remainder starts with a digit, `.`, `-` or `+`. This is a real behavior change, and it is not scoped to `Ink`. `DynamicValue<T>` is instantiated for `f32` (`src/models.rs:432` `SizeValue::Dynamic`, `src/models.rs:735-739` `FontSize`) and `u16` (`src/models.rs:986` `font_weight`), and every one of them now parses differently: `font_size: "infmm"` previously deserialized to `Literal(f32::INFINITY)` and now errors.

The narrowing itself is defensible and serves the delta's own rule (`specs/text-ink/spec.md:65`, "Anything else SHALL be refused") — without it `ink: redmm` loads as `red`. Nothing documented regresses: `docs/SPEC.md:345`'s `"80mm"` length spelling still parses, and the suite is green. The problem is the record:

- `design.md:103-107` states the opposite of what shipped: "`Option<DynamicValue<Ink>>` needs no new deserialization machinery … `ink: red` a `Literal` with no new code."
- `proposal.md:53-56` lists `src/models.rs` as "a new `Ink` colour type … `LayoutItem::Text` gains `Option<DynamicValue<Ink>>`". The shared-visitor edit is neither.
- No task covers it, and `AGENTS.md` is explicit that a behavior change produces a spec delta and an ADR in the same change. The `f32`/`u16` half of this edit sits under no delta at all.
- Its only test coverage is two ink strings (`src/templates.rs:5512` `"redmm"`, `:5513` `"#ff0000in"`). Nothing asserts the guard at the layer it lives on, so the non-`Ink` blast radius is untested in both directions — neither that `"80mm"` still parses through this branch nor that `"infmm"` now doesn't.

Round 2's N1 offered two options — a narrow `Deserialize` for `Option<DynamicValue<Ink>>`, or one sentence in the spec admitting the leniency. The implementation took a third, wider one and recorded none of it. Fix by either narrowing it to `Ink` (keeping `design.md:103` true), or keeping it and saying so in `design.md`/`tasks.md` with a test at the shared level.

## Non-blocking

### N1. Task 5.5's "survives to PDF" and "every slot" are asserted by `%PDF` magic bytes

`tasks.md:72` is checked for "an ink survives to PDF and to every slot of a multi-slot sheet". The per-slot colour assertion (`src/render/mod.rs:8515-8531`) is made against two hand-built `RenderContext`s, then `render_sheet_pages` is called and only `pdf.starts_with(b"%PDF")` is asserted (`src/render/mod.rs:8537`); the HTTP sheet test does the same (`src/lib.rs:9480`). So the sheet loop itself is never distinguished from one that hoisted the context out of the loop. It does not — `src/render/mod.rs:840` builds a fresh `RenderContext` per label [verified] — and this is the repo's existing convention for sheet content (`src/render/mod.rs:8252-8270` asserts sheet content the same way, through a separately built context), which is why I am not blocking. But round 2 raised exactly this as its blocking B1 and the change folder records no justification for the residue, which `AGENTS.md` requires. The honest alternative is to narrow the wording of 5.5.

### N2. ADR-0091 cites test code as production evidence

`docs/adr/0091-text-ink-is-a-full-colour.md:16` says "`src/driver.rs:693` advertises bi-level as a printer capability parsed at runtime". Line 693 is inside `#[cfg(test)] mod tests`, which opens at `src/driver.rs:674-675`; it is a `PrinterCapabilities::from_parts` call in an assertion. The claim is true of the production parser elsewhere in that file, but ADRs are append-only, so a wrong citation is permanent. `design.md` carries the same reference.

### N3. The spec's own listed non-string case has no test at the layer that handles it

`specs/text-ink/spec.md:70` names `ink: 16711680` among the literals that must be refused. `src/templates.rs:5509-5539` covers the five string cases and not that one. The integer path is exercised only at `src/models.rs:1381`, which calls `Ink::deserialize` directly and so bypasses `DynamicValue`'s `visit_u64` (`src/models.rs:297`) that a YAML integer actually hits. [assumption] `String::deserialize` over serde's `u64` `IntoDeserializer` errors, so it is refused — but the assertion is one layer off the mechanism.

## Confirmed correct, for the record

Round 2's fixes landed: the read-back requirement now has a test at its own layer (`src/lib.rs:9376-9405`, asserting `items[0]["ink"] == "red"` and that item 1 omits the key), and `redmm`/`#ff0000in` are refused. The 18-name table matches `design.md:95-98` and is asserted independently (`src/models.rs:1300-1329`); hex widening is arithmetically correct and cannot overflow `u8` in any of the four digit counts. Only one emission site exists (`src/render/mod.rs:1891`), and only the four integers this code produced reach the generated source. The PNG assertions are real, not magic-bytes: `(255,65,54)` glyph pixels for `ink: red`, `(128,128,128)` for `#00000080` over white, and 0 black pixels for a yellow ink under `color_mode=bilevel` (`src/lib.rs:9500-9512`) — each would fail if the fill were dropped. `check_param_ref(..., &["string","enum"])` (`src/templates.rs:1465`) and `record_ref(r, false, false, false)` (`src/templates.rs:300`) are the correct reuse, and `validate_item_references` recurses into containers (`src/templates.rs:1558-1560`) so a nested bad reference is caught. `docs/SPEC.md` is untouched, the delta is `ADDED`-only, ADR-0091 is free on `origin/main` [verified], and the UI names no layout field so none is owed.

VERDICT: REVISE
