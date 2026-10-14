## Context

A `when:` map is `BTreeMap<String, String>` from parse to render. `raw.rs` `deserialize_when_map` reads each value as an untagged scalar (string, bool, int, float) and stores its text; `templates.rs` `validate_when` refuses an empty map and blank keys or values, and `validate_when_references` checks the name, the list rule and enum membership; `render/mod.rs` `is_item_active` compares `value_to_string(value)` to the stored text and treats a missing value as false. The same map is cloned when a repeat instance is built. See proposal.md for why an emptiness test is needed.

## Goals / Non-Goals

**Goals:**

- One condition type carried from `raw.rs` to render, so load validation and render evaluation read the same value.

**Non-Goals:**

- Inequality, negation of scalar conditions, or any other operator.
- Emptiness of a `list` parameter outside a repeat scope; the existing list rule still refuses the key.
- Refusing `{ empty: ... }` on a parameter that can never be empty, such as a `boolean`. The rule is uniform: it simply never matches `empty: true`.

## Decisions

**A `Condition` enum replaces the `String` value.** `models.rs` gains `enum Condition { Equals(String), Empty(bool) }`, and every item's `when` becomes `Option<BTreeMap<String, Condition>>`. Keeping `String` and encoding emptiness in a sentinel text would collide with a real parameter value; a separate `empty_when` map per item would put one gate in two places.

**Deserialization stays in `deserialize_when_map`, written by hand.** Its condition type becomes a hand-written `Deserialize` with a visitor, as `RawDimension` already is: `visit_str`, `visit_bool`, `visit_i64`, `visit_u64` and `visit_f64` yield `Equals` holding the scalar's text, and `visit_map` deserializes a `deny_unknown_fields` struct `{ empty: bool }` through `MapAccessDeserializer` and yields `Empty`. A non-boolean `empty`, an extra key, a mapping without `empty`, a sequence and a null all fail in serde with the path `serde_path_to_error` already attaches (`layout[i].when.<key>`), and serde's own message names the offending key (an unknown field `also`, a missing field `empty`, or a type error at `.empty`), as the templates spec's "Template file keys" requires. An untagged enum would not: it reports every malformed mapping as "did not match any variant" and drops the key. No new error kind or reason slug.

**Validation applies the scalar rules to scalars only.** `validate_when`'s blank-value check and `validate_when_references`' enum-membership check skip `Empty`; the empty-map, blank-key, undeclared-name and list rules apply to both forms.

**Evaluation reads emptiness from the resolved value.** In `is_item_active`, `Empty(flag)` matches when `flag` equals "the parameter is absent from the resolved data, or `value_to_string` of it is `\"\"`". Supplied-value coercion has already turned a blank non-string value into an omission (`parameters`), so the check needs no per-type case. Inside a repeat scope the bound element is the value, exactly as for a scalar condition.

## Risks / Trade-offs

- [A hand-written visitor is more code than an untagged enum] → it is the only way serde reports the offending key inside the condition mapping; `RawDimension` is the existing precedent.
