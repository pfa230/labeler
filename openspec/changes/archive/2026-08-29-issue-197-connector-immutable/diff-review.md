# Diff review

AUTHOR: claude
REVIEWER: agy
VERDICT: APPROVE
ROUNDS: 1

I have launched `cargo test` in the background and will wait for it to complete.
## Adversarial Review of Implementation Diff for `issue-197-connector-immutable`

### 1. Scope & Verification Summary

The implementation diff for `issue-197-connector-immutable` was evaluated against:
- [proposal.md](file:///home/pfa/projects/labeler/.worktrees/issue-197/openspec/changes/issue-197-connector-immutable/proposal.md)
- [design.md](file:///home/pfa/projects/labeler/.worktrees/issue-197/openspec/changes/issue-197-connector-immutable/design.md)
- [specs/connections/spec.md](file:///home/pfa/projects/labeler/.worktrees/issue-197/openspec/changes/issue-197-connector-immutable/specs/connections/spec.md)
- [tasks.md](file:///home/pfa/projects/labeler/.worktrees/issue-197/openspec/changes/issue-197-connector-immutable/tasks.md)
- [AGENTS.md](file:///home/pfa/projects/labeler/AGENTS.md)
- Rebase delta (renumbering ADR-0070 to ADR-0087, aligning test expectations to [ADR-0075](file:///home/pfa/projects/labeler/.worktrees/issue-197/docs/adr/0075-request-rejections-use-the-error-envelope.md) / #225).

All validation gates were executed:
- `cargo fmt --check`: Clean pass.
- `cargo clippy --all-targets --all-features`: Clean pass (0 warnings, no added `allow` attributes).
- `cargo test`: 673 tests passed (0 failures, 2 intentional ignores).
- `.workflow/review-gate-check.sh`: Clean pass (SHA256 matches recorded `SPECS_SHA256`).
- `.workflow/gate-tests.sh` and `.workflow/apply-tests.sh`: All 32 + 24 tests passed.
- `openspec validate issue-197-connector-immutable`: Valid.

---

### 2. Implementation Audit & Evidence

#### A. Handler Implementation: [src/api.rs:1984-2060](file:///home/pfa/projects/labeler/.worktrees/issue-197/src/api.rs#L1984-L2060)
- **Check Precedence & Non-Staling Read**: In [`update_connection_h`](file:///home/pfa/projects/labeler/.worktrees/issue-197/src/api.rs#L1984), the connection is looked up via `state.store().get_connection(&id).await?` ([src/api.rs:1989-1993](file:///home/pfa/projects/labeler/.worktrees/issue-197/src/api.rs#L1989-L1993)). A missing ID returns `AppError::not_found` (`404`), taking strict precedence over connector immutability checking.
- **Exact String Comparison**: At [src/api.rs:1994-2002](file:///home/pfa/projects/labeler/.worktrees/issue-197/src/api.rs#L1994-L2002), `body.connector != existing.connector` triggers an immediate `AppError::invalid_request(Reason::ConnectorImmutable, format!("connector cannot be changed (existing '{}', requested '{}')", existing.connector, body.connector))`.
  - The comparison is byte-exact (no trimming, no case folding).
  - The connector registry is intentionally not consulted, ensuring unregistered and registered mismatches fail identically with `connector_immutable`.
- **Precedence Over Payload Validation**: The connector check occurs before `validate_and_normalize_url` ([src/api.rs:2006](file:///home/pfa/projects/labeler/.worktrees/issue-197/src/api.rs#L2006)) and `connector.validate_transforms` ([src/api.rs:2026](file:///home/pfa/projects/labeler/.worktrees/issue-197/src/api.rs#L2026)), ensuring the client is told the connection is immutable before field-level malformations are surfaced.
- **Locking & Persistence**: The write lock `state.write_lock.lock().await` ([src/api.rs:2039](file:///home/pfa/projects/labeler/.worktrees/issue-197/src/api.rs#L2039)) is deferred until all validations succeed.

#### B. Error Contract: [src/reason.rs:82](file:///home/pfa/projects/labeler/.worktrees/issue-197/src/reason.rs#L82)
- Added `ConnectorImmutable => "connector_immutable"` under the `InvalidRequest` grouping in `reasons!`.
- Satisfies `spec_documents_every_reason_and_invents_none` in [src/errors.rs:615-680](file:///home/pfa/projects/labeler/.worktrees/issue-197/src/errors.rs#L615-L680) via active delta scanning (#217).

#### C. Decision Record & Index: [docs/adr/0087-connection-connector-is-immutable.md](file:///home/pfa/projects/labeler/.worktrees/issue-197/docs/adr/0087-connection-connector-is-immutable.md), [docs/adr/README.md:96](file:///home/pfa/projects/labeler/.worktrees/issue-197/docs/adr/README.md#L96)
- ADR-0087 follows standard Nygard structure (`Status: Accepted`, Context, Decision, Consequences).
- Correctly indexed in `docs/adr/README.md` at line 96; `tests/adr_index.rs` asserts index completeness and passes.

#### D. Test Suite & Scenario Verification: [src/lib.rs](file:///home/pfa/projects/labeler/.worktrees/issue-197/src/lib.rs)
Every normative scenario from [specs/connections/spec.md](file:///home/pfa/projects/labeler/.worktrees/issue-197/openspec/changes/issue-197-connector-immutable/specs/connections/spec.md) has dedicated endpoint-level test coverage:
1. *Setting a new public_url (normalization)*: [`update_connection_sets_and_clears_public_url`](file:///home/pfa/projects/labeler/.worktrees/issue-197/src/lib.rs#L568-L586) sends a trailing slash (`https://hb2.example.com/`) and asserts trimmed storage (`https://hb2.example.com`).
2. *Unknown ID precedence*: [`connection_endpoints_report_404_for_an_unknown_id`](file:///home/pfa/projects/labeler/.worktrees/issue-197/src/lib.rs#L691-L722) verifies `PUT /api/connections/nope` with a mismatched connector returns `404`.
3. *Mismatch rejection*: [`update_connection_rejects_mismatched_connector`](file:///home/pfa/projects/labeler/.worktrees/issue-197/src/lib.rs#L727-L763) asserts `400 Bad Request` with `details.reason = "connector_immutable"`.
4. *Matching connector success*: [`update_connection_with_matching_connector_succeeds`](file:///home/pfa/projects/labeler/.worktrees/issue-197/src/lib.rs#L767-L804) asserts `200 OK` and fields updated.
5. *State unchanged on rejected PUT*: [`update_connection_rejected_mismatched_connector_leaves_state_unchanged`](file:///home/pfa/projects/labeler/.worktrees/issue-197/src/lib.rs#L808-L861) verifies database state remains intact after rejection.
6. *Precedence over other invalid fields*: [`update_connection_connector_mismatch_outranks_other_invalid_fields`](file:///home/pfa/projects/labeler/.worktrees/issue-197/src/lib.rs#L868-L920) asserts `connector_immutable` outranks invalid `base_url`, `public_url`, and `transforms`.
7. *Deserialization error precedence*: [`update_connection_undeserializable_body_is_rejected_before_the_connector_check`](file:///home/pfa/projects/labeler/.worktrees/issue-197/src/lib.rs#L929-L983) tests syntax error, type mismatch, and missing required keys.
8. *Case sensitivity*: [`update_connection_rejects_a_connector_differing_only_in_case`](file:///home/pfa/projects/labeler/.worktrees/issue-197/src/lib.rs#L988-L1023) tests `"Homebox"` vs `"homebox"` returning `400 connector_immutable`.

---

### 3. Rebase Delta Audit

1. **Renumbering ADR-0070 -> ADR-0087**:
   - `main` previously merged records 0070 through 0086.
   - 0087 is correctly assigned, authored, and added to the index without numbering collisions or stale references.
2. **ADR-0075 / #225 Extractor Unification Alignment**:
   - Under ADR-0075, application extractors ([src/extract.rs](file:///home/pfa/projects/labeler/src/extract.rs)) map all JSON deserialization failures to `400 InvalidRequest` with `details.reason = "json_malformed"`.
   - In [`update_connection_undeserializable_body_is_rejected_before_the_connector_check`](file:///home/pfa/projects/labeler/.worktrees/issue-197/src/lib.rs#L929-L983), the assertions for `"connector of the wrong type"` and `"required key missing"` were updated from `422 UNPROCESSABLE_ENTITY` to `400 BAD_REQUEST`, correctly matching current `main`.
   - The doc comment ([src/lib.rs:922-927](file:///home/pfa/projects/labeler/.worktrees/issue-197/src/lib.rs#L922-L927)) accurately describes ADR-0075 / #225 extractor mapping.

---

### 4. Observations & Notes

- **Working Tree / Index State**: On the current worktree, rebase updates in [src/lib.rs](file:///home/pfa/projects/labeler/.worktrees/issue-197/src/lib.rs) and [docs/adr/README.md](file:///home/pfa/projects/labeler/.worktrees/issue-197/docs/adr/README.md) remain in the working tree over staged commits, and [docs/adr/0087-connection-connector-is-immutable.md](file:///home/pfa/projects/labeler/.worktrees/issue-197/docs/adr/0087-connection-connector-is-immutable.md) and `openspec/changes/issue-197-connector-immutable/` are untracked. As per [.githooks/pre-commit:31-41](file:///home/pfa/projects/labeler/.worktrees/issue-197/.githooks/pre-commit#L31-L41), running `git add -A` prior to commit ensures the gate validates an index identical to disk.
- **Review Artifact Digest**: [review.md](file:///home/pfa/projects/labeler/.worktrees/issue-197/openspec/changes/issue-197-connector-immutable/review.md) records the approved plan state; its `SPECS_SHA256` digest matches the spec delta verbatim and passes review gate verification.

---


