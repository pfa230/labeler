1. **Null handling is contradictory.** `design.md:124-131` preserves `text.color: null` as absence, but `specs/colour-vocabulary/spec.md:13-47` requires every colour to be one of three string forms and refuses every non-string value. Both contracts cannot be implemented.

2. **The shared-vocabulary requirement forbids defaults that other requirements mandate.** `specs/colour-vocabulary/spec.md:101-108` says no colour field may carry its own default, while `specs/text-ink/spec.md:5-7` defaults omitted `text.color` to black and `specs/shape-paint/spec.md:73-99` defaults omitted `stroke.color` to black.

3. **The read-back requirement silently adds an undisclosed text API break.** `specs/colour-vocabulary/spec.md:271-273` requires every omitted colour with a default to be reported, which includes `text.color`. The existing contract instead omits an absent text colour (`openspec/specs/text-ink/spec.md:23-24`), while the proposal and design discuss only changing shape serialization and only identify the omitted-stroke behavior (`proposal.md:31-36`, `design.md:81-88`). The plan therefore does not establish whether an uncoloured text item is returned without `color` or as `color: "black"`.

4. **The authoring guide would retain the exact false distinction this issue removes.** `docs/AUTHORING.md:500-504` teaches that shape colours use CSS values while text `ink` uses Typst’s palette. Neither the proposal’s impact list nor the design includes updating it, despite `AGENTS.md:32` identifying it as the worked-example guide to the template model.

5. **The design parks follow-up work in Markdown without a tracked issue.** `design.md:124-130` calls the null asymmetry “worth a follow-up issue,” but cites none. `AGENTS.md:34-38` requires deferred work to become a GitHub issue rather than remain a documentation TODO.

## Required changes

1. Scope the vocabulary’s non-string refusal to a colour value that is present. Explicitly state in the `text-ink` requirement and a scenario that `text.color: null` is treated as absence, while retaining the existing `shape-paint` refusal of `background: null` and `stroke.color: null`. Clarify in `design.md` that the shared parser handles non-null colour strings while field-level deserialization preserves this null distinction.

2. Replace `specs/colour-vocabulary/spec.md:108` with a rule forbidding field-specific vocabularies or name tables, while explicitly allowing field-level defaults defined by `text-ink` and `shape-paint`.

3. Preserve the existing text read-back behavior: qualify `specs/colour-vocabulary/spec.md:271-273` so default materialization applies to `stroke.color`, and state in the new `text-ink` requirement—with a scenario—that omitted `text.color` remains omitted from `GET /templates/{id}`. Record that decision in `design.md`.

4. Add `docs/AUTHORING.md` to the impact/design work and replace its stale colour paragraph with the unified sixteen-name/hex/reference vocabulary, `text.color`, and the removal of `ink`.

5. Remove the untracked follow-up statement from `design.md`; the preserved null behavior is a deliberate non-goal, not deferred work in this change.

The author applies these edits and NO further review follows.

VERDICT: APPROVE_WITH_CHANGES
