## Context

See `proposal.md` — Why. The contract is in `specs/text-ink/spec.md`; this document explains how it
lands and why the vocabulary is what it is.

Two facts about the current code shape everything below.

**The renderer is colour-capable already.** Typst emits colour PNG and colour PDF; bilevel is a
post-process, a Rec.601 luma threshold at 128 with no dithering (`src/render/helpers.rs:18-27`), and
it is selected per printer, not per renderer: `color_mode` is `"color" | "bilevel"` and, when absent,
is negotiated from the printer rather than defaulted (`docs/SPEC.md:933`, ADR-0033); `/render/label`
defaults to `color` (`docs/SPEC.md:149`); `src/driver.rs:447` (`PrinterCapabilities::from_parts` at
`src/driver.rs:440`) advertises bi-level as a *printer* capability. `docs/VISION.md:5` names the
target printers as "sheet/office, tape, thermal/Zebra" and nowhere calls the media monochrome.

**A dynamic field already has a home.** `DynamicValue<T>` (`src/models.rs:218`) is `Literal(T)` or
`Ref(String)`, and its `Deserialize` (`src/models.rs:269-342`) already resolves a string by testing
for the `{…}` wrapper first and falling back to `T::from_str`. Load-time reference checking is
`check_param_ref` (`src/templates.rs:1335`), which takes the permitted parameter type names.

## Goals / Non-Goals

**Goals:**

- One field, `text.ink`, plumbed through the two-stage parse (`raw.rs` → `convert.rs` → `models.rs`),
  load-time validation, and both render paths.
- No string an author wrote ever reaches the generated Typst source.
- No silent fallback anywhere on the resolution path.

**Non-Goals:**

- Anything the proposal already excludes (a ground to reverse out of, ink on other item types,
  inheritance, legibility validation).
- Touching the measure pass. Colour changes no glyph metric, so `intrinsic`, `layout_text` and the
  whole of `resolver.rs` stay as they are. This is what keeps the change small.

## Decisions

### The vocabulary is a full colour, not the monochrome one #282 proposed

#282 and #280 both assert that "the renderer targets mono laser and thermal media" and conclude that
a palette "would invite templates that cannot print". The repository records the opposite, as the
Context above cites: colour is a negotiated output mode, and `color` is the default on the render
endpoint. Restricting the template vocabulary to black and white would impose on templates a
constraint the pipeline does not have, and would make the sheet/office printer — a colour target in
`VISION.md` — unable to receive a colour the renderer could already produce.

So `ink` accepts a colour name from a closed 18-name set, or a hex colour in the 3/4/6/8-digit forms.

*Alternatives considered.* A two-value `black | white` enum: smallest vocabulary, every value survives
the bilevel threshold unchanged, and it matches the issue as written — rejected because its premise is
not what the repo records, and because widening it later is a spec change rather than a template edit.
A continuous greyscale `0.0–1.0`: covers de-emphasis, maps to Typst `luma()` — rejected as a strict
subset of the chosen vocabulary with no advantage, since a grey is expressible as a hex triple.
A closed set of named greys: same, plus five names to defend that nothing in the repo grounds.

*The trade-off this accepts, stated plainly.* On the bilevel path any ink collapses at luma 128. A
mid-tone that looks correct in the PNG preview an author checked can print as solid black or vanish to
white on a thermal printer, deterministically but invisibly. The specs say so; nothing warns. Adding a
warning would need a warning channel for template content that does not exist today — the outcomes are
load or quarantine — and inventing one is out of this issue's scope.

**ADR-0091** records this decision: that the ink vocabulary is a full colour space, why the monochrome
premise was rejected on the evidence above, and what a colour ink does on the bilevel path. It is
scoped to text ink. #280 decides its own vocabulary for a container fill; this ADR does not decide it
for them, because #282 is text colour and nothing else.

### The domain type keeps the author's spelling and its resolved bytes together

`Ink` parses at construction and holds both the spelling it came from and the RGBA it resolved to:

```rust
pub struct Ink { spelling: String, rgba: [u8; 4] }
```

`FromStr` is the only constructor: it matches the 18 names against a table of their RGB values, or
parses a `#`-prefixed hex string in the four permitted digit counts, and errors otherwise. `Serialize`
emits `spelling`; `Deserialize` goes through `FromStr`; `ToSchema` reports a string.

Keeping the spelling is what makes `GET /templates/{id}` (`src/api.rs:1141`, which returns the parsed
`TemplateDetail`, not the file) report back what the author wrote. *Alternative considered:* normalise
everything to `#rrggbbaa` and drop the spelling — rejected because it turns `ink: red` into
`ink: "#ff4136"` on read-back, which is a silent rewrite of the author's template in the one view the
UI reads.

The two fields cannot drift: `rgba` is derived once at construction and `Ink` exposes no mutator.

### Both forms emit `rgb(r, g, b, a)`; no author string reaches the Typst source

The renderer emits the resolved bytes, for a name and for a hex alike. One code path, no carve-out.
This is also the injection boundary: the only thing that ever reaches the generated source is four
integers this code produced.

The name table is our own, pinned to the values Typst documents today (`black` `luma(0)`, `gray`
`luma(170)`, `silver` `luma(221)`, `white` `luma(255)`, `navy` `#001f3f`, `blue` `#0074d9`, `aqua`
`#7fdbff`, `teal` `#39cccc`, `eastern` `#239dad`, `purple` `#b10dc9`, `fuchsia` `#f012be`, `maroon`
`#85144b`, `red` `#ff4136`, `orange` `#ff851b`, `yellow` `#ffdc00`, `olive` `#3d9970`, `green`
`#2ecc40`, `lime` `#01ff70`). Pinning them here means a Typst upgrade cannot silently change what
`ink: red` prints. *Alternative considered:* emit the bare name and let Typst resolve it — fewer lines
and no table, and safe against injection because the set is a closed allowlist we control — rejected
because it makes the rendered colour a property of the Typst version rather than of the template.

### The text item's ink field carries its own `Deserialize`

`DynamicValue<T>`'s generic visitor includes a length-suffix fallback (`mm`/`in`) intended for
numeric and dimension types. If shared directly, that fallback would accept `redmm` or
`"#ff0000in"` by stripping the suffix and storing the bare colour name or hex, performing a silent
rewrite of the author's input—the exact behavior `spelling` exists to prevent.

Therefore, the text item's ink field in `raw.rs` carries its own `Deserialize` (`deserialize_dynamic_ink`)
that resolves `{name}` to a reference or parses strictly through `Ink::from_str`, never taking the
length-suffix branch and refusing invalid suffixes (`redmm`, `"#ff0000in"`).

Load-time reference checking reuses `check_param_ref(params, name, "ink", &["string", "enum"])`, the
call `font_weight` already makes with `&["integer"]` (`src/templates.rs:1457`).

### Resolution fails loudly; there is no default ink to fall back to

`font_weight`'s measure-pass resolution ends in `.unwrap_or(400)` (`src/render/mod.rs:1398-1406`): an
unresolvable reference silently becomes regular weight. Ink must not copy that. A `Ref` resolves in
the render pass only, next to where the weight's render-pass resolution already runs
(`src/render/mod.rs:1757-1762`), and a value that is absent, not a string, not a colour, or itself a
`{…}` reference returns an `AppError` naming the parameter.

Ink never enters the measure pass at all, because it cannot change a metric. That is why one
resolution site suffices where `font_weight` needs two.

### An ink reference is an input, and the existing walk already gates it

`derive_inputs_internal` (`src/templates.rs:191`) is what tells a client which fields to put on a
form. Its layout walk skips an item whose `when` the supplied data does not satisfy
(`src/templates.rs:261-278`) and, for an active `text`, records a `font_weight` reference with
`record_ref(r, false, false, false)` (`src/templates.rs:295`) — false for `interpolated`, because the
parameter is a layout attribute and is never substituted into a printed string.

An ink reference records identically, on the line beside it. The gating, the de-duplication and the
"interpolated wins" merge in `record_ref` (`src/templates.rs:199-220`) are all reused as they stand,
which is why the spec's three input scenarios need no new mechanism — only the one added call and
tests that prove it fires for an active item and stays silent for a gated-off one.

Missing this is what a reviewer caught in round 1: a field can be fully parsed, validated and
rendered and still be invisible to every client, because the input list is derived from a separate
walk that would not have known about it.

### The fill goes on the outer `#text`

`render_text_item` already wraps the whole block in `#text(size: {}pt{weight_arg})[…]`
(`src/render/mod.rs:1871`) around per-line `#text("…")` runs (`src/render/mod.rs:1862`). Typst's inner
runs inherit `fill` from the enclosing `#text`, so one `fill:` argument on the outer call colours
every line, the ellipsis a truncation adds, and every wrapped line. Nothing about the padding, the
alignment wrapper or the placement changes.

## Risks / Trade-offs

- **A colour ink is unpredictable on a thermal printer.** → Accepted and specified, not mitigated: the
  bilevel path thresholds it exactly as it thresholds everything else. ADR-0091 records it so the next
  reader finds the reason rather than a surprise.
- **The name table can drift from Typst's constants across a version bump.** → That is the point of
  pinning: `ink: red` keeps printing the same colour when Typst changes. A unit test asserts the table
  so a deliberate re-pin is a visible edit.
- **The change ships a field whose most interesting value is not yet demonstrable.** A `white` ink has
  nothing to sit on until #280 lands. → Accepted per the scope decision in `proposal.md`; the
  acceptance test for reversal belongs to #280, and this change proves the ink itself against the
  white page and in bilevel.
- **`deny_unknown_fields` means an author who mistypes `ink` on a `container` gets an unknown-field
  error, not a "wrong item type" one.** → Consistent with every other misplaced key; no special case
  is added for it.
- **Alpha inks composite over the page, and over nothing else today.** → Once #280 paints a ground,
  an alpha ink composites over that instead, which is the correct behaviour and needs no change here.

## Acceptance evidence

This is a rendering change, so parsing and passing tests are not evidence that a label is right. The
implementer runs a render -> open -> inspect -> fix loop against a running server
(`LABELER_CONFIG_DIR=./config-dev cargo run`, `LABELER_NO_AUTH=true`,
`POST /api/render/label?format=png`) and looks at each image before calling the change done:

1. **Default black.** An existing template, unedited, rendered before and after. The glyphs are the
   same black in the same places; nothing about the layout moved.
2. **An opaque colour.** A text item with `ink: red` and one with `ink: "#0074d9"`. Each prints its
   colour, and the fitted size, line breaks and truncation ellipsis match the same item without an
   ink.
3. **An alpha ink.** `ink: "#00000080"` over the white page reads as mid-grey, not as opaque black
   and not as nothing.
4. **A dynamic ink.** `ink: "{brand}"` rendered twice with different supplied values, printing two
   different colours from one template, plus one request supplying a value that is not a colour,
   which must come back `400 ink_param_invalid` rather than a black label.
5. **Bilevel.** The opaque and alpha cases re-rendered with `?color_mode=bilevel`. Each ink resolves
   to pure black or pure white by which side of the luma threshold it falls on, and a light ink on
   the white page disappears — which is the documented behaviour, and the point of looking.

**No task claims this.** The loop runs against a server and a config dir outside the repository, so
its only evidence is an image no later reader can retrieve; a checked box over it would be a claim
nobody can verify and no gate can refuse (#220). It is written here, as the standard the implementer
is held to, and not in `tasks.md`.

## Migration Plan

None. `ink` is optional and absent means black, which is what every existing template already renders,
so no template needs editing and nothing rolls back beyond reverting the commit.
