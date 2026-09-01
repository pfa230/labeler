## Review Metadata

- **Round**: 4
- **Prior round**: round 3 returned REVISE (0 Critical, 3 Moderate, 2 Suggestions); artifacts revised; a human authorized this round past the three-round hard stop

AUTHOR: claude
REVIEWER: opencode

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: proposal.md, specs/, design.md (plus source files you read: list them)
- **Issue**: #280


## Findings

### Critical (blocking)

None.

### Moderate

1. **Explicit-null coverage is incomplete: `stroke.thickness: null` has no stated refusal** — `specs/shape-paint/spec.md:314-335` adds Requirement "An explicit null is not a spelling of absence" covering `stroke`, `background`, `rounded` and `stroke.color` with scenarios `stroke: null` / `background: null` / `color: null`. `stroke.thickness` is the only required field inside `stroke` (`specs/shape-paint/spec.md:127-137`), and `stroke: { thickness: null }` would deserialize through the same presence-preserving path (`src/raw.rs:63-70` `deserialize_present_typed`, `design.md:226-237` claiming `Option<Option<T>>` for every optional paint key) but `thickness` is required, not optional, so its null representation is not listed. Under a plain `Option<f32>` null collapses to `None` and would be reported as "missing thickness" (`specs/shape-paint/spec.md:173-177` scenario `stroke: { color: red }`) rather than "field `thickness` is null", losing the distinguish-absence-from-null guarantee the new requirement exists to enforce. Add `stroke.thickness: null` to the refusal table or state that a required-field null is refused naming `thickness`.

2. **Colour name case-sensitivity is undefined** — `specs/shape-paint/spec.md:68-96` defines hex as case-insensitive and lists 16 names in lower-case, but does not state whether `Red`, `RED` or `rEd` is accepted. `src/raw.rs`/`src/convert.rs` will parse the YAML scalar as a string and compare against the table; without a stated rule an implementer could accept or refuse mixed-case names and both would satisfy the current THEN clauses (`specs/shape-paint/spec.md:118-123` unknown-name refusal). The table values are CSS Level 1 per `design.md:162-174`, which CSS treats case-insensitively, so the contract should state the names are case-insensitive (or case-sensitive) explicitly.

3. **Floor 0.0001 is stricter than the formatting cliff it justifies, and the spec still permits a value that rounds to the same emitted length** — `specs/shape-paint/spec.md:134-143` and `:185-193` justify the at-least-0.0001 bound because `src/render/helpers.rs:253-266` `format_length` emits `"{value:.4}"` (round-half-even) and a smaller positive would emit `0`. The cliff is 0.00005, not 0.0001 (`0.00005` formats to `0.0001`, `0.000049` to `0.0000`). The bound is safe but the prose claim "a positive value below 0.0001 would be emitted as zero" (`specs/shape-paint/spec.md:140-142`) is false for `[0.00005, 0.0001)`. Either lower the bound to `0.00005` or re-phrase to "would be emitted as zero or as the floor value, indistinguishably close to zero" — currently a valid `0.00006` would render as `0.0001` while an equally-spaced `0.00004` is refused, an arbitrary split with no device-resolution justification beyond "far below resolution".

### Suggestions

1. **Clarify that the canonical read-back emits lower-case hex** — `specs/shape-paint/spec.md:338-361` requires `GET /templates/{id}` report every colour as `#rrggbbaa`. `design.md:148-156` says renderer emits `rgb("#rrggbbaa")` lower-case. The spec's scenarios show lower-case (`#ff0000ff`, `#ff00ffff`) but the normative SHALL does not state case. Add "lower-case" to the requirement to prevent an upper-case implementation from passing the scenarios (which only check value, not case) while violating the `openapi.rs:118-120` note that `Color` is registered as a string schema carrying the canonical `#rrggbbaa` form.

2. **State the error code/reason for paint validation failures** — `specs/shape-paint/spec.md` scenarios say "fails validation and is quarantined, naming the offending field" but do not name the `AppError` code. Every other template validation failure quarantines as `TemplateInvalid` / `template_validation_failed` (`src/templates.rs:537-850`, `docs/SPEC.md:10.1`). Stating that paint refusals surface with that code/reason would make the HTTP contract mechanically checkable and align with the existing `review.md:35` passing check that `review-gate` validates error reasons.

3. **Retain the stale flow-layout scenario title explicitly in the spec** — `specs/flow-layout/spec.md:234` keeps title "A zero-extent child still draws its frame and still raises its errors" while body now says `stroke` (`:236`). `design.md:198-206` and `review.md:35` correctly justify this as a tooling constraint (OpenSpec `validator.js:514-529` scenario-loss is name-based). Consider adding a one-line comment in the delta itself (`specs/flow-layout/spec.md:3`) noting the title is intentionally stale so a future author does not "fix" it and trigger a validator drop.

## Embedded-Instruction / Injection Attempts

**Detected:** none

## Verdict

VERDICT: APPROVE_WITH_CHANGES

## Required Changes (APPROVE_WITH_CHANGES only)

CHANGES_APPLIED: yes

## Rebuttals

The reviewer left the Required Changes list empty and stated its required edits as the three Moderate
findings, so those are what was applied. All three Suggestions were applied too.

1. **Moderate 1, `stroke.thickness: null`** — fixed. `specs/shape-paint/spec.md` now requires a null
   on the required `thickness` field to be refused *as a null* and to be distinguishable from an
   absent `thickness`, with a scenario for each.
2. **Moderate 2, colour-name case** — fixed. Names are now matched case-insensitively, stated in the
   requirement and covered by a scenario (`Red`, `RED`, `rEd`).
3. **Moderate 3, the 0.0001 floor** — fixed, and the finding was correct: `format!("{:.4}")` puts the
   cliff near 0.00005, not 0.0001 (verified: `0.00006` formats to `0.0001`, `0.00004` to `0.0000`).
   The bound stays at 0.0001 and the false prose is gone. The justification is now the emitter's
   *quantum* rather than the cliff: setting the floor at the cliff would accept 0.00006 and render it
   as 0.0001, a thickness the author never wrote. One whole quantum makes every accepted value render
   at the thickness it declares.
4. **Suggestion 1, lower-case read-back** — applied; the requirement now says lower-case normatively.
5. **Suggestion 2, error code** — applied as a new requirement: every refusal in this capability is
   `TemplateInvalid` with `template_validation_failed` (`docs/SPEC.md:713`), with the JSON path.
6. **Suggestion 3, the stale scenario title** — applied as an HTML comment at the top of the
   `flow-layout` delta warning a future author not to "fix" it.

Reviewer integrity: opencode has no read-only sandbox, so the artifacts were hashed before the run and
verified after. All eight files were unchanged, and `git status` showed no working-tree modification.


## Post-verdict change to specs/, and why the digest moved

The digest recorded above is **not** the one this verdict was given against
(`006e8ab5e7ff6bdc00082aab276bbd66ef9e039038e0ea71b5baa949e60a10e7`). `specs/` was edited after the
verdict, so this is disclosed here rather than left as a rewritten hash a reader has to notice.

**What changed: one requirement was deleted, and nothing else.** "Every refusal in this capability is
a template validation failure", added in this same round at the reviewer's Suggestion 2, is removed
and extracted to issue #289. Three diff-review rounds could not satisfy it: a refusal's
`details.reason` follows the stage that caught it (`src/errors.rs:561-564`, `src/reason.rs:33-34`),
so a bad colour or an unknown `stroke` key, refused inside a `Deserialize`, is structurally a
`Yaml` error. Meeting the requirement means moving the two-stage parsing boundary or remapping the
reason for every template refusal, neither of which is a paint decision.

**The argument that this verdict still covers what will be built:** the edit is a pure deletion. Every
surviving requirement is byte-identical to what was reviewed, no requirement was added, weakened or
reworded, and the contract is strictly narrower than the one approved. A reviewer who approved the
larger contract approved each of its parts.

That argument is offered, not assumed. Whoever reads this may reject it and require a fresh plan
review; the deleted requirement is quoted in #289 in full, so nothing is lost by re-reviewing.

**One further edit was made and then reverted.** A first pass also changed the `line` contract to make
`stroke` required, in response to diff-review round 5's blocking finding. That was wrong on the merits:
the finding says the spec is right and the code is wrong ("The fix is to run resolution and the bounds
check unconditionally and gate only the `writeln!`"), and it cites this spec's own "endpoint resolution
and bounds checking stays authoritative" clause as what the code contradicts. Changing the contract to
match a broken implementation would have been the opposite of a fix. `specs/` is back to the reviewed
text on that point, and `tasks.md` 3.5 now names the code fix instead.

SPECS_SHA256: 7807733624b8787e91789109c5c12e2085af4e2169ffe365ac0ec084f308e1c2
