# Diff review

AUTHORS: agy
REVIEWER: claude
VERDICT: APPROVE
ROUNDS: 2
TREE_SHA256: 5d2d140146c07acae5d93f0ec90fb4fdbf8f2c1e68888ffde203c4c751bd126f
SPECS_SHA256: a1cc38c5db89bfec6c42f526a72531d7caea37a300346ef5e562554a8f29e6b5

I reviewed the diff against the change artifacts and AGENTS.md, and ran every gate myself.

## Verification performed

- `cargo fmt --check` exit 0, `cargo clippy --all-targets --all-features` exit 0, `cargo test` exit 0 (792 + 2 + 1 tests pass) [verified, `/tmp/gates-322.log`].
- `.workflow/review-gate-check.sh "$PWD" --plan-only` exit 0; `openspec validate --changes` passes; `.workflow/specs-digest.sh` recomputes `a1cc38c5…9e6b5`, matching `review.md`'s `SPECS_SHA256:`, so the approved plan's `specs/` are intact [verified].
- I diffed each delta requirement against its counterpart in `openspec/specs/` rather than trusting the "restated verbatim" claim. `param-resolution` changes exactly the two clauses the plan reviewer required and nothing else. `interpolation-tokens`'s MODIFIED correctly retires the paragraph "An `image` item's `name:` names a request `data` field directly…" and re-homes its legal-bare-name rule in the ADDED requirement, so nothing is dropped. All three MODIFIED requirement names match existing names, which is what `archive-merge-check.sh` resolves on [verified].
- I re-derived task 5.3's blast-radius claim independently: 23 templates under `catalog/` (5) and `tests/fixtures/templates/` (18), every bare token names a declared parameter, and none carries a `type: image` item [verified].
- Task 1.2's ordering holds: `validate_params` refuses a bare token in a `default:` at `src/templates.rs:955-967` before calling `validate_interpolated_string` at `:968`, so `default: "{message}"` keeps its own message (`src/templates.rs:7108-7112` asserts it) [verified].
- Task 6.4's rewording is now true: `mod http_tests` spans `src/lib.rs:98-8185` and `mod auth_http_tests` starts at `:8187`, so all five `src/lib.rs` hunks (`:2433`, `:3466`, `:10095`, `:11148`, `:11309`) are under `#[cfg(test)]`, as are all five `src/render/mod.rs` hunks (test module starts at `:2358`) [verified].
- No test or assertion is deleted anywhere in the diff (`git diff | grep -c "^-.*assert"` → 0), and no `#[allow(clippy::…)]` is introduced [verified].
- The refusal tests genuinely fail against pre-change code: each calls `unwrap_err()` on what previously returned `Ok`, and both HTTP tests assert counts and a status that pre-change code contradicts.
- I checked the new check against the untouched `interpolation-tokens` requirements for contradictions. `{datetime.long_date}` still reports "unknown source" (the parse-error arm returns before the new check), `{title:long_date}` over a declared `string` still reports the format message, and `{custom:Internal SKU}` still fails on the format name's character class. No scenario in "A colon attaches a format name" or "A bare name is a bare name" uses an undeclared bare name, so the new precedence (undeclared beats format-on-non-instant) contradicts nothing [verified].
- I traced the `panic!` at `src/templates.rs:373-375` independently: every name `derive_inputs_internal` collects is checked by `validate_references` (format dims at `:975-1002`, `when:` keys, extents, `font_weight`, `color`, `stroke.color`, `background`, plus the two new checks), `bare_token_names` discards parse failures that `validate_interpolated_string` would reject, so its output is a strict subset, and all four callers (`build_detail`, `placeholder_data`, `catalog-index` via `load`, the registry) read templates that `validate()` accepted. `instantiate_with_defaults` only replaces `Ref`s with literals, so it can shrink the collected set but never grow it. Unreachable [verified].

## Findings

**1. `docs/AUTHORING.md:397` overstates the rule it now teaches. (non-blocking)**

The replacement sentence reads "Every field referenced by the layout must be declared in `params:`". `{vars.<key>}` and `{sys.now}` are referenced by the layout and must *not* be declared — the change's own test `issue_322_namespaced_tokens_and_defaults_and_datetime` (`src/templates.rs:7085-7099`) asserts a template reading `{vars.site} {sys.now} {sys.now:iso_date}` with an empty `params:` loads. The same file gets it right one section down at `:609` ("like every bare token, it must be declared in `params:`"). The failure mode is over-declaration, not a quarantine, which is why this is not blocking; "every bare `{token}` the layout reads" would close it.

**2. `derive_inputs_internal` panics where design 6 named an `AppError::internal` class of failure. (non-blocking, already recorded)**

`src/templates.rs:373-375`. There is no `CatchPanicLayer` in `src/` or `Cargo.toml`, so a fire inside `build_detail` (`src/templates.rs:2261`) or `placeholder_data` (`src/api.rs:1253`) would drop the connection rather than return a 500. I could not construct a reachable case (see the trace above), and `design.md` decision 6 now carries the implementation note explaining the deviation and its cost — four public signatures — which is what the round-1 reviewer asked for. No action required.

Both round-1 blocking findings are resolved in this tree: `docs/AUTHORING.md:397` and `:609` no longer describe the removed fallback or `{datetime}` as an undeclared data field, and `tasks.md:75` plus `proposal.md`'s Code and Tests bullets now state the true file list ("production code in `src/templates.rs` only", test modules in `src/lib.rs` and `src/render/mod.rs`).

Nothing here must be fixed before landing.

