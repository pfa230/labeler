Reviewed the full working-tree diff (14 modified files plus the new ADR) against `proposal.md`, `specs/`, `design.md`, `tasks.md` and `AGENTS.md`. No `ANSWERS.md` exists; nothing blocked me, so no `QUESTIONS.md`.

Gates, run here: `cargo fmt --check` clean, `cargo clippy --all-targets --all-features` clean, `cargo test` 736 + 2 + 1 passing, 0 failed [verified, `/tmp/gates-280-r4.log`]. `.workflow/review-gate-check.sh .` exits 0 and `SPECS_SHA256` matches `specs-digest.sh` (`006e8ab5…`) [verified]. The three `MODIFIED` deltas resolve by name against `openspec/specs/` and I diffed each body: they are pure `frame` → `stroke`/`background`/`rounded` respellings with no behaviour moved [verified].

Most of round 3's blockers are genuinely fixed. A strokeless `line` is now accepted (`src/convert.rs:372-383`, asserted at `:995`), matching the spec table. `models::Frame` is gone from `src/`, `Color::WHITE`/`Color::rgb` are gone, `.nan`/`.inf` are covered on both values (`src/templates.rs:2361-2382`, `src/convert.rs:2434-2451` direct-model), ADR-0092 decision 6 now carries the value and case-matching conflict with `Ink` in a table, `docs/AUTHORING.md:498-503` warns the author about it, `catalog/` and every fixture are migrated, and `ui/src/` reads none of these fields [verified].

## Findings

### 1. BLOCKING: every paint refusal reports `template_parse_failed`, and a new test now pins that

`openspec/changes/issue-280-shape-paint-model/specs/shape-paint/spec.md:9-24` is the capability's first requirement, and its scenario enumerates the cases: "WHEN any template in this capability is refused, whether for a bad colour, **a non-positive thickness**, **an explicit null**, a removed spelling, or paint on an item that accepts none THEN the failure is `TemplateInvalid` with `details.reason` of `template_validation_failed`".

Round 2 fixed this by remapping `TemplateContent::try_from` failures in `src/api.rs`. Round 3 blocked that remap as an unreviewed contract change outside this capability (its finding 3). Round 4 reverted it: `git status` shows `src/api.rs` unmodified, and `src/api.rs:641-644` maps *every* `parse_template` error, conversion included, to `Reason::TemplateParseFailed`. Since `TemplateContent::try_from` runs inside `parse_template` (`src/parse.rs:25-33`), **no** paint refusal now reaches `TemplateValidationFailed` from any YAML or API path: not the colour parser (`src/raw.rs:32`), not `deny_unknown_fields`, and not the `TemplateError::Validation` values raised at `src/convert.rs:22-56`, `:234-280` and `:372-383`. The net effect is worse than round 3, where at least the conversion half was correct.

**Failure:** `PUT /api/templates/x` with `stroke: { thickness: 0 }` returns `details.reason: "template_parse_failed"`. A client branching on the reason the shipped contract promises never matches, for any paint mistake at all.

This is pinned rather than merely missed. `src/lib.rs:2850` adds `template_put_paint_refusals_report_template_parse_failed`, which asserts `template_parse_failed` at `:2915` for eight cases including `stroke: { thickness: 0 }`, `stroke: null`, `background: null` and `rounded: 0` [verified: the test passes]. A test that encodes the opposite of the requirement is a lock on the defect, not coverage of it.

Two things make this more than a delta-versus-code mismatch:

- Frozen `docs/SPEC.md:712-713` defines the two reasons as "The YAML did not parse" and "The template parsed but failed structural validation". `stroke: { thickness: 0 }` is well-formed YAML that parsed and then failed a bound check, so `template_parse_failed` is wrong by the frozen table's own words, independently of this delta.
- `tasks.md` 3.1 is checked and states the undelivered clause verbatim: "Every refusal is `TemplateInvalid` / `template_validation_failed` with the field's JSON path." `AGENTS.md` is explicit that a box is checked only after the work is performed. ADR-0092 decision 7 repeats the same untrue claim ("quarantined at startup with validation errors").

The resolution is the same one round 3 named: move the paint refusals to `validate()` (or a post-deserialize pass) so the reason is uniform, or amend `specs/shape-paint/spec.md:9-24`, which voids the plan verdict and needs a fresh plan review. Reverting the round-2 fix without doing either leaves the change with a capability spec that `/opsx:archive` will sync into `openspec/specs/` verbatim while a test in the same commit asserts its negation.

### 2. MINOR: the `validate()` paint bounds are still unreachable in production, and now duplicate the contract

`src/templates.rs:1883` (line stroke), `:1986` and `:1991` (container stroke, radius) re-check `is_finite()` and `>= 0.0001`. `Stroke::try_from` (`src/convert.rs:31`) and `ContainerRaw` (`src/convert.rs:270`) refuse the same values first, and `validate()` only ever runs on a `TemplateContent` conversion already produced, so no YAML path reaches them. Round 4 answered round 3's finding 4 by adding `shape_paint_direct_model_validation_boundaries` (`src/templates.rs:2400`), which constructs the model directly, so the branches are now live tested code rather than dead code. What remains is that the bound `0.0001` is written in two places with nothing asserting the two agree: change one and the suite stays green. This is entangled with finding 1, since moving the refusals into `validate()` collapses the duplication and fixes the reason in one step.

### 3. MINOR: draw order has no test, and it is one line away from silently inverting

`specs/shape-paint/spec.md` requires "the `background`, then the `stroke`, then the container's `items`", with scenarios "The fill sits behind the children" and "Padding is inside the paint". `src/render/mod.rs:2103-2145` gets this right by emitting the `#rect` before the child `#box`, but every case in `shape_paint_source_emission` (`src/render/mod.rs:7355`) carries `items: vec![]`, and the two end-to-end tests (`src/render/mod.rs:7537`, `src/lib.rs:9740`) assert only PNG/PDF magic bytes and a 200. Swapping the two `writeln!` blocks would make a filled container hide its own contents and pass the entire suite. One assertion that the `#rect` substring index precedes the child `#place` index would pin the requirement that makes the whole change useful.

### 4. MINOR: `docs/AUTHORING.md` documents the radius clamp but not the floor that quarantines `rounded: 0`

`docs/AUTHORING.md:496-497` describes `rounded` as a numeric radius "clamped at render time to half the shorter side" and says nothing about the `>= 0.0001` floor, nor that square corners are spelled by omitting the key. The spec makes this an explicit refusal: "a zero radius SHALL be refused rather than accepted as a second spelling of square". An author who writes `rounded: 0` meaning "square", which is exactly what the removed `rounded: false` meant, gets the whole template quarantined with nothing in the authoring guide to warn them. The `stroke` bullet at `:491-493` states its floor; this one should too.

### 5. TRIVIAL: `impl Default for Color` has no callers

`src/models.rs:1114-1118` implements `Default` returning `Color::BLACK`. `grep -rn "Color::default()" src/` returns nothing [verified]. Round 3 removed `Color::WHITE` and `Color::rgb` for the same reason; this one survived. A `pub` item in a lib crate draws no dead-code lint, so nothing will ever flag it.

### Observation, not a finding

A `line` with no `stroke` now parses, validates and renders nothing (`src/render/mod.rs:1762-1766`; asserted at `src/render/mod.rs:7513`). This matches the spec's uniform table and ADR-0092 decision 2, so it is not a defect. It is worth knowing that an operator migrating `line: { thickness: 0.2 }` who deletes the old key and forgets to add `stroke:` gets a silently invisible line rather than an error, and the spec carries no scenario either way.

---

Finding 1 is the one that must move. Findings 2 through 5 are cheap and none of them would block on its own.

VERDICT: REVISE
