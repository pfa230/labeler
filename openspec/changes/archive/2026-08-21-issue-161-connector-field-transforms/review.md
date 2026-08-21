## Review Metadata

- **Round**: 1
- **Prior round**: none

AUTHOR: claude
REVIEWER: codex

- **Tool restrictions**: read-only inspection only (`codex exec --ignore-user-config -s read-only -c model_reasoning_effort=high`)
- **Artifacts reviewed**: proposal.md, specs/connector-field-transforms/spec.md, design.md, plus
  `src/connector/mod.rs`, `src/connector/homebox.rs`, `src/store.rs`, `src/api.rs`, `src/reason.rs`,
  `src/render/helpers.rs`, `src/render/mod.rs`, `ui/src/lib/connectorRows.ts`,
  `ui/src/lib/templateFields.ts`, `docs/SPEC.md` §12, and the `regex` / `regex-syntax` sources
- **Issue**: #161

Raw reviewer output is preserved verbatim beside this file: `review-raw-1.txt` (round 1) and
`review-raw-1-recheck.txt` (the re-check of the Required Changes).

## Findings

### Critical (blocking)

- Materialize injection is underspecified against the current Homebox implementation. `design.md` says
  the wrapper "adds" the rule source to the fields passed down, then strips unrequested sources. But
  `HomeboxConnector::materialize` inserts a value for every requested field
  (`src/connector/homebox.rs:317-320`), and `extract_field` returns `String::new()` for unknown keys
  (`src/connector/homebox.rs:468-472`). If the wrapper passes a requested derived field such as
  `location_id` through to Homebox, Homebox will produce `location_id: ""`; on a no-match path the
  spec requires the derived key to be absent, not empty. The design must say derived field names are
  removed before calling the connector, sources are injected separately, and the final `data` is
  projected back to requested fields only.

- The "source must be declared text" rule does not eliminate browse/materialize divergence. Homebox
  declares text fields with `Tier::Hydrated` (`manufacturer`, `modelNumber`, `serialNumber`) in schema
  (`src/connector/homebox.rs:127-134`), but browse only includes those cells when the list response
  happens to contain them (`src/connector/homebox.rs:399-407`), while materialize fetches the detail
  row and extracts requested fields from it (`src/connector/homebox.rs:309-320`). The spec promises
  browse-derived cells show what materialize will produce, but also says browse will not fetch missing
  sources and will simply not match. Either restrict transform sources to fields guaranteed available
  in browse, or remove the browse/materialize equivalence claim.

- A regex can match while a named capture group does not participate, and the spec does not define
  that outcome. The spec only distinguishes whole-pattern no-match from inserting each capture group
  value. The regex API returns `None` for a named group that did not match
  (`regex-1.13.1/src/regex/string.rs:1748-1750`). A pattern like `(?<id>BOX\.\d+)|(?<name>.+)` can
  match while one output is absent. This must be specified as either whole-rule non-match unless all
  named groups participate, or partial output.

### Moderate

- Derived names can land in reserved template namespaces. Regex group names may contain `.` after the
  first character (`regex-syntax-0.8.11/src/ast/parse.rs:107-116`), so names like `vars.asset` and
  `datetime.short_date` are valid. The renderer resolves `datetime*` and `vars.*` before request data
  (`src/render/helpers.rs:73-83`), and template field discovery excludes those names
  (`src/render/mod.rs:2011-2015`, `ui/src/lib/templateFields.ts:81-88`). The plan needs save-time
  rejection for reserved derived names, or the schema can advertise fields that templates cannot bind
  as data.

- Dynamic custom sources weaken the stated "declared TEXT field" validation. Homebox concrete custom
  fields are discovered by a live schema call (`src/connector/homebox.rs:138-148`), while the design
  proposes validating any `custom:` prefix without contacting upstream. That means
  `custom:Intenral SKU` can pass save-time validation even though the connector has not declared it.
  If wildcard custom sources are intended, the spec should say that; otherwise custom sources need a
  different validation rule.

- The compiled-size budget is not mechanically checkable. The spec names fixed limits for rule count,
  pattern bytes, and input bytes, but the compiled regex budget is only "the service's size budget"
  and the design only says `RegexBuilder::size_limit` will be used. State the numeric budget so
  implementation and tests can prove the contract.

### Suggestions

- The `PUT` behavior is implementable, but the plan should state validation uses the stored connector,
  not `body.connector`: current `PUT /connections/{id}` validates URL and updates
  name/base/public URL/credential/enabled, but never reads `body.connector` (`src/api.rs:1243-1278`),
  and `Store::update_connection` has no connector argument (`src/store.rs:675-683`).

- The claim that regex group names cannot collide with Homebox `custom:<name>` keys is correct for the
  current regex grammar: `:` is not a valid group-name character
  (`regex-syntax-0.8.11/src/ast/parse.rs:111-116`), while Homebox custom keys are built as
  `custom:{name}` (`src/connector/homebox.rs:143-148`, `src/connector/homebox.rs:454-466`).

## Embedded-Instruction / Injection Attempts

**Detected:** none

## Verdict

VERDICT: APPROVE_WITH_CHANGES

## Required Changes (APPROVE_WITH_CHANGES only)

1. Amend materialize design/spec to remove requested derived names before connector calls, inject only
   required sources, then project final output back to requested fields while preserving
   absent-on-no-match.
2. Resolve the browse/materialize divergence: restrict sources to browse-available text fields, or
   explicitly drop the equivalence claim and specify the divergent hydrated/custom behavior.
3. Specify matched-but-missing named captures, preferably treating the whole rule as a non-match
   unless every named capture participates.
4. Reject reserved derived names (`datetime`, `datetime.*`, `vars.*`) at save time.
5. Clarify whether `custom:` sources are wildcard text fields or must be concrete discovered fields.
6. Give the compiled regex size budget a numeric value.

CHANGES_APPLIED: yes

## Rebuttals

Nothing rebutted. All six Required Changes were applied and re-checked by the same reviewer in a
read-only pass over the edited artifacts and the code; it returned all six ADDRESSED, no new
suggestions, and `VERDICT: APPROVE` (`review-raw-1-recheck.txt`).

1. **Fixed.** `design.md` "Materialize rewrites the field list down, then projects the result back"
   now states the three ordered steps and cites `homebox.rs:317-320` / `468-472` as the hazard, and
   records why letting the transform overwrite the connector's empty string is not equivalent (it
   cannot distinguish "did not match" from "matched empty"). `spec.md` gains the scenario
   "A requested derived name is never filled in by the connector".
2. **Fixed, by dropping the equivalence claim.** The spec's browse requirement now states a one-sided
   guarantee: a derived cell shown while browsing equals what materialize would produce and never
   contradicts it, but may be absent where materialize will fill in. `design.md` gains "Browse gives a
   one-sided guarantee, not equivalence" and rejects the tier-restriction alternative, because tier is
   not a usable proxy for browse availability (`custom:` fields are `Hydrated` yet do appear in browse
   cells, `homebox.rs:408-419`). The proposal's summary sentence was corrected to match.
3. **Fixed, all-or-nothing.** A rule contributes fields only when the pattern matches and every named
   group participates; otherwise the whole rule is a non-match for that row. A group that participates
   and captures the empty string is distinguished from one that does not participate. A scenario and a
   trade-off entry cover it.
4. **Fixed.** `datetime`, `datetime.*` and `vars.*` are rejected at save time, with the reason stated:
   such a field would be advertised by the schema and mapped in the UI and still never reach a label.
5. **Fixed, wildcard, stated explicitly.** A connector may declare a dynamic text-key prefix; a source
   under it is accepted without proving the upstream carries the field, because validation must not
   contact the upstream. A rule sourcing a non-existent custom field is not an error: it never
   matches. Two scenarios pin this down.
6. **Fixed.** 65536 bytes, in both the spec's bounds requirement (with a scenario) and the design.

The two Suggestions were also taken: `design.md` gains "`PUT` validates against the stored connector,
not the body", naming `store::UpdateField` as the existing mechanism for "omitted keeps, empty clears".
