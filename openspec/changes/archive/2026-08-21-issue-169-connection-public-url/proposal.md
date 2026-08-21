## Why

Implements [#169](https://github.com/pfa230/labeler/issues/169). When Labeler reaches Homebox over a
Docker network or an internal reverse proxy, `base_url` holds an address only the server can resolve
(`http://homebox:7745`). Every entity link Labeler emits is built from that address, so printed QR
codes and Connect-page row links resolve to a host a phone or a client browser cannot reach.

The server half of the fix already landed (`public_url` on the connection record, validated in the
connection endpoints, used by the Homebox connector for link generation), and the UI's API types
already declare the field (`ui/src/api/connectors.ts:15`, `:24`). What is missing is the only part an
operator can touch: Settings > Connections renders no input for it, so no connection can ever be given
one. This change adds that field and writes down the contract the server already implements, which no
spec or ADR records today.

## What Changes

- Settings > Connections gains an optional **public url** field on the connection form, next to
  **base url**. Leaving it blank means "no public URL"; the connection then keeps deriving links from
  `base_url`, as before.
- Saving the form always sends `public_url`, so blanking a stored value clears it and typing one sets
  it, exactly as the form already treats `transforms`. The UI validates it the way it validates
  `base_url` (parseable, `http`/`https`), and only when it is non-blank.
- The connections table gains a **Public URL** column so an operator can see which connections
  override link generation without opening each form.
- The connection contract is written down as an OpenSpec capability: the record's shape, the
  create/update/read/delete semantics including `public_url`'s omit-keeps / null-clears behavior on
  `PUT`, URL validation and normalization, and the rule that `public_url` governs *generated links*
  while `base_url` remains the only address Labeler fetches from.
- A rejected `public_url` reports `details.reason = "public_url_invalid"` instead of borrowing
  `base_url_invalid`, so a client can tell which of the two URL fields failed.
- Both connection URLs reject embedded userinfo (`https://user:pass@host`). A connection URL is
  printed into QR codes, so credentials carried in one end up on a physical label. **BREAKING** for
  the narrow case of an existing `base_url` that carries userinfo: it is untouched at rest, but the
  next save of that connection is rejected until the credentials are removed.
- One new ADR records the load-bearing decision: `public_url` is the link base, `base_url` is the
  fetch base.
- Otherwise not breaking: `public_url` stays optional, and a connection without one behaves exactly as
  it does today.

## Capabilities

### New Capabilities
- `connections`: the connection record and its CRUD contract, the `public_url` field, and the rule
  that generated entity links use the public URL while upstream requests use the base URL. Supersedes
  the connection portions of the frozen `docs/SPEC.md` §12 (the record shape, the `POST`/`PUT`/`DELETE`
  contract, and "Using a connection (UI)" as it describes the connection form), and the
  `base_url_invalid` row of the §10.1 reason table. Where `openspec/specs/connector-field-transforms/`
  already superseded those same §12 sentences to the extent of adding `transforms`, this capability
  carries `transforms` in the record and leaves every rule about it to that capability.

### Modified Capabilities
- None. `connector-field-transforms` keeps sole ownership of the transform rules; this change neither
  restates nor alters them.

## Impact

- `ui/src/pages/settings/ConnectionsSection.tsx`: the form field, its validation, the submit payload,
  and the table column.
- `ui/src/pages/settings/ConnectionsSection.test.tsx`: coverage for setting, clearing, and rejecting
  a public URL.
- `src/reason.rs` and `src/api.rs`: the new `public_url_invalid` reason, the userinfo rejection, and
  the validation call sites that raise them. `src/lib.rs` and `src/api.rs` tests that currently assert
  `base_url_invalid` for a rejected `public_url` move to the new slug.
- `docs/adr/`: one new ADR plus its row in `docs/adr/README.md`, and a note on the ADR-0018 row that
  its connections-store decision is superseded in part.
- `openspec/specs/connections/spec.md` on archive.
- No change to `ui/src/api/connectors.ts`, which already declares `public_url`, or to `src/errors.rs`,
  whose reason-documentation test already accepts a slug documented in an OpenSpec spec
  (`src/errors.rs:577-600`).
- `src/store.rs` and `src/connector/homebox.rs` already implement the rest of the specified server
  contract; the change verifies them against the new spec rather than editing them.
