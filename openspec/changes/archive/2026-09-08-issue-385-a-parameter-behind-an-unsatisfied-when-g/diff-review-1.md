TREE_SHA256: f34ce4e102887646443aebefe46613471ec39c0f8ac2f085cac73639481b527a
SPECS_SHA256: bfd8d60a2d0387ab12d52fac12bd0335aa7aeaa2f15290ab4d3453d7927843d2

I reviewed the diff, ran every gate, and verified the new tests fail against the pre-change sources.

## What I verified

- `git diff HEAD` touches exactly `ui/src/pages/{Connect,Import}.{tsx,test.tsx}` (task 4.3 holds; no Rust, no `LabelGrid.tsx`, no `labelInputs.ts`). [verified]
- `cd ui && npm run lint && npm run test && npm run build`: all green, 574 tests. `cargo fmt --check`, `cargo clippy --all-targets --all-features`, `cargo test`: all exit 0. [verified]
- Red-before-green: I copied `ui/` to a scratch dir, restored `HEAD`'s `Connect.tsx`/`Import.tsx`, and ran the two page suites. All 8 new tests (3.1-3.8) failed; the 73 pre-existing tests in those files still passed. The tests earn their claims. [verified]
- `all ⊇ per-row list` holds structurally: `derive_inputs_internal` with `resolved_data: None` walks every branch and every `repeat:` subtree once with the same `repeated_names` set (`src/templates.rs:376-406`), and both emit in `self.params` declaration order (`src/templates.rs:415`). So no column can vanish and `inputs.all` order is declaration order. [verified]
- `openspec validate --changes --strict` passes. [verified]

## Findings

**1. [BLOCKING] The delta leaves a published scenario asserting the mechanism this change removes.**

`openspec/specs/template-inputs/spec.md:217` states the Import grid "walks the `POST /api/templates/{id}/inputs` result and renders **columns** or validation in `title`, `subtitle`, `code` order", and `:222` says the Connect grid "walks the input list" to render. After this change, columns come from `GET /api/templates/{id}`'s `inputs.all` (`ui/src/pages/Connect.tsx:203-207`, `ui/src/pages/Import.tsx:111-128`), and the delta says so normatively at `specs/template-inputs/spec.md:193-195`. That requirement is not in the delta, so archive publishes both statements in one capability file with no precedence between them.

`design.md:115-125` considered this and dismissed it: "What narrows is the parenthetical describing which response the grid walks for its columns." The parenthetical in that scenario is the file citation `(ui/src/pages/Import.tsx:136)`; the response-walked claim is the THEN outcome. The justification does not match the text it excuses. Separately, that citation is now stale in its own right: `Import.tsx:136` pointed at `displayedFields` at `HEAD` and is a blank line after the diff.

Fix is small: add `An input list describes the controls one label needs` to the delta as `MODIFIED`, correcting the two scenarios' mechanism clause (columns from `inputs.all`, validation from the row's own list) and refreshing the `Import.tsx` line citation.

**2. [non-blocking] Connect derives the same list twice.** `ui/src/pages/Connect.tsx:176` (`templateFields`) and `:203-205` (`requiredUnion`) are now character-for-character the same memo over the same dependency. `displayedFields` (`:207`) could be `templateFields`. `requiredUnion` is also now a misnomer: nothing about it is required-only, and the union is the service's, not one this component builds.

**3. [non-blocking] New dead guards, inconsistent between the two pages.** `detail.inputs.all ?? []` (`ui/src/pages/Import.tsx:113`, `:119`) and `detail.inputs.all?.find(...)` (`:134`) can never fire: `TemplateInputs.all` is `InputSpec[]`, non-optional (`ui/src/api/types.ts:61`). Connect's matching lines (`:204`, `:212`) are written without them, so one change introduces two spellings of the same access.

**4. [non-blocking] An unrelated global test spy.** `ui/src/pages/Connect.test.tsx:298` adds `vi.spyOn(HTMLAnchorElement.prototype, "click")` to the file-wide `beforeEach`. The new describe already has its own at `:2171`. I deleted line 298 in the scratch copy and ran the file: 50/50 pass, with only a jsdom "Not implemented: navigation" log surfacing from a pre-existing test. So line 298 is not needed by this change and only alters the environment of the tests that came before it. Also `Connect.test.tsx` now ends with a stray blank line after the final `});`.

Findings 2-4 are quality, not correctness; only finding 1 forbids landing.

VERDICT: REVISE
