## 1. The refusal tests, red before the change

- [x] 1.1 Replace `enum_values_come_from_values_only` (`src/convert.rs:832-851`) with a parse-level
  refusal test: deserializing a `RawParamSpec` fails with an error naming the unknown field `enum` for
  `type: enum` with `enum: [a, b]`, for `type: integer` with `default: 400` and `enum: [100, 400, 700]`,
  for `type: datetime` with `enum: ["2026-01-01"]`, and for `type: integer` with `enum:` written and
  left empty. Keep the surviving half of the deleted test: `type: enum` with `values: [a, b]` still
  builds `Enum { values: ["a", "b"] }`.
- [x] 1.2 Add a registry-level quarantine test in `src/templates.rs`, modelled on
  `unmigrated_multiline_text_template_is_quarantined_with_rename_error` (`src/templates.rs:5716`): a
  directory holding one valid template and one declaring a `type: integer` parameter carrying `enum:`.
  Assert the valid template is served, the other is not, `broken[0].path` names the offending file, and
  its error carries both `params.<name>` and `enum`. Use `type: integer` and not `type: enum`, because
  the current tree already quarantines the `enum` shape through the "enum values must not be empty"
  validation error, and a test that passes today does not distinguish the fix from the bug.
- [x] 1.3 Run `cargo test` and confirm both new tests fail against the otherwise unchanged tree, each on
  its own assertion rather than on an unrelated compile or setup error.

## 2. Remove the key

- [x] 2.1 Delete the `choices` field, with its `rename = "enum"` and its deserializer, from
  `RawParamSpec` (`src/raw.rs:87-92`), so `deny_unknown_fields` refuses `enum:` on a `params:` entry of
  every type.
- [x] 2.2 Delete the datetime `enum` guard (`src/convert.rs:542-547`), leaving the four remaining
  datetime guards (`min`, `max`, `multiline`, `values`) and the `format` guard ahead of the block
  (`src/convert.rs:504-509`) untouched with their pointed messages.
- [x] 2.3 Delete the final sentence of the comment at `src/convert.rs:578-581` describing `enum:`
  (`choices`), keeping the two sentences that explain why `.flatten()` is correct.

## 3. Gates

- [x] 3.1 Confirm `choices` appears nowhere under `src/`, and that no comment or test there describes
  `enum:` as a parameter attribute.
- [x] 3.2 Confirm no file under `src/`, `ui/src/`, `catalog/` or `tests/fixtures/templates/` needs a
  further edit: no template YAML in the repository carries `enum:` on a parameter, and `ParamInput.tsx`
  already renders the stepper the post-change `integer` row specifies.
- [x] 3.3 Run `cargo fmt`, `cargo clippy --all-targets --all-features` and `cargo test`, and fix what
  they report.
