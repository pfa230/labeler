## Context

See `proposal.md` — Why. Two dead spellings survive in `src/raw.rs` and are converted away in
`src/convert.rs`; the domain model, the API and every YAML in the repository already speak only
`params:` and `when:`.

Four facts about the current tree shape the approach, and all four were checked rather than
assumed:

1. **No test asserts the desugaring.** The issue expects to find some and says to delete them. A
   search of every `.rs` file for a YAML key `options:` or `option:` at a line start, and for the
   same keys escaped inside string literals, finds only function parameters named `options` (the
   derived `Options` view, `src/templates.rs:1876`) and the renderer's `option:` argument
   (`src/render/mod.rs`). No test builds a template through either deleted field. Nothing is deleted
   from the test suite; the four tests described under Decisions are added.
2. **No YAML in the repository writes either spelling.** `catalog/`, `tests/fixtures/templates/` and
   the docs are clean, and `ui/src` never sends a template `options` field.
3. **`deny_unknown_fields` really does fire on `ContainerRaw`**, despite its `#[serde(flatten)]
   placement`. Serde's own documentation warns that the two attributes do not combine, so this was
   verified rather than trusted: a standalone probe reproducing `TemplateDefinitionRaw`,
   `LayoutItemRaw` and `ContainerRaw` (same serde, `serde_yaml_ng` 0.10 and `serde_path_to_error`
   versions as `Cargo.toml`) reports, for a container carrying `option:`,
   `path="layout[0]"`, `msg="layout[0]: layout: unknown field \`option\` at line 3 column 3"`, and for
   a top-level `options:`, `path="options"`, ``msg="options: unknown field `options`, expected ..."``.
   An empty `option: {}` gives the identical error, so the key is refused before its contents are
   read. The existing `legacy_frame.yaml` quarantine test (`src/templates.rs:2670`) is the same
   mechanism observed on `main`.
4. **Loading a file is not the only door into that parser.** `PUT /api/templates/{id}`
   (`src/api.rs:743`, the only template write route — there is no `POST /api/templates`, and a create
   is the same `PUT` under `If-None-Match: *`) runs its raw YAML body through `parse_and_validate`
   (`src/api.rs:771` → `:639`) before it takes the write lock or opens a directory handle. A parse
   failure there becomes `AppError::template_invalid(Reason::TemplateParseFailed, ...)`, which is
   `422` with `error.code` `TemplateInvalid` (`src/errors.rs:23`, `:270-277`) and
   `error.details.reason` `template_parse_failed` (`src/reason.rs:33`), carrying the same
   `serde_path_to_error` message the loader would have quarantined the file with. Deleting the two
   fields therefore narrows the set of accepted HTTP request bodies, which is externally observable
   behaviour that registry-layer tests would not pin.

## Goals / Non-Goals

**Goals:**

- Delete both fields and both conversion arms, so that the only spelling the loader accepts is the
  only spelling the model has.
- Leave the refusal to `deny_unknown_fields`, adding no bespoke error, no reason code and no branch.
- Add the four tests that hold the refusals in place, since a gate that stops firing looks exactly
  like a gate that passes.

**Non-Goals:**

- The renderer's internal option-selection plumbing, the request-side `option` map (#214), and CSV
  `option.<name>` columns. `proposal.md` — What Changes lists each and why it stays.
- Any change to `docs/SPEC.md`, which is frozen and which never documented either deleted spelling:
  its §3 top-level table has no `options` row and its §4.1 `container` bullet names only `when`.
  Deleting the fields makes the frozen text true where it was silently incomplete.
- Any change to the API's *shape*: no route, handler, OpenAPI model, status code, error code or
  `details.reason` slug moves. The write path's behaviour changes only in which bodies it accepts,
  and it reports the refusal through the envelope it already has.

## Decisions

**The refusal is `deny_unknown_fields`, not a named error.** Removing the field is the entire
implementation. The alternative — keeping the field and rejecting it in `convert.rs` with a message
explaining what to write instead — is the "second spelling" `AGENTS.md` forbids: it keeps a dead key
in the wire format, adds a code path nothing else needs, and has to be deleted again later. The probe
above shows the generic message already names the key and the item, which is what an operator needs
to fix the file.

**The two spellings land in two different capabilities, because they are two different contracts.**
`options:` is a way to declare a typed input, so it belongs to the top-level template field table —
which lives, post-#227, in `template-groups`'s *A group is a directory under the templates directory*
requirement. That requirement already exists in `openspec/specs/`, so the delta is a `MODIFIED`
carrying the whole requirement with the row struck, as the first-touch rule requires (a `MODIFIED`
is only valid against a requirement that already exists, and this one does). `option:` is a way to
gate an item's visibility, and nothing in `openspec/specs/` owns that subject, so it becomes a new
`conditional-visibility` capability. Folding the second into `template-registry` was considered and
rejected: registry requirements are about ids, files, writes and quarantine mechanics, not about
which keys a layout item accepts, and a reader looking for "how do I gate an item?" would not look
there.

**`conditional-visibility` supersedes `docs/SPEC.md` §5 only insofar as §5 names the key.** The
first-touch rule asks for the complete post-change contract rather than a difference, and the
temptation was to restate all of §5. That would have been wrong: §5's evaluation semantics are
already partly superseded by requirements that landed since it froze. §5 bullet 2 says required-
parameter validation is *lazy*, so a parameter read only inside an inactive branch may be omitted;
`param-resolution` and `template-inputs` now state the opposite ordering — "a render resolves every
declared parameter before it evaluates any `when:`"
(`openspec/specs/template-inputs/spec.md:237`, `:744`, `:814`). Restating §5 wholesale would have
re-legislated a settled contract from a stale source and contradicted two live capabilities. The
requirement therefore fixes its subject at *which key spells a condition*, states the complete
contract for that subject, and points at the capabilities that own evaluation. This is the same
scoping #291 used for `text-ink` against §4.1.

**No `REMOVED Requirements` section.** Nothing in `openspec/specs/` states a requirement that either
spelling is accepted; the only trace is one row in a table, and a struck row is a `MODIFIED`, not a
removal. Inventing a requirement in order to remove it would put a claim into the archived record
that was never true of the specs.

**The write-path refusal is stated in each capability's own requirement, not once in
`template-registry`.** Both delta requirements now say what a `PUT` body carrying their deleted
spelling does: `422`, `TemplateInvalid`, `template_parse_failed`, a message naming the key, and
nothing written. Stating it once in `template-registry` instead was rejected because that
capability's *A `422` from a template write means nothing was written* requirement is about the
families of `422` a write can raise and their common guarantee, not about which keys parse; it
already covers "the submitted body fails parsing", and what this change alters is the membership of
that set. A reader asking "what happens if I `PUT` a template with `options:`?" looks up `options`,
so the answer belongs where `options` is specified. The two requirements consequently cite the
registry requirement rather than restating its guarantee.

**The tests assert the quarantine and the envelope, not the serde message verbatim.** Each new test writes a template
carrying one deleted key into a temp directory, loads it, and asserts the file is reported broken
with an error naming the key — the shape `legacy_frame.yaml` already uses, at the layer where an
operator meets the failure. Asserting serde's exact sentence would bind the suite to a dependency's
wording; asserting only that parsing returns `Err` would pass against a template broken for any other
reason.

Two more cover the write path, one per spelling, as HTTP tests against the router rather than unit
tests a layer below it: each `PUT`s a body carrying its deleted key and asserts the whole envelope —
status `422`, `error.code` `TemplateInvalid`, `error.details.reason` `template_parse_failed`, and a
message naming the key — together with the filesystem being untouched, meaning the pre-existing file
at that id still holds its original bytes and a create-only `PUT` left no new file behind. Asserting
the status alone would pass against any other `422` the handler can raise, of which there are
several, and asserting the parse without the write would not show that nothing was written.

All four tests must be seen to fail before the deletion and pass after it, since a test whose subject
is a refusal is the kind that most easily cannot fail.

## Risks / Trade-offs

- **A user's template outside this repository carries a deleted spelling and stops loading.** →
  Intended, and it is the point of the change: until 1.0 a behaviour change breaks what came before.
  The failure is loud and local. The template is quarantined with its filename and the offending key,
  every other template still loads, and the server still starts, so the blast radius is one file and
  the fix is a one-line edit to it.
- **`deny_unknown_fields` silently stops firing on `ContainerRaw`** — a future refactor touching the
  flattened `placement`, or a serde upgrade, could reopen the key without anyone noticing, because a
  gate that stops firing looks like a gate that passes. → The added container test is exactly that
  alarm, and it asserts the quarantine rather than the parse result so it keeps meaning something if
  the error path moves.
- **`TemplateContent::options()` reads like the field being deleted.** → It is a derived view over
  the `enum` entries of `params` and is named in `proposal.md` and in the issue as out of scope.
  Deleting it would break `validate()` (`src/templates.rs:1110`) and the preview-only option
  selection, which `param-resolution` specifies as live behaviour.
