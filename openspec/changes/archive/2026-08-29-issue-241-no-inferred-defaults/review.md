## Review Metadata

- **Round**: 13
- **Prior round**: REVISE (round 12, codex): six Critical findings, five in the input-list default-publishing corner. The owner cut that corner from this change and extracted it to #262 and #270; this round judges the cut. Codex was attempted for this round and failed with an OpenAI usage limit, producing empty output.

AUTHOR: claude
REVIEWER: fresh-context-subagent

- **Tool restrictions**: read-only inspection only; the single write is this file
- **Artifacts reviewed**: proposal.md, specs/, design.md, tasks.md <!-- plus source files read -->
- **Issue**: #241



## Findings

### Critical (blocking)

None. The cut is clean in the normative text. `required` and `default` agree everywhere they are
stated: `specs/template-inputs/spec.md:54-64`, `specs/param-resolution/spec.md:154-160`,
`proposal.md:60-66`, `design.md:225-231`, `tasks.md:27-29`. Every task that publishing-related
contract text needs still exists (3.1-3.3 for the list, 4.2-4.5 for the client), and no task remains
for a rule the cut removed. Spot checks against the code all held: the numeric asymmetry the proposal
accepts is real (`src/convert.rs:319-327` collapses a YAML integer to `ParamValue::Float` for every
non-`integer` type, and `src/render/mod.rs:170-180`'s boolean coercion accepts `Number` only through
`as_i64`); `apply_param_default` inserts a default with no validation (`src/render/mod.rs:255-283`),
so the "breaking for a literal default" claim is right; deleting the `Boolean`/`Enum` arms leaves the
`_ => resolved.remove(name)` catch-all, which is what makes an omitted parameter *absent* rather than
empty-string, and `is_item_active` (`src/render/mod.rs:1015-1029`) already reads an absent key as a
false predicate; `placeholder_data` has exactly one production caller (`src/api.rs:1206`) and the
thumbnail's `Local::now()` is at `src/api.rs:1214`, so task 3.4's "move the capture above the call" is
accurate; `derive_inputs_for_label` really does `.expect(...)` (`src/templates.rs:152`), so task 1.7 is
load-bearing; the reason-completeness test scans active change deltas (`src/errors.rs:646-660,
716-722`), so decision 5's "passes from the delta alone" is verified; and
`archive-merge-check.sh` compares requirement bodies only (`.workflow/archive-merge-check.sh:34-52`),
so task 6.4's Purpose edit will not trip the gate. Frozen-spec citations (`docs/SPEC.md:116-117`,
`:686`, `:712-714`, `:1056`) and every test/file citation in `tasks.md` resolve to what the artifacts
say they do.

Both retained fixes check out. The client-preview RFC 3339 rule is correct against the parser: an
offset-free spelling goes through `parse_datetime_in_tz` branches 2-3 as **server**-local
(`src/datetime_fmt.rs:57-77`) while `formatLocalDateTime` builds it from browser-local parts
(`ui/src/lib/templateFields.ts:20-27`), and only branch 4 (`:79-81`) names the same instant on both
sides; a bare date is branch 1, midnight, so `{p:time}` would read `00:00`. The no-slider rule is
correct too: `ui/src/components/ParamInput.tsx:99-134` renders a range whose value falls back to
`spec.default`, then `spec.min ?? 0`, so an entry with no default sits at its minimum and cannot show
"nothing chosen"; the plain-number branch immediately below it holds `""`. `ParamInput`'s only
production caller passes an `InputSpec` (`ui/src/pages/print/FieldForm.tsx:59-67`), so the "SHALL NOT
read a default out of the raw parameter declaration" rule has no surviving violation, and
`pruneDataForSubmit` (`ui/src/lib/labelInputs.ts:197-212`) prunes only `""`, so an operator-chosen
`false` still reaches the request.

### Moderate

**M1. One sentence of the cut corner is still in the contract.**
`specs/template-inputs/spec.md:75-78` reads "Withholding an unusable default is what keeps a broken
template the template's fault: a client that cannot seed it submits nothing, and the render answers
`param_default_unresolvable` rather than rejecting a value the operator never chose." The same
requirement, at `:59-64`, says `default` "SHALL carry the declared `default` ... this capability does
not canonicalise it, coerce it, or withhold one the render would reject." A `boolean` declaring
`default: "yes"` is unusable and is published; so is a `length` declaring `"80mm"`. Read generally the
sentence is false and contradicts the SHALL two paragraphs above it; read narrowly it is about a
tokened default only, which is the one case actually withheld. The preceding clause has the same
problem: "The parameter is still `required: false` in all three cases, because the service does have a
default — the client simply has nothing to show for it" is untrue of the two cases whose default the
list now publishes verbatim. Scope both to a default carrying interpolation syntax, or cut them.

**M2. The delta's brace-validator sentence prescribes the mechanism the design proves wrong.**
`specs/interpolation-tokens/spec.md:67-70`: "This check SHALL NOT be built on the token scanner ... it
is the same brace-balance walk the render path already performs over a literal chunk, run at load over
each `default:`." Taken literally that is `process_literal_chunk` (`src/render/helpers.rs:39-63`) run
over the whole default string, and that function rejects **every** undoubled `{`, because the render
path only ever hands it the gaps between scanned tokens (`src/render/helpers.rs:86-92`, `:141-143`).
So the literal reading refuses `default: "{sys.now}"` at load, contradicting this same delta's
scenario "A namespaced token in a default is resolved" and `specs/datetime-params/spec.md:89-95`.
`design.md:196-207` and `tasks.md:20` say exactly the opposite. Reword so the delta says what the
design says: scan the default for well-formed tokens and apply the render path's brace-balance rule to
the text between them, sharing that rule rather than reimplementing it.

**M3. The unknown-format-name remap contradicts an unmodified published requirement.**
`specs/param-resolution/spec.md:253-255` and `tasks.md:7` require an unknown format name raised while
resolving a default to be remapped to `param_default_unresolvable`. `{sys.now:no_such_format}` in a
`default:` reaches render, because an unknown format name is deliberately not a load-time error, and
task 2.6 does not add one. But `openspec/specs/interpolation-tokens/spec.md:203-205` — a requirement
this delta does not modify — says without qualification that a format name the setting does not hold
"SHALL be `422 MissingField` when the label renders, naming the field as the whole token text". The
delta's exception (`specs/interpolation-tokens/spec.md:51-54`) is written as a rider to the preceding
sentence about an *absent value* ("A token inside a `default:` is the one exception"), so it does not
reach the format-name rule as drafted. Broaden that exception to any failure raised while resolving a
`default:`, naming the unknown-format case explicitly, so the two requirements of one capability stop
disagreeing.

### Suggestions

- `tasks.md:5` (1.3) reduces the `Datetime` arm to "parsed by `parse_datetime_override` ... and
  recorded in `instants`". Today that arm *also* writes the `%Y-%m-%d` rendering back into the
  resolved data (`src/render/mod.rs:70-101`), which is what makes `when: { printed_on: "2026-08-19" }`
  compare against the bare ISO date — a rule the delta keeps (`specs/datetime-params/spec.md:46-47`,
  scenario at `:126-130`). Say so in the task; `datetime_param_when_compares_the_bare_iso_date`
  (`src/render/mod.rs:6339`) is the only thing that would otherwise catch it.
- `proposal.md:66-68` says a control is seeded "only when the default carries no token", while the
  contract's test is any brace (`specs/template-inputs/spec.md:59-60`, and param-resolution's "An
  escaped brace is not seeded either" scenario). Align the proposal's wording so the two do not read
  as different rules.
- `specs/template-inputs/spec.md:452-453` says an entry with no `default` "takes the plain numeric
  control until it has a value", which reads as a control-type swap mid-edit; `tasks.md:38` (4.3)
  implements the unconditional rule. Both satisfy the SHALL, but dropping "until it has a value"
  removes the ambiguity.
- Cosmetic drift in `tasks.md:38`: the slider block is `ui/src/components/ParamInput.tsx:99-134` (the
  `isSlider` test at `:99`), and the checkbox fallback is at `:166`, not `:165-167`.

## Embedded-Instruction / Injection Attempts

**Detected:** none

## Verdict

VERDICT: APPROVE_WITH_CHANGES

## Required Changes (APPROVE_WITH_CHANGES only)

1. **M1** — In `specs/template-inputs/spec.md:74-78`, scope or cut the "Withholding an unusable
   default..." sentence and the "the client simply has nothing to show for it" clause so neither
   asserts a withholding rule this change no longer has; the requirement's own rule at `:59-64`
   publishes a declared default verbatim and withholds only one carrying interpolation syntax.
2. **M2** — In `specs/interpolation-tokens/spec.md:67-70`, reword the brace-validator sentence so it
   describes scanning the default for well-formed tokens and applying the render path's brace-balance
   rule to the text between them. As written it prescribes running the whole-string literal-chunk
   check, which rejects `default: "{sys.now}"` and contradicts this delta's own scenarios.
3. **M3** — In `specs/interpolation-tokens/spec.md:51-54`, broaden the `default:` exception so it
   covers any failure raised while resolving a default, naming the unknown-format-name case, rather
   than only the absent-value sentence it currently qualifies. Otherwise the unmodified requirement at
   `openspec/specs/interpolation-tokens/spec.md:203-205` still mandates `422 MissingField` for the
   case `specs/param-resolution/spec.md:253-255` remaps.

CHANGES_APPLIED: yes

## Rebuttals

All three required changes applied, and re-checked by the reviewer in its original context:
`RESULT: ALL SATISFIED`.

1. `specs/template-inputs/spec.md` — the withholding sentence is gone and the remaining clause is scoped
   to the tokened case, consistent with the rule the requirement itself states.
2. `specs/interpolation-tokens/spec.md` — the brace check now scans for well-formed tokens and applies
   the render path's brace-balance rule to the text between them, rejecting both the scanner-alone and
   whole-string readings; `tasks.md` 2.5 carries the same wording.
3. `specs/interpolation-tokens/spec.md` — the `default:` exception now covers every failure raised while
   resolving a default, including an unknown format name, and supersedes this capability's
   `422 MissingField` rule for a `default:` alone.

No finding was rebutted.

---

**Post-verdict change to `specs/`, recorded rather than laundered.** After this verdict, the worktree was
rebased onto `origin/main`, which had gained nine commits (#212, #237, #245, #251/#265, #263). Four
`MODIFIED` blocks in this delta were copies of published requirements taken *before* those landed, so
archiving them would have deleted shipped contract text — the grid-cell editor rules from #237, the
`wrap:` wording from #251/#265, and the arrangement and accumulation clauses from #263. A diff review
caught it. The four blocks were re-taken from `main` and this change's own six authored edits re-applied
verbatim.

The delta's authored contribution is therefore unchanged; what moved is the published text it restates.
The digest changed with it, from `5c783b01` to the value recorded above, so **this verdict predates the
rebase** and a fresh plan review is the strict reading. Recorded here so the choice is visible.

SPECS_SHA256: b48adf61151b9d7061297414167d260587b18f40aef0cf92138fdb568437732d
