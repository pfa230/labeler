1. The `MODIFIED` requirement unintentionally deletes existing supersession provenance. The canonical requirement records that it replaced the former requirement (`openspec/specs/datetime-params/spec.md:240-242`), but the delta omits those sentences (`openspec/changes/issue-310-a-param-s-enum-key-is-parsed-and-ignored/specs/datetime-params/spec.md:5-7`). This deletion is unrelated to the scoped `enum:` changes described in `proposal.md:52-61`.

### Required changes

Restore after line 7: `It replaces the requirement "A template declares a datetime parameter as an instant, not a rendering", which this change removes.` The author applies this edit and no further review follows.

VERDICT: APPROVE_WITH_CHANGES
