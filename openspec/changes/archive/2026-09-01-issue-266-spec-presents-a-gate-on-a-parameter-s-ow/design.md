## Context

[#266](https://github.com/pfa230/labeler/issues/266) is a spec correction. The renderer is right and
stays untouched; `openspec/specs/template-inputs/spec.md` is wrong twice, in the same way, about the
same shape.

The thumbnail builds placeholder data from `inputs.all` and gives a required, interpolated `text` or
`textarea` entry the entry's own name as its value. The renderer then evaluates `when:` against that
data, so a gate written as `when: { mode: mode }` is satisfied by the placeholder. Two requirements
use exactly that gate as the worked example of a supported case, and two scenarios assert it. Since
#263 gave `flow` containers packed children, two such gates on two packed siblings both activate and
accumulate past the container, so a thumbnail returns `422 UnsupportedLayoutItem` /
`item_out_of_frame` on a template that renders for every caller sending ordinary values, and whose
overrun only a caller deliberately sending each involved parameter's own name as its value
reproduces. Nothing in the spec prepares a reader for that.

Three constraints bound the rewrite, all from the issue:

1. The spec must keep describing what the code does: filling from `inputs.all`, the closure argument
   for it, and the fact that an invented value can decide a gate.
2. The degenerate shape must stop reading as endorsed.
3. The limit must be recorded, including the error a thumbnail returns when it is hit.

## Goals / Non-Goals

**Goals**

- Both requirements stop presenting a gate valued at its own parameter's name as an authoring shape.
- Both requirements state the packed-`flow` accumulation limit and name
  `UnsupportedLayoutItem` / `item_out_of_frame`.
- Every scenario left in either requirement asserts something the shipped renderer does, and
  everything the renderer does in this area stays documented.

**Non-Goals**

- Making an invented placeholder value unable to decide a gate. The issue weighed and rejected it: it
  costs a renderer change for a template shape nobody writes.
- Changing what a required string's placeholder is.
- Any thumbnail-only relaxation of the `flow` `overflow: fail` contract.
- Touching `src/`, `ui/src/`, `tests/`, `catalog/` or `tests/fixtures/templates/`.

## Decisions

### The closure property keeps a scenario, restated over a shape an author writes

The rule the two scenarios existed to pin is real and load-bearing: filling from `inputs.all` rather
than `inputs.default` is what stops a fill activating a branch whose own names are then unfilled.
Deleting the scenario would leave that rule unpinned. Keeping it with `mode: mode` would keep
endorsing the shape.

An invented value that decides a gate does not have to be a `string` filled with its own name. A
required, interpolated `integer` is filled with its `min`, or `1` when it declares none
(`src/templates.rs:180`), and `when:` values are compared as strings after `value_to_string`
(`src/render/mod.rs:1292`, `src/raw.rs:124`), so `when: { copies: 1 }` is satisfied by the invented
`1`. That is an ordinary gate an author writes, an ordinary fill, and the same closure property.

**Decision.** The retained scenario keeps its name — the property is unchanged — and its `WHEN`
becomes the `integer` case. The prose example in the gate paragraph moves with it.

*Alternative rejected:* a `checkbox` gated on `false`. It works, since a required interpolated
checkbox is filled `false`, but `false` as a gate value invites confusion with the `bold` case
`param-resolution` already documents as *not* invented for, and the integer case needs no such
disambiguation.

### The degenerate shape gets stated, not deleted

The `mode: mode` case is what the renderer does, so removing it would leave real behaviour
undocumented — the issue forbids that as explicitly as it forbids endorsement.

**Decision.** Each requirement gains a paragraph that names the shape, states plainly that the gate is
satisfied and its branch draws, and then says what the shape is: a gate reachable by no declared
default that is not the parameter's own name and by no caller who does not type `mode` into the `mode`
field. The paragraph names `enum` as the shape an author writes instead, with the reason it works —
the thumbnail never invents for a `select`, the default option selection supplies its value, and an
operator can choose it. A scenario carries the same fact in testable form, with an `AND` clause
recording that a caller reaches the branch only by sending the parameter's own name.

The scenario asserts the render outcome, which is checkable, and the `AND` clause states the
consequence rather than a second outcome, so nothing in it is a behaviour a test would have to invent.

*Alternative rejected:* refusing such a gate at load. That is a behaviour change, out of scope by the
issue's non-goals, and it would reject templates that load and render today.

### The limit is recorded in both requirements, not cross-referenced from one

The two requirements are read by different audiences: the thumbnail requirement by whoever debugs a
422 from `GET /api/templates/{id}/thumbnail`, the screen requirement by whoever builds or debugs the
preview pane. A reader who hits the error should find it where they already are.

**Decision.** Both requirements state the accumulation, the error code and reason, and that the fault
is the template's. Both state what the preview does *not* do about it: no withheld fill, no ignored
gate, no relaxed overflow policy. The screen requirement adds that the failure is surfaced as any
other failed render is, which is what `ui/src/lib/preview.ts:66` already does.

This is a duplication on purpose. The alternative — stating it once and pointing at it — puts the
answer one hop away from both readers rather than zero hops from one, and neither requirement is the
natural home for the other's audience.

### Both deltas are MODIFIED, and carry the complete requirement

Both requirements already live in `openspec/specs/template-inputs/spec.md`, so the first-touch rule
does not apply and no `ADDED` requirement is written. `.workflow/archive-merge-check.sh` checks that
every requirement a delta names lands verbatim, so each delta carries its requirement whole, from
`### Requirement:` through the last scenario, not the edited fragments.

One formatting fix rides along inside the screen requirement: the preview's sample-fill rationale is
currently jammed onto the end of the paragraph stating the grid rule, mid-line. The rewrite splits it
into its own paragraph. It is the same text, in the requirement this change is already replacing.

## Risks / Trade-offs

- **A scenario could assert something untrue.** The `integer` fill (`min` or `1`), the string-valued
  `when:` comparison and the 422 code were each checked against `src/` before being written into a
  scenario. The plan review is the second check.
- **Two paragraphs saying nearly the same thing can drift.** Accepted, for the reason above: a reader
  of either requirement should not have to follow a pointer. Both name `flow-layout` as the source of
  the packing rule, so the rule itself has one home.
- **No test changes, so nothing mechanical proves the spec now matches the code.** That is inherent to
  a spec correction. What is checked is that `cargo test` still passes unchanged, that no file under
  `src/` or `ui/src/` moved, and that the archive sync satisfies `archive-merge-check.sh`.

## Migration Plan

None. No behaviour, API, template or stored data changes. The change is one commit carrying the
delta, the archived change folder and the synced `openspec/specs/template-inputs/spec.md`.

## Open Questions

None.
