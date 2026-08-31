## Why

Implements [#282](https://github.com/pfa230/labeler/issues/282).

Text has no ink. Every string a template renders is black, and nothing in the template model can say
otherwise: `TextRaw` (`src/raw.rs:171`) carries `value`, placement, `font_size`, `font_weight`,
`wrap`, `multiline`, `alignment`, `overflow` and `when`, and no foreground field appears on any
layout item in `src/raw.rs` or `src/models.rs`. The renderer emits
`#text(size: {}pt{weight_arg})[...]` (`src/render/mod.rs:1871`) and never passes `fill`, so Typst's
default black is the only ink a label has ever had.

Black-only text means the strongest contrast device on a label is unreachable: nothing can be
reversed out of a dark ground, and hierarchy has to come from type size alone.

## What Changes

- `text` gains an optional `ink`, the item's foreground colour. Absent, text renders black exactly as
  it does today, so every existing template is unaffected.
- `ink` accepts a colour by name or by hex, and may instead be a `"{param}"` reference resolved per
  render, matching how `font_weight` already works (`DynamicValue<T>`, `src/models.rs:218`).
- The renderer passes the resolved colour to Typst as the `fill` argument of the `#text` call it
  already emits, for both the PNG and the PDF path.
- Validation is syntax only: an unparseable colour is refused, at load for a literal and at render
  for a resolved `{param}`. Legibility is not validated. An ink that is invisible against its ground
  is the author's problem, exactly as placing text outside the printable area already is.
- A parameter an active text item references as its ink joins the template's derived input list as a
  layout attribute, so a client's form asks for it. A `when`-gated-off item contributes nothing.
- A new ADR records why the vocabulary is a full colour space rather than the monochrome one the
  issue proposed, and what a colour ink means on a bilevel printer.

Not in scope, and deliberately so:

- **A fill behind the text.** #280 covers a fillable container and is not touched here. Consequence,
  stated plainly: until #280 lands there is no dark ground in the model, so a light ink renders
  white on white. This change ships the ink; the reversed block needs both.
- **Ink on `line`, on a frame's stroke, or a container ink descendants inherit.** #282 is text
  colour, nothing else. Each is a separate contract and gets its own issue if wanted.
- **Legibility or contrast validation**, per the decision recorded in `design.md`.

## Capabilities

### New Capabilities
- `text-ink`: the `ink` field on a `text` layout item - its vocabulary, its parameter reference form,
  when it is refused, and what it renders to.

### Modified Capabilities
<!-- None. `docs/SPEC.md` §4.1 documents the `text` item and is frozen; under the first-touch rule
     the new requirement carries the complete post-change `text` contract and names that section as
     superseded. No requirement exists in `openspec/specs/` for it to modify. -->

## Impact

- `src/models.rs`: a new `Ink` colour type, whose `FromStr` is the whole parser and whose
  `Deserialize` goes through it, so an unparseable colour fails during YAML deserialization and
  `serde_path_to_error` attaches the layout path for free. `LayoutItem::Text` gains
  `Option<DynamicValue<Ink>>`.
- `src/raw.rs`: `TextRaw` gains the matching `Option<Dynamic<Ink>>` (`deny_unknown_fields` means the
  key is refused today).
- `src/convert.rs`: the `TryFrom` for a text item carries the already-parsed value across. It adds no
  parsing of its own, because there is no post-deserialization constraint left to check — unlike
  `font_weight`, whose multiple-of-100 rule lives there.
- `src/templates.rs`: two places. `validate` checks a `{param}` reference resolves to a declared
  parameter of type `string` or `enum`, via the existing `check_param_ref`. `derive_inputs_internal`
  records an active text item's ink reference as a non-interpolated input, beside the `font_weight`
  reference it already records (`src/templates.rs:295`); the surrounding `is_active` walk is what
  makes a `when`-gated-off item contribute nothing.
- `src/render/mod.rs`: `render_text_item` emits `fill:` on the `#text` call it already builds; the
  render pass resolves a `{param}` ink and fails loudly rather than falling back to a default. The
  measure pass is untouched, because colour changes no metric.
- `src/reason.rs`: a new `InkParamInvalid => "ink_param_invalid"` under `InvalidRequest`, beside
  `DatetimeParamInvalid`, so the wire `reason` slug is stable from the start.
- `src/openapi.rs`: `Ink` is registered.
- `docs/adr/`: a new ADR plus its row in `docs/adr/README.md`.

API surface. No endpoint, request body or status code is added or removed, and no dependency is
added: Typst already renders colour, and bilevel remains the existing post-process
(`src/render/helpers.rs:18`). Two response shapes change additively:

- `GET /templates/{id}` returns `TemplateDetail`, whose layout is the serialized domain model, so a
  text item that declares an ink gains an `ink` key. An item that declares none omits it, so every
  existing response is byte-identical. The OpenAPI document changes to match, since `Ink` is a newly
  registered schema referenced from the text item.
- `POST /batch` and `POST /print` can now fail with a per-label `ink_param_invalid` entry inside the
  existing `BatchInvalid` envelope. The envelope's shape is unchanged; only the set of reasons that
  can appear in it grows.
