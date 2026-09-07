# Architecture Decision Records

**Frozen at ADR-0057, by #285. Do not add rows. Do not write new ADRs.**

Authoritative for what it covers, and never extended, in the way `docs/SPEC.md` is frozen for the same
reason: an artifact that was the right record for its era, superseded by a better one. These 57
records, the last of which is itself the one that adopted OpenSpec, are the only account of *why* for
behavior that `docs/SPEC.md` states without rationale, so they stay readable and stay cited.

The 31 records written after ADR-0057 are gone (#378). Each duplicated a change folder that
`openspec/changes/archive/` keeps permanently, and that folder is the better account: it carries the
proposal, the design, every review round and the spec delta, where the ADR carried a summary written
alongside them. Deleting them cost nothing that was not kept elsewhere, and it stopped a reader
landing on a dated decision and taking it for a current rule.

Rationale for a behavior change now lives in that change's `proposal.md` and `design.md`, and the
contract it establishes lives in `openspec/specs/`. Neither needs a second narrative, and no mechanical
gate ever required this one. Plan reviews did read ADRs, and some checked an ADR's scope and content;
what no script checked was that a change produced one at all. Other process rules go unenforced too,
and AGENTS.md and docs/WORKFLOW.md both say which. What singles this one out is that it was also
absent from the lookup path AGENTS.md gives for finding a rule, so nothing consulted the output when
answering a question about behaviour either.

Existing `ADR-NNNN` references in source comments and in `docs/` remain valid for the 57 that
survive: they are stable permalinks into this archive. Where a reference named one of the 31, it now
names the issue whose change folder holds the decision, and the index rows below do the same.

**ADR-0057 is superseded in part by #285**, and its index row below says so. It established the rule
this freeze retires, and the record itself is left unedited because ADRs are immutable and the rest of
it still holds: the frozen `docs/SPEC.md`, the precedence rule, and the OpenSpec loop. Only its
provision that every behavior change writes an ADR is retired. Reading 0057 alone would otherwise
leave you following a rule nothing else in the repository states any more.

`docs/SPEC.md` still says decisions are recorded as ADRs, in three places. It is not corrected,
because it is frozen too and correcting it would be the one thing its freeze forbids. Both documents
describe the era they were frozen in, and AGENTS.md is where a live rule is stated: read a process
claim in either frozen file as history, exactly as you already read their behaviour claims that
`openspec/specs/` has since superseded.

`tests/adr_index.rs` still runs, and still checks that the set of record numbers here and the
set in the index below are the same. It exists because the index step was silently skipped for twenty
consecutive records (#160). Freezing does not retire it: it now guards the archive against a record
added without a row, or a row left behind by a deleted record. It compares NUMBERS, so a second file
reusing a number already indexed would pass; the 87 here are uniquely numbered and agree with the 87
rows.

An ADR captures a single decision: its context, the choice made, and the consequences. ADRs are
immutable once **Accepted**; the supersession chains below record where a decision was later replaced.

## Index

| ADR | Title | Status |
| --- | --- | --- |
| [0001](0001-record-architecture-decisions.md) | Record architecture decisions | Accepted |
| [0002](0002-two-stage-template-parsing.md) | Two-stage template parsing | Accepted |
| [0003](0003-typst-rendering-engine.md) | Typst as the rendering engine | Accepted |
| [0004](0004-bottom-left-coordinate-system.md) | Bottom-left coordinate system | Accepted |
| [0005](0005-recursive-containers-with-option-gating.md) | Recursive containers with option gating | Accepted (option gating superseded by [0055](0055-parameterized-templates.md)) |
| [0006](0006-template-edit-ownership.md) | Template edit ownership: manual vs GUI | Accepted |
| [0007](0007-printer-architecture-and-transport-model.md) | Printer architecture and transport model | Accepted (record shape superseded by [0042](0042-remove-printer-enabled.md)) |
| [0008](0008-ui-delivery.md) | Web UI delivery | Accepted |
| [0009](0009-image-source-model.md) | Image source model | Accepted |
| [0010](0010-variable-interpolation-layer.md) | Variable interpolation layer | Accepted (dual-binding superseded by [0055](0055-standardize-on-value-interpolation.md)) |
| [0011](0011-unified-batch-endpoint.md) | Unified batch render/print endpoint | Accepted |
| [0012](0012-job-options.md) | Job options as format-intrinsic batch parameters | Accepted |
| [0013](0013-render-print-ux.md) | Render & Print UX decisions | Accepted (parameter pre-fill superseded in part by [#241](../../openspec/changes/archive/2026-08-29-issue-241-no-inferred-defaults)) |
| [0014](0014-csv-import-grid.md) | CSV import editable grid | Accepted |
| [0015](0015-settings-printers-ux.md) | Settings & Printers screen UX | Accepted |
| [0016](0016-deployment-and-packaging.md) | Deployment and packaging | Accepted |
| [0017](0017-app-authentication.md) | App authentication | Accepted |
| [0018](0018-api-integration-spine.md) | API integration spine (connectors) | Accepted (connections store superseded in part by [#169](../../openspec/changes/archive/2026-08-21-issue-169-connection-public-url)) |
| [0019](0019-ci-and-image-publishing.md) | CI and image publishing | Accepted |
| [0020](0020-variables-vs-settings.md) | Variables vs settings (substitution vs app config) | Accepted |
| [0021](0021-homebox-connect-hardening.md) | Homebox & Connect hardening (isLocation, row link, selection) | Accepted |
| [0022](0022-import-option-model.md) | Import option model and template-switch persistence | Accepted (option model defaults superseded in part by [#241](../../openspec/changes/archive/2026-08-29-issue-241-no-inferred-defaults)) |
| [0023](0023-template-thumbnail-endpoint.md) | Template thumbnail endpoint | Accepted |
| [0024](0024-app-settings-storage-and-api.md) | App settings storage and API | Accepted |
| [0025](0025-optional-no-auth-mode.md) | Optional no-auth mode for homelab | Accepted |
| [0026](0026-auto-length-dynamic-width.md) | Auto-length dynamic-width single labels (continuous tape) | Superseded by [#226](../../openspec/changes/archive/2026-08-27-issue-226-unify-size-resolution) |
| [0027](0027-multi-arch-image-publishing.md) | Multi-arch image publishing (amd64 + arm64) | Accepted |
| [0028](0028-datetime-interpolation-token.md) | Current-time interpolation token ({datetime.*}) | Accepted (syntax superseded in part by [#239](../../openspec/changes/archive/2026-08-27-issue-239-token-grammar)) |
| [0029](0029-runtime-base-debian-slim.md) | Runtime base image: debian-slim, not distroless | Accepted |
| [0030](0030-multiline-auto-length-tape.md) | Multi-line auto-length tape labels | Accepted |
| [0031](0031-inbound-print-webhook.md) | Inbound print webhook (POST /print) | Accepted |
| [0032](0032-ipp-auth-custom-ca.md) | IPP basic-auth + custom-CA for printing | Accepted |
| [0033](0033-capability-aware-rendering.md) | Capability-aware rendering (bi-level/resolution; media gate) | Accepted |
| [0034](0034-single-config-dir.md) | Single config dir (LABELER_CONFIG_DIR; first-run template seeding) | Accepted (seeding superseded by [0046](0046-template-catalog.md)) |
| [0035](0035-font-weight-via-variable-font.md) | Font weight via the bundled variable font | Accepted |
| [0036](0036-container-rotation.md) | Layout-aware container rotation | Accepted (amended by [#226](../../openspec/changes/archive/2026-08-27-issue-226-unify-size-resolution)) |
| [0037](0037-effortless-print-form.md) | Effortless print form: copies routing + global default printer | Accepted |
| [0038](0038-print-first-landing.md) | Print-first landing: grid as the print picker | Accepted |
| [0039](0039-per-field-render-override.md) | Per-field render override (color and resolution) | Accepted |
| [0040](0040-printer-probe-endpoint.md) | Printer probe endpoint and shared IPP egress screen | Accepted |
| [0041](0041-vertical-alignment-delegated-to-typst.md) | Vertical text alignment is delegated to Typst | Superseded by [0045](0045-vertical-text-alignment.md) |
| [0042](0042-remove-printer-enabled.md) | Remove the printer `enabled` flag | Accepted |
| [0043](0043-ink-based-vertical-alignment.md) | Vertical alignment positions the ink, not a metric box | Superseded by [0045](0045-vertical-text-alignment.md) |
| [0044](0044-baseline-relative-vertical-alignment.md) | Vertical alignment is baseline-relative, using a fixed metric box | Superseded by [0045](0045-vertical-text-alignment.md) |
| [0045](0045-vertical-text-alignment.md) | Vertical text alignment | Accepted |
| [0046](0046-template-catalog.md) | A template catalog replaces first-run seeding | Accepted |
| [0047](0047-starter-template-set.md) | The catalog is a designed five-template starter set | Accepted |
| [0048](0048-template-delete-prunes-favorites.md) | Deleting a template prunes favorites, not recents | Accepted |
| [0049](0049-weight-aware-text-measurement.md) | Text measurement tracks the font instance Typst renders | Accepted |
| [0050](0050-ink-reservation-at-slot-edges.md) | Reserve ink room at slot edges instead of changing the line box | Accepted (center clause superseded by [#245](../../openspec/changes/archive/2026-08-28-issue-245-center-ink-reserve)) |
| [0051](0051-edge-relative-and-corner-placement.md) | Edge-relative coordinates and `to:` opposite-corner placement | Accepted (amended by [#226](../../openspec/changes/archive/2026-08-27-issue-226-unify-size-resolution)) |
| [0052](0052-error-reason-discriminator.md) | A `details.reason` discriminator for `AppError` | Accepted |
| [0053](0053-max-bounds-cap.md) | `max_w`/`max_h` cap an `auto` size, not substitute for its fallback | Superseded by [#226](../../openspec/changes/archive/2026-08-27-issue-226-unify-size-resolution) |
| [0054](0054-auto-fallback-position.md) | An `auto` size falls back to the space remaining from its anchor | Superseded by [#226](../../openspec/changes/archive/2026-08-27-issue-226-unify-size-resolution) |
| [0055](0055-standardize-on-value-interpolation.md) | Standardize on value interpolation for text and QR items | Accepted |
| [0056](0056-parameterized-templates.md) | Parameterized templates and dynamic layout constraints | Accepted (implicit defaults superseded in part by [#241](../../openspec/changes/archive/2026-08-29-issue-241-no-inferred-defaults)) |
| [0057](0057-openspec-adoption.md) | Adopt OpenSpec and freeze the living specification | Accepted; its ADR-per-change rule retired by #285 |
