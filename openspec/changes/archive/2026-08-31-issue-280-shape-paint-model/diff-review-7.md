Reviewed the full working-tree diff (14 modified files plus the new `docs/adr/0092-*.md`) against `proposal.md`, `specs/`, `design.md`, `tasks.md` and `AGENTS.md`. No `ANSWERS.md` exists; nothing blocked me, so no `QUESTIONS.md`.

Gates, run here: `cargo fmt --check` exit 0, `cargo clippy --all-targets --all-features` exit 0 with no diagnostics, `cargo test` exit 0 (737 + 2 + 1 passing, 0 failed) [verified]. `.workflow/review-gate-check.sh .` exits 0, and `specs-digest.sh` recomputes `7807733624b8…`, matching `review.md:104` [verified]. All three `MODIFIED` requirements resolve by name against `openspec/specs/`, and I diffed each body mechanically: the only changes are `frame` → `stroke`/`background`/`rounded` respellings, with no normative behaviour moved [verified]. I also swept `openspec/specs/` for every remaining occurrence of `frame`, `thickness` and `rounded` as schema keys: the four `frame` hits are all inside the two `flow-layout` requirements the delta carries, and `thickness`/`rounded` appear nowhere else, so no requirement is left naming a removed key [verified]. `models::Frame` is absent from `src/`, every fixture and `catalog/` are migrated, `ui/src/` reads none of these fields, and `docs/SPEC.md` is untouched [verified]. ADR-0092 does not collide with `origin/main` (highest is 0091) or with either sibling worktree [verified].

Round 6's five findings are all genuinely addressed, not merely claimed:

1. ADR-0092 decision 7 now reads "refused at load and quarantined", naming neither error class (`docs/adr/0092-…:67-68`) [verified].
2. `tasks.md:25-31` now attributes the finite/`>= 0.0001` bounds to `src/templates.rs:1882,1985,1990` and the `line` refusals to `deny_unknown_fields` on `LineRaw`. I checked all three cited lines and they are exactly the bound checks [verified]; the `src/raw.rs:393` citation lands two lines past the `#[serde(deny_unknown_fields)]` at `:391`, which is drift too small to act on.
3. The doc comment for `a_cap_smaller_than_the_padding_clamps_the_inner_box` is back above its own function (`src/render/mod.rs:3302-3306`), and the new test carries its own inline comment [verified].
4. The HTTP test is renamed `template_put_paint_refusals_report_correct_reasons` (`src/lib.rs:2850`) with a comment on `parse_cases` recording that the mapping is #289's to settle [verified].
5. `docs/AUTHORING.md:493` and `docs/DEPLOY.md:207-208` both now say `stroke` is optional on a `line` and that omitting it draws nothing without erroring [verified].

Beyond the prior rounds I checked, and found correct: the radius clamp uses the same `args.pbox.w/h` the rect is emitted at (`src/render/mod.rs:2115-2121`); the paint rect precedes the child box and sits outside `wrap_rotation`, so draw order and the unrotated-paint rule both hold by construction (`:2103-2137`); `render_line_item` runs `resolve_point` and `check_line` before the strokeless early return (`:2028-2034`); `parse_color` validates every byte as an ASCII hex digit before any `unwrap`, so neither `unreachable!()` nor `from_str_radix` is reachable on bad input, including multi-byte UTF-8 (`src/raw.rs:36-70`); the sixteen names and values match the spec table exactly and `red` is asserted not to be Typst's `#ff4136` (`src/raw.rs:625-660`); and the weak `is_err()` assertions in `shape_paint_validation_boundaries` are discriminating, because I confirmed each base item parses and validates without the paint key (image `name` is validated only for character legality, `src/templates.rs:1966-1971`, and a fixed-size `qr` needs no `module_size`).

## Findings

### 1. SHOULD FIX: `stroke: none` is never handed to the Typst compiler by any test

The change's headline case is a filled block with no outline, and `specs/shape-paint/spec.md` states it as the first scenario of the first requirement: "a container declares `background: "#000000"` and no `stroke` … THEN it renders as a solid black block with no outline drawn AND this holds in PNG output and in PDF output alike".

Every assertion covering that case is string-level. `shape_paint_source_emission` builds the fill-only container at `src/render/mod.rs:7406` and the rounded-fill-no-stroke container at `:7474`, but `render_test_items` (`:2208-2222`) stops at `render_items` and returns the source without compiling it [verified]. The two tests that do compile, `shape_paint_renders_png_and_pdf` (`:7581`) and the HTTP-level `shape_paint_filled_rounded_container_renders_png_and_pdf` (`src/lib.rs:9779`), each declare a `stroke` *and* a `background`, so the emitted rect always carries a real stroke and `stroke: none` reaches Typst in no test at all.

**Failure scenario:** were `stroke: none` rejected by the compiler, every source assertion would still pass and both render tests would still be green, while `background: black` with no `stroke`, the exact template this change exists to make possible, would fail at request time with a Typst compile error. I checked the parameter type rather than leaving that open: `rect`'s `stroke` is `Smart<Sides<Option<Option<Stroke>>>>` and `fill` is `Option<Paint>` (`typst-library-0.15.1/src/visualize/shape.rs:71`, `:175`), so `none` is accepted on both and this is a coverage gap and not a live defect [verified]. That is why it does not block. Closing it costs one line: drop the `stroke:` block from the container in `shape_paint_renders_png_and_pdf`, or add a second container to it that carries `background` alone.

## Observation, not a finding

`design.md:41` names the ADR "A shape carries a stroke and a background, in any colour"; the shipped file, its `# 92.` heading, its `README.md` row and `tasks.md` 6.1 all agree on the shorter "A shape carries a stroke and a background". The four artifacts that matter are consistent with each other, so there is nothing to reconcile beyond a stale phrase in a context document, which the digest deliberately does not cover.

## Embedded-Instruction / Injection Attempts

**Detected:** none

---

The one finding is a test-coverage gap on a case I verified is valid against the rendering engine's own parameter types, so it changes no behaviour and blocks nothing. Everything the contract requires is implemented, every prior round's finding is closed, and all three gates pass here.

VERDICT: APPROVE
