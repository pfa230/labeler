# Architecture Decision Records

This directory records the significant architectural decisions for Labeler, using
[Michael Nygard's ADR format](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions).

An ADR captures a single decision: its context, the choice made, and the consequences. ADRs are
immutable once **Accepted**: to change a decision, add a new ADR that supersedes the old one (mark the
old one `Superseded by ADR-NNNN` and the new one `Supersedes ADR-NNNN`).

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
| [0013](0013-render-print-ux.md) | Render & Print UX decisions | Accepted |
| [0014](0014-csv-import-grid.md) | CSV import editable grid | Accepted |
| [0015](0015-settings-printers-ux.md) | Settings & Printers screen UX | Accepted |
| [0016](0016-deployment-and-packaging.md) | Deployment and packaging | Accepted |
| [0017](0017-app-authentication.md) | App authentication | Accepted |
| [0018](0018-api-integration-spine.md) | API integration spine (connectors) | Accepted |
| [0019](0019-ci-and-image-publishing.md) | CI and image publishing | Accepted |
| [0020](0020-variables-vs-settings.md) | Variables vs settings (substitution vs app config) | Accepted |
| [0021](0021-homebox-connect-hardening.md) | Homebox & Connect hardening (isLocation, row link, selection) | Accepted |
| [0022](0022-import-option-model.md) | Import option model and template-switch persistence | Accepted |
| [0023](0023-template-thumbnail-endpoint.md) | Template thumbnail endpoint | Accepted |
| [0024](0024-app-settings-storage-and-api.md) | App settings storage and API | Accepted |
| [0025](0025-optional-no-auth-mode.md) | Optional no-auth mode for homelab | Accepted |
| [0026](0026-auto-length-dynamic-width.md) | Auto-length dynamic-width single labels (continuous tape) | Accepted |
| [0027](0027-multi-arch-image-publishing.md) | Multi-arch image publishing (amd64 + arm64) | Accepted |
| [0028](0028-datetime-interpolation-token.md) | Current-time interpolation token ({datetime.*}) | Accepted |
| [0029](0029-runtime-base-debian-slim.md) | Runtime base image: debian-slim, not distroless | Accepted |
| [0030](0030-multiline-auto-length-tape.md) | Multi-line auto-length tape labels | Accepted |
| [0031](0031-inbound-print-webhook.md) | Inbound print webhook (POST /print) | Accepted |
| [0032](0032-ipp-auth-custom-ca.md) | IPP basic-auth + custom-CA for printing | Accepted |
| [0033](0033-capability-aware-rendering.md) | Capability-aware rendering (bi-level/resolution; media gate) | Accepted |
| [0034](0034-single-config-dir.md) | Single config dir (LABELER_CONFIG_DIR; first-run template seeding) | Accepted (seeding superseded by [0046](0046-template-catalog.md)) |
| [0035](0035-font-weight-via-variable-font.md) | Font weight via the bundled variable font | Accepted |
| [0036](0036-container-rotation.md) | Layout-aware container rotation | Accepted |
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
| [0050](0050-ink-reservation-at-slot-edges.md) | Reserve ink room at slot edges instead of changing the line box | Accepted |
| [0051](0051-edge-relative-and-corner-placement.md) | Edge-relative coordinates and `to:` opposite-corner placement | Accepted |
| [0052](0052-error-reason-discriminator.md) | A `details.reason` discriminator for `AppError` | Accepted |
| [0053](0053-max-bounds-cap.md) | `max_w`/`max_h` cap an `auto` size, not substitute for its fallback | Accepted |
| [0054](0054-auto-fallback-position.md) | An `auto` size falls back to the space remaining from its anchor | Accepted |
| [0055](0055-standardize-on-value-interpolation.md) | Standardize on value interpolation for text and QR items | Accepted |
| [0056](0056-parameterized-templates.md) | Parameterized templates and dynamic layout constraints | Accepted |
| [0057](0057-openspec-adoption.md) | Adopt OpenSpec and freeze the living specification | Accepted |
| [0058](0058-duplicate-template-id-refuses-the-file.md) | A duplicate template id refuses the file, not the server | Accepted |
| [0059](0059-auto-length-text-box-is-the-alignment-slot.md) | Auto-length text box is the alignment slot | Accepted |

## Adding an ADR

1. Copy the structure of an existing record. Use the next zero-padded number.
2. Set `Status: Proposed`, fill in Context / Decision / Consequences.
3. Add a row to the index above.
4. On acceptance, set `Status: Accepted` and specify the new behavior as an OpenSpec delta under
   `openspec/changes/<change>/specs/`, which folds into `openspec/specs/` on archive.
   [`../SPEC.md`](../SPEC.md) is frozen (ADR-0057) and is no longer updated.
