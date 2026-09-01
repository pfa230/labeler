Reviewed the full diff (13 modified files + new ADR) against `proposal.md`, `specs/`, `design.md`, `tasks.md`, and `CLAUDE.md`/`AGENTS.md`. Gates pass locally on this branch: `cargo fmt --check` clean, clippy clean, `cargo test` 709 + 2 + 1 passing [verified, `/tmp/gates-280.log`]. `.workflow/review-gate-check.sh .` exits 0, the MODIFIED deltas resolve by name against `openspec/specs/` and are pure respellings [verified].

The code itself is sound: the two-stage `ColorRaw`/`StrokeRaw` → `Color`/`Stroke` split is real, `deny_unknown_fields` and `deserialize_present_typed` do what the design claims, the radius clamp uses the same `pbox.w/h` the rect is emitted at (`src/render/mod.rs:2094-2101`), and draw order is background → stroke → children. The problems are elsewhere.

---

## Findings

### 1. BLOCKING — ADR-0091 already exists on `main`

`origin/main` carries `docs/adr/0091-text-ink-is-a-full-colour.md` (commit `ab7a89c`, "Give text an ink, so a string can be any colour", `Fixes #282`), with its row at `docs/adr/README.md:100`. This branch adds `docs/adr/0091-a-shape-carries-a-stroke-and-a-background.md` and a second `| [0091](...) |` row at `docs/adr/README.md:100`.

The branch is 4 commits behind main and was never rebased (`git rev-list --left-right --count HEAD...origin/main` → `0 4`) [verified]. `design.md` says "next free number; 0090 is the highest on `main`" — that was true when the plan was written and is not true now.

**Failure:** after merge, `tests/adr_index.rs:72` (`assert_eq!(listed.len(), rows.len(), "lists an ADR more than once")`) fails, because two rows share the key `0091` in a `HashSet`. `docs/adr/README.md` also conflicts textually. Renumber to 0092 after rebasing, and update `tasks.md` 6.1/6.2 and `design.md`.

### 2. BLOCKING — two divergent colour vocabularies land in one schema

`main` already ships `Ink` (`src/models.rs:831-966` on `main`), the #282 text-ink colour type. This change adds a second, independent `Color` (`src/models.rs:946`) plus `parse_color` (`src/raw.rs:44`). They disagree on nearly every axis:

| | `ink` (on main) | `background`/`stroke.color` (this diff) |
| --- | --- | --- |
| `red` | `#ff4136` | `#ff0000` |
| `gray` | `#aaaaaa` | `#808080` |
| `green` | `#2ecc40` | `#008000` |
| name count | 18 (adds `orange`, `eastern`) | 16 |
| name matching | case-**sensitive** (`match s`, main `models.rs:860`) | case-**insensitive** (`raw.rs:105`) |
| API read-back | authored spelling verbatim (main test `parsed_ink_serializes_back_to_exact_authored_string`) | canonical `#rrggbbaa` (`models.rs:987`) |
| `{param}` refs | yes (`DynamicValue<Ink>`) | no |
| Typst emission | `rgb(255, 65, 54, 255)` (main `render/mod.rs:1874`) | `rgb("#ff0000ff")` (`render/mod.rs:2088`) |

**Failure scenario:** a template with `background: red` on a container and `ink: red` on its text child renders two visibly different reds on one label. `background: Red` loads fine; `ink: Red` quarantines the whole template. A client reading `GET /templates/{id}` gets `"#ff0000ff"` for one colour key and `"red"` for the other.

The plan is not wrong to have kept #282 free — `proposal.md` says so deliberately — but #282 resolved first, and this diff was implemented against a base that predates it. Landing it as-is ships the contradiction silently. This needs an explicit decision (unify on one type, or state the divergence in ADR + spec as a proven exception per the *Exceptions* rule in `CLAUDE.md`), not a merge.

### 3. BLOCKING — task 4.6 is checked but was not performed

Task 4.6: "Add an **HTTP-level** test that a filled, rounded container renders successfully to PNG and to PDF, **at the status-code level rather than one layer below it**."

What was added is `shape_paint_renders_png_and_pdf` at `src/render/mod.rs:7425`, a unit test inside `mod tests` that calls `render_single_label_image` (`:7461`) and `render_single_label_pdf` (`:7473`) directly and asserts the `PNG`/`%PDF` magic bytes. No test touches `/api/render/label`. `git status tests/` shows no new integration test [verified].

`CLAUDE.md` states this case by name: "a task saying to add an HTTP test is not satisfied by a unit test one layer below the status code." The box must be unchecked or the test written.

### 4. BLOCKING — task 3.3 is checked but was not performed

Task 3.3: test that `frame:`, a bare `line.thickness`, or `rounded: true`/`false` is quarantined naming the field, **and** that the registry still serves every other template.

No such test exists. `grep -rn "rounded" src tests` returns no `rounded: true` or `rounded: false` anywhere, and `grep -rn "frame:" src tests` returns only unrelated `frame: Option<(f32, f32)>` parameters [verified]. The spec's entire "The superseded spellings no longer parse" requirement — three scenarios, and the only thing standing between an operator and a silently-changed meaning — has zero executable coverage. `shape_paint_validation_boundaries` (`src/templates.rs:2318`) covers bounds and non-shape refusals, not the removed spellings.

This matters more than a normal coverage gap: it is the one requirement whose whole point is that a *removed* spelling stops working, and nothing proves it does.

### 5. MAJOR — `docs/AUTHORING.md:97` now documents the opposite of the shipped behaviour

The edit substituted `stroke` for `frame` in this sentence:

> "That guard does **not** reach inside the nested objects: a typo within `format`, `alignment`, `params` or `stroke` is dropped rather than reported."

It was true of `frame`, which had no `deny_unknown_fields` (`design.md` Context says so). It is false of `stroke`: `StrokeRaw` carries `#[serde(deny_unknown_fields)]` (`src/raw.rs:117`), the spec requires "An unrecognised field SHALL be refused at load rather than ignored", and `src/templates.rs:2352` proves `stroke: { thickness: 1.0, width: 2.0 }` errors. An author reading this doc is told to expect their `stroke.thicknes` typo to be silently ignored, when it in fact quarantines the template.

### 6. MINOR — task 4.5's "rounded fill with no stroke" case is not asserted

Task 4.5 lists "a rounded fill with no stroke" among the source assertions. The only rounded case in `shape_paint_source_emission` is `rounded_clamped` (`src/render/mod.rs:7365`), which carries `stroke: Some(...)`. The spec scenario "A filled shape with no outline still rounds" — the exact coupling ADR-0091 decision 3 exists to break — has no test. A `background` + `rounded`, `stroke: None` case would cost two lines.

### 7. MINOR — LaTeX notation in a doc that uses none

`docs/AUTHORING.md:491` writes `must be $\ge 0.0001$`. It is the only `$…$` math in `docs/` [verified: `grep -n '\$' docs/*.md` returns only shell variables in `DEPLOY.md`]. Obsidian/GitHub will render it as math or as literal `$\ge$` depending on the viewer. Write `≥ 0.0001` or `at least 0.0001`.

### 8. TRIVIAL — dead serde attribute

`src/models.rs:1013`: `#[serde(default)]` on `Stroke::color`. `Stroke` derives `Serialize` only, so `default` never applies. Harmless, but it reads as if `Stroke` round-trips through deserialization, which it does not.

---

Findings 1, 3 and 4 are mechanical and cheap. Finding 2 is the one that needs a decision before anything else, and finding 5 is a doc statement that contradicts a spec requirement in the same change.

VERDICT: REVISE
