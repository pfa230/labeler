TREE_SHA256: 9424142d13ecb8a04adbbd32b1f599442ff8ef433ccda4769f512673fe2bd70a

I have everything I need. Findings below, each verified against the code.

## Findings

### 1. BLOCKING — a stale assertion still lists `enum` among the datetime-forbidden attributes

`src/convert.rs:770-781`, `datetime_param_rejects_forbidden_attributes`:

```rust
assert!(try_build_param("type: datetime\nvalues: [a, b]\n").is_err());
assert!(try_build_param("type: datetime\nenum: [a, b]\n").is_err());   // :778
```

Every other line in that test exercises the datetime guards in `TryFrom<RawParamSpec>` (`src/convert.rs:518-541`). Line 778 no longer does: the guard it was written for was deleted (`git diff src/convert.rs`, task 2.2), so it now passes because serde refuses the key one layer earlier. Three things make it a defect rather than harmless redundancy:

- It contradicts the requirement this change ships. The delta states `format`, `min`, `max`, `multiline` and `values` are the datetime-rejected set and that "`enum` is no longer among them either, for a different reason: it is not an attribute of any parameter type" (`openspec/changes/.../specs/datetime-params/spec.md:31-36`). The test that enumerates that set still enumerates `enum` in it.
- It reinstates, in the test suite, exactly the false belief `design.md` says the change exists to kill: that `enum` is a datetime-specific rejection, and therefore valid somewhere else.
- The case is already covered, correctly, at `src/convert.rs:831` (`"type: datetime\nenum: [\"2026-01-01\"]\n"` in `enum_key_is_refused_as_unknown_field`), which additionally asserts the error is the unknown-field one.

Task 3.1 required confirming "no comment or **test** there describes `enum:` as a parameter attribute". That confirmation is false and its box is checked (`tasks.md`, 3.1). Fix is deleting line 778.

### 2. MINOR — two checked gate tasks confirm something the diff itself disproves

Task 3.2 is checked over "Confirm no file under `src/`, `ui/src/`, `catalog/` or `tests/fixtures/templates/` needs a further edit". A file under `src/` did need one: the diff removes `enum: [400, 700]` from the inline fixture in `raw_template_deserializes_params_dynamic_values_and_when` (`src/templates.rs:3865`, previously line 3868). The edit itself is correct and harmless (that test asserts param count, dynamic width and `when:` predicates, none of which the line fed), but `proposal.md`'s Impact line for `src/templates.rs` says "(one new registry-level quarantine test)" and omits it, so the record of what the change touched is incomplete. Combined with finding 1, the section-3 boxes were checked without the sweep they claim.

### 3. MINOR — the registry test does not pin the message class the spec requires

`src/templates.rs:5806-5815` asserts `broken[0].error` contains `params.weight` and `enum`, but not that it is the unknown-key error. The requirement is specific: the message "SHALL be the service's generic unknown-key message and SHALL NOT be type-specific" (delta spec `:38-43`). The parse-level test asserts `msg.contains("unknown field")` (`src/convert.rs:840`); the registry test, which is the only one covering the quarantine path the scenarios are written against, does not. `contains("enum")` alone would also be satisfied by a type-specific message. One extra `contains("unknown field")` closes it.

## What I verified as correct

- `choices` is gone from `src/` and never existed in `ui/src/`, `catalog/`, `tests/fixtures/templates/` or `openspec/specs/` (`rg` across all five).
- `RawParamSpec` is reachable only through `RawTemplate.params` (`src/raw.rs:188`), which is deserialized only on the file-load path, so quarantine is the sole outcome — no request path turns this into a 4xx/5xx.
- `options:` desugars by building `ParamSpec` directly (`src/convert.rs:621-630`) and never touched `choices`; untouched, as claimed.
- The UI claim holds: `ParamInput.tsx:109-118` renders the stepper for `integer` with no `enum` branch, matching the post-change table.
- The `MODIFIED` delta is scoped: diffing it against the canonical requirement (`openspec/specs/datetime-params/spec.md:236-364`) shows only the intended edits — the forbidden-attribute sentence, the new `enum:` paragraph, the `integer` row, and four new scenarios. The supersession provenance the plan reviewer asked for is present. `docs/SPEC.md:346` sits inside §3.0, which this requirement supersedes, so the frozen table is correctly left alone.
- Both new tests are red before the change by construction: with `choices: Option<Option<Vec<Value>>>` present, all four YAML fragments deserialize, so `expect_err` panics, and the `type: integer` template loads, so `registry.len()` is 2. [verified by reading the deleted field's type; not re-run against HEAD]
- Plan gate is intact: `specs-digest.sh` recomputes `54bf42f2…`, matching `review.md`, and `review-gate-check.sh "$PWD" --plan-only` exits 0.
- Gates: `cargo fmt --check` 0, `cargo clippy --all-targets --all-features` 0, `cargo test` 0 (759 passed, 0 failed).

Finding 1 must be fixed before this lands.

VERDICT: REVISE
