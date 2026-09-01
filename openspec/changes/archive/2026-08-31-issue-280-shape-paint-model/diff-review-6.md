Reviewed the full working-tree diff (14 modified files plus the new `docs/adr/0092-*.md`) against `proposal.md`, `specs/`, `design.md`, `tasks.md` and `AGENTS.md`. No `ANSWERS.md` exists; nothing blocked me, so no `QUESTIONS.md`.

Gates, run here: `cargo fmt --check` and `cargo clippy --all-targets --all-features` clean, `cargo test` exits 0 [verified]. `.workflow/review-gate-check.sh .` exits 0 and `specs-digest.sh` recomputes `7807733624b8…`, matching `review.md:73` [verified]. All three `MODIFIED` requirements resolve by name against `openspec/specs/`, and I diffed each body: they are pure `frame` → `stroke`/`background`/`rounded` respellings with no behaviour moved [verified]. `models::Frame` is absent from `src/`, every fixture, `catalog/` and `docs/AUTHORING.md` are migrated, and `ui/src/` reads none of these fields [verified].

Round 5's blocking finding 1 is genuinely fixed: `render_line_item` now runs `resolve_point` and `check_line` before the `let Some(stroke) = stroke else { return Ok(()) }` early return (`src/render/mod.rs:2031-2034`), with `a_strokeless_line_still_fails_its_render_checks` (`src/render/mod.rs:3283`) asserting `line_degenerate` on the strokeless variant [verified]. Round 5's findings 2 and 3 are dissolved rather than fixed: the requirement they cited was deleted from `specs/` and extracted to #289, disclosed in `review.md:80-92`. I accept that disposition — the surviving requirements name no reason code and no JSON path for the numeric bounds, so the code no longer contradicts the contract.

## Findings

### 1. MUST FIX: ADR-0092 decision 7 states an outcome the same commit's own test contradicts, in an append-only document

`docs/adr/0092-a-shape-carries-a-stroke-and-a-background.md:67-68` reads "Templates using these legacy spellings are quarantined at startup with **validation errors**."

All three removed spellings fail inside `Deserialize`, not inside `validate()`: `frame:` and bare `line.thickness` hit `deny_unknown_fields` on `ContainerRaw`/`LineRaw` (`src/raw.rs:393-431`), and `rounded: true` fails the `f32` type. At registry load these are recorded as `TemplateRegistryError::Parse`, whose Display is `"failed to parse template {path}: {source}"` (`src/templates.rs:824-838`, `:958-962`) — not `TemplateRegistryError::Validation`, `"template {path} failed validation: {message}"` (`:963-964`) [verified by reading both arms].

The change's own test pins the contradiction: `src/lib.rs:2906-2954` asserts `details.reason == "template_parse_failed"` for the `legacy_frame` case [verified, it passes]. So the ADR asserts "validation errors" while the shipped code and a test in the same commit report a parse failure.

This matters more than a word choice because #289 exists precisely to settle the parse/validate boundary, and ADRs here are append-only and superseded rather than edited (`AGENTS.md`, "Where behavior is specified"). A later reader of #289 finds ADR-0092 stating the opposite of what the code does, and cannot correct it without a superseding ADR. Round 5 raised this and it was not addressed.

The fix is one clause: say the templates are refused at load and quarantined, without naming which of the two error classes catches them, matching the deliberate silence `proposal.md` and `design.md` already keep on the reason code.

### 2. MUST FIX: `tasks.md` 2.4 is checked and names `src/convert.rs` for three pieces of work that live elsewhere

`tasks.md:25-27` reads "Convert both in `src/convert.rs` via `TryFrom` … refusing: a missing or non-finite or sub-`0.0001` `thickness`; a non-finite or sub-`0.0001` `rounded`; an explicit null on any paint key; and `background` or `rounded` on a `line`."

Of the four clauses, only the nulls and the missing `thickness` are in `convert.rs` (`src/convert.rs:16-28`, `:37-42`, `:226-260`, `:356-366`). The finite/`>= 0.0001` bounds live solely in `src/templates.rs:1882-1883`, `:1985-1986`, `:1990-1991`; `grep -n "is_finite\|0\.0001" src/convert.rs` returns only the unrelated flow-gap checks at `:171,:181` and test literals [verified]. `background`/`rounded` on a `line` is refused by `deny_unknown_fields` on `LineRaw` (`src/raw.rs:388-400`), which is why the assertion at `src/convert.rs:963` checks for `"unknown field \`background\`"` rather than a `TemplateError::Validation` this task claims to raise.

Relocating the bounds into `validate()` was the right call and is what makes `template_validation_failed` correct for them (`src/lib.rs:2852-2905`). The defect is the checked box: `AGENTS.md` is explicit that a checked box is a claim the next reader trusts instead of redoing the work, and this change folder is archived permanently. Round 5 raised this as its finding 4 and it was not addressed. Re-word 2.4 to name where each refusal landed.

### 3. SHOULD FIX: a new test was inserted between an existing doc comment and the function it documents

`src/render/mod.rs:3278-3283`: the doc comment "A cap below the container's own padding leaves no inner box at all. When child items are inactive, the inner dimensions clamp at zero rather than going negative, emitting no negative dimensions" now sits directly above `a_strokeless_line_still_fails_its_render_checks`, and `a_cap_smaller_than_the_padding_clamps_the_inner_box` (`:3306`) is left undocumented [verified by reading the file, not the diff].

**Failure:** a reader of the strokeless-line test is told it is about padding caps and negative dimensions, which is not what it asserts; a reader looking for why the padding-cap test exists finds nothing. The new test carries its own inline comment (`:3284-3285`), so the fix is to move the block comment back below it.

### 4. SHOULD FIX: the new HTTP test's name asserts the opposite of half its cases, and pins a reason this change declined to decide

`src/lib.rs:2850` is named `template_put_paint_validation_failures_report_template_validation_failed`, but only the first table does that. The second, `parse_cases` (`:2906-2954`), asserts `details.reason == "template_parse_failed"` for `stroke_null`, `bg_null`, `line_bg`, `bad_color` and `legacy_frame`.

Two problems, both cheap. The name is false for five of the ten cases, so a reader grepping for what the suite guarantees about paint reasons is misled. And `proposal.md` states "This change therefore pins no reason code", while the test pins today's mapping for exactly the refusals #289 exists to remap — so #289 must break a test written by the change that declined to decide the question. Neither is a contract violation now that the requirement is deleted, and a characterization test of current behaviour is defensible; but it should say so. Rename the test to cover both classes and add a one-line comment on `parse_cases` recording that the reason is #289's to settle and that this table is expected to move with it.

### 5. SHOULD FIX: nothing tells an operator that dropping a line's `thickness` yields a silently invisible line

`docs/DEPLOY.md:204-208` tells an operator to move `line.thickness` into `stroke: { thickness, color }`. `docs/AUTHORING.md:491-493` says `stroke` is "Accepted on any shape" and that `thickness` is required *inside* `stroke`. Neither says `stroke` itself is optional on a `line`, nor that omitting it draws nothing.

**Failure:** an operator migrating `line: { …, thickness: 0.2 }` who deletes the bare key and forgets to add the `stroke:` block gets a template that loads, validates and renders a page with the rule missing and no error anywhere — the one migration mistake quarantine does not catch. `src/convert.rs:356-359` accepts it and `src/render/mod.rs:2031-2034` renders nothing [verified]. This is the approved contract (`specs/shape-paint/spec.md`, "Omitted: no outline"), so it is a documentation gap and not a defect. One sentence in the §9 `stroke` bullet closes it. Round 5 raised it as its finding 5 and it was not addressed.

## Observation, not a finding

The numeric-bound refusals return a bare string with no JSON path: `"stroke thickness must be finite and >= 0.0001"` (`src/templates.rs:1883`, `:1986`) and `"rounded radius must be finite and >= 0.0001"` (`:1991`), and `validate_layout_items` (`:1833-1852`) propagates with `?` adding no prefix [verified]. So a template with twenty containers, one carrying `rounded: 0`, is told a rule and not a location. This is not a regression and no surviving requirement demands a path here — the checks it replaced (`"line thickness must be greater than 0"`, `"container frame thickness must be greater than 0"`) were identical in this respect, and the scenarios that *do* say "naming the field" are the nulls, the missing `thickness`, the unknown `stroke` key and the bad colour, all of which carry paths through `serde_path_to_error` [verified at `src/convert.rs:889-906`, `:938-950`]. Worth a later change, not this one.

## Embedded-Instruction / Injection Attempts

**Detected:** none

---

Findings 1 and 2 must move before this lands: both are permanent records (an append-only ADR, an archived task list) stating something the shipped code contradicts, and each is a one-line edit. Findings 3 through 5 are cheap and none would block on its own.

VERDICT: REVISE
