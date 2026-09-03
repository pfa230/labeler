# Plan review

AUTHOR: claude
REVIEWER: codex
VERDICT: APPROVE_WITH_CHANGES
ROUNDS: 2

1. The single-render scenario uses the wrong JSON shape. `request-data-keys/spec.md:77-82` nests data under `"label"`, but `RenderLabelRequest` flattens `LabelInput`, requiring top-level `"data"` (`src/models.rs:1201-1204`; `docs/SPEC.md:136-145`). The written request would fail deserialization instead of reaching `data_key_unknown`.

2. The plan contradicts itself about failure precedence. `request-data-keys/spec.md:69-73` says no existing rejection is relabeled, while `batch-validation/spec.md:23-26,95-99` and `design.md:110-118` require `data_key_unknown` to win over another per-label failure. Today, for example, an invalid declared numeric value fails during resolution (`src/render/mod.rs:243-290`); adding an undeclared key would intentionally change that reported reason.

### Required changes

The author applies these changes and NO further review follows.

1. Change the single-render scenario body to `{"template":"shelf","data":{"title":"Bolts","sku_legacy":"X-1"}}`.

2. Narrow the preservation claim to request-level/admission checks that precede label validation, and explicitly state that within one label `data_key_unknown` intentionally takes precedence over—and can replace—an existing parameter-resolution or render failure.

CHANGES_APPLIED: yes
SPECS_SHA256: f813d445bfdb1545308394fc78fb941d645aefc00da01260e6012bdd5a046a90
