## Context

See `proposal.md` for motivation and `specs/template-groups/spec.md` for the contract. What shapes
the approach here:

- Templates are files. The registry loads `{config}/templates/*.yaml` one level deep
  (`src/templates.rs:95`), keys on the `id` inside each file, and remembers which file each id came
  from (`src/templates.rs:190`). `POST /api/templates` writes `{id}.yaml`; every other write resolves
  the path through the registry rather than guessing at a filename (`src/api.rs:332`).
- Parsing is two-stage and rejects unknown fields: YAML → `TemplateDefinitionRaw` (`src/raw.rs:104`)
  → `TemplateDefinition` via `TryFrom` (`src/convert.rs:265`), then `validate()`
  (`src/templates.rs:306`) returns a human message. A file that fails either is quarantined, not
  fatal (`openspec/specs/template-registry/spec.md`).
- ADR-0006 forbids a GUI from rewriting a hand-authored template, because a YAML parse-and-re-emit
  round trip destroys comments and normalizes key order. That ADR is the reason the editor is a raw
  source textarea today (`ui/src/pages/TemplateDetail.tsx`).
- The Labels page loads the full template list once and derives Favorites and Recents from it
  (`ui/src/pages/Templates.tsx:107`), so it already holds every summary in memory.

## Goals / Non-Goals

**Goals:**

- Group assignment is a one-click action from the Labels page, on one card or on many, and never
  requires typing YAML.
- A move changes exactly one line of the file and leaves comments, key order, and formatting intact.
- The group survives export, backup, catalog install, and copying a file to another machine.
- No grouping fault reaches further than the file that carries it. An invalid hand-authored `group:`
  quarantines that one template, exactly as any other validation failure does, and never aborts
  startup or touches another template; and the move endpoint cannot create such a file, because it
  validates before it writes.

**Non-Goals:**

- Hierarchy. `Shipping/Pallets` is a name, not a path (decision 2).
- Group entities. There is no create-group or delete-group operation and no place a group is stored
  apart from the templates naming it, so an empty group cannot exist.
- Renaming a group across every template that uses it, and drag-and-drop onto a filter chip. Neither
  is part of #164, and neither is queued: each gets its own issue if and when it is wanted.
- Directory reorganization. `{config}/templates/` stays one flat level.

## Decisions

### 1. The group is a YAML field on the template, not a directory and not a store row

Alternatives were directory mapping (`templates/<group>/x.yaml`) and a row in the SQLite store
beside favorites.

Directory mapping loses on four counts, all in code that exists today. The loader walks one level
(`src/templates.rs:95`) and every downstream identity is a basename, including `BrokenTemplateSummary.filename`
(`src/models.rs:36`) and the duplicate-id tie-break just frozen in `template-registry` ("the one whose
filename sorts first in byte order"); recursion turns all of those into relative paths and reopens a
requirement written days ago. It puts a caller-supplied string on the write path, where
`template_file_path` currently derives the filename from an id restricted to `[A-Za-z0-9_-]`
(`src/api.rs:310`), so a group segment would need its own traversal, separator, and case-collision
guards on an endpoint that writes files. It turns a regroup into mkdir plus cross-directory rename
plus pruning the emptied directory, with a half-failed state the current single atomic rename does
not have. And it puts the group outside the artifact, so a catalog install (`ui/src/pages/Catalog.tsx:127`
POSTs the YAML body) arrives ungrouped.

Store metadata fails the portability test the same way, and additionally splits template truth across
a file and a database, which is what ADR-0006 exists to prevent.

The YAML field costs one field in each of the three parse-path files and nothing else. Its one real
loss: reorganizing `/config/templates` by hand with `mv` does not group anything, because the
directory carries no meaning. Accepted knowingly.

Recorded as **ADR-0061, "A template's group is a YAML field, not its directory"**.

### 2. One flat level

`group` is a single name. The service does not split on `/` or any other character, so
`Electronics / Cables` is one group whose name happens to contain a slash.

Hierarchy was rejected for this change, not forever: it costs a tree control with expansion state in
the Labels view, a decision about whether selecting a parent includes its children, per-segment
validation, and a depth cap, for a set that is currently a few dozen templates. Widening later is a
pure loosening of `validate_group_name`, and no existing file or response has to change, so the
cheap option here does not become the wrong one later. Same ADR-0061.

### 3. Validation lives in one function, shared by the parse path and the move endpoint

`validate_group_name(&str) -> Result<String, String>` returns the whitespace-stripped name or a
message naming `group`. It is called from `TemplateDefinition::validate()` (so a bad group quarantines
the file like any other validation failure) and from the move handler before it patches anything (so a
bad name is a `422` with nothing written). One definition, so the file path and the API path cannot
drift.

The domain model stores the stripped value: `TemplateDefinition.group: Option<String>`, `raw.rs` gains
`group: Option<String>` with `#[serde(default)]`, and `convert.rs` strips on the way through. A
non-string value fails at deserialization with `serde_path_to_error` naming `group`, which is why the
spec says "parse or validation message" rather than naming one.

Bounds: non-empty after stripping, at most 64 characters, no control characters. 64 is a display
bound, not a storage one: these names sit in a filter row and on cards.

Case is significant, so `Warehouse` and `warehouse` are two groups. Case-folding would be wrong for
`3M` and `pH`, and cross-file collision detection would need a registry-wide rule for something that
is only a naming preference. The mitigation is in the UI: the move dialog offers the names already in
use, so the normal path picks an existing name instead of retyping it.

### 4. Move is `PUT /api/templates/{id}/group`, a targeted single-line file patch

The endpoint replaces one subresource and is idempotent, which is `PUT`, not `PATCH`.

The patch is textual, never a parse-and-re-emit, which is what keeps it inside ADR-0006's intent
rather than against it. Under `state.write_lock`, with the path resolved through the registry
(`existing_template_file`), the handler:

1. reads the stored file;
2. refuses a file holding more than one YAML document (`422`), since "the top-level `group`" is
   ambiguous there, and likewise a file whose root is not a block mapping written one key per line.
   A top-level flow mapping parses fine and has no line to patch or to insert, so reflowing it into
   block style would be exactly the whole-file rewrite this design exists to avoid;
3. finds a top-level `group:` line: a line whose first character is at column 0, that is not a
   comment, and whose key is exactly `group`. Column 0 is what makes a nested `group:` key
   (inside `params`, or a layout item) unreachable, since nested keys are indented;
4. cross-checks that scan against the parsed template. The parser accepts spellings the scan does not
   recognize, `"group": Shipping` among them, so when the parsed template has a group and the scan
   found no line to replace, or found more than one, the handler refuses with `422` instead of
   inserting. Without this check step 7 would append a second top-level `group` key;
5. if the matched line's value is not a scalar it can replace in place (a block scalar `|`/`>`, a
   flow collection, an anchor or alias, or an empty value introducing a nested block), refuses with
   `422` rather than guessing. Plain and quoted scalars are both replaceable, which matters because
   `group: "Shipping"` is a valid and expected spelling;
6. builds the new line as `group: ` plus the name emitted through the YAML serializer, so a name
   needing quotes gets them, and re-attaches any trailing comment the old line carried. Finding that
   comment means skipping a quoted scalar before looking for ` #`, so `group: "A # B"  # keep me`
   keeps both the `#` inside the value and the comment after it; that case is a test, not a comment
   in the code;
7. for a clear, deletes the whole line and its terminator; for an insert, places the new line
   immediately after the top-level `name:` line, falling back to after `id:`, then to the top of the
   document body after any leading comments and `---` marker;
8. parses and validates the patched text, and asserts the group reads back as requested. Only then
   does it write, through the same atomic temp-then-rename as every other template write, and reload
   the registry.

Step 8 is the safety net that makes the textual patch acceptable: a patch that would produce a file
the service cannot load never reaches the disk. Step 4 is the one that keeps a *loadable* file from
being wrong: a duplicated key can round-trip through a parser that takes the last value, so the
scan-versus-parse disagreement has to be caught before the write, not after it. Line endings are preserved by splitting on line
terminators and keeping each line's own ending, so a CRLF file stays CRLF.

Recorded as **ADR-0062, "The service may rewrite one key of a hand-authored template"**, which
qualifies ADR-0006: a targeted single-key patch that preserves every other byte is not the lossy
round trip that ADR forbids, and the guarantee is enforced by tests that assert byte equality outside
the patched line.

New reasons in `src/reason.rs`: `template_group_invalid` (`422`, a name that fails validation) and
`template_group_unpatchable` (`422`, a file the patcher will not touch). Existing reasons cover the
rest: `template_id_invalid`, `template_write_failed`, `template_registry_io`.

### 5. The server filters, the UI does not have to

`GET /api/templates?group=` exists for API clients, and its three states (absent, a name, empty) are
distinguishable in axum as `Option<String>` from `Query`, so "all" and "ungrouped" do not collide.

The Labels page keeps fetching the unfiltered list and filters in the browser, because it needs the
full set anyway to resolve Favorites and Recents (`ui/src/pages/Templates.tsx:107`) and to build the
filter row itself. Refetching per filter selection would add a round trip and make the group list
depend on the current filter. The endpoint parameter and the UI's client-side filter are specified to
mean the same thing, and the API tests are what hold them together.

### 6. UI shape

- A filter row above the grid: `All`, each group in use in ascending Unicode code-point order,
  `Ungrouped` only while something is ungrouped. Code-point order is what `summaries()` already
  applies to ids (`src/templates.rs:204`, Rust's `str` comparison is UTF-8 byte order, which is
  code-point order). JavaScript's `<` is not that: it compares UTF-16 code units, so a group name
  outside the Basic Multilingual Plane, an emoji being the realistic case, sorts before a name in the
  `U+E000`-`U+FFFF` range while the server would put it after. The comparator therefore compares
  code-point sequences (`Array.from(name)`), not the raw strings, and `localeCompare` is wrong for a
  different reason: its order depends on the viewer's locale. Selection is component state, composed with the existing search box by
  `useMemo`, so both narrow at once. A selection other than `All` also hides the Favorites and
  Recents rows, reusing the `searching` condition already in `ui/src/pages/Templates.tsx:137`: those
  rows are drawn from the whole set, and leaving them on screen under a group filter would show
  cards from the groups the user just filtered out.
- Each card shows its group next to the format badge, and gains a **Move to…** item.
- The move dialog is a text input over a `<datalist>` of the groups in use, plus a "Make ungrouped"
  action. Typing an unused name and confirming creates that group by moving a template into it.
- Multi-select: a checkbox per card and a selection bar reading "N selected · Move to…". A bulk move
  issues one request per template with `Promise.allSettled`, then reports successes and failures
  per template, so a partial failure is visible rather than silent.
- One `useMoveTemplateGroup` mutation invalidating the `templates` query, so the grid and filter row
  both refresh from the server's answer rather than from optimistic local state.

## Risks / Trade-offs

- **The patcher corrupts a hand-authored file.** → It never writes text the service cannot parse and
  validate (decision 4, step 8), the write stays atomic, and the tests assert byte equality outside the patched
  line across the whole `catalog/` tree, which is the real corpus of comment-carrying templates.
- **A move silently overwrites a source edit open in another tab.** The editor PUTs the whole file
  with no version check, so a stale draft saved after a move reverts the group. This hazard exists
  today between two editors; grouping widens it to a second writer. Documented here, and optimistic
  concurrency on template writes is a separate issue rather than a rider on this one.
- **Rollback quarantines grouped templates.** `deny_unknown_fields` means a binary from before this
  change rejects a file carrying `group:`. Per `template-registry` those files are quarantined and
  reported as broken, not fatal, so an older binary starts and serves everything else. Recovery is to
  roll forward, or to delete the `group:` lines. Stated in ADR-0061's consequences.
- **Group names drift by case and spelling.** Only the dialog's list of existing names pushes against
  it. Deliberate: enforcement would mean a registry-wide uniqueness rule for a naming preference.
- **Bulk move is N requests.** At a few dozen templates that is fine; a bulk endpoint is not worth its
  contract until the count justifies it.

## Migration Plan

No data migration. Every existing template, request, and response stays valid, and `group` is omitted
from responses when unset, so no consumer sees a shape change. Deployment is the normal image roll
(ADR-0016); rollback is the previous image, with the caveat above.

## Open Questions

None. The three decisions that would have changed the specs or the task breakdown (storage model,
flat versus hierarchical, who rewrites the file) were settled before this document was written.

## ADR numbering

ADR-0061 and ADR-0062, assigned at merge time. When this was planned the next free numbers looked
like 0062 and 0063, because #161, #169 and #180 had each claimed one of 0059 through 0061 in their
own worktrees. By the time this merged, #180 had taken 0059 and #161 had taken 0060, and 0061 was
free again, so the pair moved down one to keep the sequence dense. That is the rule working, not a
correction: the number is not real until the change lands.
