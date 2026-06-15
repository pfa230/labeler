# 2. Two-stage template parsing

**Status:** Accepted

## Context

Template YAML favors authoring ergonomics: placement fields (`at`, `size`, `max_w`, …) are flattened
onto each item, `padding` accepts either a single number or a `[t, r, b, l]` array, and containers
default their size to "fill the parent". The internal model that validation and rendering consume wants
the opposite: explicit, normalized, already-checked structures. Deserializing straight into the domain
model would force the wire shortcuts and the internal invariants into one set of types, and would make
error messages hard to locate within a nested document.

## Decision

Parse in two stages. First deserialize into dedicated `raw.rs` types that mirror the wire format and
use `deny_unknown_fields`, wrapped in `serde_path_to_error` to attach a JSON-path location to every
failure. Then convert raw → domain (`models.rs`) via `TryFrom` implementations in `convert.rs`, where
shorthands are expanded and defaults applied. `parse.rs` orchestrates the two stages.

## Consequences

- The wire format can evolve independently of the validated internal model.
- Parse errors carry a precise path (e.g. `layout[2].items[0].padding`).
- Adding or changing a layout field touches three places: `raw.rs`, `models.rs`, and the `TryFrom` in
  `convert.rs`. This coupling is deliberate but must be remembered.
