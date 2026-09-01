## Context

See proposal.md — Why. What the code looks like today, since the merge is mostly a subtraction:

- `Ink` (`src/models.rs:831`) is `{ spelling: String, rgba: [u8; 4] }` with `FromStr`, `Serialize`
  (writes the spelling), `Deserialize` and a hand-written utoipa string schema. Its table is Typst's
  eighteen names, matched case-sensitively.
- `Color` (`src/models.rs:1090`) is `{ r, g, b, a }`, `Copy`, with `Color::BLACK`, `Color::rgba()`
  and `hex()`. It is parsed by `parse_color` (`src/raw.rs:40`) over the sixteen CSS names, matched
  case-insensitively, and serializes canonically as `#rrggbbaa`.
- The dynamic form is `DynamicValue<T>` (`src/models.rs:218`), whose `Ref` variant serializes back as
  `"{name}"`. `TextRaw.ink` is `Option<Dynamic<Ink>>` behind a bespoke visitor
  (`deserialize_dynamic_ink`, `src/raw.rs:223`); `StrokeRaw.color` and the container's `background`
  are plain `ColorRaw`, with no reference form.
- Load-time reference checking is `check_param_ref(params, name, field, &["string", "enum"])`
  (`src/templates.rs:1339`), called for `ink` at `src/templates.rs:1465`. Input derivation records the
  same reference at `src/templates.rs:299`. Render-time resolution is `resolve_dynamic_value_ink`
  (`src/render/helpers.rs:223`), reached from `src/render/mod.rs:1776`.
- Three emission sites consume a colour: the text fill (`src/render/mod.rs:1875`), the line stroke
  (`src/render/mod.rs:2045`), and the container rect's fill and stroke (`src/render/mod.rs:2114`).
  The last two format with `Color::hex()`, the first with the `rgba()` components.

Constraints that shape the approach: adding or moving a layout field means editing `raw.rs`,
`models.rs` and `convert.rs` together; load-time validation and render-time resolution are separate
walks that must agree; every model in the API is registered in `src/openapi.rs`; no shipped template
or fixture writes `ink:`, so the breaking rename has no in-repo migration.

## Goals / Non-Goals

**Goals:**

- One type, one parser, one name table, reached by all three colour-bearing fields.
- The parameter-reference form on `stroke.color` and `background`, plumbed through the same three
  places text already uses: load-time check, input derivation, render-time resolution.
- A test that proves the cross-field invariant on the emitted paint rather than on the parser.

**Non-Goals:**

- No new field takes a colour. Paint is still not inherited, `line` still has no interior, and no item
  gains a colour it did not have.
- No change to layering, stroke geometry, rounding, clamping, or the bilevel path.
- No migration path for `ink:`. #291 settles this: no alias, no deprecation window, no warning.
- The `text-ink` capability directory keeps its name. Renaming a capability moves every requirement in
  it and is worth its own issue; this change leaves the name stale and says so.
- The UI's `--ink` CSS token (`ui/src/theme.css`) is the application chrome's foreground colour, owned
  by `ui-colour-palette`. It is a different concept and is untouched.
- Null handling is not unified. `text.color: null` remains an absence and the paint keys keep refusing
  an explicit null; see the decision below for why that is consistent rather than deferred.

## Decisions

### The merged type is `Color { spelling, rgba }`, and `Ink` is deleted

`Ink`'s shape survives under `Color`'s name, which is what #291 decision 1 asks for: the type must
carry `Ink`'s capabilities, not `Color`'s. `Color::from_str` becomes the single parser, built from
`parse_color`'s table and case-insensitivity and `Ink`'s hex handling and stored spelling.
`parse_color` and `deserialize_dynamic_ink` both disappear into it, and `ColorRaw` becomes a
`DynamicValue<Color>` deserializer shared by the three fields.

Consequences worth naming before implementation, because they touch call sites the diff review will
otherwise meet cold:

- `Color` stops being `Copy`, because it holds a `String`. Every site that copies one clones instead.
  The compiler finds them all.
- `Color::BLACK` stops being a `const` for the same reason and becomes a constructor. Its callers are
  `src/convert.rs:33` (the stroke default) and render tests.
- `hex()` stays, because the Typst emitter needs a canonical form regardless of what the API reports.

Alternative considered: keep `Color { r, g, b, a }` and drop the spelling, giving up the authored form
in the API. Rejected because #291 decision 1 states the surviving type keeps a spelling; see the next
decision for where that costs something.

### A colour reads back as authored, on every field

This is the one place where #291's decisions and a shipped requirement collide, and it is the point a
reviewer should attack first.

One type has one serialization. `Ink` writes its spelling; `Color` writes canonical `#rrggbbaa`
(`shape-paint`, "A colour is reported canonically wherever a template is read back", asserted at
`src/lib.rs:9876`). Merging them forces a choice, and a per-field carve-out — canonical on shapes,
authored on text — would rebuild the split this change exists to delete.

Chosen: **authored spelling everywhere**, which follows #291 decision 1's stated purpose ("so what the
author wrote is recoverable") and keeps `spelling` a field something reads. `background: red` reports
`"red"`; an omitted `stroke.color` reports `"black"`, since a `stroke` block always carries a colour
and that default is a name rather than an absence.

What changes is how a colour is spelled, not which keys appear. An omitted `text.color` stays absent
from the response, exactly as an omitted `ink` is absent today (`openspec/specs/text-ink/spec.md`,
"SHALL omit the key for an item that declared none"), and an omitted `background` likewise. Reporting
a materialized `"black"` for an uncoloured text item would be a second, undeclared API break riding
on this one, and this change makes none: only `stroke.color`, whose default is part of the `stroke`
block's own contract, is reported when it was not written. The rule is stated in
`colour-vocabulary`'s read-back requirement and its text half is scenario-tested in `text-ink`.

The cost is real and belongs in the record: it withdraws the normalization #280 shipped one day
earlier, so a client that compared `#rrggbbaa` strings must now resolve names itself. What it does
not cost is the authored template, which `GET /templates/{id}/source` (`src/api.rs:1163`) has always
returned verbatim.

Alternative considered: **canonical `#rrggbbaa` everywhere**, dropping `spelling` from the type. It is
the better API — one representation per colour, comparable by string, no name table in the client —
and the source endpoint already recovers the authored form, which is the stated job of `spelling`. It
was rejected only because #291 decision 1 is explicit that the type keeps a spelling, and the
acceptance criteria do not name read-back as something the change may move. Flipping to it later is
one requirement in `colour-vocabulary`, one `Serialize` impl, and the `spelling` field: if the
reviewer or the issue's author prefers it, it is a small edit, not a re-plan.

### `{param}` reaches shape paint by the path text already uses

`Stroke.color` becomes `DynamicValue<Color>` and the container's `background` becomes
`Option<DynamicValue<Color>>`. Nothing new is invented:

- **Load:** `validate_item_references` gains the two shape fields, calling `check_param_ref` with the
  same `["string", "enum"]` allowlist and a field name that reaches the error message
  (`background`, `stroke.color`).
- **Inputs:** the `Container` and `Line` arms of the input walk record the reference exactly as the
  `Text` arm records `ink` today, as not-interpolated, and inherit the existing `when`-gating.
- **Render:** `resolve_dynamic_value_ink` becomes `resolve_dynamic_value_color` and is called at all
  three emission sites before formatting.

Alternative considered: resolving shape colours at load time, since a container's paint is not
per-label the way a text value is. Rejected: a parameter is per-request by definition, and a
load-time resolution would need a value nobody has yet.

### The failure reason is renamed to `color_param_invalid`

`Reason::InkParamInvalid` names a field that will not exist and now covers three fields, only one of
which was ever called ink. It is renamed with its `AppError` constructor and message text. This is a
wire-visible break not listed in #291's decisions; it is derived from decisions 3 and 4, and it is
called out in the proposal and in the `text-ink` delta's migration note so it is not discovered by a
client. The alternative — keeping a reason named after a deleted concept — is the naming rot the
issue exists to remove.

### Null is a field rule, and the shared parser never sees one

`ink: null` on a text item parses as absence today (`src/raw.rs:604`), while `background: null` and
`stroke: { color: null }` are refused (`shape-paint`, "An explicit null is not a spelling of
absence"). Both behaviours survive the merge unchanged, and they do not contradict the shared
vocabulary, because they are decided one layer above it.

The split is in the deserializers, not in `Color`. Each field's `raw.rs` deserializer decides what a
missing key and a written-but-empty key mean before any colour exists: `text.color` maps both to
`None`, and the paint keys keep their `Option<Option<_>>` presence encoding, which is what lets
`convert.rs` refuse `background: null` naming the field. Only a present, non-null value reaches
`Color::from_str`, which is why the vocabulary can refuse every non-string value without touching
either rule. `colour-vocabulary`'s first requirement says exactly this, and points at the two
capabilities that own the two answers.

Keeping the asymmetry is a deliberate non-goal of this change, not deferred work: #291 renames a
field and merges two types, and inventing a refusal for `color: null` would break templates no
requirement here covers.

### Spec layout: a third capability owns the vocabulary

`colour-vocabulary` is new and owns what a colour is; `text-ink` keeps only the field on a text item;
`shape-paint` keeps the shape fields and points at the vocabulary. Neither existing capability was a
defensible home for a shared contract: `text-ink`'s Purpose says it covers the text item's colour "and
nothing else", and `shape-paint`'s covers "how a shape declares what it is drawn with". The repo
already separates shared vocabularies this way (`param-resolution`, `interpolation-tokens`).

Requirements that move do so as REMOVED here plus ADDED there, not as RENAMED, because their content
changes as well as their home, and `archive-merge-check.sh` checks that a named requirement landed
verbatim or is gone.

One manual step this creates: the `text-ink` capability's `## Purpose` still describes a field called
`ink`. A delta's Purpose is ignored for an existing capability, so `openspec/specs/text-ink/spec.md`
must be edited directly in the same commit that archives this change.

### The authoring guide is part of the change, not a follow-up

`docs/AUTHORING.md` is the worked-example guide to the template model, and its colour paragraph
(`docs/AUTHORING.md:500-504`) teaches the exact distinction this change deletes: that shape paint uses
the CSS values "whereas text `ink` uses Typst's typography color palette". Left alone it would be the
last place in the repository still asserting two vocabularies, in the document an author is most
likely to read.

The same commit rewrites that bullet to state one vocabulary — the sixteen CSS Level 1 names matched
case-insensitively, hex in 3, 4, 6 or 8 digits, or a `"{param}"` reference — accepted by
`stroke.color`, `background` and `text.color` alike, with the name denoting the same colour on each,
and documents `color` on the text item, noting that `ink` is gone and a template using it is
quarantined. The `stroke.color` bullet's `#000000ff` phrasing becomes `black`, matching what the API
now reports. Nothing else in the guide moves.

## Risks / Trade-offs

- **A text template's colours change value silently.** `color: red` parses before and after and paints
  a different red. This is the only silent break in the change; every other break is loud (`ink:`
  unknown, `eastern`/`orange` refused). → Nothing in the service can detect it, so it is documented in
  the proposal, in the `text-ink` removal's migration note, and in ADR-0093, and an author wanting the
  Typst value writes it as hex. No template in this repository is affected.
- **Withdrawing canonical read-back breaks a client that compared strings.** → Stated as a REMOVED
  requirement with a migration note rather than left to be discovered; `GET /templates/{id}/source` is
  unchanged; the decision above records how to flip it back if that trade is judged wrong.
- **`Color` losing `Copy` ripples through render and test code.** → Mechanical and compiler-driven, but
  it makes the diff wider than the change sounds; the reviewer should expect churn that is not
  behaviour.
- **Two walks must agree.** Load-time validation and render-time resolution are separate code paths
  (`templates.rs` and `render/`), and #150 and #155 are what happens when they drift. → The two shape
  fields are added to both in the same task, and the load-time refusals are tested per field, not once.
- **A partially-migrated merge leaves two tables.** Deleting `parse_color` and `Ink::from_str` in
  favour of one parser is the whole point; leaving either in place would reintroduce the defect
  quietly. → The acceptance criterion is textual (`Ink` appears nowhere in `src/`) and checkable by
  grep, and the cross-field test compares emitted paint rather than parser output.

## Migration Plan

No data migration: templates live in a config directory and are read at load. A template written
against the old spellings fails loudly at load, is quarantined with a path-carrying error, and does
not stop the server or any other template from serving (`template-registry`). Rollback is reverting
the commit; nothing is written or transformed on disk.

Order that keeps the tree green at each step: merge the type and its parser first (both tables into
one), then the field rename, then the reference form on the two shape fields, then the read-back and
reason-code changes with their tests, then OpenAPI, ADR-0093 and its index row.
