## 1. Implementation

- [x] 1.1 `src/templates.rs`, `placeholder_data`: replace `InputControl::Select => continue` with the
      first of `input.values`, taken only where the entry declares no default, which under the
      enclosing `required` guard is `input.default_error.is_none()`. An entry whose `values` is `None`
      or empty cannot occur and must fail loudly rather than skip silently (design.md, Implementation
      notes).
- [x] 1.2 `src/render/mod.rs`: delete `default_option_selection` and its doc comment.
- [x] 1.3 `src/render/mod.rs`: delete the `if let Some(opt) = option` merge at the head of
      `resolve_parameters_mode`, and drop the `option` argument from `resolve_parameters` and
      `resolve_parameters_mode` (design.md, D2). Update the call sites at `src/render/mod.rs:683` and
      `:1009` and `src/templates.rs:145`, leaving `normalize_option` and `RenderContext.selected_option`
      alone.
- [x] 1.4 `src/api.rs`, `thumbnail`: drop the `option` binding and pass `None` to
      `render_thumbnail_png`.

## 2. Tests for the new contract

- [x] 2.1 A thumbnail of a template printing `{orientation}` where `orientation` declares
      `values: [horizontal, vertical]` and `default: vertical` shows `vertical`. Pin what it shows by
      comparing the rendered bytes against a control template carrying the literal `vertical`, and
      assert they differ from a `horizontal` control (design.md, Test plan).
- [x] 2.2 A thumbnail of the same template with no `default:` shows `horizontal`, the first of its
      `values`, and renders rather than failing on an unresolved token.
- [x] 2.3 A thumbnail of a template gating an item on an `enum` that declares no `default:` and that
      no active item prints renders with that item inactive.
- [x] 2.4 A thumbnail of a template gating an item on an `enum` that declares a matching `default:`
      renders with that item, through the declared default.
- [x] 2.5 A thumbnail of a template whose printed `enum` declares a default that cannot be resolved is
      `422` with `details.reason` `param_default_unresolvable` naming the parameter.
- [x] 2.6 A thumbnail of a template whose printed `string` declares a default that cannot be resolved
      still fills the entry with its own name and renders, so the split between `select` and every
      other control is pinned on both sides.
- [x] 2.7 At least one of 2.1 to 2.6 goes through `GET /api/templates/{id}/thumbnail` in `src/lib.rs`,
      beside the existing `thumbnail_*` tests.

## 3. Existing tests

- [x] 3.1 Delete `default_option_selection_picks_first_values` (`src/render/mod.rs`) with its subject.
- [x] 3.2 Rework `dump_all_template_renders` (`src/render/mod.rs`) to vary `orientation` through the
      label `data` rather than through an option map.
- [x] 3.3 Update `every_template_renders` and
      `avery5163_asset_tag_thumbnail_renders_horizontal_branch` (`src/render/mod.rs`) for the dropped
      `option` argument and for the `outline` container the avery fixture's thumbnail no longer draws.
- [x] 3.4 Update `thumbnail_closure_renders_required_and_min_values`,
      `gate_key_not_interpolated_is_never_invented_for`,
      `name_service_resolves_on_its_own_is_never_invented_for` and
      `thumbnail_tests_for_new_default_rules` (`src/templates.rs`), keeping each test's subject and
      changing only what the new rule changes.

## 4. Gates

- [x] 4.1 `cargo fmt`
- [x] 4.2 `cargo clippy --all-targets --all-features`
- [x] 4.3 `cargo test`
