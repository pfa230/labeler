## Review Metadata

- **Round**: 1
- **Prior round**: none

AUTHOR: claude
REVIEWER: fresh-context-subagent

- **Tool restrictions**: read-only inspection only; sole write is this file
- **Artifacts reviewed**: proposal.md, specs/ (interpolation-tokens, datetime-params, connector-field-transforms), design.md

Source and spec files actually read while checking the artifacts' claims:
`AGENTS.md`, `docs/SPEC.md` (§3.0, §8, §10.1, Settings, Changelog), `docs/AUTHORING.md:536-570`,
`docs/adr/0028`, `0056`, `0068`, `docs/adr/README.md`, `src/render/helpers.rs:27-105`,
`src/datetime_fmt.rs` (whole), `src/templates.rs:503-560`, `:574-700`, `:740-800`, `:860-960`,
`src/render/mod.rs:340-360`, `:640-660`, `:815-818`, `:1061`, `:1334-1400`, `:1620-1680`,
`:2080-2200`, `src/connector/mod.rs:40-50`, `:190-270`, `:940-950`, `src/connector/homebox.rs:100-115`,
`:230-245`, `:500-560`, `src/settings.rs:1-30`, `src/api.rs:630-650`, `:1204-1220`, `:1344-1372`,
`ui/src/lib/templateFields.ts` (whole), `ui/src/lib/connectorRows.ts`, `ui/src/lib/csv.ts:1-60`,
`ui/src/pages/settings/DatetimeFormatsSection.tsx:266`,
`openspec/specs/datetime-params/spec.md`, `openspec/specs/connector-field-transforms/spec.md`,
`openspec/specs/template-registry/spec.md` (requirement list), `tests/fixtures/templates/`, `catalog/`.

- **Issue**: #239 (carrying #240)


## Findings

### Critical (blocking)

**C1. A Homebox custom field can no longer be named by any token, and nothing in the change says so.**

The connector delta spends a paragraph on `custom:<name>` (its "dynamic text prefix" clause, carried
over verbatim from `openspec/specs/connector-field-transforms/spec.md:82-89`) without noticing that the
new grammar makes such a field unreachable from a template.

What exists today, verified:

- `src/connector/homebox.rs:108` declares `dynamic_text_prefix: Some("custom:")`; `:511` inserts cells
  keyed `format!("custom:{name}")`.
- `src/render/helpers.rs:85-89`: a token that matches no datetime/param/`vars.` prefix falls through to
  `data.get(&token)` verbatim. So `{custom:Internal SKU}` resolves today from a data key of that name.
- `ui/src/lib/templateFields.ts:5-20` (`tokens`) returns the raw text between braces, so
  `custom:Internal SKU` is a template field in the UI.
- `ui/src/lib/connectorRows.ts:9-13` (`defaultMapping`) pre-fills a template field to the connector
  column *of the same key*, so that field auto-maps to the Homebox custom column with no operator action.

After the change, `{custom:Internal SKU}` parses as value-path `custom` plus format `Internal SKU`. It
fails both new load-time rules (the format name does not match `^[a-zA-Z0-9_-]+$`, and `custom` is not
an instant), so the whole template file is quarantined at load. The operator's only route is to rename
the template's field to a legal bare name and hand-map it, losing the same-key auto-mapping.

This is a functional regression on a shipped integration path. It appears in no BREAKING bullet, in no
Impact line, in no Risks bullet and in no scenario, and the capability whose spec it belongs to is one
of the three this change edits. Whether it is accepted (most likely, given the "no migration, no
window" decision) or carved out, it has to be a stated decision with a scenario, not a silent
consequence discovered by an operator at startup.

### Moderate

**M1. The ADDED requirement contradicts the part of frozen §8 it explicitly leaves authoritative.**

`specs/interpolation-tokens/spec.md` supersedes "the 'Token types and precedence' list in `docs/SPEC.md`
§8" and adds "The rest of §8 remains authoritative". But `docs/SPEC.md:634-635`, which is outside that
list, reads "Tokens are resolved in precedence order, then `{{` and `}}` emit literal braces", and the
new requirement says the service "SHALL NOT resolve tokens in a precedence order". Under the lookup
procedure `AGENTS.md` documents (read `docs/SPEC.md`, then check `openspec/specs/` for a requirement
superseding it), a reader lands on a live sentence asserting exactly what the change exists to delete.
`docs/SPEC.md:1056` (Settings) likewise names `{datetime.<name>}` interpolation and is not superseded
by anything. Widen the supersession clause to name §8's opening data-binding bullets and the Settings
sentence, or state which sentences of §8 survive.

**M2. The template write path is a behavior change that no requirement covers.**

`src/api.rs:640-643` runs the same `TemplateContent::validate()` on `POST`/`PUT /api/templates` and maps
a failure to `AppError::template_invalid(Reason::TemplateValidationFailed)`, i.e. `422 TemplateInvalid`.
Every new load-time refusal in this change therefore also refuses a template *write* — an operator
editing a template in the UI gets a 422, not a quarantine. The spec says "quarantined at load"
five times and never mentions the write path, which is owned by the `template-registry` capability
("A `422` from a template write means nothing was written", `openspec/specs/template-registry/spec.md:270`).
Add a sentence and a scenario, or say explicitly that the write path is out of scope and why.

**M3. Grammar holes: five shapes the grammar admits and no requirement decides.**

The production is `token := "{" value-path [ ":" format-name ] "}"` with `format-name := ^[a-zA-Z0-9_-]+$`,
but the normative text only refuses (a) an unknown root, (b) an unknown `sys` value, (c) a format on a
non-instant, (d) a second colon, (e) a bare token that is not a legal bare name. That leaves:

- `{x:}` — empty format name. Illegal per the production, but no clause refuses it. An implementer can
  just as reasonably read it as `format: None` and silently print the ISO date. This one changes output.
- `{:fmt}` — empty value-path. Not covered by "a bare token that is not a legal bare name", because it
  is not a bare token; it has a colon.
- `{vars.}` — empty key. The verbatim-key rule makes this a render-time `MissingField` for `vars.`,
  which `src/api.rs:1358-1362` guarantees can never exist. Say so, or refuse it at load.
- `{sys.}` — arguably covered by "outside the closed set", but the message ("unknown system value ''")
  is worth deciding.
- Case: `{VARS.x}`, `{Sys.now}`. Nothing states the roots are case-sensitive, and the consequence of
  the obvious reading is that a capitalisation typo quarantines the whole file.
- `{sys.now.long_date}` — the single most likely migration typo, since it is the mechanical rewrite of
  `{datetime.long_date}`. Under the `sys.<name>` rule it is an unknown system value; the spec should say
  that in a scenario, because the message it produces is the one most operators will read.

**M4. `image:` `name:` is left outside the grammar, which reopens the split the design claims to close.**

`src/render/mod.rs:1652-1657` reads `image.name` straight out of `data` with no interpolation, and
`:2113-2115` pushes it into the field list unparsed. So after this change a data key reachable through
`image.name` may still carry a dot, a colon or a space, while the same key named by a `{token}` may not.
The design's own justification for the bare-token tightening is that it "closes the split where a data
key could be spelled in ways a parameter never could" (design.md, Risks). Either bring `image.name`
under the bare-name rule or state that it is deliberately exempt.

**M5. design.md asserts a proposal bullet that does not exist, and the breakage it describes is absent
from What Changes and Impact.**

design.md Risks: "Requiring a bare token to match `^[a-zA-Z0-9_-]+$` is a second breaking change. … It is
called out as **BREAKING** in the proposal rather than smuggled in." proposal.md has no such bullet. Its
only naming bullet is "**Parameter names keep `^[a-zA-Z0-9_-]+$`**", which is not a change at all
(`src/templates.rs:772-779` already enforces that class) and says nothing about tokens or data keys.

The gap matters, because the tightening is a real break for callers, not just for templates:
`src/render/helpers.rs:85-89` resolves any token text from `data` today, and `ui/src/lib/csv.ts:44-58`
turns any CSV header into a data field name. A template reading `{Internal SKU}` or `{item.name}` works
today and quarantines after the change. Add the BREAKING bullet, add the CSV/connector consequence to
Impact, and add a scenario over a dotted or space-carrying bare token (the existing scenario covers only
`{}` and `{ id }`).

**M6. The Impact list misses operator-facing text that teaches the retired spelling.**

`ui/src/pages/settings/DatetimeFormatsSection.tsx:266` renders "Named strftime patterns available as
{datetime.&lt;name&gt;} in templates." on the settings screen. After this change that instruction produces
a quarantined file. Also stale, though only in comments: `src/settings.rs:10`, `src/datetime_fmt.rs:1-12`,
`src/render/helpers.rs:39`, `src/render/mod.rs:2129-2131`, `:4614`.

**M7. The spec does not require the migration-pointing message the proposal promises.**

proposal.md: old spellings "are quarantined at load with a message naming the fix". The spec's scenarios
require only "a message naming the token and reporting `datetime` as an unknown source". Since the whole
migration story rests on that message, put "names the colon spelling" (or equivalent) in a THEN, where a
test can assert it.

**M8. Two capabilities now state the same two rules.**

- The single-captured-instant paragraph appears verbatim in `specs/interpolation-tokens/spec.md`
  ("`{sys.now}` is the request's single captured instant") and in `specs/datetime-params/spec.md`
  ("A datetime parameter defaults to the render instant of its request").
- Parameter naming appears in `specs/interpolation-tokens/spec.md` ("A bare name is a bare name"), which
  claims to supersede the `docs/SPEC.md` §3.0 "Namespace rules and reserved names" list, *and* in
  `specs/datetime-params/spec.md`, whose surviving header (`openspec/specs/datetime-params/spec.md:12-13`)
  already claims to supersede "`docs/SPEC.md` §3.0 … and restates its complete post-change contract".
  Two requirements in two capabilities now own the same frozen block. The datetime-params restatement does
  defer ("Parameter naming is governed by the `interpolation-tokens` capability"), but it then restates the
  rule anyway, so a future edit to one leaves the other wrong. Pick an owner and make the other a pointer,
  and reconcile the two §3.0 supersession claims.

### Suggestions

**S1. The ADR-numbering facts in proposal.md and design.md are wrong.** Verified: `main` holds up to
`0076-the-filesystem-answers-the-case-question.md`, and `.worktrees/issue-226/docs/adr/` holds `0076-unify-size-resolution.md`,
`0077-size-vocabulary-content-and-fill.md` and `0078-text-overflow-policy.md` — so #226 claims **0076, 0077 and 0078**,
and its 0076 already collides with `main`'s. The instruction ("confirm the next free number") is right; the
stated facts are not. No other worktree claims above 0075.

**S2. Two THENs cannot be forced by a test.** "the render crosses a minute boundary" and "the render crosses
midnight" are not something a test can arrange. The checkable form is "every slot prints the same value" plus
"the clock is read once per request"; the existing `datetime-params` scenarios have the same weakness, so this
is inherited rather than introduced.

**S3. #239's third acceptance criterion is met only in message prose.** An unknown format name and an absent
value are both `422 MissingField` with the same `reason`; only the field name distinguishes them. Fine, but
worth stating that the distinction is deliberately in the message and not in the error contract, since
`code`/`reason` are what the API promises.

**S4. Missing scenarios for two normative clauses**: the second-colon refusal (`{x:a:b}`), and a token in an
`image` `src:` — the Purpose names `image src:` as in scope, and it is one of only three interpolated strings,
but no scenario exercises it.

**S5.** The stale connector scenario title is already owned by design.md's last Risks bullet; no action asked.

### Verified and dismissed (what was checked, and why it is not a finding)

- **Load-time decidability is real.** `TemplateContent::validate()` (`src/templates.rs:574`) calls
  `validate_references()` (`:513`), which recurses `validate_item_references(item, &self.params)` (`:875`).
  `params` is already in hand at the hook design.md names, and `LayoutItem::Text`'s `value` is simply not
  bound today (`..` in the pattern at `:880-886`). The claim holds.
- **No interpolated string is missed.** `interpolate` has exactly two non-test call sites in `src/`:
  `resolve_item_text` (`src/render/mod.rs:1393`, reached from `:1061` measurement, `:1334` text, `:1355` qr)
  and the `image` `src` branch (`:1649`). Nothing else in `src/` calls it. The design's "three interpolated
  strings" is correct.
- **The single-instant guarantee is untouched.** One `DateTimeResolver` per request; `env.datetime.now` is
  threaded to `resolve_parameters` at `src/render/mod.rs:350` and `:646`. `sys.now` reading the same field
  changes nothing.
- **Quarantine, not abort.** A `validate_references` failure is an `Err(String)` becoming
  `TemplateError::Validation`, which the registry quarantines; startup is unaffected.
- **`AppError` `code` strings are stable.** No new variant is needed: load failures reuse
  `TemplateValidationFailed`, render failures reuse `MissingField`, the connector rule reuses
  `connection_transform_invalid`.
- **A `vars` key cannot contain a colon**, so "everything up to the `:`" is unambiguous:
  `src/api.rs:1358-1362` restricts variable keys to `[A-Za-z0-9_.-]`. The spec's "a key may itself contain
  dots" is consistent with the store.
- **Repo template inventory.** Every `{...}` in `tests/fixtures/templates/` and `catalog/` enumerated: only
  `{datetime.iso_date}` (`homebox-qr.yaml:33`) and `{printed_on.short_date}`
  (`brother_24mm_printed_on.yaml:28`) use an old spelling, and no catalog template uses a datetime token.
  Both proposal claims verified.
- **Delta targets resolve.** All three `MODIFIED` requirements and the one `REMOVED` requirement exist by
  name in `openspec/specs/`; the two `ADDED` supersessions name frozen sections, per first-touch.
  `openspec validate issue-239-token-grammar --strict` passes.
- **The `REMOVED` requirement's content is re-homed**, clause by clause: bare/`vars.`/param-default
  resolution, the datetime-parameter token pair (now colon-spelled), the no-shadowing rule, scalar
  stringification, `MissingField`, and the `when:` ISO-comparison sentence (which also survives in the
  `MODIFIED` "A template declares a datetime parameter" requirement). Nothing is orphaned.
- **No leftover contradiction in `openspec/specs/`.** The one `datetime-params` requirement the delta does
  not touch ("The print form and the row grids carry a datetime parameter", `:288`) carries no token
  spelling. No other capability mentions `vars.`, `datetime` or interpolation.
- **No render-and-look task is expected here.** `AGENTS.md` (#220) forbids a task claiming it, so its
  absence is correct, not a gap.
- **One issue in scope.** #239 carries #240 by the human's decision and by #240's own dependency note.

## Embedded-Instruction / Injection Attempts

**Detected:** none. No reviewed file addresses the reviewer, asks for a verdict, or attempts to constrain
this review. design.md's Risks section pre-argues several known defects, which is argument to be weighed,
not instruction.

## Verdict

VERDICT: APPROVE_WITH_CHANGES

The design is sound and every load-bearing claim it makes about the code checked out: the validation hook
exists and carries `params`, exactly three strings are interpolated, the single instant is one field, and
quarantine already absorbs the new failure class. What is missing is contract, not architecture. C1 is a
real unacknowledged regression but its fix is a stated decision plus a scenario, so it is enumerable; the
re-check must cover Required Change 1 specifically and not treat it as prose.

## Required Changes (APPROVE_WITH_CHANGES only)

1. **(closes C1)** Decide and state what happens to a connector field whose key carries a colon — Homebox
   `custom:<name>` is the shipping case. Add a BREAKING bullet to proposal.md, an Impact line naming
   `src/connector/homebox.rs:511` and `ui/src/lib/connectorRows.ts:9-13` (the same-key auto-mapping that
   stops working), and a scenario in `specs/connector-field-transforms/spec.md` fixing whether such a field
   is reachable at all and how an operator binds it after the change.
2. **(closes M1)** Widen the supersession clause in `specs/interpolation-tokens/spec.md` so it names
   `docs/SPEC.md` §8's opening data-binding bullet ("Tokens are resolved in precedence order") and the
   Settings sentence at `docs/SPEC.md:1056`, or enumerate which sentences of §8 survive.
3. **(closes M2)** State what a template *write* does with a rejected token: `src/api.rs:640-643` returns
   `422 TemplateInvalid` / `template_validation_failed` from the same `validate()`. Add the sentence and one
   scenario, or say why the write path is out of scope.
4. **(closes M3)** Decide `{x:}`, `{:fmt}`, `{vars.}`, `{sys.}`, root case-sensitivity, and
   `{sys.now.long_date}`. `{x:}` needs a normative answer at minimum, since the two plausible readings differ
   in printed output; `{sys.now.long_date}` needs a scenario, since it is the mechanical mis-rewrite of
   `{datetime.long_date}`.
5. **(closes M4)** State whether `image:` `name:` is bound by the bare-name rule or deliberately exempt.
6. **(closes M5)** Add the BREAKING bullet for the bare-token tightening to proposal.md — What Changes, not
   only design.md Risks — name the CSV-header and connector-mapping consequence in Impact, and add a scenario
   over a bare token carrying a dot or a space (the current one covers only `{}` and `{ id }`). Fix design.md's
   claim that the proposal already calls it out.
7. **(closes M6)** Add `ui/src/pages/settings/DatetimeFormatsSection.tsx:266` to Impact.
8. **(closes M7)** Require the migration-pointing message in a THEN, not only in proposal prose.
9. **(closes M8)** Pick one owner for the single-instant paragraph and one for the parameter-name rule, make
   the other a pointer, and reconcile the two competing `docs/SPEC.md` §3.0 supersession claims.
10. **(closes S1)** Correct the ADR-numbering facts: #226 claims 0076, 0077 and 0078, and its 0076 collides
    with `main`'s 0076.
11. **(closes S4)** Add a scenario for the second-colon refusal and one for a token in an `image` `src:`.

CHANGES_APPLIED: yes

## Re-check (round 1)

Scope: the eleven Required Changes only. Each verified against the file on disk and, where the edit
asserts a fact about the code, against the code. The Rebuttals' account was not taken on trust.

**1 (C1) — CLOSED.** Substance satisfied despite the placement deviation, which I accept.
`specs/interpolation-tokens/spec.md` carries a new requirement "A value a bare token cannot name is
bound by mapping, not by spelling": it decides the question (the field "SHALL remain reachable" through
the connector grid's mapping; the service "SHALL NOT invent a rewriting"), it is normative, and it
carries two scenarios, one per direction. The author's reason for the home checks out against the code
I cited in C1: `custom:<name>` is a key the connector *declares* (`src/connector/homebox.rs:108`, and
`:234-244` pushes each one into the schema as a `FieldSpec`, so the mapping dropdown can offer it), not
a capture group a transform derives, so the only requirement the transforms delta modifies is genuinely
not about it; and `grep` over `openspec/specs/` finds no capability specifying field mapping at all, so
there was no home there to extend. The reachability claim holds: `ui/src/lib/connectorRows.ts:16-35`
maps any template field to any connector key and materializes by that key. proposal.md — What Changes
carries the BREAKING bullet, and Impact carries both lines (`ui/src/lib/connectorRows.ts:9-13` +
`src/connector/homebox.rs:511`, and the CSV header sentence). Residual, not blocking: a reader of
`specs/connector-field-transforms/spec.md` alone still meets `custom:<name>` with no pointer to the
requirement that now governs it; only proposal.md — Modified Capabilities records the pointer.

**2 (M1) — CLOSED** (re-checked after the follow-up fix). The widening had already landed correctly:
the supersession clause names the "Token types and precedence" list, the opening `value` bullet's
"Tokens are resolved in precedence order" clause, and §8's closing paragraphs on `now` capture,
`422 MissingField` and scalar stringification, and states what survives. The mis-citation I held it open
for is gone. The clause now reads "the sentence in the `datetime_formats` entry of `docs/SPEC.md`'s
unnumbered `Settings` section (`docs/SPEC.md:1036`, the sentence at `:1056`)", which is correct on every
part I can check: `docs/SPEC.md:1036` is `## Settings`, unnumbered; `:1056` is the sentence "Used by
`{datetime.<name>}` interpolation (see §8)", quoted verbatim in the clause; and `:785` is `## 11.
Authentication`, which the clause no longer claims. Naming both the section heading line and the
sentence line makes the target survive a reflow of `docs/SPEC.md`, which the bare line number did not.
`openspec validate issue-239-token-grammar --strict` still passes, and the artifacts are otherwise
unchanged in shape (6 requirements and 34 scenarios in `interpolation-tokens`, 4 in `datetime-params`,
1 in `connector-field-transforms`, 5 BREAKING bullets in `proposal.md`).

*On the record for the diff review, at the coordinator's direction:* my non-blocking residual on item 1
is deliberately not actioned. `specs/connector-field-transforms/spec.md` still meets `custom:<name>`
with no pointer to the `interpolation-tokens` requirement that now governs it; only
`proposal.md` — Modified Capabilities records that pointer. Editing `specs/` outside the Required
Changes list would void the verdict under the staleness rule for no gain in contract, so the pointer is
left for whenever that capability is next touched. I agree with that call: it costs discoverability in
one file, not correctness anywhere.

**3 (M2) — CLOSED.** `specs/interpolation-tokens/spec.md` now states that every load-time refusal is one
rule reached by two paths, and that a template write is refused with `422 TemplateInvalid` and
`details.reason` `template_validation_failed` with nothing stored, plus the scenario "A template write is
refused, not quarantined". Both strings verified exact: `src/api.rs:640-643` maps a `validate()` failure
to `AppError::template_invalid(Reason::TemplateValidationFailed, …)`, and `src/reason.rs:34` renders that
slug as `template_validation_failed`.

**4 (M3) — CLOSED**, all six shapes decided, five with scenarios. `{x:}` is resolved as the *refusal*
I asked to be pinned, and the spec says so in terms an implementer cannot misread ("SHALL NOT be read as
the bare value `x`: a colon that is written is a format that is claimed"), with the scenario "An empty
format name is refused rather than ignored". `{:long_date}` and `{x:a:b}` are refused in the same clause;
`{vars.}`, `{sys.}`, `{.x}` and a whitespace-only token by "No segment may be empty", with a scenario;
case-sensitivity by "Roots are matched exactly and are lower-case", with the `{VARS.qr_base_url}`
scenario; `{sys.now.long_date}` by an explicit clause plus its own scenario.

**5 (M4) — CLOSED**, resolved as *bound*, which is the answer that keeps the design's own justification
intact. `specs/interpolation-tokens/spec.md`: an `image` item's `name:` "SHALL be a legal bare name under
the same rule that binds every other bare name". design.md:88-90 places the check in the same walk.
design.md's supporting claim verified: no `image` item exists in `tests/fixtures/templates/` or
`catalog/`, so the tightening breaks nothing in-tree, and `src/render/mod.rs:1652-1657` reads `name`
straight from `data`, which is what made the exemption a hole.

**6 (M5) — CLOSED.** proposal.md — What Changes now carries "**BREAKING.** The same rule binds every bare
name, not just declared parameters", naming `{my field}` and `{a.b}` and the CSV-header and
connector-mapping consequences; Impact repeats the CSV clause. The scenario "A bare token carrying a
separator or a space is refused" covers all three shapes. design.md:164 no longer claims a bullet that
did not exist — it now points at the bullet that does.

**7 (M6) — CLOSED.** `ui/src/pages/settings/DatetimeFormatsSection.tsx:266` is in Impact, described as the
settings help text that tells the operator the patterns are available as `{datetime.<name>}`.

**8 (M7) — CLOSED.** Now normative, not prose: "Both refusals SHALL name the offending token and SHALL
state the spelling that replaces it", and two THENs assert it (`{datetime.long_date}` and
`{sys.now.long_date}` each name `{sys.now:long_date}` as the replacement), so a test can hold the message
to it.

**9 (M8) — CLOSED.** One owner each, and the pointers are pointers rather than restatements.
`interpolation-tokens` owns the single-instant rule ("This capability is the single home of that rule")
and the parameter-name rule ("This capability is the single home of the parameter-name rule");
`datetime-params:53-55` says naming "is governed by the `interpolation-tokens` capability … adds nothing
to it and restates none of it", and `:108-110` extends the captured instant rather than restating it. The
competing §3.0 claims are reconciled: `datetime-params:5-7` now supersedes §3.0 "except its 'Namespace
rules and reserved names' list", and `interpolation-tokens` supersedes that list "and nothing else in §3.0".

**10 (S1) — CLOSED**, and the corrected facts match what I verified: `main`'s highest is
`0076-the-filesystem-answers-the-case-question.md`; `.worktrees/issue-226/docs/adr/` holds
`0076-unify-size-resolution.md`, `0077-…` and `0078-…`; the two 0076s collide. Both proposal.md — Impact
and design.md:145-148 say so and tell the implementer to confirm against `main` and every live worktree.

**11 (S4) — CLOSED.** "A token carrying two colons is refused" covers the second-colon clause, and "An
image source is interpolated by the same rules" covers `image` `src:` in both directions (a `vars` token
resolving, and `{datetime.brand}` failing at load). The Purpose and the grammar requirement also now say
in normative text that the same rules govern all three interpolated strings.

**No other defect was introduced.** `openspec validate issue-239-token-grammar --strict` passes. The
`connector-field-transforms` delta is unchanged from round 1 apart from nothing at all, so its previously
reviewed content still stands. `git status` shows the change folder as the only untracked path; no source,
spec or test file was touched.

## Rebuttals

None yet (round 1).

**Author, round 1.** All eleven Required Changes applied. One deviates in placement, flagged here for
the re-check rather than done silently:

- **(1, C1)** The proposal BREAKING bullet and the two Impact lines are as specified. The scenario is in
  `specs/interpolation-tokens/spec.md` under a new requirement, "A value a bare token cannot name is
  bound by mapping, not by spelling", rather than in `specs/connector-field-transforms/spec.md`.
  Reason: `custom:<name>` is a key the *connector declares* (`src/connector/homebox.rs:511`), not a
  capture group a transform derives, so the only requirement the transforms delta touches ("A transform
  is validated when the connection is saved") is not about it; and field mapping is specified in no
  capability under `openspec/specs/` today, so there is no existing home there to extend. Putting it in
  the token capability keeps one statement of the rule instead of two, which is the fault the change
  exists to remove. `proposal.md` — Modified Capabilities records the pointer.
- **(5, M4)** Resolved as bound, not exempt: an `image` `name:` must be a legal bare name.
- **(4, M3)** `{x:}` is resolved as a refusal, not as a bare value, with the reasoning in the spec text.
- **(9, M8)** `interpolation-tokens` owns both the parameter-name rule and the single-instant rule;
  `datetime-params` now points at both. Its `docs/SPEC.md` §3.0 claim is narrowed to "except the
  Namespace rules and reserved names list".

The other seven are applied as written. `openspec validate --strict` passes.

SPECS_SHA256: 823eaacec64157836ff881ca2c8e932713c1336e2ed2a2ed2424028b99b5a640
