# Diff review

AUTHORS: opencode
REVIEWER: claude
VERDICT: APPROVE
ROUNDS: 2
TREE_SHA256: 6f952c2310940af545da99a1e751041b11c184c4a1b654c3c0c5df078adf48ca
SPECS_SHA256: 54bf42f2128b1e35015cbef978f41b31b17b24bb8e20c0d770433a14d9ace917

## Diff review — `issue-310-a-param-s-enum-key-is-parsed-and-ignored`

**Scope reviewed:** `git diff` (`src/convert.rs`, `src/raw.rs`, `src/templates.rs`) against `proposal.md`, `specs/datetime-params/spec.md`, `design.md`, `tasks.md`, and `AGENTS.md`. Round 1's three findings (`diff-review-1.md`) are all resolved: `src/convert.rs:778`'s stale datetime `enum` assertion is deleted, the registry test now asserts `contains("unknown field")`, and `proposal.md`'s Impact line records the `src/templates.rs:3865` fixture edit.

### Findings

**1. MINOR (non-blocking) — the assertion that the error "names the unknown key `enum`" is satisfied by the fixture's own filename.**

`src/templates.rs:5808-5812` asserts `broken[0].error.contains("enum")` under the message `"error must name the unknown key \`enum\`"`. But the file is written as `bad_enum.yaml` (`src/templates.rs:5791` region) and `TemplateRegistryError::Parse` embeds the path. The real string is [verified, printed from an out-of-tree build of this exact tree]:

```
failed to parse template bad_enum.yaml: yaml error at params.weight.enum: params.weight.enum: params.weight: unknown field `enum`, expected one of `type`, `default`, `min`, `max`, `multiline`, `values`, `format`, `time`, `description` at line 13 column 5
```

`contains("enum")` matches `bad_enum.yaml` before it ever reaches the key, so that line adds nothing over its two neighbours. Not blocking: the conjunction with `contains("params.weight")` and `contains("unknown field")` still pins the outcome, and the regression this test exists to catch (re-adding `choices`) makes the template load and fails `registry.len() == 1` first. Tightening it to `contains("unknown field \`enum\`")`, or renaming the fixture to `bad_param.yaml`, would make the assertion mean what its message claims.

### Verified correct

- **Both new tests are red before the change, each on its intended assertion** [verified empirically, not by inspection]: I spliced HEAD's `src/raw.rs` and HEAD's pre-`mod tests` half of `src/convert.rs` under the new tests in an out-of-tree copy. `enum_key_is_refused_as_unknown_field` panics at `src/convert.rs:841` (`RawParamSpec { … choices: Some(Some([String("a"), String("b")])) }`), and `enum_key_on_integer_param_is_quarantined_with_unknown_field_error` panics at `src/templates.rs:5798` with `left: 2, right: 1`. This satisfies task 1.3, whose box was previously checked without a run recorded.
- Tasks 2.1–2.3 are exact: the `choices` field is gone, the datetime `enum` guard is gone, the other four datetime guards (`min`, `max`, `multiline`, `values`, `src/convert.rs:518-541`) and the `format` guard (`:504-509`) keep their pointed messages, and only the final sentence of the `.flatten()` comment was cut.
- `choices` appears nowhere under `src/` or `ui/src/`; `src/raw.rs` still needs `serde_yaml_ng::Value` for `default` and `format`, so no dead import. `RawParamSpec` is the only param-shaped raw struct, and no `enum:` key survives in `catalog/`, `tests/fixtures/templates/` or any repo YAML — the only occurrence is the new fixture at `src/templates.rs:5791`.
- No other capability contracts `enum` as a parameter attribute (`openspec/specs/`, swept); `template-inputs/spec.md:40-50` maps `integer` to the `integer` control with no `enum` branch, and `docs/AUTHORING.md` documents no param-attribute table, so the "no UI file, no doc file" claim holds. `docs/SPEC.md` is correctly untouched.
- The delta's `MODIFIED` heading matches the canonical requirement name at `openspec/specs/datetime-params/spec.md:236`, and the supersession sentence round 1's plan review asked for is present verbatim.
- `specs-digest.sh` recomputes `54bf42f2…`, matching `review.md`'s `SPECS_SHA256`; `review-gate-check.sh "$PWD" --plan-only` exits 0.
- Gates on this tree: `cargo fmt --check` clean, `cargo clippy --all-targets --all-features` clean, `cargo test` 759 passed / 0 failed plus the integration binaries green.
- Apply correctly stopped at implementation: nothing committed, archived, or synced into `openspec/specs/`.

