Reviewed the full working-tree diff (14 modified files plus new `docs/adr/0092-*.md`) against `proposal.md`, `specs/`, `design.md`, `tasks.md` and `AGENTS.md`. No `ANSWERS.md` exists; nothing blocked me, so no `QUESTIONS.md`.

Gates, run here: `cargo fmt --check` clean, `cargo clippy --all-targets --all-features` clean, `cargo test` 736 + 2 + 1 passing, 0 failed [verified]. `.workflow/review-gate-check.sh .` exits 0 and `SPECS_SHA256` recomputes to `006e8ab5…`, matching `review.md` [verified]. The three `MODIFIED` deltas resolve by name and are pure `frame` → `stroke`/`background`/`rounded` respellings [verified].

Round 4's findings 2 through 5 are genuinely fixed: the bound checks now live only in `src/templates.rs:1882,1985,1990` (the `convert.rs` duplicates are gone), `shape_paint_source_emission` gained the draw-order assertion at `src/render/mod.rs:7538-7545`, `docs/AUTHORING.md:495-498` now states the `rounded` floor and that `rounded: 0` is refused, and `impl Default for Color` is gone. `models::Frame` is fully absent from `src/`, every fixture and `catalog/` are migrated, and `ui/src/` reads none of these fields [verified].

## Findings

### 1. BLOCKING: a strokeless `line` skips its render-time endpoint resolution and bounds checking

`src/render/mod.rs:1762-1766` guards the entire call to `render_line_item` behind `if let Some(stroke)`. `render_line_item` is the **only** caller of `self.resolve_point` and `self.check_line` for a line (`src/render/mod.rs:2031-2033`), and `check_line` has exactly one call site (`grep -n "check_line" src/render/mod.rs` → `:1202` definition, `:2033` call) [verified]. The measurement pass at `src/render/mod.rs:1348` returns a bare axis requirement and raises nothing.

So a `line` with no `stroke`, a spelling this change newly accepts (`src/convert.rs:356-359`, asserted at `:937-943`), still contributes its extent to sizing but no longer resolves its endpoints or checks them at render.

**Failure:** take the template in `a_container_with_no_room_left_fails_cleanly_at_render` (`src/render/mod.rs:3259`) and delete the two `stroke:`/`thickness:` lines from the child line. Load still passes. `render_single_label` now returns `Ok` and emits a page, where the same template with a stroke returns `line_degenerate` (`:3269-3274`). The load-time check in `validate_layout_item` cannot cover this: `validate_accepts_an_edge_relative_line_on_a_dynamic_width_label` (`src/templates.rs:2841`) exists precisely because a dynamic-width label's frame is unknown at load, which is why the render-time mirror was added.

This contradicts two requirements this change is landing under. `specs/shape-paint/spec.md` states that "Every other clause of those bullets (placement, `when`, `padding`, `items`, **endpoint resolution and bounds checking**) stays authoritative". The `layout-sizing` delta this change writes says "an active `line` fails its bounds or degeneracy checks if its endpoints require room the frame does not have", with the scenario "AND an active `line` whose endpoints need room the inner box lacks fails its own bounds check". The skip is silent and untested: `src/render/mod.rs:7506-7514` asserts only that no `#line` is emitted.

The fix is to run resolution and the bounds check unconditionally and gate only the `writeln!`.

### 2. BLOCKING: most paint refusals still report `template_parse_failed`, and a new test pins that

`specs/shape-paint/spec.md` opens with "Every refusal in this capability is a template validation failure", whose scenario enumerates the classes: "WHEN any template in this capability is refused, whether for **a bad colour**, a non-positive thickness, **an explicit null**, **a removed spelling**, or **paint on an item that accepts none** THEN the failure is `TemplateInvalid` with `details.reason` of `template_validation_failed`".

Round 5 fixed one of the five by moving the numeric bounds into `validate()`, which `src/api.rs:643-644` maps to `TemplateValidationFailed`. The other four are unchanged. `src/api.rs:641-642` maps every `parse_template` error to `Reason::TemplateParseFailed`, and `TemplateContent::try_from` runs inside `parse_template` (`src/parse.rs:25-33`), so colour refusals (`src/raw.rs:32`), `deny_unknown_fields` refusals, and the explicit-null refusals raised at `src/convert.rs:22-52`, `:227-260` and `:357-364` all surface as `template_parse_failed`.

**Failure:** `PUT /api/templates/x` with `background: chartreuse`, `stroke: null`, `frame: { thickness: 0.02 }`, or `background` on a `text` returns `details.reason: "template_parse_failed"`. A client branching on the reason the delta promises never matches.

This is pinned rather than merely missed. `src/lib.rs:2906-2954` adds a `parse_cases` table asserting `template_parse_failed` at `:2952` for `stroke_null`, `bg_null`, `line_bg`, `bad_color` and `legacy_frame` [verified: it passes]. A test asserting the negation of a requirement in the same commit is a lock on the defect. `/opsx:archive` will sync the requirement into `openspec/specs/` verbatim alongside it.

Two aggravating facts, both independent of the delta:

- Frozen `docs/SPEC.md` defines the reasons as "The YAML did not parse" and "The template parsed but failed structural validation". `background: chartreuse` and `frame: {…}` are well-formed YAML that parsed and then failed a schema rule, so `template_parse_failed` is wrong by the frozen table's own words.
- `tasks.md` 3.1 is checked and states the undelivered clause verbatim, and `docs/adr/0092-*.md` decision 7 repeats it ("quarantined at startup with validation errors"), where in fact `src/templates.rs:824-836` records them as `TemplateRegistryError::Parse`. The ADR is append-only, so an untrue sentence there outlives the change.

Round 3 blocked remapping `src/api.rs` as an unreviewed contract change outside this capability, and that objection stands. The remaining exits are to move these refusals to a post-deserialize pass reached by `validate()`, or to amend `specs/shape-paint/spec.md` and take the fresh plan review that a `specs/` edit costs. Reverting to `template_parse_failed` and adding a test for it is neither.

### 3. BLOCKING: no paint refusal carries a JSON path, which the same requirement demands

The same requirement says the failure carries "the JSON path of the offending field", and its scenario adds "AND it names the JSON path of the field responsible". `tasks.md` 3.1 restates it: "with the field's JSON path".

The refusals that now correctly report `template_validation_failed` return a bare string: `src/templates.rs:1883` and `:1986` return `"stroke thickness must be finite and >= 0.0001"`, `:1991` returns `"rounded radius must be finite and >= 0.0001"`. `validate_layout_items` (`src/templates.rs:1850`) and `validate_layout` (`:1828`) propagate with `?` and add nothing, and `TemplateContent::validate` (`:1133`) passes the string straight to `AppError::template_invalid` (`src/api.rs:643-644`) [verified by reading the whole chain].

**Failure:** a template with twenty containers, one of which declares `rounded: 0`, is refused with `"rounded radius must be finite and >= 0.0001"` and nothing else. The author is told a rule, not a location, and cannot tell which container to edit. `src/lib.rs:2882-2904` asserts only the code and the reason, so nothing catches this.

### 4. MINOR: `tasks.md` 2.4 is checked but the work it names is not in `src/convert.rs`

Task 2.4 reads "Convert both in `src/convert.rs` via `TryFrom` … refusing: a missing or non-finite or sub-`0.0001` `thickness`; a non-finite or sub-`0.0001` `rounded`". `grep -n "is_finite\|0\.0001" src/convert.rs` returns only the unrelated flow-gap checks at `:171,:181` and test literals [verified]; both bounds live solely at `src/templates.rs:1882,1985,1990`. Relocating them was the right call for finding 2, but the box claims work performed where it was not, and `AGENTS.md` is explicit that a checked box is a claim the next reader trusts instead of redoing.

### 5. MINOR: nothing warns an operator that dropping a line's thickness yields a silently invisible line

`docs/DEPLOY.md:204-208` tells an operator to move `line.thickness` into `stroke: { thickness, color }`, and `docs/AUTHORING.md:490-492` says `thickness` is required inside `stroke`. Neither says that `stroke` itself is optional on a `line`, or that omitting it renders nothing. A leftover bare `thickness:` is caught loudly by quarantine; deleting it and forgetting to add `stroke:` is not caught at all, and by finding 1 that line also stops being bounds-checked. One sentence in the §9 `stroke` bullet closes it.

## Observation, not a finding

`Color` (CSS Level 1, case-insensitive, canonical read-back) and `Ink` (`src/models.rs:858-946`: Typst palette, case-sensitive, verbatim read-back, 18 names) now coexist, so `background: red` is `#ff0000` while `ink: red` is `#ff4136`. This is the approved contract, recorded in ADR-0092 decision 6 and warned about at `docs/AUTHORING.md:502-503`, and the table there is accurate against the code [verified]. It is a wart a later change should collapse, not a defect in this one.

Findings 1, 2 and 3 must move before this lands. Finding 1 is new and is the one I would fix first: it is three lines and it restores a guarantee two requirements assert.

VERDICT: REVISE
