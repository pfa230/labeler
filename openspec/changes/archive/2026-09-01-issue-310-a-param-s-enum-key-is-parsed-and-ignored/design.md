## Context

See `proposal.md` — Why. The behavior to remove is small and lives in three places: the `choices` field
on `RawParamSpec` (`src/raw.rs:87-92`), the datetime guard reading it (`src/convert.rs:542-547`), and two
assertions pinning the ignoring (`src/convert.rs:838-850`). The single constraint that shapes the whole
approach is that `RawParamSpec` carries `deny_unknown_fields` (`src/raw.rs:73`): deleting the field is
not merely a cleanup, it *is* the refusal, because serde then rejects the key during deserialization,
before `ParamSpec::try_from` ever runs.

Ordering matters for the same reason. `parse.rs` deserializes through `serde_path_to_error`
(`src/parse.rs:7-9`), and the registry quarantines a file whose parse fails while the server still starts
(`template-registry`). So the post-change error is produced one layer earlier than today's, by serde
rather than by a hand-written guard, and reaches the operator through the existing quarantine path with
no new plumbing.

## Goals / Non-Goals

**Goals:**

- One outcome for `enum:` on a parameter of any type: an unknown-key parse error, quarantining the file.
- Delete every line that exists only to keep the key parseable, so nothing is left describing it.
- Tests that fail against the current tree, for the reason under test.

**Non-Goals:**

- Any constrained-integer or integer-dropdown capability, now or as a follow-up.
- Any change to `options:`, which desugars to an `Enum` params entry through its own path
  (`src/convert.rs:628-637`) and never touched `choices`.
- Any change to `ui/src/`. The post-change `integer` control is the stepper `ParamInput.tsx:110-118`
  already renders, and `template-inputs` already maps `integer` to the `integer` control with no `enum`
  branch (`openspec/specs/template-inputs/spec.md:44`).

## Decisions

**Delete the field rather than read it.** The alternative is to implement what the table promised: an
`enum:` list on `integer` constraining accepted values and driving a dropdown. Rejected because the issue
drops that capability outright, and an `enum` parameter with string values already covers the picker
case. A third option — keep parsing and log a warning — is rejected by the project's no-silent-fallback
rule: a warning is still a second spelling that does not do what its author wrote, and it leaves the
misleading "enum values must not be empty" in place for `type: enum`.

**Take the generic unknown-key message, not a pointed one.** A pointed "enum is not a parameter
attribute" message is only producible by *keeping* the field so something can inspect it, which
reinstates exactly what this change removes. The generic message also says the right thing: the key
belongs to no type's schema, where a type-specific message implies it is valid on some other type. The
message still identifies the parameter, because `serde_path_to_error` attaches the JSON path
(`params.<name>.enum`) and the registry records the filename alongside it.

**Delete the datetime guard rather than leave it.** Once `choices` is gone the guard cannot compile, let
alone run: it reads `raw.choices`. It is removed, not rewritten against a different signal, and the
datetime case falls to the same unknown-key refusal as every other type. The four remaining guards in
that block (`min`, `max`, `multiline`, `values`) are untouched and keep their pointed messages, because
each of those keys is valid on some other type and so can only be refused after parsing. The `format`
guard ahead of the block (`src/convert.rs:504-509`) is untouched too: it is refused on every type, but
its message tells the author where the format does belong, which is a real instruction and not an
implication that the key is valid elsewhere.

**Trim the comment, not the paragraph around it.** Only the final sentence of the comment at
`src/convert.rs:578-581` describes `enum:`. The two sentences before it explain why `.flatten()` is
correct and stay.

**Two tests, at two layers, chosen so each fails today for the reason under test.**

- A parse-level refusal test in `src/convert.rs` replacing `enum_values_come_from_values_only`. It
  asserts that deserializing a `RawParamSpec` from `type: enum` + `enum: [a, b]`, from `type: integer` +
  `enum: [100, 400, 700]`, from `type: datetime` + `enum:`, and from `enum:` written empty, all fail with
  an error naming the unknown field `enum`. Today every one of those deserializes.
- A registry-level quarantine test in `src/templates.rs`, modelled on
  `unmigrated_multiline_text_template_is_quarantined_with_rename_error` (`src/templates.rs:5716`): a
  directory holding one valid template and one carrying `enum:` on a parameter, asserting the valid one
  is served, the other is quarantined, `broken[0].path` names the file, and the error carries both
  `params.<name>` and `enum`. The parse-level test cannot make that claim: `try_build_param`
  (`src/convert.rs:744-748`) deserializes without `serde_path_to_error` and knows no filename, and the
  acceptance criterion names both the file and the key.

  **This test uses `type: integer`, deliberately.** With `type: enum` it would pass against the current
  tree for the wrong reason — today that template is *also* quarantined, by the "enum values must not be
  empty" validation error — so the assertion would not distinguish the fix from the bug. With
  `type: integer` the current tree loads the template successfully and the test fails red before the
  change.

## Risks / Trade-offs

- **A deployed template carrying `enum:` stops loading.** → Intended, and the project's stated rule until
  1.0: the dropped spelling becomes a parse error naming the file and the key, and the registry
  quarantines that one file while the server and every other template still start. No template under
  `catalog/` or `tests/fixtures/templates/` carries the key, so nothing in the repository is affected.
- **The error is less specific than today's datetime message.** → Accepted, and argued above: specificity
  here would assert the key is valid elsewhere, which is the false belief that produced the issue.
- **A reader later wonders what `enum:` used to do.** → The record is this change folder, kept
  permanently under `openspec/changes/archive/`. Nothing is added to `AGENTS.md` or `docs/AUTHORING.md`,
  neither of which documents the key today.

## Migration Plan

None, and none is permitted: the breaking-changes rule bars a migration, a desugaring, a deprecation
window and a second spelling until 1.0. The exception in that rule covers stored user data
(`src/store.rs`), which this change does not touch — templates are files an author owns and the parse
error names for them. Rollback is reverting the commit.
