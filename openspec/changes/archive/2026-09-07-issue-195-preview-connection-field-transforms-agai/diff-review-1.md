TREE_SHA256: 174c438381213e91bbb3e88a5fd7f1cddece2c6196737768abfd15ff7e3b81da
SPECS_SHA256: 7c7ece3636e057154f9bc1faa97fcb79d9e855f12084ec6351c78ebc8453a24a

# Diff review: issue-195-preview-connection-field-transforms-agai

Gates run locally, all green [verified]: `cargo fmt --check` clean, `cargo clippy --all-targets --all-features` no warnings, `cargo test` 892 passed, `ui/` lint + `tsc --noEmit` + 499 tests passed.

I verified the substantive Rust contract claims and they hold: `transform_source` is computed from the same predicate `validate_transforms` applies (`src/connector/mod.rs:328` vs `src/connector/mod.rs:196-205`), `CompiledTransforms::compile` preserves index alignment so `evaluate_cells(..).find(|e| e.index == req.rule)` selects the right rule, the stored-transforms-cannot-shadow-a-candidate-source argument in `design.md` is sound (a derived name matches `^[a-zA-Z0-9_-]+$`, so it can never be a `custom:` key, and validation already forbids it colliding with a declared column of that resource), `apply_to_cells` is a behaviour-preserving caller of `evaluate_cells` and the seven `transform_pass_*` tests are unchanged, and the UI's revision + per-rule sequence gate correctly implements the review-round-4 correction (the revert-during-flight case is discarded by the revision, the out-of-order case by the sequence).

## Blocking

**1. The suspended editor shows the edited rules, not the stored ones, while the save silently drops the edits.** `ui/src/pages/connect/ConnectionsSection.tsx:308` renders `transforms.map(...)`, which is live component state, not `initial.transforms`. The spec is explicit: "It SHALL fall back to showing the **stored** rules read-only, with the reason, and the form SHALL then omit `transforms` from the save so the stored rules are kept" (`specs/connector-field-transforms/spec.md`, "The rule editor is schema-driven and previews each rule").

Failure scenario: operator opens the edit form, changes rule 0's pattern from `A` to `B`, then edits **base url**. `isDirtyDetails` flips (`ConnectionsSection.tsx:130`), the editor suspends and the read-only panel renders `pattern: B`. The operator saves. `submit` omits `transforms` (`ConnectionsSection.tsx:229`), so the stored rule keeps pattern `A`. The screen shows `B`, the server holds `A`, and nothing says so. That is the same class of misleading feedback the change exists to remove, and it is what the "SHALL fall back to showing the stored rules" wording was written to prevent. Test 7.7 does not catch it because it never edits a rule before changing the base url.

## Non-blocking

**2. The "Previewing..." label sticks forever after a discarded response.** `setPreviewLoadingRule(idx)` runs unconditionally at `ConnectionsSection.tsx:181`, but both clears (`:192`, `:196`) sit inside the `revision && seq` freshness guard. Any preview whose response is discarded, which is exactly the path spec'd in test 7.8, leaves `previewLoadingRule` pinned to that rule. The button is not disabled, so it is cosmetic rather than a lockout, but the editor claims a request is in flight when none is.

**3. The `1..=200` page-size clamp is spelled twice.** `src/api.rs:2305-2310` re-derives the clamp that `src/connector/homebox.rs:327` owns (`req.page_size.unwrap_or(PAGE_DEFAULT).clamp(1, 200)`). `design.md` says preview "inherits its clamp", and the handler does pass the value through, but it also needs the effective size for the row truncation at `src/api.rs:2325`, so it re-derives the bound rather than asking the connector for it. If a connector's clamp ever changes, `row_count` and the truncation length drift from what was actually requested with nothing to catch it. This is the second-spelling pattern the design itself rejects for the UI-side source predicate.

**4. A connector resource id is still hardcoded in the UI.** `ConnectionsSection.tsx:158` falls back to `resource: firstRes?.id ?? "entities"`. `CONNECTOR_RESOURCES` was deleted precisely because the UI must not name the connector's resources, and the spec says "No resource list SHALL be hardcoded in the UI." The branch is unreachable today (the "+ Add rule" button only renders when `schema` is truthy), so this is a latent smell rather than a live defect, but the string should come from the schema or the add should refuse.

**5. The three new schema fields are optional in the UI types, which reintroduces a silent fallback.** `ui/src/api/connectors.ts:35` declares `transform_source?: boolean` (beside the required `multi_valued: boolean` in the same interface), and `:43-44` do the same for `dynamic_source_prefix` and `fields_incomplete`. The spec says all three are "always present". If `transform_source` were ever absent, `columns.filter((c) => c.transform_source)` at `ConnectionsSection.tsx:323` yields an empty picker with no message, which is the silently-drops-every-column failure this change was written to eliminate. Making them required in the type turns that into a compile-time or parse-time signal.

**6. Preview evaluates every rule on the resource to report one.** `src/connector/mod.rs:135` returns an outcome per matching rule, and `src/api.rs:2334-2335` calls it per row then discards all but `req.rule`. With the maximum 32 candidate rules on one resource over a 200-row page that is 6400 regex executions where 200 were asked for. Correct, and the regex crate is linear-time so there is no ReDoS exposure, but the shared entry point could take the rule index.

## Not defects (checked and cleared)

The `?? "entities"` aside, `handlePreview`'s per-index `ruleSeqRef` keying survives a mid-flight rule removal, because the revision guard covers the index shift the sequence guard cannot. `truncate_to_512_bytes` (`src/api.rs:2262`) terminates and cuts on a character boundary. An over-8192-byte source reports `matched: false`, which agrees with what browse would derive. `value_truncated` is `src_cut || caps_cut` when matched and `src_cut` otherwise, matching the spec. The malformed-body enumeration was updated with the new endpoint and the count assertion raised to 20 (`src/lib.rs:9395,9419`). No new `details.reason` slug was minted, so `src/reason.rs` and the completeness test stay untouched as the proposal states.

VERDICT: REVISE
