## 1. Condition model and parsing

- [ ] 1.1 Add `Condition { Equals(String), Empty(bool) }` to `models.rs`, make every item's `when` a `BTreeMap<String, Condition>`, and give `deserialize_when_map` a hand-written condition visitor whose `visit_map` reads a `deny_unknown_fields` `{ empty: bool }` struct (design.md); verify by tests that `when: { code: { empty: true } }` and `{ empty: false }` load, that `when: { bold: true }` and `{ bold: "true" }` still load as the same condition, and that `{ empty: "yes" }`, `{ empty: true, also: 1 }`, `{ not: "" }`, `{}` and a sequence as a condition value are refused at load with a message naming `when.code` and the item's layout path, where the extra-key case's message names `also`, the `{ not: "" }` case's names `not`, and the `{}` case's names `empty`.
- [ ] 1.2 Restrict `validate_when`'s blank-value check and `validate_when_references`' enum-membership check to scalar conditions; verify by tests that `{ empty: true }` on an `enum` parameter loads, that an `empty` condition naming an undeclared parameter or a `list` outside a repeat scope is still refused, and that every case in the existing "Refused gates" scenario is still refused.

## 2. Evaluation

- [ ] 2.1 Evaluate `Empty(flag)` in `is_item_active` as "absent or prints the empty string" equal to `flag`; verify by render tests with `code` a string with no default and two containers gated `{ empty: true }` and `{ empty: false }`: omitting `code` and sending `""` draw only the first, sending `XT30` and `" "` draw only the second.

## 3. Docs

- [ ] 3.1 Document `{ empty: true }` / `{ empty: false }` in `docs/AUTHORING.md` "Containers, `when:` and rotation" with a two-branch example; verify the section names both forms and links the layout spec's when requirement.

## 4. Gates

- [ ] 4.1 Run the cut pass, and name for each new test the regression that makes it fail; verify `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test` and `npx --no-install openspec validate --all --strict --no-interactive` pass.
