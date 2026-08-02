# 43. Vertical alignment positions the ink, not a metric box

Date: 2026-08-02

## Status

Accepted. Supersedes the "centre the cap-height box" clause of
[ADR-0041](0041-vertical-alignment-delegated-to-typst.md); its "delegate placement to `#align`
rather than computing offsets from font metrics" decision stands and is in fact reinforced here.
Issue [#127](https://github.com/pfa230/labeler/issues/127), which also closes
[#124](https://github.com/pfa230/labeler/issues/124).

## Context

ADR-0041 fixed *which* box Typst draws but left the default box in place: cap-height to baseline.
That box is string-independent, so how centred a label looks depends on which glyph classes the
string happens to contain. Measured on `brother_12mm` at 180 dpi, ink gap above vs below in a 56 px
slot:

| string | gap top | gap bottom |
|---|---|---|
| `MESSAGE` | 11 | 12 |
| `test` | 14 | 12 |
| `typogy` | 14 | 3 |
| `message` | **20** | **3** |

`message` has no capitals and no ascenders, so its ink starts well below the box top, and its `g`
hangs below the baseline — visibly low, with the descender nearly touching the slot edge. `bottom`
was worse: it pins the baseline, so descenders fell outside the clipped box and were cut (#124).

Two things were checked before choosing a fix.

**No metric box can fix this.** Switching to the full font box (`top-edge: "ascender"`,
`bottom-edge: "descender"`) was implemented and measured: `message` still reported 20/3. Inter's
ascender (0.969 em) and descender (0.240 em) centre the baseline 0.3645 em above centre; the
cap-height box (0.727 em) centres it at 0.3635 em — a difference of 0.001 em. Every
string-independent box lands the baseline in the same place, because the skew comes from the string's
own glyphs.

**Metric centring is the industry default, and we are departing from it knowingly.** Server-side text
stacks overwhelmingly centre a string-independent box and keep the baseline fixed: Pango documents
logical extents (not ink extents) as "usually what you want for positioning"; Pillow's `anchor="mm"`
is the midpoint of the ascender and descender lines, not of the drawn pixels; TeX's `\strut` exists
precisely to force constant height and depth so baselines cannot wander; Flutter's `StrutStyle` is
the same device; Typst's own default is the cap-height/baseline box. The documented reason is
stability: with ink centring, `HI` and `gy` and `123` and `()` each sit differently.

That stability is worth little here and costs a lot. Each tape label is printed separately and read
on its own; there is no column of labels whose baselines must agree. A user printing `message` sees
one label, and it looks wrong. The trade was put to the project owner explicitly and the answer was
that per-label correctness wins over cross-label consistency.

## Decision

**Set `top-edge: "bounds"` and `bottom-edge: "bounds"` on the emitted Typst source.** Typst then makes
the line box the bounding box of the glyphs actually drawn, and the existing `#align` (ADR-0041)
centres, tops or bottoms *that*. `alignment.vertical` is therefore defined against the ink:

- `top` — highest ink on the slot top
- `bottom` — lowest ink, descenders included, on the slot bottom
- `center` — ink box centred in the slot

**Use Typst's own facility rather than computing a correction.** An implementation was written that
measured glyph bounds with `fontdue` and emitted a `#move(dy:)` correction — about 90 lines across
two helpers, a wrapper, and a derivation whose sign was wrong on the first pass. `bounds` replaces all
of it with two words of configuration *and measures better*: every test string lands within 0.5 px of
centre, where the hand-rolled version varied by 1-2 px. This is a direct consequence of ADR-0041's
principle: font-metric arithmetic in the renderer is a bug source, and the engine already knows the
answer.

**Blank first and last lines are dropped before emission.** They carry no ink but still get a line
box, which pushes the visible text off centre by a full line advance; `fit_text_auto_length` preserves
them (`"\nmessage"` measures as `["", "message"]`). Interior blank lines are kept — they are real
spacing between visible lines.

## Consequences

- Every existing label shifts slightly. Bundled templates were re-rendered and inspected; none needed
  a YAML change.
- **Baselines are no longer constant between labels from one template.** `MESSAGE` and `message` sit
  at slightly different heights. Accepted, per Context.
- Short punctuation- or digit-only strings are centred on their own small ink box, which can read
  oddly (`.` centres its dot in the slot). Known and accepted; a template wanting otherwise can use
  `top`/`bottom` and explicit geometry.
- `bottom` no longer clips descenders, closing #124 without separate work.
- Round-letter overshoot (`O`, `S` drawn slightly beyond the cap line by design) is now included in
  the centred box. This is sub-pixel at label sizes and is the same compromise any ink-based centring
  makes.
- Regression coverage is pixel-level: `centering_follows_the_ink_across_glyph_classes` renders caps,
  lowercase-with-descender and mixed strings and asserts the measured ink box;
  `top_and_bottom_pin_the_ink_not_the_baseline` compares ink *height* against a centred render, since
  clipped ink also reaches the slot edge and a naive edge assertion passes vacuously;
  `blank_edge_line_does_not_shift_centering` guards the trim.
- If a future template family ever does need agreeing baselines (a sheet of many labels read as a
  block), the fix is a per-template opt-in back to a metric box — the `\strut`/`StrutStyle` pattern —
  not a global revert.
