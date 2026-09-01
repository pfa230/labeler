Reviewed the full working-tree diff (15 modified files plus the new ADR) against `proposal.md`, `specs/`, `design.md`, `tasks.md` and `AGENTS.md`. No `ANSWERS.md` exists; nothing blocked me, so no `QUESTIONS.md`.

Gates, run here: `cargo fmt --check` clean, `cargo clippy --all-targets --all-features` clean, `cargo test` 735 + 2 + 1 passing, 0 failed [verified]. `.workflow/review-gate-check.sh .` exits 0 and `SPECS_SHA256` matches `specs-digest.sh` [verified]. The three `MODIFIED` deltas resolve by name against `openspec/specs/` and are pure respellings; I diffed each requirement body and the only changes are `frame` → `stroke`/`background`/`rounded` wording [verified].

The core work is right. `ColorRaw`/`StrokeRaw` → `Color`/`Stroke` is a real two-stage split, `deny_unknown_fields` and `deserialize_present_typed` behave as designed, the radius clamp (`src/render/mod.rs:2115-2121`) uses the same `pbox.w/h` the rect is emitted at, draw order is fill → stroke → children, and `models::Frame` is fully gone. I ran a live server on a temp config dir and rendered a filled/rounded container, a stroke-only container and an alpha line: the label is correct, text draws over the ground, the boundary stroke clips at the label edge. The prior round's blockers on the ADR number, the HTTP-level render test, the removed-spelling quarantine test, the `.nan`/`.inf` coverage, the AUTHORING `deny_unknown_fields` sentence and the canonical read-back test are all genuinely fixed (`src/lib.rs:9740`, `:9788`, `src/templates.rs:2380`, `src/convert.rs:930-960`) [verified].

## Findings

### 1. BLOCKING — five of the seven refusal classes still report `template_parse_failed`, and a new test pins that

`specs/shape-paint/spec.md:9-24` is the capability's first requirement, and its scenario enumerates the cases by name: "WHEN any template in this capability is refused, whether for **a bad colour**, a non-positive thickness, an explicit null, **a removed spelling**, or **paint on an item that accepts none** THEN the failure is `TemplateInvalid` with `details.reason` of `template_validation_failed`".

The round-2 fix moved only the `TryFrom` half. `src/api.rs:641-648` now maps `TemplateError::Validation` to `TemplateValidationFailed`, but colour parsing (`src/raw.rs:32`, `E::custom` inside `ColorRaw::deserialize`) and every `deny_unknown_fields`/type refusal happen during `serde_path_to_error::deserialize` in `parse_template` (`src/parse.rs:28-31`), which yields `TemplateError::Yaml` → `TemplateParseFailed`.

Measured against a live server (`PUT /api/templates/{id}`, each returning 422) [verified]:

| Template | `details.reason` | Spec requires |
| --- | --- | --- |
| `background: chartreuse` | `template_parse_failed` | `template_validation_failed` |
| `background: '#ff00f'` | `template_parse_failed` | `template_validation_failed` |
| `frame: { thickness: 0.02, rounded: false }` | `template_parse_failed` | `template_validation_failed` |
| `rounded: true` | `template_parse_failed` | `template_validation_failed` |
| `line` with bare `thickness: 0.2` | `template_parse_failed` | `template_validation_failed` |
| `background: red` on a `text` | `template_parse_failed` | `template_validation_failed` |
| `stroke: { thickness: 0.2, dash: dotted }` | `template_parse_failed` | `template_validation_failed` |
| `stroke: { thickness: 0 }` (control) | `template_validation_failed` | ✓ |

**Failure:** a client branching on the reason the shipped contract promises never matches for a bad colour, a legacy spelling, or paint on a non-shape. Worse, `src/lib.rs:2906-2919` now *asserts* the violating behaviour — the test named `template_put_paint_validation_failures_report_template_validation_failed` ends by requiring `template_parse_failed` for `background` on a `line`, which is the spec's "paint on an item that accepts none" case. A test that encodes the opposite of the requirement is not coverage, it is a lock on the defect.

Secondary, same requirement: the scenario also says the failure "names the JSON path of the field responsible". The colour and unknown-field errors report `layout[0]`, not `layout[0].background` (serde's internally-tagged buffering flattens it), while the conversion refusals correctly report `layout[0].stroke.thickness`.

Either the refusals move to `validate()` (or a post-deserialize colour/shape pass) so the reason is uniform, or `specs/shape-paint/spec.md:9-24` is amended — and a `specs/` edit voids the plan verdict and needs a fresh plan review. It cannot land as written.

### 2. BLOCKING — a `line` with no `stroke` is refused, and the digest-locked spec says the opposite

`specs/shape-paint/spec.md:37` gives one universal table row: `stroke`, accepted on "every shape", "Omitted: no outline." `:160` restates it as a rule with no shape carve-out: `"No outline" is spelled by omitting stroke, so it has exactly one spelling.` No requirement or scenario makes `stroke` mandatory anywhere.

`src/convert.rs:373-378` refuses it: `PUT` of a `line` with `at`/`to` and no `stroke` returns `template_validation_failed` / "line stroke is required" [verified against the live server], and `src/convert.rs:1048` asserts that.

The decision was recorded in ADR-0092 decision 2, but the ADR is not the contract — `openspec/specs/` is, and `/opsx:archive` will sync this delta verbatim. The repo would then carry a capability spec whose own table contradicts the shipped code, which is exactly the drift the archive gate exists to prevent (it checks names, not prose). This was raised in round 2 as finding 5 and answered in the ADR rather than in the spec, so the contradiction is still what lands. Fix it in the spec (a stated exception living next to the rule it bends, per `AGENTS.md` *Exceptions*) or accept the omitted-key meaning in code.

### 3. MAJOR — the reason remap silently changes the wire contract for refusals outside this capability

`src/api.rs:641-648` routes **every** `TemplateContent::try_from` failure to `TemplateValidationFailed`. On `main` all of them reported `TemplateParseFailed` (`git show main:src/api.rs:641-642`) [verified].

Measured on the live server [verified]:

- container with both `size` and `to` → was `template_parse_failed`, now `template_validation_failed`
- `line` as a packed flow child → was `template_parse_failed`, now `template_validation_failed`

Neither is shape paint. Frozen `docs/SPEC.md:712-713` defines the two reasons and nothing in `openspec/specs/` supersedes that section, so under the first-touch rule this needed an `ADDED` requirement carrying the complete post-change contract. `proposal.md`'s Impact section lists `api.rs` nowhere at all, `design.md` does not mention the remap, no ADR records it, and no test covers either case — which is why the gates are green. The change may well be an improvement (these *are* "parsed but failed structural validation"), but it is a shipped-contract change nobody reviewed and nobody can check.

### 4. MINOR — the new bound checks in `validate()` are unreachable, and the test named for them exercises conversion instead

`src/templates.rs:1882` (line stroke), `:1984` and `:1989` (container stroke, radius) re-check `is_finite()` and `>= 0.0001`. `validate()` only ever runs on a `TemplateContent` that conversion already produced, and `Stroke::try_from` (`src/convert.rs:31`) and `ContainerRaw` (`src/convert.rs:270`) refuse the same values first, so no YAML path reaches them. `shape_paint_validation_boundaries` (`src/templates.rs:2328`) goes through `parse_and_validate`, so every one of its 25 `is_err()` assertions is satisfied by the conversion error; deleting all three `validate()` branches leaves the test green. Before this change the container check was the *only* check, so this is newly-dead code, not inherited. Either drop them or reach them from a test that constructs the model directly.

### 5. TRIVIAL — dead public API on `Color`

`src/models.rs:1103` (`Color::WHITE`) and `:1114` (`Color::rgb`) have no callers anywhere in `src/`, tests included [verified]. `AGENTS.md` asks for minimal, focused changes; a `pub` item in a lib crate draws no dead-code lint, so nothing will ever flag these.

### 6. TRIVIAL — unnecessary test-module rename

`src/raw.rs:544` renames `mod raw_tests` to `mod tests` and drops the blank line before `#[cfg(test)]`. Unrelated churn in a diff that is already large.

---

Findings 1 and 2 are the same shape of problem: the code and the contract this change is about to freeze into `openspec/specs/` disagree, and in both cases the round-2 finding was answered somewhere other than the artifact that governs. Finding 3 is a real contract change carried in as a side effect of fixing finding 1.

VERDICT: REVISE
