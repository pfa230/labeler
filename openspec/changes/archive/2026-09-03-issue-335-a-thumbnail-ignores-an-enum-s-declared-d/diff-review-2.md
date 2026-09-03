TREE_SHA256: 04dc0ffeac297d800132eb109edaac8ffadca8294a549c02ac518bbdf4e5fc55

I reviewed the diff, the four artifacts, both spec deltas, and verified the key behavior empirically against a build of the base commit.

## What I confirmed independently

Gates are green in this worktree: `cargo fmt --check` exit 0, `cargo clippy --all-targets --all-features` exit 0, `cargo test` exit 0, `openspec validate` reports the change valid. [verified]

`SPECS_SHA256` recomputes to `f959027…`, matching the value in `review.md`, so `specs/` is untouched since the approving plan verdict. [verified]

The core mechanism is correct. The new `Select` arm (`src/templates.rs:183-194`) sits inside the `interpolated && required` guard at `src/templates.rs:163`, where `required` is true for "no default" or "broken default" and `default_error` separates them (`src/templates.rs:414-419`), so `if input.default_error.is_some() { continue; }` is exactly D1's condition. Both `panic!`s are unreachable: `Select` is produced only for `ParamType::Enum` (`src/templates.rs:388`), `values` is `Some` for exactly that type, and `validate_param_spec` refuses an empty enum (`src/templates.rs:1280-1284`). [verified]

The `option` argument removal is safe for callers: no request model carries one, and a CSV `option.<name>` column is folded into `row.data` before the label is built (`src/api.rs:2767-2771`), so no production path ever supplied the deleted merge. `normalize_option` and `RenderContext.selected_option` are untouched, per D2. [verified]

The three findings the previous round raised are genuinely fixed: the HTTP test now does the byte comparison against `control_vertical` / `control_horizontal` (`src/lib.rs:1332-1409`), the avery test asserts item activity (`src/render/mod.rs:7440-7454`), and `thumbnail_broken_string_default_is_masked` now asserts both `code()` and `reason()` (`src/templates.rs:6714-6715`). [verified]

## Findings

**1. BLOCKING — an unlisted BREAKING change: a thumbnail that referenced an undefaulted `enum` through a colour or dimension `{ref}` now fails where it rendered, and `design.md` states the opposite.**

`derive_inputs_internal` records a colour, background, stroke-colour or dynamic-size reference with `interpolated: false` (`src/templates.rs:295`, `:340`, `:362`, `:368`). `placeholder_data` only fills `interpolated && required` (`src/templates.rs:163`), so the new `Select` arm never reaches such a name: an `enum` declaring no `default:` is now **absent** there. An absent colour ref is `color_param_invalid` (`src/render/helpers.rs:232`) and an absent dimension ref is `missing_field` (`src/render/helpers.rs:157`, `:196`). The deleted `default_option_selection` supplied these names before, because `TemplateContent::options()` (`src/templates.rs:82-94`) walks every declared `enum` in `params` regardless of whether a token reads it, and the deleted merge inserted it ahead of the parameter loop.

Verified end to end rather than by reading. Two templates outside the repo — a `text` with `color: "{palette}"` and a container with `background: "{brand}"`, each over an `enum` with `values: [red, blue]` and no `default:`:

| | base `e57d5ef` | this branch |
|---|---|---|
| `GET /api/templates/enum_color/thumbnail` | `200`, 1774-byte PNG | `400 InvalidRequest`, `color_param_invalid` |
| `GET /api/templates/enum_bg/thumbnail` | `200`, 855-byte PNG | `400 InvalidRequest`, `color_param_invalid` |

[verified: base built from `git archive HEAD` into `/tmp/l335base`, both servers run against equivalent config dirs]

This is exactly the shape `parameter_referenced_color_renders_on_background_and_stroke` already exercises in the tree (`src/render/mod.rs:9862-9866` declares `palette` as an `enum` and `src/render/mod.rs:9884` reads it as `color:`), so it is not hypothetical. And unlike the broken-default case, a caller's render of such a template succeeds the moment it supplies `palette` — the template is healthy.

Two things must change. `proposal.md:12-27` lists three `**BREAKING**` bullets and this class is not among them. `design.md:176-177` says "**A template that previewed now fails.** → Only one whose `enum` default cannot be resolved, which is a template every render of which already fails" — that is false, and it is the sentence a later reader will use to judge blast radius. Both files are context rather than contract, so correcting them costs nothing and does not touch `SPECS_SHA256`. If you conclude the delta should also name the case, note that editing `specs/` voids the plan verdict and belongs on the plan-review path, not a silent re-run of `specs-digest.sh`.

There is also no test for it. Task 2.3 covers the `when:`-gated shape only; nothing covers an active item reading an absent `enum` through a `{ref}`.

**2. Minor — the gate-drop breaking change is never exercised through the thumbnail handler.** `thumbnail_enum_only_gate_without_default_is_absent` (`src/templates.rs:6544-6588`) and the reworked avery test (`src/render/mod.rs:7420-7461`) both build their own `RenderContext` with `selected_option: None` and call `render_thumbnail_png(…, None, …)`, i.e. they assert what the library does when handed no option map rather than that the handler hands it none. What actually pins `src/api.rs:1254`'s `None` is the byte comparison in `thumbnail_enum_with_default_shows_declared_default_via_http`, since a merged option map would override `vertical` with `horizontal`. Coverage exists, but it is indirect and one HTTP-level gate assertion would make it direct.

**3. Minor — leftover shadowing in the reworked avery test.** `src/render/mod.rs:7455-7459` redeclares `dt_formats` and `dt` with a fresh `chrono::Local::now()` after `dt_resolved` (`:7421-7424`) already exists, so the render runs against a different resolver than the activity assertions. Harmless for this fixture, which has no datetime parameter, but it is residue from the edit.

VERDICT: REVISE
