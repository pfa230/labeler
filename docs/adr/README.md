# Architecture Decision Records

**Frozen at ADR-0092 (2026-09-01), by #285. Do not add rows. Do not write new ADRs.**

Authoritative for what it covers, and never extended, in the way `docs/SPEC.md` is frozen for the same
reason: an artifact that was the right record for its era, superseded by a better one. The 57 records
through ADR-0057, which is itself the one that adopted OpenSpec, are the only account of *why* for
behavior that `docs/SPEC.md` states without rationale, so they stay readable and stay cited. The 31
after it duplicate a change folder that `openspec/changes/archive/` keeps permanently.

Rationale for a behavior change now lives in that change's `proposal.md` and `design.md`, and the
contract it establishes lives in `openspec/specs/`. Neither needs a second narrative, and no mechanical
gate ever required this one. Plan reviews did read ADRs, and some checked an ADR's scope and content;
what no script checked was that a change produced one at all. Other process rules go unenforced too,
and AGENTS.md and docs/WORKFLOW.md both say which. What singles this one out is that it was also
absent from the lookup path AGENTS.md gives for finding a rule, so nothing consulted the output when
answering a question about behaviour either.

Existing `ADR-NNNN` references in source comments and in `docs/` remain valid: they are stable
permalinks into this archive.

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
| [0013](0013-render-print-ux.md) | Render & Print UX decisions | Accepted (parameter pre-fill superseded in part by [0088](0088-explicit-parameter-defaults.md)) |
| [0014](0014-csv-import-grid.md) | CSV import editable grid | Accepted |
| [0015](0015-settings-printers-ux.md) | Settings & Printers screen UX | Accepted |
| [0016](0016-deployment-and-packaging.md) | Deployment and packaging | Accepted |
| [0017](0017-app-authentication.md) | App authentication | Accepted |
| [0018](0018-api-integration-spine.md) | API integration spine (connectors) | Accepted (connections store superseded in part by [0063](0063-connection-public-url-is-the-link-base.md)) |
| [0019](0019-ci-and-image-publishing.md) | CI and image publishing | Accepted |
| [0020](0020-variables-vs-settings.md) | Variables vs settings (substitution vs app config) | Accepted |
| [0021](0021-homebox-connect-hardening.md) | Homebox & Connect hardening (isLocation, row link, selection) | Accepted |
| [0022](0022-import-option-model.md) | Import option model and template-switch persistence | Accepted (option model defaults superseded in part by [0088](0088-explicit-parameter-defaults.md)) |
| [0023](0023-template-thumbnail-endpoint.md) | Template thumbnail endpoint | Accepted |
| [0024](0024-app-settings-storage-and-api.md) | App settings storage and API | Accepted |
| [0025](0025-optional-no-auth-mode.md) | Optional no-auth mode for homelab | Accepted |
| [0026](0026-auto-length-dynamic-width.md) | Auto-length dynamic-width single labels (continuous tape) | Superseded by [0080](0080-unify-size-resolution.md) |
| [0027](0027-multi-arch-image-publishing.md) | Multi-arch image publishing (amd64 + arm64) | Accepted |
| [0028](0028-datetime-interpolation-token.md) | Current-time interpolation token ({datetime.*}) | Accepted (syntax superseded in part by [0079](0079-token-grammar.md)) |
| [0029](0029-runtime-base-debian-slim.md) | Runtime base image: debian-slim, not distroless | Accepted |
| [0030](0030-multiline-auto-length-tape.md) | Multi-line auto-length tape labels | Accepted |
| [0031](0031-inbound-print-webhook.md) | Inbound print webhook (POST /print) | Accepted |
| [0032](0032-ipp-auth-custom-ca.md) | IPP basic-auth + custom-CA for printing | Accepted |
| [0033](0033-capability-aware-rendering.md) | Capability-aware rendering (bi-level/resolution; media gate) | Accepted |
| [0034](0034-single-config-dir.md) | Single config dir (LABELER_CONFIG_DIR; first-run template seeding) | Accepted (seeding superseded by [0046](0046-template-catalog.md)) |
| [0035](0035-font-weight-via-variable-font.md) | Font weight via the bundled variable font | Accepted |
| [0036](0036-container-rotation.md) | Layout-aware container rotation | Accepted (amended by [0080](0080-unify-size-resolution.md)) |
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
| [0050](0050-ink-reservation-at-slot-edges.md) | Reserve ink room at slot edges instead of changing the line box | Accepted (center clause superseded by [0084](0084-centred-text-reserves-its-ink.md)) |
| [0051](0051-edge-relative-and-corner-placement.md) | Edge-relative coordinates and `to:` opposite-corner placement | Accepted (amended by [0080](0080-unify-size-resolution.md)) |
| [0052](0052-error-reason-discriminator.md) | A `details.reason` discriminator for `AppError` | Accepted |
| [0053](0053-max-bounds-cap.md) | `max_w`/`max_h` cap an `auto` size, not substitute for its fallback | Superseded by [0080](0080-unify-size-resolution.md) |
| [0054](0054-auto-fallback-position.md) | An `auto` size falls back to the space remaining from its anchor | Superseded by [0080](0080-unify-size-resolution.md) |
| [0055](0055-standardize-on-value-interpolation.md) | Standardize on value interpolation for text and QR items | Accepted |
| [0056](0056-parameterized-templates.md) | Parameterized templates and dynamic layout constraints | Accepted (implicit defaults superseded in part by [0088](0088-explicit-parameter-defaults.md)) |
| [0057](0057-openspec-adoption.md) | Adopt OpenSpec and freeze the living specification | Accepted; its ADR-per-change rule retired by #285 |
| [0058](0058-duplicate-template-id-refuses-the-file.md) | A duplicate template id refuses the file, not the server | Accepted |
| [0059](0059-auto-length-text-box-is-the-alignment-slot.md) | Auto-length text box is the alignment slot | Superseded by [0080](0080-unify-size-resolution.md) |
| [0060](0060-connection-scoped-field-transforms.md) | Connection-scoped field transforms | Accepted |
| [0061](0061-template-group-yaml-field.md) | A template's group is a YAML field, not its directory | Superseded by [0073](0073-group-is-a-directory-id-is-the-filename.md) |
| [0062](0062-service-may-rewrite-single-template-key.md) | The service may rewrite one key of a hand-authored template | Superseded by [0073](0073-group-is-a-directory-id-is-the-filename.md) |
| [0063](0063-connection-public-url-is-the-link-base.md) | A connection's public URL is the link base, its base URL is the fetch base | Accepted |
| [0064](0064-svar-grid-for-the-connector-browser.md) | The connector browse table uses SVAR DataGrid; the ordering rules stay ours | Accepted |
| [0065](0065-template-writes-verify-the-id-they-wrote.md) | A template write verifies the id it wrote, and a contested id refuses the delete | Accepted |
| [0066](0066-format-badge-carries-icon-colour-and-count.md) | The format badge carries an icon, its own colour and a position count, and is delineated by a border | Accepted (the --accent-deep token decision superseded by [0071]) |
| [0068](0068-datetime-parameter-type.md) | Template parameter type for datetime with dynamic rendering and override support | Accepted (token list and formatting syntax superseded by [0079](0079-token-grammar.md); render-instant default superseded by [0088](0088-explicit-parameter-defaults.md)) |
| [0069](0069-connect-opens-on-a-default-connection.md) | Connect opens on a default connection named by an instance-wide setting | Accepted |
| [0070](0070-service-derives-the-input-list.md) | Service derives the input list | Accepted (amended by [0088](0088-explicit-parameter-defaults.md)) |
| [0071](0071-one-accent-colour-with-a-defined-ink.md) | One accent colour, dark enough to carry text, with a defined ink on its fill | Accepted |
| [0072](0072-two-filter-scopes-named-not-merged.md) | Two filter scopes named, not merged | Accepted |
| [0073](0073-group-is-a-directory-id-is-the-filename.md) | Group is a directory; ID is the filename | Accepted |
| [0075](0075-request-rejections-use-the-error-envelope.md) | Request rejections use the standard JSON error envelope | Accepted |
| [0076](0076-the-filesystem-answers-the-case-question.md) | The filesystem answers the case question | Accepted |
| [0079](0079-token-grammar.md) | Token grammar: namespaces, system values, and format syntax | Accepted |
| [0080](0080-unify-size-resolution.md) | One size-resolution protocol across validation, measurement, and rendering | Accepted (amended by [0083](0083-packed-children-flow-layout.md)) |
| [0081](0081-size-vocabulary-content-and-fill.md) | Sizing vocabulary is `content` and `fill` | Accepted (amended by [0083](0083-packed-children-flow-layout.md)) |
| [0082](0082-text-overflow-policy.md) | Text overflow is an authored policy (`ellipsis` or `fail`) | Accepted |
| [0083](0083-packed-children-flow-layout.md) | A packed child is anchorless, and its container's arrangement places it | Accepted (amended by [0089](0089-wrapping-and-the-overflow-policy.md)) |
| [0084](0084-centred-text-reserves-its-ink.md) | Centred text reserves its ink | Accepted |
| [0085](0085-text-wrap-flag.md) | Text `wrap` flag, segmentation, and field-level shortening | Accepted |
| [0086](0086-a-grid-cell-editor-follows-the-reported-control.md) | A grid cell editor follows the reported control | Accepted |
| [0087](0087-connection-connector-is-immutable.md) | A connection's connector is immutable, and a contradiction is reported | Accepted |
| [0088](0088-explicit-parameter-defaults.md) | A parameter is required unless its template declares a default | Accepted |
| [0089](0089-wrapping-and-the-overflow-policy.md) | Wrapping and the overflow policy | Accepted |
| [0090](0090-a-declared-default-is-deferred-not-copied.md) | A declared default is deferred, not copied | Accepted |
| [0091](0091-text-ink-is-a-full-colour.md) | Text ink is a full-colour RGBA value | Accepted |
| [0092](0092-a-shape-carries-a-stroke-and-a-background.md) | A shape carries a stroke and a background | Accepted |
