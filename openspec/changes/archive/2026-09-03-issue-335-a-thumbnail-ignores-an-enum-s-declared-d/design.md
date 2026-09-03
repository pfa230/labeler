## Context

See proposal.md ("Why"). The mechanics that produce the defect, verified in the tree at
`e57d5ef`:

- `TemplateContent::placeholder_data` (`src/templates.rs:157-189`) fills an entry that is
  `interpolated && required`, by `control`, and hits `InputControl::Select => continue`. An `enum` is
  therefore never filled from placeholder data.
- `default_option_selection` (`src/render/mod.rs:2401-2413`) builds `{ name -> first value }` for
  every declared `enum`, through `TemplateContent::options()`, which synthesizes an option map out of
  the `params` block. The `thumbnail` handler (`src/api.rs:1254`) is its only production caller.
- `resolve_parameters_mode` (`src/render/mod.rs:226-232`) merges that map into the request data
  *before* the per-parameter loop, so the declared `default:` is never consulted.

Two further consumers of the `option` argument are **not** this change's business and stay:
`normalize_option` (`:1161-1185`), which validates a caller-supplied map, and
`RenderContext.selected_option`, whose fallback in `is_item_active` (`:1352-1356`) answers a `when:`
key the data does not carry. After this change nothing populates either in production, and #214
deletes both.

Constraints that shape the work:

- Clippy runs with `-D warnings` in CI (`.github/workflows`), so an argument that stops being read
  cannot simply be left in place.
- `InputSpec` already carries what the new rule needs: `values` for an `enum`, and the pair
  (`default`, `default_error`) that distinguishes "no default declared" (both `None`) from "declared
  and unresolvable" (`default_error: Some`, `required: true`), per `src/templates.rs:406-411`.
- `openspec validate` refuses a `MODIFIED` requirement that drops or renames any scenario the
  published spec still has (`findMissingCurrentScenarios`).

## Goals / Non-Goals

**Goals:**

- An `enum` resolves on the thumbnail path through the same two sources every other type uses.
- A template whose printed `enum` declares no default still previews, with a legal value.
- The published specs stop describing a preview-only option selection that no longer exists.

**Non-Goals:**

- Deleting the option-map plumbing this leaves dead (#214).
- Changing what the browser fills for its own previews (`ui/src/lib/preview.ts`), which is #343.
- Deciding whether a preview should mask a broken default at all, which is #344. Stating that it does,
  on every control but `select`, is a goal: it is the rule this change publishes.

## Decisions

### D1. The `select` stand-in is conditioned on "declares no default", not on `required`

The rule, stated once and stated the same way in every artifact: placeholder eligibility is
`interpolated && required`; a `select` additionally requires that its parameter declare no `default:`.
`required` is true both for a parameter declaring no default *and* for one whose declared default
fails to resolve (`template-inputs`, "A parameter whose declared default fails to resolve is
`required: true`"), so the consequence is an asymmetry: **a broken default on any control but `select`
is masked by that control's placeholder and the thumbnail renders, while a broken `enum` default
propagates as `param_default_unresolvable`.**

Taken because #335 says so in terms ("The first-value stand-in applies only where no default is
declared at all. Do not write a contract that lets a broken default fall back"), and because the
uniform alternative would keep the exact defect this change exists to remove for the templates that
most need it surfaced: an `enum` whose default is broken would go on showing the first of its
`values`, which is a legal value of that parameter and therefore indistinguishable in the catalog grid
from a healthy template's thumbnail. Every other fill is self-announcing (`title`, a 1×1 PNG, `false`,
`1`), so the asymmetry buys something real. The delta states the condition and the reason together,
next to the rule it bends, as `CLAUDE.md` requires of a surviving exception.

**Alternative rejected:** fill a `select` on `interpolated && required` like everything else. It is
the simpler rule and it changes nothing for a broken default (today the option map masks it either
way), but it writes into the contract that a preview may show an invented enum value for a template
whose author asked for a different one and whose every render is `422`.

**Consequence to state plainly:** this is a behavior change beyond #335's acceptance-criteria list. A
thumbnail of a template whose `enum` default cannot be resolved is `422 param_default_unresolvable`
where it used to render. #335's own text asserts that a broken default already fails every preview; it
does not, on the enum path because the merge stands in first, and on every other control because the
placeholder does. The criterion "a declared default that fails to resolve still fails, unchanged" is
met in the sense the issue means. Whether the resulting split between `select` and everything else is
the right behavior is **#344**; this change publishes it and states it in both capabilities rather
than leaving either one claiming that a parameter declaring a default is never stood in for.

### D2. Delete the `option` argument from `resolve_parameters` / `resolve_parameters_mode`

Removing the merge leaves that argument unread, which is a `-D warnings` failure and, per the pre-1.0
rule, a parameter read and ignored. It is deleted from both functions and from every call site
(`src/render/mod.rs:683,1009`, `src/templates.rs:145`, plus the test call sites). No cascade follows:
`compile_label_source` keeps its own `option` for `normalize_option` and `RenderContext`, so the rest
of the plumbing #214 lists is untouched.

**Alternative rejected:** rename it `_option` and leave it for #214. That is a parameter kept alive
only to satisfy a linter, in a function whose contract no longer mentions it.

### D3. Both stale spec sentences are corrected here, in four `MODIFIED` requirements

`template-inputs` and `param-resolution` each carry two requirements that state, as fact, what the
thumbnail passes alongside its data. Two of them are the contract this change rewrites; the other two
merely describe it, and this change is what makes them false:

- `param-resolution` / "A parameter is required unless the template declares a default" says the
  option-selection argument "is populated by nothing but a preview, and is specified by the preview
  requirement below". After this change nothing populates it and no requirement specifies it.
- `param-resolution` / "A preview invents values" says a parameter that declares a `default:` is never
  stood in for. That is false for every control but `select` (`src/templates.rs`,
  `thumbnail_tests_for_new_default_rules`, case 5, and a `qr` value records its names as interpolated
  at `src/templates.rs:302-304`), and this change is what makes the precise rule statable, so the
  claim and the one scenario resting on it are corrected here. The behavior is untouched and is #344.
- `template-inputs` / "A screen renders the reported inputs and decides nothing else" says "a
  thumbnail leaves one to the default option selection it passes alongside the data". It no longer
  passes one.

Correcting a description of this change's own subject is inside its scope; leaving `openspec/specs/`
with two requirements contradicting each other about the thumbnail is not an option, and #214 declares
that it carries no delta, so nothing later would fix them. What a *client* fills is left exactly as
published.

### D4. Scenario and requirement names are kept even where they read stale

`openspec validate` refuses a `MODIFIED` block that drops or renames a scenario the published spec
still has, and there is no scenario-level `REMOVED`. So:

- `template-inputs` / "A printed enum shows the option selection where the two differ" keeps its name
  and gets a body stating that the two cannot differ, because nothing is merged. The alternative,
  `REMOVED` + `ADDED` of the whole requirement under a new name, is a heavier operation that also
  moves the requirement to the end of the published file.
- `param-resolution` / "A thumbnail still shows an enum-gated branch" keeps its name and is made true
  again by giving `outline` a `default: yes`; the undefaulted case moves to a new scenario beside it.
- `param-resolution` / "A thumbnail fails on a broken default a token reads" keeps its name and is
  made true again by moving it from a `string` read by a `qr` item, where the placeholder masks the
  broken default, to an `enum` read by a `text` item, where it propagates. The masking case gets its
  own scenario beside it, so the requirement states both halves of the rule rather than one.
- The requirement name "The thumbnail renders the default selection from placeholder data" is stale
  for the same reason and is kept for the same one: archive resolves `MODIFIED` by name.

## Implementation notes

- `src/templates.rs`, `placeholder_data`: replace `InputControl::Select => continue` with the first of
  `input.values`, taken only when the entry declares no default. Given the enclosing `required` guard,
  that is `input.default_error.is_none()`: a resolved default makes the entry `required: false` and it
  never reaches the arm, and a broken one carries `default_error: Some`. An entry whose `values` is
  `None` or empty cannot occur (validation refuses an `enum` with empty `values`), and the code should
  fail loudly rather than silently skip if it ever does.
- `src/render/mod.rs`: delete `default_option_selection` and the `if let Some(opt) = option` merge at
  the head of `resolve_parameters_mode`, and drop the argument (D2).
- `src/api.rs`, `thumbnail`: drop the `option` binding and pass `None`.

## Test plan

The acceptance criteria ask for tests that pin *what the label shows*, which no assertion on a PNG can
read directly. Two techniques already in the tree cover it:

1. **Byte-identical render comparison.** Render the enum template's thumbnail and the thumbnail of a
   control template identical but for a literal `vertical` in place of `{orientation}`; assert the
   bytes are equal, and assert they differ from the `horizontal` control. Precedent:
   `src/lib.rs:12681`, `src/render/mod.rs:3745`.
2. **Resolved-data assertions** on `placeholder_data` + `resolve_parameters`, which pin the value the
   renderer is handed. Precedent: `thumbnail_tests_for_new_default_rules` (`src/templates.rs`).

Cover, at minimum: the declared default (`vertical`), the undefaulted printed enum (`horizontal`), the
gate-only undefaulted enum leaving its item out, and the broken enum default returning `422` with
`param_default_unresolvable`. At least one goes through the HTTP endpoint in `src/lib.rs`, beside the
existing `thumbnail_*` tests, since the criteria are about `GET /api/templates/{id}/thumbnail`.

Existing tests that must be updated rather than deleted wholesale:
`every_template_renders`, `avery5163_asset_tag_thumbnail_renders_horizontal_branch`,
`dump_all_template_renders` (it iterates orientations through the option map and must vary `data`
instead), `thumbnail_closure_renders_required_and_min_values`,
`gate_key_not_interpolated_is_never_invented_for`,
`name_service_resolves_on_its_own_is_never_invented_for`, `thumbnail_tests_for_new_default_rules`.
`default_option_selection_picks_first_values` goes with its subject.

## Risks / Trade-offs

- **A shipped template's thumbnail changes.** → Nothing under `catalog/` declares an `enum`. Two
  fixtures do: `tests/fixtures/templates/avery5163_asset_tag.yaml` (`outline: [yes]`, no default)
  gating a stroked container, and `tests/fixtures/templates/container_circle_gated.yaml`
  (`enabled: [yes, no]`, default `no`) gating a stroked circle. Both thumbnails change — the outline
  disappears and the circle stops drawing (it was forced to `yes` by `default_option_selection` ahead
  of the declared `no` / absence). That is the intended behavior; the rule is pinned by the synthetic
  gate-only tests (`src/templates.rs` + `src/lib.rs` HTTP twin), while `every_template_renders`
  continues to assert the id set and PNG magic only and passes with the circle either way.
- **A template that previewed now fails.** → Two classes. (1) An `enum` default that cannot be
  resolved, which is a template every render of which already fails (D1). (2) An active item that
  references an undefaulted `enum` through a colour (`color`, `background`, `stroke.color`) or
  dimension `{ref}` (`src/templates.rs:295,340,362,368` are `interpolated: false`): `placeholder_data`
  only fills `interpolated && required` (`src/templates.rs:163`), so the name is now absent and the
  thumbnail is `400 color_param_invalid` / `missing_field`, while a caller's render that supplies the
  `enum` succeeds. The previous option selection supplied every declared `enum` via
  `TemplateContent::options()` (`src/templates.rs:82-94`).
- **The delta is large** (four full requirements restated, ~950 lines). → `MODIFIED` requires the
  complete post-change block. The edits were applied programmatically to text extracted from
  `openspec/specs/`, so the untouched prose is verbatim; `archive-merge-check.sh` verifies that at the
  landing commit.
- **The browser's own preview keeps filling every declared `enum`**, so a template preview can differ
  from the same template's thumbnail. → Out of scope, filed as #343, and recorded in the spec text
  where the difference is stated.
- **A broken default is masked on every control but `select`.** → Published rather than hidden, in
  both capabilities, and filed as #344 for the decision this change does not make.
