## Why

Closes [#226](https://github.com/pfa230/labeler/issues/226).

The layout engine has no single sizing rule. It has ten, each added by a feature that needed one
more case than the last, and `docs/SPEC.md` §4 admits it in its own words: "`auto` handling is not
fully uniform across item types (ADR-0054): the qr/image fallback exclusion, the
content-width-vs-frame-remainder split, and the zero-remainder split are three distinct asymmetries,
not one."

The cost is measurable. Vertical alignment took four ADRs (0041, superseded by 0043, by 0044, by
0045). The measure and render passes are coupled through a positional replay list that needed its
own error code (`auto_length_cursor_mismatch`), nineteen lines of warning comments, and still
shipped a bug. Sizing is implemented twice, and `docs/SPEC.md` §7 names keeping the two in step as a
standing hazard rather than a solved problem. Most tellingly, [#212](https://github.com/pfa230/labeler/issues/212)
(flow layout) was planned against this engine by an author who had read the code, the ADRs and the
frozen spec, and two independent adversarial reviews each found four blocking defects. Every one was
an interaction with a rule living somewhere else.

Now, because #212 is blocked behind it and each further feature adds another interaction to the ten.

## What Changes

**One protocol.** Every node with a box answers two questions instead of one, per axis:

```
intrinsic(node, constraints) -> Size    bottom-up; this is what sizes the parent
resolved(node, final_frame)  -> Rect    top-down, once the parent is fixed
```

`content` resolves to the node's intrinsic size on that axis, clamped by `max_w`/`max_h`. `fill`
stretches: it reports its content upward, so it can still size an auto-length label, and takes the
frame downward, so it still reaches the edge. Nothing "contributes zero" and no traversal order
matters. The rule is the same for every item type, both axes, and every format.

- **BREAKING. `auto` is removed from the size vocabulary**, replaced by `content` (hug) and `fill`
  (stretch). `auto` is no longer a valid `size` component on any axis or item type; a template
  carrying it fails validation with a message naming both replacements, and quarantines per
  ADR-0058 rather than crashing the server. This is the migration mechanism: `auto` meant fill on
  fixed frames, content-for-text-but-fill-for-containers on tape, and exactly-`max_w` on qr/image,
  so silently re-meaning it would relayout existing templates with no error anywhere.
- **BREAKING. Centring is authored, not inferred.** ADR-0059 gives a centred auto-width text the
  alignment slot as its box while a left-aligned one gets the fitted width. Under the protocol the
  author writes `fill` to say "stretch" and `alignment.horizontal: center` to say "centre within
  it", and the box no longer depends on the alignment.
- **BREAKING. `content` on `qr`/`image` is the item's natural size**, capped by `max_w`/`max_h`,
  rather than resolving to exactly `max_w` and erroring without one. A QR's natural size is its
  modules times `module_size` plus the quiet zone; an image's is its pixel dimensions at the
  template `dpi`. The protocol requires an intrinsic size wherever an axis asks for one, and `qr` and
  `image` were the two item types that could not supply it, so the capability
  [#149](https://github.com/pfa230/labeler/issues/149) asks for arrives here as a structural
  consequence rather than as a feature. #149 was **closed as superseded by #226** on 2026-08-25, so
  this change carries one accepted issue: the capability could not be split back out, because a
  protocol in which `qr` and `image` alone lack an intrinsic size is the asymmetry the change
  removes.
- **BREAKING. Four `details.reason` slugs leave the contract**: `size_auto_without_max` and
  `size_auto_no_room` (no extent uses `max_*` as a substitute for its source, and a resolved zero is
  a render outcome, not an error), `container_padding_no_room` (a data-dependent zero inner box
  renders empty; a padding that exceeds a *constant* box is still refused at load), and
  `auto_length_cursor_mismatch` (there is no replay list left to desynchronize).
  `intrinsic_size_undefined` is added. `docs/SPEC.md` §10.1 fixes
  the reason set as part of the contract and makes changing it an ADR-0052 decision, so all six
  movements are recorded: the four withdrawals and `intrinsic_size_undefined` against the protocol
  ADR, and `text_does_not_fit` against the overflow ADR that defines the policy raising it.
- **BREAKING. A `qr` asking for a content or frame extent must declare `module_size`.** An intrinsic
  size is a content extent times a scale, and `module_size` is a QR's scale exactly as `font_size` is
  a text's. `font_size` is mandatory; `module_size` is not, so the engine would otherwise have to
  invent a pitch, which is the class of thing this change deletes. Numeric and constant-`to` QRs are
  unaffected, because an authored extent needs no scale.
- **BREAKING. `module_size` becomes a length in the template `unit` and `quiet_zone` a count of
  modules.** A QR cannot have an intrinsic size without a module pitch, and `module_size` is the only
  field that could carry one; today it is a minimum generated-SVG pixel pitch per module and
  `quiet_zone` is read only for whether it exceeds zero, in which case the encoder's own four-module
  zone applies. `quiet_zone: 0.0` keeps its meaning; a positive `quiet_zone` becomes that many
  modules rather than four. No template in the catalog or the fixtures sets `module_size`.
- **BREAKING. A `to` that shrinks as its frame grows is refused on an unresolved axis.** `at`
  sign-negative paired with a plain `to` resolves to `to − at − F`, which narrows as the label widens
  and inverts past `to − at`. It has no contribution that could size a frame, so it is refused
  wherever the frame's extent on that axis is not known before the items inside are sized: the width
  axis of a dynamic-width `single`, the inner box of a `content` container, and, beneath a quarter
  turn, the author axis those map onto. It is unaffected everywhere else. This is the case the frozen
  `docs/SPEC.md` §6 rule against right-anchored frame-dependent widths was actually protecting; the
  `content` and `fill` spellings that rule also forbade become legal, because a right-anchored item's
  available extent is its own inset and so does not depend on the frame at all.
- **BREAKING. `max_w`/`max_h` now cap a stretching `to`, which therefore stops short of the corner it
  names.** Today `to` resolution returns before any cap is consulted, on both the validation and the
  render path, so a cap beside a `to` is silently inert where it is not an outright error. Leaving it
  inert would mean `size: [fill, h]` and `to: [-0.0, h]` resolving differently under the same
  `max_w`, which is the asymmetry this change exists to remove.
- **A `content` or `fill` claim is bounded by the space available to it**, so an item never overflows
  its frame by hugging. A QR or image whose natural size exceeds its frame is drawn at the frame's
  size rather than failing a bounds check. Load establishes the same bound without inspecting image
  bytes; render reads only the dimensions header when an intrinsic is demanded and never decodes the
  pixels for sizing.
- **Load-time validation and render-time resolution become one implementation.** Validation runs the
  same resolver against parameter defaults and `format.width.max` instead of a parallel conservative
  copy, closing the `docs/SPEC.md` §7 duplication. It does not measure text, encode a QR or decode an
  image, so the load/render boundary is where it is today, now stated as a requirement.
- **No circularity check.** The issue proposes refusing "`auto` whose intrinsic size depends on a
  stretch descendant". Working the model out shows there is nothing to refuse: a `fill` node reports
  its intrinsic size upward, so a `content` parent's size is known before the child's frame is fixed
  and the fixed point is reached in one pass each way. Adding `size_circular` would have been an
  eleventh special case in the change that exists to delete ten. A `fill` child of a `content` parent
  simply renders at its own intrinsic size.
- **BREAKING. A text's layout is decided against its constraint box, unconditionally**: break, then
  shrink if `font_size` is a range, then apply an overflow policy. The engine never defers line
  breaking to the renderer, so `LengthMode` disappears from this question as well as from sizing. A
  fixed `font_size` in an author-given box is currently emitted unfitted and clipped mid-glyph by
  Typst; it is now broken and ellipsized like everything else. One template in the repository is
  affected without being edited: `tests/fixtures/templates/avery5163_asset_tag.yaml` has four
  fixed-`font_size` multiline items, so its rendering changes and it needs render-and-look
  acceptance in this change.
- **`text` gains an `overflow` field** (`ellipsis` default, `fail`), making explicit what is
  currently emergent. Today's behaviour is `ellipsis` for a `font_size` range on any format,
  `ellipsis` for a fixed `font_size` whose width is frame-dependent on an auto-length label, and
  silent mid-glyph clipping everywhere else: three spellings, two behaviours, chosen by the page
  format rather than by the author. `overflow: fail` raises `text_does_not_fit` for an author who
  would rather know than be silently shortened. **Clipping ceases to be an outcome of the overflow
  policy entirely**: a box that cannot hold even the shortest representable form of its content, one
  narrower than `...` or shorter than a single line, raises `text_does_not_fit` under either policy
  rather than emitting a clipped fragment. Today both cases emit something and let the renderer cut
  it; a serial number sliced through the middle wastes a label and can be worse than no label, and a
  `/batch` request names the failing row instead. ADR-0050's ink-outside-the-band clipping is
  untouched: the policy is evaluated on the metric model, which cannot see it.
- **The two passes exchange sizes per node, not a positional replay list.** `MeasuredText` and its
  cursor are deleted.
- **All twelve repository templates that spell `auto` are migrated here.** This reverses the earlier
  scope split that assigned their migration to [#228](https://github.com/pfa230/labeler/issues/228):
  #226 is self-contained and landable on its own. The four catalog tapes and eight fixtures are
  migrated one file at a time according to the layout or regression intent each encodes, not by a
  token rename. In particular, the QR fixtures cannot retain the old "exactly `max_w`" behaviour by
  changing `auto` to `content`, and `brother_24mm_weights` must retain the #152 invariant that its
  `max_w: 117` cap at `at.x: 1.5` on a `width.max: 120` tape does not perturb the render. The four
  catalog tapes receive before/after render-and-look acceptance. This change makes no claim about
  #228's remaining purpose or status. The `SizeValue::Auto(...)` constructions in `src/` unit tests
  are likewise converted here according to what each test asserts.

### Kept, because each is a real feature

Edge-relative coordinates and the `-0.0` sentinel (ADR-0051 §1-2), including right-anchored `at`,
both-corners-edge-relative `to`, and `line` endpoints; `to` corner extents and ADR-0051 §5's
frame-dependence analysis, now split by orientation so that only `at`-plain/`to`-edge-relative
produces a stretch node, carrying the right-edge inset of ADR-0051 §6; container rotation (ADR-0036); `font_size` range shrink-and-ellipsize;
ink-based vertical alignment metrics (ADR-0045, 0049, 0050); `when` gating (ADR-0056); `max_w`/`max_h`
as caps (ADR-0053).

### Out of scope

Flow layout (#212) is rebuilt on this protocol afterwards and is not part of this change.
List-valued data (#213) is independent. Auto-height labels do not exist today, so the page node's
height does not participate: `format.height` stays a fixed `Dimension`.

## Capabilities

### New Capabilities

- `layout-sizing`: the complete size-resolution contract. The `content` / `fill` / numeric / `to`
  vocabulary, the intrinsic and resolved contract for every item type on both axes, how caps and
  padding apply, what a resolved zero and a declared zero mean, how
  sizes compose through container rotation, and what an item contributes to an auto-length label's
  width. Supersedes, by name, the frozen `docs/SPEC.md` §3.1 (`auto` item width and the multiline
  wrap budget), §4 (the `size`/`max_w`/`max_h` placement rows and the whole `auto` fallback and
  zero-remainder passage), §4.1 (the per-item-type sizing clauses), §4.2 (the "No `auto` under
  rotation" rule), §6 (the measured-content-extent paragraphs), §7 (the compile-time/render-time
  duplication note) and §10.1 (the four withdrawn reason slugs plus `intrinsic_size_undefined` and
  `text_does_not_fit`).

### Modified Capabilities

- `auto-length-layout`: REMOVED. Its single requirement makes an auto-width text's box depend on
  `alignment.horizontal`, which the protocol replaces with an authored `fill`. The capability has no
  other requirement, so it is removed rather than modified.

## Impact

**Code.** `src/render/mod.rs` (the `measure`/`render_items` pair, `resolve_size`,
`resolve_size_value`, `measure_box_height`, `measure_container_footprint`, `LengthMode`,
`AutoLength`, `item_anchor`, and the rotated container path that largely duplicates the unrotated
one); `src/render/helpers.rs` (`fit_text_auto_length` gains general callers, `MeasuredText` is
deleted); `src/templates.rs` (its parallel `resolve_size`, `resolve_size_value`, `resolve_to_extent`
and `subtree_uses_auto` are deleted in favour of the shared resolver); `src/models.rs`,
`src/raw.rs`, `src/convert.rs` (the `SizeValue` vocabulary, per the three-files-together rule);
`src/reason.rs` and `src/errors.rs` (the reason set).

**API.** `SizeValue` and the `text` item's new `overflow` field are exposed through
`src/openapi.rs` and must be registered. No endpoint, status code or error `code` string changes;
four `details.reason` slugs are withdrawn and two are added (`intrinsic_size_undefined`,
`text_does_not_fit`).

**Templates.** The four catalog tapes (`catalog/tape/brother/brother_{9,12,18,24}mm.yaml`) and all eight
fixtures that spell `auto` (`brother_24mm_lines_divider`, `brother_24mm_multiline`,
`brother_24mm_qr`, `brother_18mm_qr`, `brother_24mm_printed_on`, `brother_24mm_weights`,
`brother_24mm_max_w_cap`, and `homebox-qr`) are migrated here, each according to its own intent.
Roughly 45 sites in `src/` construct `SizeValue::Auto(...)` in unit tests and are converted here out
of compilation necessity, likewise according to what the test asserts. A user's own templates using
`auto`, the ADR-0059 centring inference, or `auto` + `max_w` on qr/image quarantine individually with
a per-template message.

**Docs.** Three ADRs plus their rows in `docs/adr/README.md`: the protocol (superseding ADR-0026,
0053, 0054 and 0059, and amending 0036 §5 and 0051 §4/§10/§11), the `content`/`fill` vocabulary, and
the text `overflow` policy.
`docs/AUTHORING.md` §4, §5 ("`auto` means two different things" ceases to be true), §6, §7 and §8.
The ADR-0052 reason-set decision is split across the ADRs that cause it rather than given a fourth
record of its own: the four withdrawals and `intrinsic_size_undefined` against the protocol ADR,
`text_does_not_fit` against the overflow ADR.
`docs/SPEC.md` is frozen and is not edited.

**UI.** `ui/src` carries no sizing vocabulary today; confirm during apply that nothing renders or
validates a `size` component, and that `overflow` needs no print-form surface (it is an authoring
field, like `multiline`'s layout half).
