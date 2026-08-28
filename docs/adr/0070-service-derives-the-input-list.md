# 70. Service derives the input list

Date: 2026-08-24

## Status

Accepted. Implements [#200](https://github.com/pfa230/labeler/issues/200). Subsumes [#215](https://github.com/pfa230/labeler/issues/215). Builds on [ADR-0056](0056-parameterized-templates.md) and [ADR-0068](0068-datetime-parameter-type.md).

## Context

The UI previously decided for itself which fields a template required by walking the layout AST in TypeScript (`ui/src/lib/templateFields.ts`). That logic duplicated the service's own template resolution and item traversal rules across a process boundary. Over time, the implementations drifted:

1. `templateFields.ts` gated container children on legacy `it.option`, which had been rewritten to `when:` on load since ADR-0056; because the API never produced `option` maps on containers, the UI never excluded gated items and displayed all inputs unconditionally.
2. `when:` predicates on individual items (`text`, `qr`, `image`, `line`) were ignored by the UI walker.
3. The service maintained separate walkers: `walk_placeholder` in `src/render/mod.rs` for thumbnail placeholder data, which ignored `when:` and invented values for inactive branches, causing thumbnail defects where gate keys or defaulted enums were overwritten with their own names.
4. The print form rendered all declared parameters regardless of whether their containing branch was active, violating the lazy missing-field semantics of `docs/SPEC.md` §5.

Porting the `when:` evaluator and normalization logic into TypeScript would perpetuate duplicate business logic and invite further divergence whenever parameter coercion or template features evolve.

## Decision

**1. Unified derivation in the service.** The backend service is the single source of truth for template input derivation. The service walks format dimensions and layout items, accounting for `when:` conditions, attribute references, interpolation tokens, and parameter definitions to derive an ordered list of `InputSpec` records.

**2. Dynamic input endpoint and detail embedding.**
- `POST /api/templates/{id}/inputs` accepts a batch of label payloads `{ labels: [{ data?: ... }] }` and returns `{ inputs: [[InputSpec]] }`, evaluating `when:` gating against each label's data.
- `GET /api/templates/{id}` embeds `inputs: { all: InputSpec[], default: InputSpec[] }` and `variables: string[]`, eliminating extra round-trips for initial form render, catalog detail views, and Connect field mapping.

**3. Lenient resolution during input derivation.** When deriving inputs for partially filled forms, the service resolves parameter values leniently: values that fail type coercion (e.g. an incomplete enum or non-numeric string for an integer) are treated as absent, allowing defaults to apply without returning 4xx errors. Rendering (`/api/render/label`, `/api/batch`) remains strict and enforces validation errors upon submission.

**4. Single placeholder rule for thumbnails and previews.** Legacy walker functions (`walk_placeholder`, `template_fields`) are deleted. Thumbnails and previews invent sample values exclusively from `inputs.all` for entries that are `interpolated` and `required` (e.g., sample 1×1 PNG for images, input name for text/textarea, `min` or 1 for numbers/integers). Declared enums and non-interpolated gate parameters keep their defaults, ensuring thumbnails render accurately.

**5. UI acts as a thin renderer of `InputSpec[]`.** All UI forms (`FieldForm`, `PrintForm`, `Import`, `Connect`) render controls directly driven by the server's `InputSpec[]`. The UI maintains an LRU-cached, debounced hook (`useLabelInputs`) that keeps previous inputs visible while requests are in flight. On submission, fields not present in the current active input list or empty non-text controls are omitted from the payload, preventing inactive stale data from failing server-side validation.

## Consequences

- The client is completely decoupled from layout traversal, parameter defaulting, and `when:` conditional logic.
- Adding or changing template layout structures or parameter types only requires changes to the Rust backend.
- UI forms dynamically adjust fields as operators toggle controlling parameters, requiring only the fields relevant to the active branch.
- Thumbnails and catalog indexes share a single unified derivation path with runtime input resolution.
- Deprecated layout walker functions and obsolete `option`/`options` types are permanently removed from the TypeScript codebase.
