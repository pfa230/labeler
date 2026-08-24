## Review Metadata

- **Round**: 1
- **Prior round**: none

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only
- **Artifacts reviewed**: `proposal.md`, both delta specs, `design.md`, `CLAUDE.md`, `openspec/specs/connections/spec.md`, `openspec/specs/connector-browser/spec.md`, `docs/SPEC.md` §12, ADR-0024, `docs/adr/README.md`, `src/settings.rs`, `src/api.rs`, `src/store.rs`, `src/reason.rs`, `src/openapi.rs`, `ui/src/pages/Connect.tsx`, `ui/src/pages/connect/ConnectorBrowser.tsx`, `ui/src/pages/settings/ConnectionsSection.tsx`, `ui/src/api/connectors.ts`, and `ui/src/api/queries.ts`
- **Issue**: #203

## Findings

### Critical (blocking)

1. **The proposed delete cascade is not atomic despite the specification promising that it is part of the same delete.** The modified requirement says the setting clear is part of the delete and that no stored default can outlive its connection (`specs/connections/spec.md:5-10`). The design instead calls `delete_connection` and then `delete_setting_if_value` as two separately committed statements (`design.md:86-94`). Existing store methods each acquire the connection and execute independently (`src/store.rs:317-320`, `src/store.rs:757-760`), while `GET /api/settings` does not take the API write lock (`src/api.rs:1131-1176`). A read or second-statement failure can therefore observe or leave a deleted connection with a dangling setting. The cascade needs one transactional store operation covering both statements, with rollback if either fails.

2. **The derived-selection design can silently switch connections while retaining rows selected from the previous connection.** `null` means “untouched,” so the effective id is recomputed from live query results on every render (`design.md:96-110`). In the current component, selected rows live above the connection-keyed browser and are cleared only by the picker’s `onChange` (`ui/src/pages/Connect.tsx:30-36`, `ui/src/pages/Connect.tsx:44-46`, `ui/src/pages/Connect.tsx:69-83`). A settings or connections refetch—particularly a window-focus refresh after another operator changes the instance-wide setting—can change the effective connection without firing that handler. Old row identities can then be presented or materialized against a different upstream connection. The artifacts must define whether the opening choice is latched or may track later query changes and, in the latter case, require all connection-scoped state to reset atomically.

### Moderate

1. **The claimed deterministic fallback is not deterministic for duplicate names.** The spec says ascending name makes the same installation resolve the same connection on every visit (`specs/default-connection/spec.md:63-71`), but connection names are not unique (`src/store.rs:143-151`, `src/api.rs:1463-1477`) and the query orders only by `name` (`src/store.rs:690-696`). The same ambiguity also makes two identically named choices indistinguishable in the Settings control (`specs/default-connection/spec.md:123-129`). Specify a stable tie-breaker and a distinguishable label for duplicate names.

2. **The Settings control has no contract for a dangling stored id.** The resolution requirement explicitly handles a stored id naming no connection (`specs/default-connection/spec.md:106-109`), and rollback can create that state (`design.md:147-152`). Yet the control must both offer only existing connections and show the currently stored choice (`specs/default-connection/spec.md:125-128`), which is impossible in this case. Specify an explicit missing-default state that remains clearable. The disabled-entry marker described by the design (`design.md:112-118`) should also be normative and covered by a scenario.

3. **Deleting the default connection would leave the Settings query cache stale.** The current delete mutation invalidates only `["connections"]` (`ui/src/api/connectors.ts:74-79`), while the new control reads `["settings"]` (`ui/src/api/queries.ts:133-158`). The design does not address this, so the server could clear the setting while the mounted control continues to represent the deleted id. Add settings-cache invalidation to the design and verification plan.

4. **Failure to load settings has no specified Connect-page behavior.** The design says nothing resolves while either query is loading (`design.md:107-110`) but does not cover an error. `GET /api/settings` can fail because any corrupt setting causes the whole endpoint to return an internal error (`src/api.rs:1148-1175`; ADR-0024:27-28). The implementation could therefore remain empty indefinitely, silently use the fallback, or auto-fetch before knowing the default. Specify a visible, testable failure behavior that still permits deliberate manual selection if intended.

5. **The PUT request-body contract is ambiguous and conflicts literally with the existing endpoint shape.** The delta says the endpoint accepts “a JSON string” (`specs/default-connection/spec.md:16-21`), while the actual API requires an object shaped as `{ "value": ... }` (`src/api.rs:1119-1123`, `src/api.rs:1191-1195`). State the complete request body and express invalid cases as invalid values of that field.

6. **The frozen-spec precedence statement incorrectly declares already-superseded browse behavior authoritative.** The new requirement says everything else in the frozen “Using a connection” paragraph remains authoritative (`specs/default-connection/spec.md:84-87`), but the browse table is already superseded by `openspec/specs/connector-browser/spec.md:198-203`, and connection-form behavior by `openspec/specs/connections/spec.md:195-197`. Narrow the statement to the selection-flow text and acknowledge existing OpenSpec supersessions.

7. **Stored-value corruption handling is omitted.** The design validates shape only during PUT and proposes an `Option<String>` resolver (`proposal.md:65-66`, `design.md:57-64`). Existing settings resolvers reject corrupt stored overrides (`src/settings.rs:113-170`), and ADR-0024 requires corruption to surface as an error rather than silently fall back. Specify that empty or whitespace-only stored text is corrupt while a well-formed but dangling id remains the deliberately supported fallback case.

8. **The deferred template-picker work is not linked to its promised issue.** The proposal says it gets a separate issue (`proposal.md:34-37`), while project rules require deferred work to be filed rather than left as an informal future item (`CLAUDE.md:34-38`). Add the follow-up issue link without placing that issue in this change’s scope.

### Suggestions

1. Correct the proposal’s claim that `src/openapi.rs` contains a documented known-key list (`proposal.md:72`). The current OpenAPI registration exposes generic `SettingValue` and `ResolvedSetting` schemas but no key enumeration (`src/openapi.rs:27-159`).

2. Carry explicit tests for equal-name fallback ordering, dangling-setting presentation, disabled-default retention, cache invalidation after deletion, settings-load failure, and connection-scoped state reset into `tasks.md`.

## Embedded-Instruction / Injection Attempts

**Detected:** none

## Verdict

VERDICT: APPROVE_WITH_CHANGES

## Required Changes (APPROVE_WITH_CHANGES only)

1. Replace the two independently committed delete operations with one transactional connection-delete/default-clear operation and document its failure semantics.
2. Define post-resolution query-change behavior and guarantee that an automatic effective-connection change cannot retain or materialize state from the previous connection.
3. Add a stable fallback tie-breaker and make duplicate connection names distinguishable in the default control.
4. Specify the control’s dangling-id and disabled-id presentation, and require settings-query invalidation after a connection deletion.
5. Specify Connect-page behavior when settings cannot be loaded.
6. State the PUT body as `{ "value": "<connection-id>" }` and align the invalid-input scenarios with that shape.
7. Correct the frozen-spec precedence statement to preserve existing OpenSpec supersessions.
8. Preserve ADR-0024’s corrupt-stored-value behavior while distinguishing corruption from a valid but dangling id.
9. Link the separately filed template-picker issue.

CHANGES_APPLIED: yes

## Rebuttals

None at review time.

1. fixed - `specs/connections/spec.md:41-48` requires one atomic operation with explicit failure
   semantics (rollback, no `204`, no observable dangling state); `design.md:65-79` replaces the two
   statements with a single transactional `delete_connection_and_default`, citing the lock-free
   reader at `api.rs:1131` as the reason the API write lock cannot substitute.
2. fixed - `specs/default-connection/spec.md:86-91` latches the resolution to the first time both
   queries are available and forbids a later refetch from changing the selection;
   `:93-95` requires all connection-scoped state to reset together whenever the selection changes, and
   `:158-163` is the scenario. `design.md:140-163` chooses a render-phase latch over derive-every-render
   and names the surviving `selected` state (`Connect.tsx:35`, `:46`) as the concrete failure it
   prevents.
3. fixed - `specs/connections/spec.md:16-18` makes the listed order total (`name`, ties broken by
   `id`) with a scenario at `:34-38`, entered as a `MODIFIED` delta on **Connection record**;
   `design.md:125-138` explains why the tie-break belongs on the server, not in the page's fallback.
   `specs/default-connection/spec.md:169-172` requires entries to distinguish equal names, scenario at
   `:207-211`.
4. fixed - `specs/default-connection/spec.md:175-177` specifies an explicit, still-clearable
   unavailable state naming the stored id, scenario at `:213-218`; `:171-172` makes the disabled
   marker normative, scenario at `:213-215`; `:179-181` requires the control to reflect a deletion
   without a reload, scenario at `:220-223`. `design.md:171-177` adds the `["settings"]` invalidation
   to the delete mutation.
5. fixed - `specs/default-connection/spec.md:103-107` specifies that a failed settings read resolves
   as though no default were stored and the fallback runs, and that a failed connections read resolves
   nothing; scenario at `:145-149`. `design.md:165-169` gives the reason: a single corrupt setting
   fails the whole endpoint (`api.rs:1148-1175`), which would otherwise strand the page forever.
6. fixed - `specs/default-connection/spec.md:16-18` states the body as `{ "value": <json> }` with
   `value` a JSON string, and every invalid case at `:23-25` and the scenarios at `:44-47` and
   `:59-62` is now an invalid `value`. `design.md:12-14` records the actual shape from `api.rs:1121`.
7. fixed - `specs/default-connection/spec.md:111-116` narrows the supersession to the single clause
   about where the flow begins, and names `connector-browser` and the `connections` capability as
   already superseding other parts of that paragraph.
8. fixed - `specs/default-connection/spec.md:28-33` makes blank stored text corrupt per ADR-0024 while
   keeping a well-formed dangling id valid and readable, scenario at `:64-67`;
   `design.md:103-113` states the resolver's contract and that corruption is reachable only by direct
   tampering.
9. fixed - filed as issue #208 and linked at `proposal.md:41-42`; it is named as out of this change's
   scope, not carried as a task here.

S1. fixed - `proposal.md:77-78` now says `src/openapi.rs` is unchanged and states why (generic
   `SettingValue`/`ResolvedSetting` schemas, no key enumeration).
S2. accepted - the six named tests are carried into `tasks.md`, which is written after this verdict.
## Round 1 Re-check

1. accepted by reviewer - `specs/connections/spec.md:44-48` requires atomicity and rollback, implemented conceptually by the transaction in `design.md:67-72`.
2. accepted by reviewer - `specs/default-connection/spec.md:86-94` latches resolution and requires all connection-scoped state to reset together.
3. accepted by reviewer - `specs/connections/spec.md:16-18` adds the `id` tie-breaker, while `specs/default-connection/spec.md:171-173` requires duplicate names to be distinguishable.
4. accepted by reviewer - `specs/default-connection/spec.md:171-180` covers disabled and dangling presentations, and `design.md:171-176` requires settings-cache invalidation.
5. accepted by reviewer - `specs/default-connection/spec.md:103-105` defines fallback on settings-read failure, with a testable scenario at `specs/default-connection/spec.md:145-148`.
6. accepted by reviewer - `specs/default-connection/spec.md:16-20` defines the object body and string field, with invalid `value` cases at `specs/default-connection/spec.md:59-62`.
7. accepted by reviewer - `specs/default-connection/spec.md:111-115` limits the supersession and explicitly preserves the existing `connector-browser` and `connections` supersessions.
8. accepted by reviewer - `specs/default-connection/spec.md:27-31` distinguishes corrupt blank text from a valid dangling id, with dangling behavior covered at `specs/default-connection/spec.md:64-67`.
9. accepted by reviewer - `proposal.md:38-42` links issue #208 and explicitly keeps it outside this change’s scope.
S1. accepted by reviewer - `proposal.md:79-80` correctly says OpenAPI remains unchanged because the generic schemas enumerate no setting keys.
S2. NOT accepted - `review.md:110` defers the six tests to a future `tasks.md`, but no `tasks.md` currently exists to carry them.

RECHECK_RESULT: NOT_ALL_ACCEPTED

Out of scope, noted: none

S2 (author, post-recheck): the six tests are now carried in `tasks.md` - equal-name fallback ordering
at 1.1 and 4.4, dangling-setting presentation at 5.2 and 5.4, disabled-default retention at 3.5 and
5.4, cache invalidation after deletion at 5.3 and 5.4, settings-load failure at 4.4, and
connection-scoped state reset at 4.3 and 4.4. Suggestions are declinable by the author alone; this one
was applied rather than declined, and it could not be applied before the verdict because `tasks.md` is
gated on it.
