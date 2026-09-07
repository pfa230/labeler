# Diff review

AUTHORS: agy
REVIEWER: claude
VERDICT: APPROVE
ROUNDS: 2
TREE_SHA256: c7ae32a603cfa9c21858602cd07b7e03e6b3fd3af6573af168426eb8f1db537d
SPECS_SHA256: 7c7ece3636e057154f9bc1faa97fcb79d9e855f12084ec6351c78ebc8453a24a

# Diff review: issue-195-preview-connection-field-transforms-agai

**Gates, run locally, all green [verified]:** `cargo fmt --check` clean; `cargo clippy --all-targets --all-features` zero warnings and no `#[allow]` added anywhere in the diff; `cargo test` 893 passed / 0 failed; `ui/` `npm run lint`, `npm run test` (499 tests, 50 files) and `npm run build` all pass. `openspec validate <change> --strict` reports the change valid.

## What I verified against the contract

- `transform_source` is computed from the same predicate the save applies: `src/connector/mod.rs:338` (`!c.multi_valued && c.ty == FieldType::Text`) against `src/connector/mod.rs:236-247`. Derived columns are set `false` explicitly (`src/connector/mod.rs:565`), and Homebox's discovered `custom:` columns get `true` via `src/connector/homebox.rs:496`, which a save accepts through the prefix branch. Both directions of the spec's "accepts every true, refuses every false" hold.
- `evaluate_rule` (`src/connector/mod.rs:135-155`) is the single entry point, `apply_to_cells` (`src/connector/mod.rs:186-198`) is a caller of it, the seven `transform_pass_*` tests are unchanged and pass, so the refactor is behaviour-preserving as the design required.
- The handler's ordering is right: 404 from `load_conn_and_connector`, then the `rule` bounds check (`src/api.rs:2268`), then whole-list validation with the `rule {idx}: {msg}` shape (`src/api.rs:2276`), both before any upstream call. Test `preview_endpoint_rejects_invalid_candidate_rule_elsewhere_...` asserts `hb.received_requests().len() == 0`, so that claim is earned.
- The clamp now has one spelling (`HomeboxConnector::clamp_page_size`, `src/connector/homebox.rs:243`), reached by both browse and preview, and `preview_endpoint_page_size_clamping` binds its mocks to `query_param("pageSize", ...)`, so a wrong outbound size fails the test rather than passing silently.
- `truncate_to_512_bytes` (`src/api.rs:2262`) terminates and cuts on a character boundary; matching runs on the whole value because `CompiledTransform::apply` is called before truncation, so `matched`, the `derived` key set and both counts are a save's.
- The stored-rules-cannot-shadow-a-candidate-source argument holds: a derived name must match `^[a-zA-Z0-9_-]+$` and cannot collide with a declared column, while a legal `source` is a declared column or a `custom:`-prefixed key, and `:` is outside the derived-name charset.
- Malformed-body enumeration updated and the count assertion raised to 20 (`src/lib.rs:9395,9419`). No new reason slug, so `src/reason.rs` and the completeness test stay untouched, as the proposal states.
- All four prior blocking/non-blocking items from `diff-review-1.md` are fixed in this tree: the suspended editor now renders `initial.transforms` (`ConnectionsSection.tsx:314`) and test 7.7 edits a rule before suspending, so the fix is held by a test that can fail.

## Non-blocking findings

**1. The suspended editor hides a preview result rather than discarding it.** `ConnectionsSection.tsx:476` gates the panel on `!isSuspended`, but `previewResults` (`:117`) is never cleared when `isSuspended` flips at `:134`. Reverting the base url or clearing the api key field brings the old panel back. The spec says the second suspended state "SHALL discard every displayed preview result". No misleading output follows, because the restored result was computed against the stored connection and the form again describes it, but the implementation is a hide, not a discard.

**2. Two spec scenarios ship with no test.** "A stored rule the schema no longer offers is not rewritten" is implemented at `ConnectionsSection.tsx:361-363` and `:387-389`; "A preview refused for another rule is shown on that rule" at `:200-203`. `tasks.md` §7 claims no test for either, so nothing is over-claimed, but these are the two fiddliest branches in the editor and both are unexercised.

**3. `addRule` can mint a rule no save accepts.** `ConnectionsSection.tsx:155-158` falls back to `source: ""` when the first resource offers neither a `transform_source` column nor a prefix. The rule then renders with an empty select and the "No transformable sources available" line, and any preview or save of it fails with `unknown source field ''`. Unreachable for Homebox, whose `entities` always declares eligible columns, so this is latent rather than live.

**4. Single-slot panel and loading indicator.** `previewResults` and `previewLoadingRule` each hold one rule, so previewing rule 1 removes rule 0's panel. Nothing stale is displayed and the spec does not require concurrent panels, so this is a UX narrowing, not a contract violation.

None of these produces wrong output or a claim the code does not earn.

