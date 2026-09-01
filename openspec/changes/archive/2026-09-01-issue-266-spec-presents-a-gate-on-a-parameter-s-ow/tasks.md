## 1. Verify every added sentence against the shipped code

- [x] 1.1 Confirm a required, interpolated `integer` parameter is filled with its `min`, or with `1`
  when it declares none (`src/templates.rs`), so the `copies`-with-no-`min` `WHEN` in the restated
  closure scenario of each requirement describes the fill the service performs.
- [x] 1.2 Confirm a `when:` value is compared as a string after `value_to_string` (`src/render/mod.rs`,
  `src/raw.rs`), so `when: { copies: 1 }` is satisfied by the invented `1` and the gated container in
  those scenarios renders.
- [x] 1.3 Confirm a required, interpolated `text` or `textarea` entry is filled with the entry's own
  name (`src/templates.rs`), so `when: { mode: mode }` is satisfied by the placeholder exactly as the
  malformed-gate paragraph added to each requirement states, and so a caller reaches that branch only
  by sending the parameter's own name as its value.
- [x] 1.4 Confirm the thumbnail builds its fill set from `inputs.all` rather than `inputs.default`, so
  the closure argument the rationale keeps is the rule the code follows.
- [x] 1.5 Confirm a `flow` container packs only its active children and accumulates their extents, and
  that a child landing outside the padded inner box under the default `overflow: fail` is
  `422 UnsupportedLayoutItem` with `details.reason` of `item_out_of_frame` naming that child
  (`src/errors.rs`, `src/reason.rs`, and the `flow-layout` capability), so both overrun paragraphs and
  both overrun scenarios name the failure the service actually returns.
- [x] 1.6 Confirm `ui/src/lib/preview.ts` fills samples by the thumbnail's rule with the `select`
  difference the requirement already states, and surfaces a failed render rather than suppressing or
  retrying it, so the paragraphs added to the screen requirement describe the shipped preview.

## 2. Verify the delta resolves and carries what the proposal claims

- [x] 2.1 Confirm both `### Requirement:` headings in the delta match, verbatim, requirements already
  present in `openspec/specs/template-inputs/spec.md`, so archive resolves each `MODIFIED` by name.
- [x] 2.2 Confirm each `MODIFIED` block carries its complete requirement, from `### Requirement:`
  through its last scenario rather than the edited fragments, and that the delta names no other
  requirement — which is what `.workflow/archive-merge-check.sh` checks when the change is archived.
- [x] 2.3 Confirm the delta differs from those two requirements only in the ways `proposal.md` lists:
  the worked example moved to the `integer` fill, one malformed-gate paragraph and one overrun
  paragraph added to each requirement, the closure scenario restated in each, three scenarios added,
  and the preview's sample-fill rationale split into its own paragraph.
- [x] 2.4 Confirm every scenario left in either requirement asserts something the shipped renderer
  does, and that nothing the renderer does in this area was dropped from the record.

## 3. Verify nothing outside the change folder moved

- [x] 3.1 Confirm `git status` reports no change under `src/`, `ui/src/`, `tests/`, `catalog/`,
  `tests/fixtures/templates/` or `docs/`, and that the change folder is the only path touched.

## 4. Gates

- [x] 4.1 Run `cargo fmt`, `cargo clippy --all-targets --all-features` and `cargo test`, and confirm
  each passes with no test added, edited or removed.
