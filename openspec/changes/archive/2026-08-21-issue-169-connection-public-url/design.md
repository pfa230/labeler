## Context

See `proposal.md` for motivation and `specs/connections/spec.md` for the contract. The relevant
current state, on this branch after merging `main` at `c37f80e`:

- The server already stores and honours `public_url`. `src/store.rs:52` carries it on `Connection`,
  the migration `ALTER TABLE connections ADD COLUMN public_url TEXT` is shipped (`src/store.rs:167`),
  and `UpdateConnection` carries it as an `UpdateField<String>` so a `PUT` can keep, clear, or set it
  (`src/store.rs:73`, applied in `update_connection` at `src/store.rs:699`).
- `src/api.rs:1073` exposes it on `ConnectionView`; `ConnectionInput` models the three input forms as
  `Option<Option<String>>` (`src/api.rs:1101`) via the shared `deserialize_optional_field`
  (`src/api.rs:1110`), and the handlers translate that into `UpdateField`.
- `src/connector/homebox.rs:128` resolves the link base with `external_base_url`, falling back to
  `base_url` when `public_url` is absent or whitespace, and threads it through `browse` and
  `materialize`. Upstream fetches go through `base(conn)` (`src/connector/homebox.rs:123`), which
  reads `base_url` only.
- `ui/src/api/connectors.ts:15` and `:24` already declare `public_url` on `Connection` and
  `ConnectionInput`. `ConnectionsSection.tsx` mentions it nowhere: the form has no input for it
  (fields end at the enabled checkbox), the submit payload omits it (`ConnectionsSection.tsx:52-59`),
  and the table has no column for it (`:254-259`, row cells at `:204-210`). That is the whole gap.
- `validate_and_normalize_url` (`src/api.rs:1122`) takes a field name for the message but hardcodes
  `Reason::BaseUrlInvalid`, so a rejected `public_url` reports the wrong discriminator. It checks
  scheme, host, query, and fragment, and does not check userinfo.
- #161 landed just before this change and moved two things this plan depended on. The connection
  record now also carries `transforms`, and `openspec/specs/connector-field-transforms/spec.md:31-35`
  already supersedes the frozen §12 record sentence "to the extent of adding `transforms`". And
  `src/errors.rs:577-600` now scans `openspec/` for backticked reason slugs, so a reason documented in
  an OpenSpec spec rather than in the frozen §10.1 table already satisfies the reason-documentation
  test. Neither needs doing here.

## Goals / Non-Goals

**Goals:**

- Make the shipped server capability reachable from Settings > Connections.
- Record the connection contract in `openspec/specs/`, where only the transform slice of it lives.
- Make the error discriminator name the field that actually failed.

**Non-Goals:**

- Probing or reachability-checking the public URL. It is by definition an address the server may not
  be able to resolve, so validation stays syntactic.
- Per-template or per-render URL overrides. The `qr_base_url` variable already covers that case and is
  untouched here.
- Any change to how Labeler fetches from upstream, to cursors, to transforms, or to the connector
  schema.
- A second connector. Only Homebox exists; the requirement is written in connector-neutral terms but
  only Homebox implements it.

## Decisions

**A connection carries two addresses with distinct jobs, and the ADR says so.** `base_url` is the
address Labeler dials; `public_url` is the address Labeler prints. This change adds
**ADR-0061, "A connection's public URL is the link base, its base URL is the fetch base"**, plus its
row in `docs/adr/README.md`. 0059 and 0060 are taken by the changes that merged while this one was
being planned, so 0061 is the next free number.

It supersedes the **Connections store** decision of
[ADR-0018](../../../docs/adr/0018-api-integration-spine.md), which records the connection row as
`(connector, name, base_url, credential, enabled)`
(`docs/adr/0018-api-integration-spine.md:39-42`) and would otherwise stand as an accepted record
contradicting the field this change makes reachable. Only that decision is superseded: ADR-0018's
browse-cursor, connector-schema, and Homebox-endpoint decisions stand, so ADR-0018 keeps the status
`Accepted` with its row noting the partial supersession. ADR-0060 amended the same record from the
transform side without superseding ADR-0018; both amendments stand and neither contradicts the other.
The `public_url` server work itself landed in commits `d18ebad`, `2eafccf`, `78504a3` on 2026-08-17
with no ADR at all, which is the gap this closes.

*Alternative considered:* one URL plus a rewrite rule (store only `base_url` and a host substitution
applied at link time). Rejected: it encodes the same information less directly, and an operator
reasoning about "what does the QR code point at?" would have to apply the rule mentally.

**Clearing is expressed as `null`, and the form always sends the key.** The API distinguishes
key-absent (keep) from `null`/blank (clear), which exists for API clients doing partial updates. The
form is not a partial update: it renders every field, so it always sends `public_url`, as `null` when
the input is blank. That makes "empty the box and save" clear the stored value, which is the only
behavior an operator would predict from a text input, and it is how the form already treats
`transforms` (`ConnectionsSection.tsx:57`).

*Alternative considered:* omit the key when blank. Rejected: it makes the field write-once from the
UI, with no way to undo a public URL short of a hand-written request.

**Field-typed URL validation.** `validate_and_normalize_url` gains a small `UrlField` enum
(`Base`/`Public`) in place of its `&str` field name, with the wire name and the `Reason` hanging off
it. One value decides both the message text and the discriminator, so the two cannot drift, and the
call sites stop passing a bare string that controls neither. `Reason::PublicUrlInvalid =>
"public_url_invalid"` joins the `reasons!` macro list in `src/reason.rs`; adding a slug is additive
and breaks no client, since only renames are breaking (`src/reason.rs:3`). No test scaffolding is
needed for the new slug: `src/errors.rs:577-600` already counts a slug as documented when an OpenSpec
spec names it in backticks, and `specs/connections/spec.md` names this one.

*Alternative considered:* keep the `&str` parameter and add a `Reason` parameter beside it. Rejected:
two parameters that must agree is exactly the drift the enum removes.

*Alternative considered:* leave the discriminator as `base_url_invalid`. Rejected as recorded in this
change: a machine-readable reason that names the wrong field is worse than no distinction at all, and
the UI's own error text comes from `message`, so nothing else compensates.

**Userinfo is rejected in both URL fields.** A connection URL is printed into QR codes and rendered as
a browser link, so `https://user:pass@homebox.example.com` would put credentials on a physical label.
Rejecting it on `public_url` alone would leave two fields with two validation rules and one spec
carrying the exception, so the rule applies to both. It bites only at write time: a stored `base_url`
with userinfo keeps working until someone saves that connection.

*Alternative considered:* accept userinfo and document the hazard. Rejected as recorded in this
change; validation that already checks scheme, host, query, and fragment has no reason to wave through
the one component that leaks a secret.

**The UI validates the public URL exactly as it validates the base URL, and only when non-blank.**
`ConnectionForm` already parses `base_url` with `new URL()` and checks the protocol before sending
(`ConnectionsSection.tsx:47-49`); the public URL reuses that shape so both fields fail the same way,
in the form, without a round trip. Blank skips validation entirely, because blank is a legal value
meaning "none".

**The table gets a Public URL column.** The form is per-connection; the column answers "which of these
print external links?" at a glance, which is the question an operator debugging a scanned QR code
actually has. An unset value renders `-`, matching the table's habit of rendering a short token
(`set`/`none`, `yes`/`no`) rather than an empty cell.

**The `connections` capability names `transforms` but owns nothing about it.** Writing the record whole
is what the first-touch rule asks for, and leaving `transforms` out would make the record wrong the day
it is archived. Restating its rules would duplicate `connector-field-transforms` and invite the two to
drift, so the record requirement names the field and points at that capability for every rule.

*Alternative considered:* a `MODIFIED` delta against `connector-field-transforms`. Rejected: that
capability's requirement is about transforms, not about the connection record or its URLs; widening it
to carry `public_url` would put two unrelated contracts in one requirement.

**`connector` on `PUT` is documented as inert, not made strict.** The update handler never reads
`body.connector`, so a payload naming a different or unknown connector is accepted and changes nothing,
while `POST` rejects an unknown one. The spec records that as the contract rather than tightening it:
rejecting a mismatched connector on update is a behavior change outside #169's scope, so it is filed as
[#197](https://github.com/pfa230/labeler/issues/197) and left out of this change.

**Server code is verified against the spec, not rewritten.** Apart from the reason and userinfo
changes, the tasks read the existing store, API, and connector paths against each requirement and add a
test only where one is missing. The spec is being written after the implementation here, so the
verification step is what makes the two match rather than merely appear to.

## Risks / Trade-offs

- **The spec is written from the code it describes, so a bug in the code becomes a bug in the spec.**
  → The requirements were drafted from the API contract and the issue's intent, then checked against
  the code path by path; the review artifact judges them against the issue, not against the diff.
- **A misconfigured `public_url` silently produces unreachable printed links, and a printed label
  cannot be fixed after the fact.** → Syntactic validation catches typos in scheme and host, and the
  table column surfaces the effective value. Reachability is out of scope by design (see Non-Goals).
- **Tightening `base_url` validation can reject a configuration that saves fine today.** → Only a
  `base_url` carrying userinfo, which Homebox auth never needed (the credential travels as a Bearer
  header), and only on the next save of that connection. The proposal calls it out as the change's one
  breaking edge.
- **Two capabilities now describe the same connection record.** → `connections` owns the record and
  its CRUD; `connector-field-transforms` owns the transform rules; the record requirement says so in
  the text, so a later reader is not left choosing.
- **The connection form is now long: six fields plus a transforms editor.** → The field row already
  wraps (`flex flex-wrap`, `ConnectionsSection.tsx:71`), and the public url field takes the same
  `flex-1` sizing as base url, so narrow viewports get another line rather than a squeezed input.
- **Nothing in the product tells an operator that a stored `public_url` on a disabled or unused
  connection is stale.** → Out of scope; the column at least makes it visible.

## Migration Plan

No data migration: the `public_url` column shipped with the server work and is already present in every
deployed database. No config change, no restart semantics change. Deploying this change makes an
existing column editable; rolling it back hides the field again and leaves stored values in place and
still honoured by the server.
