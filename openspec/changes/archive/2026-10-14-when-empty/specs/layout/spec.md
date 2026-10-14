## MODIFIED Requirements

### Requirement: when gates an item

Any item MAY carry `when:`, a map from a parameter name to a condition. A condition is a scalar or the mapping `{ empty: <boolean> }`. The item is **active** only when every condition matches the label's resolved parameter value. A scalar matches a value equal to it, compared as text; a scalar condition on an absent parameter is false. `{ empty: true }` matches when the parameter is absent or its value is the empty string, and `{ empty: false }` matches otherwise. An inactive item, and everything inside an inactive container, SHALL be excluded from measurement and rendering: it imposes no requirement, paints nothing and raises no error.

At load, each of these SHALL be refused: `when: {}`; an empty or whitespace-only key or scalar value; a sequence value; a mapping value other than `{ empty: <boolean> }`, including one without `empty`, one with any other key, and one whose `empty` is not a boolean; a key naming an undeclared parameter; a scalar value outside an `enum` parameter's `values`; a key naming a `list` parameter outside a repeat scope (including on the repeating container itself), naming the key and the item's layout path. A scalar condition value is held as its scalar text, so `true` and `"true"` are one condition.

#### Scenario: An inactive branch does not require its fields

- **WHEN** a text whose value reads `{v_text}` sits behind a gate that does not match and the request omits `v_text`
- **THEN** the label renders

#### Scenario: Refused gates

- **WHEN** a container carries `when: {}`, `when: { mode: "" }`, `when: { tags: KIDS }` over a declared list, `when: { size: medium }` for an enum without `medium`, or a key naming no declared parameter
- **THEN** the template is refused at load

#### Scenario: A scalar condition is its text

- **WHEN** a container carries `when: { bold: true }` for a boolean `bold` resolving to true
- **THEN** it is drawn, as with `when: { bold: "true" }`

#### Scenario: An empty value selects the empty branch

- **WHEN** a string `code` with no default gates one container under `when: { code: { empty: true } }` and another under `when: { code: { empty: false } }`, and the request omits `code` or sends `""`
- **THEN** only the first container is drawn

#### Scenario: A value selects the non-empty branch

- **WHEN** the same two containers render with `code` sent as `XT30`, or as `" "`
- **THEN** only the second container is drawn

#### Scenario: Refused empty conditions

- **WHEN** a container carries `when: { code: { empty: "yes" } }`, `when: { code: { empty: true, also: 1 } }`, `when: { code: { not: "" } }` or `when: { code: {} }`
- **THEN** the template is refused at load, naming the key and the item's layout path
