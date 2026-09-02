TREE_SHA256: 892e26b1ab74e8f56e46a1392cb35b8cb9f5b77438d247b9e187f9a2fcec666b

## Diff review — issue-287-container-geometry

Reviewed: `src/{raw,models,convert,templates,resolver,render/mod,reason,openapi,lib}.rs`, the nine new fixtures, against `proposal.md`, `specs/shape-paint/spec.md`, `design.md`, `tasks.md`, `ANSWERS.md` and `AGENTS.md`. Gates run: `cargo fmt --check` = 0, `cargo clippy --all-targets --all-features` = 0, `cargo test` = **101 (fail)**.

### Blocking

**1. `cargo test` is red. A new fixture cannot render.** [verified]
```
test render::tests::every_template_renders ... FAILED
panicked at src/render/mod.rs:5330: render container_circle_gated:
  422 UnsupportedLayoutItem "circle container at 'layout[0]' is not square"
```
`tests/fixtures/templates/container_circle_gated.yaml` declares `w: default 30` against `size: ["{w}", 20]`, and `every_template_renders` drives enums through `default_option_selection`, which picks the **first** value (`src/render/mod.rs:2247-2253`), i.e. `enabled: "yes"` — not the fixture's `default: "no"`. The container is therefore active at 30×20 and correctly refused. The check is right; the fixture is wrong (`default: 20` makes it square, and the HTTP test that supplies `w: 14` still exercises both the gated-off and gated-on branches). Task 7.3 is unchecked and this is why.

**2. `validate()` was switched from the instantiated layout to the raw one, silently changing load-time validation for every template with a parameter reference.** `src/templates.rs:1156-1157` now passes `&self.layout` / `self.options()` where it passed `&instantiated.layout` / `instantiated.options()`. The circle check needs the un-instantiated spelling (otherwise `source_of` sees `Literal` for `"{w}"` and `fixed_by_template` is wrongly true), so *something* was needed — but this is a blanket swap with two unspecified side effects, neither in the delta, `design.md`, `tasks.md`, nor covered by a test:
- **`font_weight: "{param}"` is no longer range-checked.** `validate_font_weight` matches only `DynamicValue::Literal` and returns `Ok` for a `Ref` (`src/templates.rs:2218-2229`). Instantiation used to fold a ref into `Literal(resolve_u16_default(..))` (`1716-1760`, `1640-1650`), so a template declaring `font_weight: "{w}"` against a param defaulting to `350` was quarantined at load and now loads.
- **Size refs now resolve through a different table.** Validation now reaches `source_of` with a `Ref` and resolves it via `load_geometry_values` (`1608-1623`) instead of `resolve_f32_default` (`1625-1638`). The two disagree: a `length` param with `max:` and no `min`/`default` yields `max` under the first and `0.0` under the second (and a numeric-string default is parsed by the second only). So `size: ["{w}", 10]` with `w: { type: length, max: 40 }` was quarantined as `size width must be greater than 0` (`precheck`, `resolver.rs:378-388`) and now loads and is bounds-checked at 40.

Either fix the scope (compute `fixed_by_template` from the raw placement while validating the instantiated layout) or state the new validation semantics in the delta and cover both cases with tests. As it stands the change quietly moves the quarantine line for templates that have nothing to do with geometry.

### Major

**3. `tasks.md` records only §1 as done.** Boxes 2.1–7.3 are unchecked while the code, tests and fixtures for all of them are in the tree. `AGENTS.md` makes a checked box a claim a later reader trusts, and archive forbids unchecked tasks — the record has to match what was actually performed (and 7.3 is currently a truthful `[ ]`, see finding 1).

**4. Five of the nine new fixtures are inert.** `container_ellipse_padded`, `container_ellipse_square`, `container_ellipse_stroked_cross`, `container_rect_rounded_corner` and `container_rect_stroked_edge` appear exactly once in the codebase each — in the expected-template-id list at `src/render/mod.rs:5297-5305`. No test renders or asserts them. `AGENTS.md`: "The nine YAML files under `tests/fixtures/templates/` are test inputs, and what makes them right is the test that reads them." Task 6.1 added them for scenarios that nothing then checks.

### Minor

**5. The byte-identity test does not test what task 6.6 asks.** `src/lib.rs:10545-10581` compares `container_default_rect` (no `shape`) against an inline copy carrying `shape: rect`. Both go through the new emitter, so it proves the default mapping and nothing about "renders identically to the same template before this key existed". No test in the suite compares against the pre-collapse `#rect` output; that claim rests on reasoning only.

**6. The draw-order test lost its assertion.** The old test found `#rect` and `child_text` and asserted `rect_idx < child_idx`; it is now two `contains` checks (`src/render/mod.rs:7683-7684`). Ordering is now Typst's (`fill_and_stroke` prepends outside the clipped group, `typst-layout-0.15.1/src/shapes.rs:664-680`), which is fine — but the paint-coverage requirement still mandates background→stroke→children and nothing now exercises it.

**7. The render check re-implements `resolver::place`.** `src/render/mod.rs:1319-1372` duplicates `place`'s `resolve` / `resolve_unmeasured` composition (`src/resolver.rs:427-446`) inline instead of calling `place`/`resolve_packed`. It is arithmetically identical today; it is also the second reading of the resolver's composition that `AGENTS.md`'s "adding a source or a bound means editing `resolver.rs` alone" exists to prevent.

**8. `docs/AUTHORING.md` §9 is now stale.** It lists the container paint keys with no `shape` (`docs/AUTHORING.md:491-517`, `639`), and describes `rounded` as rounding "the container's stroke and background" (`496-498`) — which the delta reverses: the radius now also clips children, and `rounded` is refused on `ellipse`/`circle`.

**9. No PDF assertion for the ellipse.** The delta's ellipse scenario says "this holds in PNG output and in PDF output alike"; the new tests assert PNG (`src/render/mod.rs:~995` nested-shapes case) and the Typst source string only.

### Verified as correct

`fixed_by_template` matches the delta's spelling table row for row (`src/resolver.rs:103-165`, test at `880-955`); the load check branches on that flag alone and reuses `BOUNDS_EPSILON`; the render check sits after `is_item_active`, so a gated-off circle is never measured; `#box` carries fill/stroke/radius/clip for `rect` and Typst does **not** inset a box's body for its stroke (`typst-layout-0.15.1/src/inline/box.rs:26-71`), so children keep their coordinates while `clip_rect` cuts at the stroke's inner edge as specified; `circle` emits `#ellipse`, never `#circle`; `rounded` on a round geometry, an unknown `shape`, and `shape` on a non-container are all refused with the value and accepted set; `SPECS_SHA256` in `review.md` matches the current `specs/` digest.

VERDICT: REVISE
