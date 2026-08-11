# 47. The catalog is a designed five-template starter set

Date: 2026-08-11

## Status

Accepted. Issue [#135](https://github.com/pfa230/labeler/issues/135), which ADR-0046 named as the
follow-up deciding what the catalog should contain. Corrects a stale pointer in
[ADR-0005](0005-recursive-containers-with-option-gating.md) (see Consequences; ADRs are append-only,
so 0005 is not edited).

## Context

ADR-0046 moved templates into `catalog/` but inherited its contents unexamined: seven Brother
variants (including two QR and one multiline demo) plus `avery5163` published as a five-field,
two-option, two-orientation asset tag with a container-rotation example baked in. That is not a
starter set, it is the engine's own test corpus wearing the "install this" label. An import maps one
CSV column to one template field; `avery5163`'s `id, name, description, url, tags` forced either five
mapped columns or four blank ones, and `orientation`/`outline` needed an operator to already understand
the layout model before printing a single label.

The catalog is also not new territory here. #134 (`346b78e`) already drew this line once: it trimmed
the *shipped* set to four Brother sizes and moved `avery5163`, `homebox-qr`, `brother_18mm_qr`,
`brother_24mm_qr` and `brother_24mm_multiline` into `tests/fixtures/templates/` specifically because
they demonstrate engine features rather than being printed as-is. #137 (`ff510b7`) then restructured
templates into `catalog/<media-class>/<vendor>/` and, in doing the move, swept all seven Brother
variants and `avery5163` back onto the install-facing list — the five #134 had deliberately set aside
came back along with the rest of `tests/fixtures/templates/`'s Brother entries. This decision is
therefore as much a regression fix as it is new design: it restores the split #134 already made and
gives it a durable gate so a future restructure cannot silently repeat #137's mistake.

## Decision

**The catalog is exactly five templates, each with one text field named `message`:** `brother_9mm`
(7.1mm printable), `brother_12mm` (9.9mm), `brother_18mm` (15.8mm), `brother_24mm` (18.1mm), and
`avery5163`, now a plain 2x4 inch label, ten per US Letter sheet, no options. One field means one CSV
column maps to any of the five with no per-template branching in an importer.

**Engine demonstrations move to `tests/fixtures/templates/`.** `avery5163_asset_tag` (the previous
rich `avery5163`: options, rotation, five fields) joins `brother_18mm_qr`, `brother_24mm_qr`,
`brother_24mm_multiline` and `homebox-qr` there, five fixtures total. They are not installable from the
UI; they exist to back tests and to be read as examples when authoring a template by hand.
`templates::load_all_for_tests()` reads both `catalog/` and `tests/fixtures/templates/` into one flat
registry so the HTTP test suite keeps exercising sheet format, options, container rotation, QR layout
and interpolation without those templates being publishable.

**Other tape widths (6mm, 36mm, ...) are documented, not shipped.** An owner copies the closest
Brother template and edits three fields: `format.height` (printable height, narrower than the nominal
tape), `format.media_width` (nominal width, used for print preflight) and the `font_size` range. Adding
a sixth or seventh catalog entry per tape width was rejected: the four already in the catalog cover the
common Brother TZe sizes, and each additional width is a three-field copy an operator can make in under
a minute, not a maintenance burden worth carrying in the repo for widths most installs never touch.

**Three CI gates pin this shape**, replacing the single "catalog renders" check ADR-0046 introduced:
`every_template_renders` (the exact ten ids across both roots must parse, validate and render),
`catalog_is_exactly_the_starter_set` (`catalog/` must be exactly the five ids above, no more, no
fewer), and `template_ids_are_unique_and_match_filenames` (extended to check both roots, so a fixture
and a catalog entry cannot collide).

## Consequences

- **Redefining the published `avery5163` id is a breaking catalog contract change.** Before this
  change, `catalog/index.json` advertised `avery5163`'s fields as `description, id, name, tags, url`;
  after, it advertises `message`. Anything that treated the id `avery5163` as a stable contract — a
  saved CSV import column mapping, a print webhook posting the old field set — breaks silently: the
  server accepts a differently-shaped `data` object and either renders blank fields or returns a
  validation error, neither of which points back at this change. This is not filed under "content
  imports are user-owned" (ADR-0046's framing for template edits); it is a breaking change to what a
  catalog id means, made deliberately because the old shape was wrong for a starter catalog, and
  recorded here so it is discoverable from the id rather than only from a diff.
- A fresh install's catalog browse list shrinks from ten entries to five. Existing installs are
  unaffected: nothing is re-seeded or re-installed, and any already-installed `avery5163` (the old
  shape, saved to `{config}/templates/avery5163.yaml` before this change) keeps rendering exactly as
  it did.
- [ADR-0005](0005-recursive-containers-with-option-gating.md)'s "see `templates/avery5163.yaml`" now
  points at neither: that path predates ADR-0046's move to `catalog/` and this change on top of it.
  The canonical multi-variant option/rotation example is now
  `tests/fixtures/templates/avery5163_asset_tag.yaml`. ADR-0005 is not edited (ADRs are append-only);
  this paragraph is the correction.
- The README, SPEC and DEPLOY docs' worked examples that named the old `avery5163` shape (its five
  data fields, its `orientation`/`outline` options) now name `avery5163_asset_tag` where they are
  teaching options, and describe the new plain `avery5163` where they are describing the catalog.
