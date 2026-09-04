## Findings

**F1 (blocker). The delta silently deletes 17 published scenarios and one normative paragraph.**

A `MODIFIED` requirement replaces the named requirement whole, scenarios included. Evidence in this repo: the last delta on this very requirement, `openspec/changes/archive/2026-08-28-issue-251-text-wrap-flag/specs/layout-sizing/spec.md`, carried 20 scenarios, and `openspec/specs/layout-sizing/spec.md:689-934` now holds exactly those 20. The same holds for `2026-09-03-issue-363-.../specs/layout-sizing/spec.md` (8 scenarios in, 8 published).

This delta carries 6 scenarios: 4 new, plus `A fully authored text is still laid out and still enforces its policy` and `An over-wide glyph is shortened when the marker still fits`. Archiving it therefore deletes 18 published scenarios, of which only `A long word is split, not overflowed` (`openspec/specs/layout-sizing/spec.md:804-808`) is intentional. The other 17 sit contiguously at `openspec/specs/layout-sizing/spec.md:818-933` and cover behavior this change does not touch: marker/box refusals, hugging texts, blank-line accounting, CRLF, alignment, format independence, and the centred-ink overflow rule.

The prose diff drops one paragraph too, unmentioned anywhere in `proposal.md` or `design.md`: `openspec/specs/layout-sizing/spec.md:729-734`, the record that this requirement supersedes ADR-0045's blank-edge rule and the previous step-2/step-4 ordering. Dropping it removes a supersession record, which under the precedence rule in `AGENTS.md` would silently reinstate the frozen ADR's claim.

**F2. The plan asserts a test outcome that is false, and leaves a `wrap: true` test it breaks unaddressed.**

`proposal.md:62` says "existing `wrap: false` tests pass untouched" and says nothing about existing `wrap: true` tests. `src/render/helpers.rs:1602-1636`, `layout_text_ellipsizes_every_over_wide_line_not_only_the_last`, is a `wrap: true` test: it lays out `"WW"` at `FontSize::Fixed(6.0)` in a box between `text_width("...")` and `text_width("W")`, then asserts `m.lines.len() > 1`. The two lines come from the chunking loop at `src/render/helpers.rs:912-921` splitting `WW` into `W` / `W`. With that loop deleted, `WW` is one word wider than the box, stays whole, and `wrap_text` returns one line, so the assertion fails. The plan neither lists it under Impact nor says whether it should be rewritten (the behavior it names, per-line ellipsizing, stays live via multi-word and `\n` values) or deleted. Left as is, the implementer decides that on their own.

**F3. The new scenario `No emitted line is ever a mid-word fragment without a marker` is false as written.**

`specs/layout-sizing/spec.md:143-144` enumerates the permitted forms exhaustively: "every emitted line is either a whole word, words joined by single spaces, or a line carrying the `...` marker". A blank line is none of those, and the same requirement mandates blank lines at `specs/layout-sizing/spec.md:34-35` ("Every line produced by step 1, blank or not, gets its own line box"). `wrap_text` returns `vec![String::new()]` for a whitespace-only segment (`src/render/helpers.rs:896-898`), pinned by `whitespace_only_segment_keeps_its_line` (`src/render/helpers.rs:1804-1817`, `wrap: true`, asserts `m.lines[1] == ""`). A scenario contradicted by unchanged behavior is a scenario no implementation can satisfy.

**F4. The delta does not say which line carries the marker when the over-wide word is not the last line, which is the case this change makes common.**

`src/render/helpers.rs:836-841` ellipsizes each over-wide line in place, independently of the dropped-lines path. So `"Refrigeration unit"` at the floor emits `Refrig...` then `unit`: the marker sits mid-block and the final line carries none. The delta's shortening prose still reads "Shortening keeps the lines that fit and appends `...` to the last" (`specs/layout-sizing/spec.md:62`), and the new paragraph at `:74-78` only says "step 3 shortens or refuses it". Under the old contract this case could not arise for `wrap: true`, because chunking guaranteed no line was over-wide, so this is newly reachable and unspecified. The code's actual behavior is already pinned by `layout_text_ellipsis_leaves_a_final_line_that_fits_intact` (`src/render/helpers.rs:1643-1678`); the contract should name it rather than leave the implementer to infer it.

**F5. `design.md:7-10` states a premise that is demonstrably wrong.**

It claims the width loop in `text_fits` "is dead code today" under `wrap: true`. It is not. The chunker pushes only when the chunk is non-empty (`src/render/helpers.rs:914`), so a glyph wider than the box yields an over-wide chunk that reaches the width loop at `src/render/helpers.rs:670-673` and fails it. That is exactly the published over-wide-glyph scenario and the mechanism the test at `src/render/helpers.rs:1602` exercises. The accurate premise is "unreachable except for an over-wide glyph". The conclusion survives, but the design is the permanent rationale record and this sentence misstates the code it cites.

**F6. Citation errors carried over from the issue.**

`proposal.md:6-7` attributes the chunking to "`break_lines` (`src/render/helpers.rs:900-965`)". `break_lines` is at `src/render/helpers.rs:637-652` and does no chunking; the chunking is in `wrap_text`, `src/render/helpers.rs:895-965`. `proposal.md:26` cites the first chunking loop as `906-920`; it is `909-926` (inner loop `912-921`). The second citation, `941-956`, is right, as are `670-674`, `801-819` and `836-841`.

## Verified as sound, for the record

- `text-wrap-flag` needs no delta: `grep` over `openspec/specs/text-wrap-flag/spec.md` finds no mention of splitting, words or characters.
- The absence of `tasks.md` is correct, not a gap: `tools/openspec-loop/workflow/run-stage.sh:295` withholds it from the plan stage deliberately.
- `MODIFIED` rather than first-touch `ADDED` is right: the requirement already lives in `openspec/specs/layout-sizing/spec.md:689`.
- The "own line" claim holds mechanically: with the loops gone, an over-wide word cannot share a line, because `current_width + space_width + word_width <= width_pt` (`src/render/helpers.rs:930`) is already false once `current_width > width_pt`.
- The "~76 iterations" figure in `design.md:107` matches the comment at `src/render/helpers.rs:689-690`.
- No caller outside `src/render/` reaches `layout_text` or `largest_fitting_font`, so the load-time-parity claim at `design.md:27-30` holds.

VERDICT: REVISE
