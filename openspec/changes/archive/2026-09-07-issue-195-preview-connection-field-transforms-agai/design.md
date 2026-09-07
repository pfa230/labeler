## Context

See `proposal.md` for motivation and the delta specs for the contract. What shapes the approach:

- Transforms already compile and apply through `CompiledTransforms` (`src/connector/mod.rs:103-158`),
  and `Connectors::browse` (`src/connector/mod.rs:521-538`) applies the **stored** rules to every
  browsed row before the handler sees the page.
- `validate_transforms` (`src/connector/mod.rs:161-274`) is the single save-time gate; `POST`/`PUT`
  call it through `Connectors::validate_transforms` and both turn its `(index, message)` into
  `AppError::invalid_request(Reason::ConnectionTransformInvalid, format!("rule {idx}: {msg}"))`
  (`src/api.rs:1960-1965`, `src/api.rs:2079-2085`).
- The connector schema is fetched, not static: Homebox discovers per-item custom fields at runtime and
  publishes them as `custom:<name>` text columns (`src/connector/homebox.rs:252-263`), swallowing a
  discovery failure with `unwrap_or_default()`, and `Connectors::schema` appends a `derived`-tier
  column per capture group of the stored rules (`src/connector/mod.rs:494-519`). So a schema response
  can be short without saying so, and `tier` mixes connector-derived columns with transform-derived
  ones.
- The UI form is one component for create and edit (`ui/src/pages/connect/ConnectionsSection.tsx:21`),
  and it always sends `transforms`. On `PUT`, omitting the key keeps the stored rules and sending `[]`
  clears them, so "when does the form send the key" is a correctness question, not a style one.
- Existing endpoint-level tests for browse and materialize stand a `wiremock` upstream and an egress
  that permits loopback (`src/lib.rs:6965-7018` is the closest model).

## Goals / Non-Goals

**Goals:**

- One evaluation path shared by preview and browse, so "what the preview showed" and "what a save
  produces" cannot drift.
- One validation path shared by preview and save, including the rule indices in its messages.
- A source control that expresses exactly the set `validate_transforms` accepts, columns and
  prefixed names alike, so the two save-time faults it can prevent become unreachable and nothing it
  accepts is refused by a save.
- A preview response bounded by the contract, not by what the upstream happens to hold.
- A displayed preview that can only be the answer for the rules on screen.

**Non-Goals:**

- Changing what a transform means, when it is accepted, or where it is applied.
- Previewing an unsaved connection, previewing against a literal sample string, or paging a preview.
- Making a preview cheap: it costs one upstream page fetch per press, the same as a browse.

## Decisions

### One rule per request, every row reported

The response covers the one rule the request names, and carries an entry for every row it evaluated.
That is a deviation from the issue's request shape, and it is forced by two constraints that cannot
both hold otherwise.

The issue requires a row's source value, derived fields and match status **for each row**. It also
fixes `page_size` bounds at browse's, which is 200. A response covering every candidate rule on the
resource therefore multiplies 200 rows by as many as 32 rules, each row carrying a source value and one
value per capture group a 512-byte pattern can name, which is about 70. No hard bound holds that
without dropping rows, and dropping rows is precisely what the per-row requirement forbids. An earlier
draft tried a byte budget that stopped emitting row entries; that traded the accepted contract for a
number, and the number was wrong anyway, because it counted neither the row references nor the JSON
around them.

Reporting one rule removes the multiplication instead of papering over it. The response is then at most
one page of rows, each carrying one row reference exactly as a browse row does and at most 512 bytes
per reported value: kilobytes at the default page of 10, and at the 200-row maximum with a pathological
pattern a few megabytes, which is the order of the browse page it is drawn from. That is a bound
arithmetic can state, so the spec states it rather than claiming a round number.

It also matches the issue's own UI model, "preview is per rule, in the connection form", and it costs
nothing in fidelity: transforms are a flat pass and no rule may derive a name another rule could
source, so evaluating one rule alone gives the same answer as evaluating it inside the filtered list.
The whole candidate list is still **validated**, which is what makes ordering and collisions real.

The request therefore names the rule by index and carries no `resource`: the previewed resource is that
rule's own. Two fields that must agree is a field that can disagree with itself, and the repo has one
rule about a second spelling. `resource_unknown` disappears with it, since a rule naming a resource the
connector does not offer is already `connection_transform_invalid` from the shared validation, so this
change now publishes no new reason slug at all. An out-of-range `rule` index is `request_body_invalid`,
which the error-envelope capability already publishes as the mapping for a body rejected for a reason
it names no more specific one for.

### The spec names every key

The wire format is fixed in the delta rather than described, because only the synced spec is normative
and prose about "a matched count" leaves an implementer free to pick any spelling and any OpenAPI
shape. Every key, its type and whether it is always present is stated there: the request's
`{ transforms, rule, page_size? }`, the response's
`{ rule, resource, source, row_count, matched_count, rows: [ { id, source_value?, matched,
value_truncated, derived? } ] }`, and the schema's three additions, `FieldSpec.transform_source`,
`ResourceSpec.dynamic_source_prefix` and `ResourceSpec.fields_incomplete`.

Absence carries meaning in two places, and both are expressed by an omitted key rather than a
sentinel: `derived` is absent when `matched` is false and present-with-every-capture when it is true,
so a group capturing the empty string is `{"id": ""}` and a non-match is nothing at all; `source_value`
is absent when the row carries no text value for the source, which is the case browse creates when the
source is a field it does not return. `value_truncated` is the opposite, always present and boolean: a
reader must be able to tell "nothing was cut" from "this client does not know about cutting".

### Values are truncated; rows and fields are not

The remaining size lever is the source value itself, which a stored rule may read up to 8192 bytes of,
and the capture groups, which a 512-byte pattern can name about seventy of. Both are reported truncated
at 512 bytes with `value_truncated` set on the row entry.

Truncating the text rather than dropping the row or the key keeps every part of the accepted contract:
every row appears, every derived field appears, and matching runs against the whole value, so `matched`,
the set of keys in `derived`, `row_count` and `matched_count` are exactly a save's. Only what is
displayed is shortened, and the flag says so, which is the difference between a bound and a silent
fallback. Five hundred and twelve bytes is far more label field than anyone prints and far less than an
8 KiB description repeated down a page.

### One evaluation entry point, and `apply_to_cells` written in terms of it

`CompiledTransforms` gains a method that, for a resource and a row's cells, returns one outcome per
matching rule: the rule's index, the source value it read (if any) and its captures (if it matched).
`apply_to_cells` becomes a caller of it that inserts the captures, so the preview and the browse path
run the same code rather than two copies of the same three guards ("is the source a text cell", "does
the pattern match", "did every named group participate").

The alternative was for the preview handler to loop over `for_resource(resource)` and call
`CompiledTransform::apply` itself. That is the same matching function, but it restates the cell lookup
and the text-only guard, which is exactly where a preview would silently start disagreeing with browse
(a future connector cell type, say, that browse learns to read and the preview does not). Refactoring
`apply_to_cells` is behavior-preserving: the existing unit tests over it are the check.

`apply_to_map` (the materialize path, over `RowValue`) is left alone. It duplicates `apply_to_cells`
today; folding both is a wider refactor than this issue authorises.

### Rows come from the shipped browse call, stored rules and all

The preview calls `Connectors::browse`, which applies the connection's **stored** rules to the page
before returning it. The preview then evaluates the **candidate** rules against those cells. That is
safe rather than merely convenient, and provably: a derived name must match `^[a-zA-Z0-9_-]+$` and may
not equal a column the connector declares for that resource, while a legal `source` is either a
declared column or a key under the connector's dynamic prefix (`custom:`, which contains a character
no derived name may carry). So no stored derived cell can ever occupy the key a candidate rule reads.

The alternative was a browse path that skips the stored rules. It buys nothing the paragraph above
does not already guarantee, and it adds a second read path to a connector surface the issue puts out
of scope.

### `page_size` follows browse's bounds, and the row count is enforced here

An earlier draft capped preview at 50 rows. That was a bound the issue did not ask for, and the report
budget above already bounds the response whatever the row count is, so the cap bought nothing and cost
a deviation. Preview passes `page_size` into the same `BrowseRequest` browse takes and inherits its
clamp of `1..=200` (`src/connector/homebox.rs:321`), with 10 substituted for an absent value. `0`
clamping to `1` is that same clamp, now stated rather than left to be discovered.

What the connector does not do is hold itself to the number: it maps every item the upstream returned
(`src/connector/homebox.rs:370-374`), so an over-returning upstream would enlarge a preview that had
asked for ten rows. The preview handler therefore truncates the returned page to the effective size
before evaluating anything, and `row_count` is what it evaluated. Fixing the connector to enforce its
own page size instead would change browse's behaviour for every caller, which is a separate issue, not
a line to slip into this one.

### No new reason slug, because the request no longer carries a resource

An earlier draft added `resource_unknown` for a `resource` field the connector does not offer, which
mattered because Homebox's browse treats every resource that is not `locations` as `entities`
(`src/connector/homebox.rs:340`) and would have answered a typo with item rows and no rule. Taking the
resource from the named rule removes the field and the fault together: a rule naming a resource the
connector does not offer is refused by the shared validation with `connection_transform_invalid`, which
already names the rule.

The one new rejection is a `rule` index that names no candidate. That is `request_body_invalid`, which
the `request-error-envelope` mapping already publishes for a body rejected for a reason it names no
more specific one for. Minting a slug for it was rejected: the published catch-all exists precisely so
that every one-off body fault does not become public vocabulary.

### The whole candidate list is validated, and the consequence is stated

Preview validates every rule in the request, not just those on the previewed resource, because the
issue requires collisions and ordering to be the real ones. The consequence is that an unfinished rule
anywhere in the editor refuses a preview of any rule. The error names that rule's index and the UI
already parses `rule N:` messages (`ConnectionsSection.tsx:36-45`), so the refusal lands on the rule
that caused it. The alternative, validating only the previewed resource's rules, would let a preview
succeed on a list that cannot be saved, which is the blindness this change removes.

### Source eligibility is a schema mark, not a `tier` heuristic

An earlier draft had the picker exclude every `tier: derived` column and called that "exactly what
validation accepts". It is neither half of that. `validate_transforms` resolves a source against the
connector's static `ColumnDef` list and checks only `multi_valued` and `ty`
(`src/connector/mod.rs:196-205`), so Homebox's `item_url` (`homebox.rs:85-91`) and `location_url`
(`homebox.rs:116-121`) are `derived`-tier single-valued text and are sources a save **accepts**, while
a transform-derived column is also `derived`-tier and is a source a save **refuses**. `tier` cannot
separate them, because it answers a different question: where a value came from, not who may read it.

`FieldSpec` therefore gains a boolean saying whether a transform may source that column, set from the
same predicate the validation applies (`multi_valued == false && ty == Text` over the connector's own
columns) and set false on the derived columns `Connectors::schema` appends. The UI reads the mark and
applies no rule of its own. A UI-side heuristic was the alternative and is what produced this defect:
any rule the client re-derives is a second copy of the validation, free to drift from it.

### The dynamic prefix is reported, so the picker is the whole accepted set

A column mark alone cannot reach the accepted set. A save accepts any `source` beginning with a
resource's dynamic text prefix, reported or not (`src/connector/mod.rs:205-209`), and that set is
unbounded, so no enumeration can hold it. An earlier draft called the picker a subset and warned about
the gap. That fails the issue's requirement on its own terms, and it fails the operator concretely:
when discovery is down, no `custom:` rule can be authored at all, while the API still accepts one and
the shipped spec guarantees an unreachable upstream does not block saving transforms.

So `ResourceSpec` reports `dynamic_source_prefix`, and the source control offers, beyond the marked
columns, one choice that names a field under that prefix. The operator types the name; the editor
supplies the prefix from the schema. Marked columns plus prefixed names is exactly the accepted set,
with no free-text `source` anywhere: what can be typed is a field's name, not a source key.

The risk this reopens is the one the issue set out to close, that a name can be typed which matches
nothing. It does not reopen it, because that specific name is the one case the shipped contract already
declares unprovable ("a rule sourcing a custom field that does not exist is not an error, it simply
never matches"), and preview is what now catches it: `matched_count` 0 is the answer a typo gets.

Hardcoding `"custom:"` in the UI was the alternative and is what the old `CONNECTOR_RESOURCES` map
already proved wrong: the connector owns that string.

### Each schema field is written where that shape is already published

The three schema fields land in two different deltas, which is precedence rather than taste.
`FieldSpec` is defined whole by `connector-multi-valued-fields`
(`openspec/specs/connector-multi-valued-fields/spec.md:76-85`), so `transform_source` arrives as a
MODIFIED of that requirement, restating the shape with the new key and pointing at
`connector-field-transforms` for what decides it. `ResourceSpec` has never been migrated: it is
described only by the frozen `docs/SPEC.md:871-874`, so the first-touch rule applies and the delta
writes an ADDED requirement carrying the complete post-change resource contract, naming the sentence it
supersedes.

That is also why `fields_incomplete` is stated in the same requirement as the resource shape rather
than in one of its own: first-touch wants the whole contract in one place, and a second requirement
adding a key to a shape published a paragraph earlier is how a spec ends up contradicting itself.

### An incomplete field list is reported, and only a failed request disables the editor

The claim that "the schema needs the upstream, so an unreachable upstream makes the editor read-only"
was wrong about the code. `HomeboxConnector::schema` fetches the custom-field list and swallows every
failure with `unwrap_or_default()` (`src/connector/homebox.rs:252-256`), so a dead or unauthorized
upstream returns `200` with the static columns and no custom fields: a complete-looking schema that is
short. The read-only path would almost never fire, and the case that actually happens, a picker that
silently loses every `custom:` field, would have gone unhandled.

So the two states are separated. The schema response marks a resource whose runtime discovery failed
as **incomplete**, and the editor says so where it offers that resource's sources. A schema request
that genuinely fails, which for Homebox means a `base_url` the connector cannot parse or a transport
failure between the browser and this service, still puts the editor into read-only with the reason.

Failing the schema request outright when discovery fails was the alternative. It was rejected here for
scope: `GET /schema` also feeds the browse grid, and turning a partial schema into a `502` changes a
shipped endpoint's behaviour for every consumer, which is a change to argue on its own issue. Reporting
the incompleteness is additive, is what this change's picker actually needs, and removes the silent
fallback rather than trading it for an outage.

### An edited base url or api key suspends the editor

The same form edits the connection's access details and its rules, while both the schema and every
preview are taken from the **stored** connection. So an operator can point `base_url` at a new Homebox,
press Preview, and read counts computed against the old one, with the panel still on screen at save.
The rule-list key does not catch this: the rules did not change, the upstream did. The picker has the
same defect one step earlier, since its columns, `custom:` fields included, were discovered from the
stored upstream.

Rather than a second key over the access fields, the editor reuses the state it already has: it is live
only while the schema and the preview describe the connection the form shows. A `base_url` differing
from the stored one, or a typed api key, suspends it exactly as a failed schema request does, read-only
with the reason, no preview offered, displayed results discarded, and `transforms` omitted from the
save. Saving refetches the schema for that id and the editor comes back live against the new details.

Two saves are now needed to move a connection and rewrite its rules. That is the honest cost: the
alternative, keeping the editor live and merely disabling the preview button, leaves the source picker
offering the old upstream's fields with nothing on screen saying so, which is the same defect wearing a
smaller coat.

`public_url`, `name` and `enabled` do not suspend anything. Only the base URL and the credential decide
which upstream answers and as whom.

### The form omits `transforms` when it cannot edit them

The form sends `transforms` unconditionally today, so a save while the editor is read-only would post
either rules the operator could not see or `[]`, and `[]` clears them. When the editor is not live, the
`PUT` omits the key, which the stored-rules contract already defines as "keep". Falling back to
free-text inputs was rejected outright: two spellings of one field is what this change removes.

### A stored rule the schema no longer offers keeps its value

A `<select>` whose `value` is not among its options renders as the first option, so a stale stored rule
would silently become a different rule the moment the form opened. Each select therefore adds the
stored value as an option marked unavailable, which is the pattern the same file already uses for a
dangling default connection (`ConnectionsSection.tsx:331-335`).

### A displayed result is gated on a list revision and a request sequence

The request carries every candidate rule, so a result is only meaningful beside the exact list that
produced it. Nothing in the browser enforces that by default: `useMutation` keeps its last result
across edits, and two presses can resolve out of order, so a panel can end up showing list A's counts
beside list B's rules, which is the misleading feedback this change exists to remove.

The editor therefore keeps a **candidate-list revision**, a counter incremented on every edit to a
rule's resource, source or pattern, and on every rule added, removed or reordered. Issuing a preview
captures the revision current at that moment, and a response is displayed only while the revision it
captured is still the current one. The rules themselves still form the request body; they do not serve
as the freshness token.

Hashing the serialized rules was the obvious token and is wrong. It answers "does this result describe
the same text", not "has the operator edited since", and those differ: change a pattern and change it
back while a request is in flight, and the hash matches again, so a result computed before the
round-trip is displayed beside rules the operator has been editing. Rows are fetched live, so the two
lists being textually equal does not make the answers equal. A counter cannot be restored by restoring
the text, which is exactly the property required.

The revision alone is not enough either. Two presses of the same rule with no edit between them capture
the same revision, so an earlier response arriving late passes the revision test and overwrites the
later one. Each request therefore also carries a per-rule sequence number, and a response is displayed
only when it holds the highest number issued for that rule. Both tests are needed and neither subsumes
the other: the revision says the rules have not moved under the answer, the sequence says this is the
answer to the latest question asked about that rule.

### Preview follows browse on connection state

The handler resolves the connection with the same helper browse and materialize use
(`load_conn_and_connector`, `src/api.rs:2135-2148`), which does not consult `enabled`. Preview
therefore works on a disabled connection exactly as browse does. Recorded as an assumption rather than
a new rule: making `enabled` gate reads is a different change to a different set of endpoints.

## Risks / Trade-offs

- **A preview costs an upstream fetch per press, and the panel is per rule, so previewing four rules
  fetches four pages.** → The default page is 10 rows and the fetch is the same one browse makes. No
  caching: a cached page would show a rule against rows that are no longer what the upstream holds,
  which is the failure mode this endpoint exists to remove.
- **Refactoring `apply_to_cells` touches the shipped read path.** → The refactor is behavior-preserving
  and the existing unit tests over `apply_to_cells` and the browse e2e tests are what hold it. If the
  refactor cannot be made behavior-identical, the fallback is the handler-side loop described above,
  and the reviewer should be told which one shipped.
- **The create form loses the rule editor, so a fresh connection needs a save before its first rule.**
  → Intended by the issue, and stated in the form.
- **A name typed under the dynamic prefix can still match nothing**, which is the shape of fault this
  change exists to expose. → It is the one source the shipped contract declares unprovable without
  contacting the upstream, and preview is exactly what now reports it: `matched_count` 0. The name is
  all that is typed; the prefix comes from the schema.
- **`FieldSpec` gains one field and `ResourceSpec` gains two**, which every schema consumer sees. → All
  three are additive and serialize alongside the existing keys; the UI types gain them, and no existing
  consumer reads a key that changed meaning.
- **Changing a connection's base url or api key now takes a separate save from changing its rules.** →
  Stated in the editor when it happens. The alternative is an editor whose picker and panel describe an
  upstream the form no longer names.
- **The incompleteness mark makes `GET /schema` report a state it never reported before.** → Nothing
  downstream branches on it yet except the rule editor. The grid keeps rendering the columns it gets.
- **A preview reports one rule, where the issue's response sketch covered every applied rule.** → The
  deviation is derived above from the issue's own per-row and `page_size` requirements, which no
  all-rules response can satisfy together. Previewing four rules is four requests, which is what a
  per-rule panel does anyway.
- **A shortened value can hide the tail of what a rule captured.** → It is flagged per row, the counts
  and the key set stay exact, and 512 bytes is well beyond any field a label prints. Silence about the
  cut would be the fault; the cut itself is a bound.
- **A 200-row preview of a pattern naming seventy groups answers with megabytes.** → It is bounded and
  the arithmetic is in the spec rather than a round number. It takes a deliberate API call: the UI
  never sends `page_size`, so it gets ten rows.

## Open Questions

None.
