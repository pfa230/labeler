## Why

Issue [#251](https://github.com/pfa230/labeler/issues/251). `multiline` on a `text` item decides two
unrelated things under one name. Step 1 of the text layout pipeline
(`openspec/specs/layout-sizing/spec.md`, "Text is laid out against the box it will get") says
`multiline: true` wraps the value to the box width and `multiline: false` **keeps only the first input
line**. So one flag controls both soft wrapping — a layout decision the template owns — and whether a
line break the caller typed survives at all, which is not a layout decision. A newline in a value is
stated intent; discarding everything after it loses caller data with no error and no trace.

The conflation leaks outward. `params[].multiline` is a UI form-control hint, yet the print form has to
guess the control from the layout, because a newline typed into a field that a single-line item renders
would be thrown away.

**This change was first planned against the pre-#226 renderer and is re-scoped onto what landed.**
ADR-0080/0081/0082 unified the layout pipeline, replaced `auto` with `content`/`fill`, and made
overflow an authored policy. Several things the original plan set out to fix are already fixed there:
fitting no longer depends on the font size kind, the text is re-broken at each candidate size, and
clipping is no longer an outcome. What remains is the conflation itself, and the two rules that follow
from resolving it.

## What Changes

- **Hard line breaks are always honored.** Step 1 stops discarding input lines: a `text` item lays out
  every line of its value, and only soft wrapping stays under the flag. What the box cannot hold is
  still dropped by the overflow policy in step 3, which marks the loss rather than hiding it.
- **BREAKING: the flag is renamed `multiline` → `wrap` on `text` layout items.** `wrap` names what it
  now does. A template still saying `multiline:` on a `text` item is refused on the key's **presence**
  — including an explicit YAML null — and quarantined with an error naming the file, the layout path
  and the rename. No alias, no deprecation window. `params[].multiline` keeps its name and meaning: it
  is a UI hint and is untouched.
- **BREAKING: blank lines are rendered, edges included.** Step 4 currently drops a blank first or last
  line at emission (#127, ADR-0045), and the spec pins the step 2/step 4 order as normative so a
  leading newline shrinks the font without gaining a line. Both go: a value's lines are its segments
  at `\n`, every one of them gets a line box, and the two line counts the current requirement
  reconciles collapse into one. A newline is caller intent at the edges exactly as it is in the middle.
- **The overflow marker reports the field, not the line.** Under `overflow: ellipsis`, *any* line
  dropped for height earns the marker, blank or not, and it lands at the end of the last retained line
  whatever that line holds. Without this, honoring a trailing blank would let `"message\n"` in a
  one-line box render as plain `message`, looking complete while a line the caller wrote is missing.
  With it the label says so, and making room for the marker is what may cost a character or two of
  `message` itself.
- **CRLF is normalised before measurement.** `\r` is unmapped in Inter, so it is charged the `.notdef`
  advance (1344/2048 em) while rendering nothing: a CRLF value reports a label a third wider than its
  ink. The code this replaces used `str::lines()`, which strips a trailing `\r`. A **lone** `\r` is out
  of scope and filed as [#259](https://github.com/pfa230/labeler/issues/259), because treating it as a
  terminator changes what a line is.
- **The input list's controls are out of scope.** Removing the layout-derived control and the
  truncation flag is [#269](https://github.com/pfa230/labeler/issues/269), which depends on this
  change: those rules are only wrong once a `text` item lays out every `\n` segment of a value regardless of `wrap`. Until #269
  lands, the service still derives `multiline_text` and `truncated_elsewhere` and the print form still
  shows the note. That is a stale warning about a truncation that no longer happens, not a wrong
  render.
- **Templates in the repository adopt `wrap:`**, and `catalog/sheet/avery/avery5163.yaml` declares
  `multiline: true` on its `message` param, which it renders through a wrapping item but never
  declared.

## Capabilities

### New Capabilities
- `text-wrap-flag`: the `wrap` field's schema and default, and the refusal of `multiline` on a text
  item — on the key's presence, with the file/layout-path/rename diagnostic, at load and at the write
  endpoint. It supersedes the frozen `docs/SPEC.md` §4.1 clauses naming `multiline` in the text item's
  field list and describing the first-line discard. The layout consequences of the flag stay in
  `layout-sizing` and the input list's controls in `template-inputs`; this capability owns the schema
  and the migration alone.

### Modified Capabilities
- `template-inputs`: two requirements change **without changing behaviour**. The flag `wrap` replaces
  `multiline` where they name the layout item, and `truncated_elsewhere` is redefined to say what it
  now reports — that some `wrap: false` item reads the name — together with the fact that this no
  longer implies any loss, since every item lays out every segment; only the authored `overflow` policy
  may then shorten a line, drop lines, or reject the render, and a shortened or dropped line is marked. Removing the field and the
  layout-derived control is #269; leaving the canonical spec asserting a truncation that cannot happen
  is not an option, because a proposal disclaimer does not reach the archived contract.
- `layout-sizing`: **two** requirements. "Text is laid out against the box it will get, and what does
  not fit is authored" changes in three places — step 1 no longer discards input lines and names `wrap`, step 4
  no longer trims blank edges (which also collapses the intrinsic-height reconciliation and the
  normative step 2/4 ordering note), and the `ellipsis` policy's shortening rule states that any
  dropped line earns the marker. It also names the frozen §3.1 blank-edge bullet it supersedes.
  "Vertical fitting reserves the ink each alignment can expose" (ADR-0084, which landed while this
  change was in review) is carried **behaviourally unchanged**: its only edits are the flag's name in
  its scenarios and dropping its claim that the frozen §3.1 blank-edge bullet stays authoritative,
  which the first requirement now supersedes.

`datetime-params` already specifies the `string` parameter's input-or-textarea mapping and is
unchanged.

**Re-scoped twice, then split.** This change was first planned against the pre-#226 renderer, then
re-scoped onto the unified pipeline #226 landed. While it was in review #200 landed `template-inputs`,
moving input derivation from the client into the service — including the layout-derived control and the
truncation warning this change originally removed. That half is now
[#269](https://github.com/pfa230/labeler/issues/269), which depends on this one: its rules are only
true once a `text` item lays out every `\n` segment of a value regardless of `wrap`. Splitting keeps each half against a base that
holds still, after two collisions in the same files.

## Impact

- **Template schema (breaking).** `src/raw.rs` `TextRaw` (`deny_unknown_fields`), `src/models.rs`,
  `src/convert.rs`, and `src/templates.rs`, which clones the layout item's flag explicitly
  (`src/templates.rs:1197-1218`) and is easy to miss. Every user template using `multiline:` on a
  `text` item stops being served until edited.
- **Renderer.** The break/shrink/overflow/emit pipeline in `src/render/`, plus whatever the removal of
  the blank-edge trim touches in intrinsic-size reporting.
- **Templates.** `catalog/tape/brother/*.yaml`, `catalog/sheet/avery/avery5163.yaml`,
  `tests/fixtures/templates/*`.
- **Docs.** An ADR superseding ADR-0045's blank-edge rule and amending ADR-0082 in three places — its
  step 1, its step 4, and its overflow decision, which gains the rule that any dropped line earns the
  marker — plus its row in `docs/adr/README.md`, and `docs/AUTHORING.md`/`README.md` where the old name
  appears.
- **Blast radius.** A value already carrying `\n` renders as several lines where a non-wrapping item
  rendered one; a value with blank edges is fitted as more lines and may shrink or gain a marker. CSV
  imports and connector fields carry both.
- **Downstream.** Blocks [#237](https://github.com/pfa230/labeler/issues/237). Related to
  [#200](https://github.com/pfa230/labeler/issues/200), which touches the same UI walker.

## Notes

The pre-#226 implementation of this change is preserved at commit `ff3bd9c` on this branch's reflog.
Its decisions carry over; its code does not, because the pipeline it modified no longer exists.
