## Context

See `proposal.md` — Why. This change has **three** contracts, and all are implemented here:

- `specs/layout-sizing/spec.md`, two `MODIFIED` requirements. "Text is laid out against the box it
  will get, and what does not fit is authored" is the rendering half. "Vertical fitting reserves the
  ink each alignment can expose" (ADR-0084) is carried behaviourally unchanged, because a `MODIFIED`
  block replaces the whole requirement and this change renames the flag its scenarios name; its only
  other edit drops its claim that the frozen §3.1 blank-edge bullet stays authoritative, which the
  first requirement supersedes.
- `specs/template-inputs/spec.md`, two `MODIFIED` requirements, **behaviour unchanged**. They name the
  layout flag, so they name `wrap` now, and `truncated_elsewhere` is redefined to say what it actually
  reports once every segment is laid out: that some `wrap: false` item reads the name, which is no
  longer a loss. The field, its computation and the note the print form renders from it all survive
  here and are removed by #269. Leaving the canonical requirement asserting a truncation that cannot
  happen was the alternative, and a disclaimer in this proposal does not reach the archived contract.
- `specs/text-wrap-flag/spec.md`, `ADDED` — the schema half: the field's name and default, and the
  refusal of the old spelling. Nothing about controls.

The base to build against, in the places this change touches it:

## Goals / Non-Goals

**Goals:**

- One flag, one meaning: `wrap` decides soft wrapping and nothing else. Every `\n` segment of a value
  enters layout regardless of it, and only the authored `overflow` policy may then shorten a line, drop
  lines, or reject the render — a loss the label carries a marker for, rather than one no step
  announces.
- The four-step pipeline keeps its shape. This change edits steps 1 and 4 and one sentence of the
  overflow rule; it does not restructure what #226 just landed.
- The failure a user meets on an unmigrated template names the field, its layout path and the new
  spelling, and fires on the key's presence rather than its value.

**Non-Goals:**

- No change to `params[].multiline`, to the `content`/`fill` vocabulary, to the `overflow` policy's
  values, or to the metric model ADR-0045, ADR-0050 and ADR-0084 define — including ADR-0084's centred
  reservation (`src/render/helpers.rs:959-990`), which this change leaves exactly as it found it.
- No fix for a lone `\r` (#259) and none for glyph ink leaving a metric box (#257 — and ADR-0082's
  "clipping SHALL NOT be an outcome" already covers the policy half of it).
- No attempt to reuse the pre-#226 implementation at `ff3bd9c`. Its decisions carry; its code edited
  functions that no longer exist.

## Decisions

### Reject `multiline` in conversion, on presence, not in serde

`TextRaw` renames the field to `wrap: bool` and keeps a capture for the old name, deserialized with
the repository's presence-preserving helper so an explicit YAML null is distinguishable from absence:

```rust
#[serde(default, deserialize_with = "deserialize_present")]
pub multiline: Option<serde_yaml_ng::Value>,
```

`TryFrom<LayoutItemRaw>` then refuses it with a `TemplateError::Validation` whose path grows into the
layout path and whose message names the rename. `deny_unknown_fields` would also refuse the key, but
with serde's message — it names the field and never mentions `wrap`, which is the one thing the reader
needs. `Value` rather than `bool` keeps `multiline: "yes"` on the rename error rather than a type
error, the same choice `format` makes on `RawParamSpec` and for the same reason.

### Step 1 segments; it does not select

The discard at `:656-659` becomes segmentation: normalise `\r\n` to `\n`, split on `\n`, keep every
segment. `wrap: true` then wraps each segment to the box width with the existing `wrap_text`;
`wrap: false` passes them through. The `input_text`/`raw_lines` split in the current code exists only
because step 1 could return a single collapsed string — with segmentation it becomes one list produced
once and re-derived per candidate size, which is what step 2 already does.

CRLF is normalised here rather than at the API boundary because this is the only place that can state
why: `\r` is unmapped in the bundled font, so `text_width` charges it the `.notdef` advance
(1344/2048 em, measured) while the typesetter renders nothing.

### Step 4 stops trimming, and the two counts become one

The trim disappears and the `lines_to_trim` binding with it. That removes one of the two reasons the
measured and emitted blocks differ, not both: step 3 still shortens the block when the `ellipsis`
policy drops lines (`src/render/helpers.rs:748-767` truncates the vector), so the intrinsic height must
name the post-policy lines rather than claiming one list throughout. What the change buys is that the
remaining difference is always visible on the label as a marker, never a line removed for carrying no
glyphs.

This reverses ADR-0045's blank-edge rule, which is the one decision here that a reader will want
justified rather than asserted. Its argument was that a blank edge line carries no ink but takes a
line box and shoves the visible text off centre. That is true and is now the caller's business: they
wrote the newline. The rule was formed when a non-wrapping item emitted exactly one line, so a leading
newline could only ever be an accident of input; once every line survives, an exception for the first
and last is the same silent discard the rest of this change removes.

### Emission must force a line box, or honoring a blank line is a no-op

Deleting the trim is necessary and not sufficient. `layout_text` decides the lines, but the Typst
source is produced later by `render_text_item`, which joins text nodes with `#linebreak()`
(`src/render/mod.rs:1738-1766`), and the repository's own test records that **Typst gives a
trailing empty line no box** (`src/render/mod.rs:4035`). A retained trailing blank would therefore
vanish at emission while the fitter counted it, which is the measure/render disagreement this change
claims to remove, in the opposite direction.

The emission therefore does two things, and the second is the one that is easy to miss.

**A blank line needs a box.** A trailing `#linebreak()` after the last line when that line is empty,
so the box exists; the same for a value that is entirely empty, which must still occupy one line box.

**A blank line's box must be the fitted size.** Emission currently puts `size` and `weight` inside
each `#text` and leaves the `#linebreak()` between them under whatever styles surround the block
(`src/render/mod.rs:1738-1766`). Typst collects a linebreak using the linebreak element's own
surrounding styles (`typst-layout-0.15.0/src/inline/collect.rs:185-189`) and gives an otherwise empty
line its height from a fallback shaped run (`.../inline/line.rs:211-227`, `:308-313`). So a blank line
emitted that way takes the **ambient default** size and weight, not `TextFit.font_size_pt` — the line
count would be right and the block height wrong, which is exactly the measure/emit disagreement this
change exists to close, hidden one level down. The item's size and weight therefore surround the whole
emitted block, so every line, every linebreak and every fallback run inherits the fitted values, and
the individual runs stop carrying their own `size`. Adding the final linebreak without this fixes the
count and leaves the metrics wrong.

Two further segment cases are exceptions in the current code and must go, because the contract now
says every segment is laid out:

- `layout_text` returns zero lines and zero intrinsic height immediately for an empty string
  (`src/render/helpers.rs:662`). That early return goes: an empty value is one empty line.
- `wrap_text` drops a segment consisting only of whitespace, because `split_whitespace` yields no
  words and only a non-empty accumulator is pushed (`src/render/helpers.rs:834`, `:900`). A
  whitespace-only segment must survive as an empty line.

Both are the same shape as the blank-edge trim: a line that carries no glyphs treated as a line that
does not exist. Verification has to be at the pixel level and at a font size away from the default,
because a wrong blank-line height is invisible in a passing render: assert the rendered block height
against what the fitter measured for a leading blank, an interior blank, a trailing blank, and an
empty value.

### The marker reports the field

`Overflow::Ellipsis` currently shortens when the block does not fit. It must also fire when the block
fits only because lines were dropped — otherwise honoring a trailing blank turns `"message\n"` in a
one-line box into a label that looks complete while a line the caller wrote is missing. The condition
becomes "any line was dropped", and the marker lands at the end of the last retained line whatever
that line holds, including a blank one.

`Overflow::Fail` needs no change: a dropped line means the content did not fit, which is already the
condition it errors on.

### The control half is out of scope, and why that is not a gap

The print form's control derivation and its truncation note were part of this change until #200 moved
them from the client into the service. They are now [#269](https://github.com/pfa230/labeler/issues/269),
which depends on this one: an undeclared name's control may stop consulting the layout, and
`truncated_elsewhere` may be removed, only once a `text` item lays out every `\n` segment of a value regardless of `wrap`, which is
what this change establishes.

Until #269 lands the flag stays true for a field read by a `wrap: false` item, and the note it drives
stays visible. That is a stale warning, not a wrong render: every `\n` segment enters layout under
either flag, and only the authored `overflow` policy may shorten a line, drop lines, or reject the
render, marking what it shortens or drops. This change touches no UI file at all: #200 stopped shipping
layout items to the client, so there is no layout field there left to rename.

### ADR

A new ADR records the split of the two meanings, the rename and its hard migration, the blank-edge
reversal, and the field-level overflow marker. It supersedes ADR-0045's blank-edge rule and amends
ADR-0082 in three places: step 1, step 4, and its **overflow decision**, which gains the rule that any
dropped line earns the marker regardless of what that line carried. ADR-0082 owns both the pipeline and
the policy, so recording the marker rule only in the spec would leave a behavior decision with no ADR. **The ADR is 0085.** `main` holds 0083 (packed children) and 0084 (centred ink reservation), both landed while this change was in review, so this decision took the next free number and its row is in `docs/adr/README.md`. It has already collided twice; check the index rather than assuming.

## Risks / Trade-offs

- **A value with blank edges is now fitted as more lines**, so it may select a smaller font or gain a
  marker where it gained neither. Connector-sourced values with a trailing newline are the realistic
  case. → Stated in the requirement and pinned by the two scenarios that previously asserted the old
  behaviour, rewritten rather than dropped so the change is visible to a reader of the spec.
- **Every unmigrated user template leaves the served set** until edited. → The error names file, path
  and rename, and is visible in the startup log, the `broken` list and the reload count. The
  alternative, a permanent alias, buys a two-spelling schema forever.
- **A formerly non-wrapping item lays out every line of a value carrying `\n`**, so a label that fits
  today can overflow and, under `fail`, start erroring. → This is the correction the issue asks for;
  what changes is that the loss is now visible instead of silent.
- **The blank-line rule touches emission, fitting and wrapping in three separate places**, and a miss
  in any one of them silently restores the old behavior for that path. → The scenarios pin all four
  cases (leading, interior, trailing, empty) at the requirement level, and the design names each code
  site rather than describing the rule abstractly.
- **The plan artifacts, not the code, are what survived from the first attempt.** → The old review
  verdicts were deleted rather than carried: they judged a plan against a renderer that no longer
  exists, and a stale APPROVE is worse than no verdict.
