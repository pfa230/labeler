## Why

[#309](https://github.com/pfa230/labeler/issues/309). `POST /print` takes one parameter map under two
names. `PrintRequest` declares both `data` and `fields` (`src/models.rs:1229-1240`) and the handler
picks whichever arrived: `req.data.or(req.fields).unwrap_or_default()` (`src/api.rs:2555`).
`docs/SPEC.md:238` names it for what it is: "`fields` is accepted as a legacy synonym."

The second spelling is not inert. Our own UI is on it: `ui/src/pages/print/PrintForm.tsx:180` posts
`fields: submittedData`, and `ui/src/api/client.ts:86-91` declares both keys optional. The synonym is
what let the client drift onto the older name with nothing to notice it. Until 1.0 a change that
alters behavior breaks what came before, so the fix is deletion, not a deprecation window.

## What Changes

- **BREAKING.** `PrintRequest` loses `fields`. A `POST /print` body carrying `fields` is rejected
  instead of printed, and the rejection names the key.
- **BREAKING.** `data` becomes required: no `Option`, no `serde` default, and
  `unwrap_or_default()` goes with it. A `/print` body with neither key currently prints a label from
  an empty map; with one spelling it either carries `data` or it is a bad request. An explicit
  `data: {}` stays legal — what is removed is the default, not the empty map.
- **BREAKING.** `PrintRequest` gains `deny_unknown_fields`. Without it, `{"data":{…},"fields":{…}}`
  would print and drop `fields` on the floor, which is the silent-ignore this change exists to
  remove; with it, the rejection names the offending key. This widens the break past `fields`: any
  stray key on `/print` now fails. That is accepted.
- The rejection is the existing one, not a new code path. `print_label` extracts through
  `crate::extract::Json` (`src/api.rs:25`), so a body that will not deserialize maps to
  `400 InvalidRequest` with `details.reason` `json_malformed` at `src/errors.rs:468-486`, exactly as
  the `request-error-envelope` capability already requires of every JSON endpoint. No new `Reason`
  variant, no new status, no handler-side check.
- The UI moves onto the surviving spelling: `printLabel`'s body type declares the four keys the
  service accepts and `PrintForm` sends `data`. `option?`, which the type declares and no caller
  sends, goes too — under `deny_unknown_fields` it is a `400`, and a client type advertising a key
  the service refuses is the same drift in a second place.
- `copies` validation and its `1..100` range, the `404` / `409` / `413` / `422` / `502` paths, the
  `BatchSummary` response and the trusted-LAN posture are unchanged.

## Capabilities

### New Capabilities

- `print-request-body`: the complete post-change contract for `POST /print`'s request body — the
  keys it accepts, which are required, that no other key is accepted, and how a body it will not
  accept is rejected — together with the requirement that the print form posts that body. First
  touch of behavior documented only in the frozen spec, so both requirements are `ADDED`.

### Modified Capabilities

None. `request-error-envelope` already states the `400` / `InvalidRequest` / `json_malformed`
mapping this change relies on, for every endpoint including `POST /api/print`; nothing in it changes,
and the new capability cites it rather than restating it.

## Impact

- `src/models.rs:1229-1240` — `PrintRequest`: one map field, named `data`, required, plus
  `#[serde(deny_unknown_fields)]`.
- `src/api.rs:2555` — `req.data.or(req.fields).unwrap_or_default()` becomes `req.data`.
- `src/openapi.rs:142` — `PrintRequest` is already registered; its generated schema follows the
  struct and is asserted rather than edited.
- `ui/src/api/client.ts:86-91` — `printLabel`'s body type: `data` required, `fields` and `option`
  gone.
- `ui/src/pages/print/PrintForm.tsx:177-180` — sends `data: submittedData`.
- `src/lib.rs` HTTP tests posting `fields` to `/print`: 6368, 6390, 6406, 6421, 6446, 7957, 8848 move
  to `data`. `api_print_accepts_data_or_fields` (6477) and `api_print_data_precedes_fields` (6498)
  exist only to pin the synonym and go away as such.
- `ui/src/pages/print/PrintForm.test.tsx:161, 327, 449` assert `body.fields` and move to `body.data`.
- `docs/SPEC.md` §2.3's request field table and its `curl` example (the `fields` spelling at
  `docs/SPEC.md:281`) are superseded by the new capability, not edited: the file is frozen.
- No change to `/batch` or `/render/label`, whose `LabelInput` (`src/models.rs:1221-1223`) already
  requires `data` and has no synonym. This change makes `/print` agree with them.
