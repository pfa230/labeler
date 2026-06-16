# 12. Job options as format-intrinsic batch parameters

**Status:** Accepted

## Context

ADR-0011's `/batch` introduced `start_slot`: a knob that applies to a whole render/print job, not to
any one label. It is neither per-row `data` (label content) nor a per-row template `option` (a declared
choice selected per label, e.g. orientation). It is a third thing, a job-level setting, and it is not
the last of its kind. Other candidates exist: skipping arbitrary sheet slots (holes or damaged cells),
per-job sheet margins, and continuous-tape print behavior (cut per-label vs at-end, gap). Without a
convention, each would be added ad hoc and risk being conflated with data or options.

## Decision

Define **job options** as a distinct category: job-level parameters **intrinsic to the template's
format**, passed as optional fields on the `/batch` request and validated against the format. A job
option is rejected (`400`) for a format that does not support it, exactly as `start_slot` is rejected
for single templates.

- **Not template-declared.** These knobs are inherent to a format (any sheet has slots; any continuous
  tape has a cut/gap concept), so templates do not declare or gate them. This keeps the template schema
  unchanged and avoids ceremony for format-inherent behavior. (Per-row template `options`, which *are*
  declared and chosen per label, remain a separate mechanism.)
- **Flat for now.** The current set is a single field (`start_slot`). New job options are added as
  further optional `/batch` fields. If the set grows enough to clutter the request, group them under a
  `job` object then; not worth the nesting for one field today.
- **Current member:** `start_slot` (sheet) — skip a leading N slots on the first page of a
  partially-used sheet. Already implemented; it covers the partial-sheet case in practice, so no further
  job option is built now.

### Taxonomy (documented, deferred)

| Knob | Format | Kind | Status |
| --- | --- | --- | --- |
| `start_slot` | sheet | render-time | Implemented |
| skip arbitrary slots | sheet | render-time | Deferred (start_slot suffices today) |
| per-job margins / inset | sheet | render-time | Deferred |
| cut behavior (per-label / at-end) | continuous tape | print-time | Deferred (needs driver/IPP support) |
| gap | continuous tape | print-time | Deferred (needs driver/IPP support) |

Render-time knobs change the rendered artifact; print-time knobs change how it is sent (driver/IPP
attributes) and are blocked until non-CUPS drivers or IPP media configuration exist.

## Consequences

- A new job option has a clear, consistent home: an optional `/batch` field, format-validated, with no
  template-schema change. The next person does not re-derive the category.
- No code change ships with this ADR; `start_slot` already exists and remains the canonical example.
- Deferred knobs (slot-skipping, margins, tape cut/gap) are recorded so future work can pick them up
  without rethinking the model. Tape print-time knobs wait on the driver families that are themselves
  later-phase.

## Alternatives considered

- **Template-declared job options** (templates list supported knobs + defaults, like `options`).
  Rejected: ceremony and schema for behavior that is inherent to the format, not a template choice.
- **Hybrid** (format defines knobs; templates carry defaults/limits). Rejected: two places to look for a
  knob's value, for no concrete need today.
