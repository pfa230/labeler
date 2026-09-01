Reviewed the full working-tree diff (14 modified files plus the new ADR) against `proposal.md`, `specs/`, `design.md`, `tasks.md` and `AGENTS.md`. No `ANSWERS.md` exists; nothing blocked me, so no `QUESTIONS.md`.

Gates, run here: `cargo fmt --check` clean, `cargo clippy --all-targets --all-features` clean, `cargo test` 733 + 2 + 1 passing [verified, `/tmp/gates280.log`]. `.workflow/review-gate-check.sh .` exits 0, `SPECS_SHA256` matches `specs-digest.sh` [verified]. The three round-1 blockers on ADR numbering, the missing HTTP test and the missing removed-spelling test are fixed: ADR is now 0092 with its README row (`docs/adr/README.md:101`), `src/lib.rs:9666` drives `/api/render/label` for PNG and PDF at the status-code level, and `src/templates.rs:2397` loads four legacy-spelling templates and asserts quarantine plus a still-served registry. The MODIFIED deltas are pure respellings: I diffed each delta requirement against its counterpart in `openspec/specs/` and the only changes are `frame`→`stroke`/`background`/`rounded` wording [verified].

The core code is right. `ColorRaw`/`StrokeRaw` → `Color`/`Stroke` is a real two-stage split, `deny_unknown_fields` and `deserialize_present_typed` do what the design claims, the radius clamp at `src/render/mod.rs:2115-2121` uses the same `pbox.w/h` the rect is emitted at, and draw order is fill then stroke then children.

## Findings

### 1. BLOCKING — every paint refusal reports `template_parse_failed`, the spec requires `template_validation_failed`

`specs/shape-paint/spec.md:9-24` makes this the capability's first requirement, and its scenario is explicit: "WHEN any template in this capability is refused, whether for a bad colour, a non-positive thickness, an explicit null, a removed spelling, or paint on an item that accepts none THEN the failure is `TemplateInvalid` with `details.reason` of `template_validation_failed`".

Every one of those refusals lives in the conversion stage: `Stroke::try_from` (`src/convert.rs:19-56`), `ContainerRaw` (`src/convert.rs:234-280`), the line branch (`src/convert.rs:372-388`), `parse_color` (`src/raw.rs:44`) and `deny_unknown_fields`. Conversion runs inside `parse_template`, and `src/api.rs:640-642` maps *every* `parse_template` error to `Reason::TemplateParseFailed`; only `content.validate()` failures reach `TemplateValidationFailed` (`src/api.rs:643-644`).

**Failure:** `PUT /api/templates/x` with `stroke: { thickness: 0 }`, `background: chartreuse`, `stroke: null`, `frame: {...}` or `background` on a `text` returns `details.reason: "template_parse_failed"`. A client branching on the reason the spec promises never matches. This is not an accident of the codebase: `src/lib.rs:2451-2485` pins the two reasons apart deliberately ("#151: one code, two causes, told apart without reading the prose"), so the delta's requirement contradicts a shipped contract. The duplicate bound checks added at `src/templates.rs:1881` and `:1985` do produce the validation reason, but conversion refuses first and they are unreachable from any YAML path.

Nothing tests the reason, which is why the gates are green. Either the requirement is amended (a `specs/` edit voids the plan verdict and needs a fresh plan review) or the refusals move to `validate()`. It cannot land as written.

### 2. BLOCKING — task 3.4 is checked but the `.nan`/`.inf` half was not performed

Task 3.4: "Test the numeric boundary both ways: `0.0001` accepted, `0.00001` refused, on both `thickness` and `rounded`; plus **`.nan`, `.inf`**, `0` and a negative."

`grep -n 'nan\|NaN\|infinit\|INFINITY' src/convert.rs src/templates.rs` returns nothing [verified]. `shape_paint_container_refusals_and_defaults` (`src/convert.rs:828`) and `shape_paint_validation_boundaries` (`src/templates.rs:2328`) cover `0.0001`, `0.00001`, `0` and a negative on both values, and stop there. Two spec scenarios rest entirely on this: "A non-finite thickness is refused" (`specs/shape-paint/spec.md:195-199`, including "no Typst source is generated from the value") and "A zero, non-finite, or unrenderable radius is refused" (`:249-253`).

The `!t.is_finite()` guards at `src/convert.rs:31` and `:270` look correct by inspection, but the design's own argument for the check (`design.md:129-136`: a NaN thickness would otherwise reach Typst as the literal `NaNmm` and fail at render time on some later request) is exactly the kind of claim that needs a red-then-green test, and `AGENTS.md` is explicit that a box is checked only after the work is performed. Four `assert!` lines, or uncheck 3.4.

### 3. MAJOR — the `Ink`/`Color` divergence is now acknowledged but not recorded accurately

Round 1 blocked on two colour vocabularies landing in one schema. ADR-0092 decision 5 (`docs/adr/0092-...:40-41`) now names the split, but describes it only as "Shape paint enforces CSS standard values and canonical hex normalization on read-back, while text ink preserves authored spellings and supports dynamic parameter substitution". That omits the part an author or maintainer actually collides with:

| | `text.ink` (shipped, ADR-0091) | `background` / `stroke.color` |
| --- | --- | --- |
| `red` | `#ff4136` (`src/models.rs:875`) | `#ff0000` (`src/raw.rs:100`) |
| `gray` | `#aaaaaa` (`src/models.rs:864`) | `#808080` (`src/raw.rs:99`) |
| `green` | `#2ecc40` (`src/models.rs:878`) | `#008000` (`src/raw.rs:106`) |
| name matching | case-sensitive (`match s`, `src/models.rs:861`) | case-insensitive (`src/raw.rs:96`) |

**Failure scenario:** a container with `background: red` holding a text with `ink: red` prints two visibly different reds on one label; `background: Red` loads while `ink: Red` quarantines the whole template. Nothing an author reads warns them: `docs/AUTHORING.md:494` lists the sixteen names with no note that `ink` reads the same words differently, and the ADR sentence a maintainer would consult does not state that the two tables disagree on values at all. `design.md:255-258` argues at length that the shape table deliberately differs from Typst's constants, and that reasoning applies with more force to a table shipped in this same repo.

I am not re-litigating the decision to keep the two vocabularies separate; the proposal made that call deliberately. What must land with it is an accurate record: name the value conflict and the case-matching conflict in ADR-0092, so the next reader finds it argued rather than discovers it on a printed label.

### 4. MAJOR — task 6.1's stated ADR content is not in the ADR

Task 6.1 requires ADR-0092 to record "the project-owned CSS name table **and why it deliberately differs from the renderer's**". Decision 4 (`docs/adr/0092-...:36-39`) records the table and the canonical form, and says nothing about Typst's constants (`red` is `#ff4136` there) or why the table is ours. That reasoning exists in `design.md:167-179` and dies with the change folder unless the ADR carries it, which is the whole point of the row in `docs/adr/README.md`. Same fix as finding 3, one paragraph.

### 5. MINOR — a `line` without `stroke` is refused, but the spec says the key is optional on every shape

`specs/shape-paint/spec.md:37` gives one universal row: `stroke`, "Accepted on: every shape", "Omitted: no outline". `src/convert.rs:373-378` refuses a line with no stroke ("line stroke is required"), asserted at `src/convert.rs:1014`. Requiring it is defensible and preserves what `line.thickness` did, but the shipped contract says the opposite and carries no scenario either way, so an author reading the table writes a strokeless line and gets a quarantined template. Decide it in the spec text or accept the omitted-key meaning in code.

### 6. MINOR — two spec requirements have no executable coverage

- "A colour is reported canonically wherever a template is read back" (`specs/shape-paint/spec.md:376-401`, three scenarios) is an API-visible contract. The only assertions on canonical form are on the model (`src/convert.rs:975`) and on Typst source (`src/render/mod.rs:7375`); nothing serializes a `LayoutItem` or hits `GET /api/templates/{id}` [verified: `grep -rn '#ff0000ff\|#000000ff' src tests`]. The behaviour is correct by inspection (`Color::serialize` → `hex()`, `src/models.rs`), so this is a coverage gap rather than a defect, but it is the one requirement a client is told it may compare strings against.
- "A line has no interior to fill" (`specs/shape-paint/spec.md:80-83`): no test declares `background` or `rounded` on a `line`. `LineRaw` carries `deny_unknown_fields` (`src/raw.rs:390-391`) so the refusal is real, and two `assert!` lines beside `shape_paint_line_refusals` would pin it.

### 7. TRIVIAL — five stale `ADR-0091` references in artifacts that will be archived

`proposal.md:110`, `:113`, `:122` and `design.md:122`, `:258` still say ADR-0091, which on `main` is `docs/adr/0091-text-ink-is-a-full-colour.md`, the #282 decision. `design.md:33` was corrected to 0092 and the rest were not, so the archived plan sends a reader to the wrong ADR precisely where it discusses the monochrome reversal and the name-table divergence. Editing these does not touch the digest.

### 8. TRIVIAL — the new doc bullets do not wrap

`docs/AUTHORING.md:490-494` and `docs/DEPLOY.md:204` are single lines of 200 to 400 characters in files that otherwise wrap near 100.

VERDICT: REVISE
