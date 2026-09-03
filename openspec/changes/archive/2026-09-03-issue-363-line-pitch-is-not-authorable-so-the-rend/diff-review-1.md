TREE_SHA256: 2357cfe9fad206c98c5958b46d597bdf7b04ecc6c98312a8ec52d79f4b75aedd
SPECS_SHA256: 274d82f4498e37a65d580491c63391a488a7e0fb206fc21a6d900012ac0ac8a6

Checked the diff against proposal, both spec deltas, design, tasks, and AGENTS.md. All targeted tests pass [verified], full suite 862 passed 0 failed [verified], `cargo fmt --check` and `cargo clippy` clean [verified], `openspec validate --all --strict` 28 passed [verified].

Schema/validation: `src/raw.rs:207-253` presence-tracked `line_spacing` refuses explicit null via `Some(None)` in `src/convert.rs:412-418`; non-numbers become `Invalid` and are refused with the key path (`src/convert.rs:430-437`, prefixed to `layout[idx].line_spacing` via `src/convert.rs:767-768`); range check mirrored in `src/convert.rs:419-428` and `src/templates.rs:2366-2372`; closed surface preserved by `deny_unknown_fields` on all item types; read-back omits absent via `src/models.rs:1081-1082`. Load-quarantine and write-refusal tests pass.

Fitter/emitter agreement: old `leading()` and `cap_height + 0.65em` stacking deleted [verified: no matches in `src/render/helpers.rs`]; `pitch(s) = line_spacing x s` (`src/render/helpers.rs:1069-1071`), derived `pitch - cap_height` (`src/render/helpers.rs:1073-1088`), budget divisor pitch (`src/render/helpers.rs:821-827`), block-scoped `#set par(leading:)` inside the item's `#text` block (`src/render/mod.rs:2267-2273`, scope ends at `]` before `pad_block`/`align` wrapping). Agreement probe extended to 0.5/0.99/1.2/1.5 over 1-3 lines within 1% and passes; render-measured pitch tests pass.

Non-blocking notes (not land-blockers): `text_fits`/`largest_fitting_font` tuple-izing (`src/render/helpers.rs:655-693`) is gratuitous churn; `RawLineSpacing`'s `Value::Null` arm (`src/raw.rs:233`) is unreachable through `deserialize_present_typed`; `block_height_with_align_and_spacing_for_test` is dead code; `metric_block_height`'s `lines == 0 => 0.0` differs from the old `max(1)` but is unreachable (segments from `split` always non-empty, and the `emitted_count == 0` guard predates this change).

No blocking finding.

VERDICT: APPROVE
