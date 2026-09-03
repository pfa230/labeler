TREE_SHA256: 3b962308b5a90d916443b4df696d048134379dde618ce2345d0aa38b144c8b51

# Diff review — issue-335-a-thumbnail-ignores-an-enum-s-declared-d

Reviewed the diff (`src/api.rs`, `src/lib.rs`, `src/render/mod.rs`, `src/templates.rs`) against `proposal.md`, `design.md`, both spec deltas, `tasks.md`, `AGENTS.md`, and the three prior diff-review rounds. No files edited.

## What I verified independently

Gates green here: `cargo fmt --check` exit 0, `cargo clippy --all-targets --all-features` clean, `openspec validate` reports the change valid, `cargo test` 815 passed / 0 failed (see finding 4 for one flake). [verified]

The core mechanism is right. The new `Select` arm (`src/templates.rs:183-194`) sits under the `interpolated && required` guard at `src/templates.rs:163`. `resolve_declared_defaults` (`src/render/mod.rs:507-536`) inserts an entry for exactly the params that declare a default, `Resolved` or `Error`, and `derive_inputs_internal` maps those to `(default, default_error, required)` at `src/templates.rs:416-421`. So under that guard `default_error.is_none()` is precisely "declares no `default:`", which is D1. [verified] Both `panic!`s are unreachable: `InputControl::Select` is produced only for `ParamType::Enum` (`src/templates.rs:391`) and never for an `image_bound` name (`:387`), `values` is `Some` for exactly that type (`:423-427`), and an empty enum is refused at load (`:1280-1283`). [verified]

The `option` deletion is complete and production-safe: the surviving callers pass `None` (`src/api.rs:1254,2677,2681`), no request model carries an option field, and `normalize_option` / `RenderContext.selected_option` are untouched per D2. [verified] An enum default that *resolves* to a non-member also lands on `param_default_unresolvable` via `coerce_param_value` (`src/render/mod.rs:459-466`), so the delta's "cannot be resolved" wording covers that edge. [verified] The delta bodies differ from the published requirements only in the enum rule and its scenarios; the surrounding prose is verbatim. [verified] Round 3's three findings are all closed in this tree. [verified]

## Findings

**1. BLOCKING — `thumbnail_enum_colour_ref_without_default_fails` cannot fail, so the BREAKING colour/dimension-`{ref}` class has no test that detects it** (`src/templates.rs:6719-6774`).

`color: DynamicValue::Ref(r)` is recorded with `interpolated: false` (`src/templates.rs:296`; same for `background` `:368`, `stroke.color` `:362`, dynamic `size` `:287`). The new `Select` arm is inside the `interpolated && required` guard (`:163`), so it is never reached for `palette` — `placeholder_data` returns the same map before and after this change. The test then calls `render_thumbnail_png(&template, &ph, None, ...)`, and `normalize_option(template, None)` returns `Ok(None)` (`src/render/mod.rs:1146-1154`), so the deleted merge was already a no-op on that path. Pre-change and post-change both produce `400 InvalidRequest` / `color_param_invalid`, and both pass every assertion in the test.

`diff-review-3.md` closed round 2's blocking finding on the claim that this test "fails pre-change because the deleted option map supplied `palette`". That is true only through the handler (`src/api.rs:1251-1254`), which this test does not call. The 200→400 change is real at `GET /api/templates/{id}/thumbnail` — round 2 measured it against a base build — and nothing in the suite would catch its reintroduction or its removal.

The fix is the technique the change already uses three times: an HTTP twin via `build_app_in`, writing a template with `color: "{palette}"` over an undefaulted `enum`, asserting `400` and `color_param_invalid`. That assertion does fail pre-change, where `default_option_selection` supplied `palette`.

**2. Minor — six of the seven new unit tests are invariant across the change, for the same structural reason.** By the argument above, `render_thumbnail_png(t, data, None, …)` is byte-identical pre- and post-change, so a `src/templates.rs` test can only detect the change through `placeholder_data`, i.e. only for an entry that is `interpolated && required && default_error.is_none()`. That holds for exactly one: `thumbnail_printed_enum_without_default_shows_first_value` (`:6497`). `thumbnail_printed_enum_with_declared_default_shows_default` (`:6413`, `required: false`), `thumbnail_enum_only_gate_without_default_is_absent` (`:6531`) and `..._with_default_is_present` (`:6579`) (both `interpolated: false`), `thumbnail_broken_enum_default_fails` (`:6628`, `default_error: Some` → `continue` both ways) and `thumbnail_broken_string_default_is_masked` (`:6674`, `Text` control) all pass against the unfixed code. The contract is nonetheless pinned, by the three HTTP tests (`src/lib.rs:1333,1410,1458`), each of which does fail pre-change. These six are regression guards, which is a legitimate role — the problem is only that three names (`shows_default`, `is_absent`, `broken_enum_default_fails`) read as pins on the fix.

**3. Minor — `proposal.md` and `design.md` both claim `every_template_renders` covers the `container_circle_gated` change; it does not.** That test (`src/render/mod.rs:5452-5510`) asserts the id set and PNG magic only, and passes identically with the circle drawn or absent. The fixture's thumbnail change is real (`enabled` declares `default: "no"`, gate is `when: { enabled: "yes" }`) and is unasserted at fixture level. Same class as round 3's finding 2, one notch milder: the synthetic tests cover the rule, so this is an artifact-accuracy point, not a contract gap.

**4. Observation, not this change's — one flaky test.** `http_tests::auth_login_malformed_password_not_in_logs` failed on my first full `cargo test`, then passed six consecutive runs (four `--lib`, two full) and in isolation. It rests on a thread-local log buffer plus a `Once`-guarded global subscriber (`src/lib.rs:7652-7689`); nothing in this diff reaches auth or login. Pre-existing, but it can trip the gate stage.

**5. Minor — `dump_all_template_renders` keys its restored branch on a parameter name** (`src/render/mod.rs:5983-5988`): any template declaring a param called `outline` gets the string `"yes"`, whatever its type. It is an `#[ignore]`d visual harness and the comment explains the intent, so the cost is bounded.

VERDICT: REVISE
