## Context

See `proposal.md` — Why. The state that shapes the approach:

- `PrintRequest` (`src/models.rs:1229-1240`) carries `data: Option<_>` and `fields: Option<_>`, both
  `#[serde(default)]`, and no `deny_unknown_fields`. `print_label` (`src/api.rs:2555`) resolves them
  with `req.data.or(req.fields).unwrap_or_default()`.
- `LabelInput` (`src/models.rs:1221-1223`), which `/batch` and `/render/label` use, already declares
  `data: HashMap<..>` — required, no `Option`, no default, no synonym. `/print` is the outlier, and
  this change makes it agree.
- `print_label` extracts through `crate::extract::Json` (`src/api.rs:25`), whose `FromRequest` maps
  every `JsonRejection` to an `AppError` (`src/errors.rs:468-486`). `JsonDataError` — which is what
  serde returns for a missing required field and for an unknown field under `deny_unknown_fields` —
  becomes `400 InvalidRequest` with `details.reason` `json_malformed` and `details.error` carrying
  the parser's message. That machinery predates this change and is already normative in
  `openspec/specs/request-error-envelope/spec.md`, which names `POST /api/print` explicitly.
- `PrintForm.tsx:177-180` is the sole caller of `printLabel`, and it sends `fields`. Three
  assertions in `PrintForm.test.tsx` (161, 327, 449) read `body.fields`.

## Goals / Non-Goals

**Goals:**

- One spelling for the print parameter map, enforced by the deserializer rather than by a handler
  check, so the failure is impossible to reintroduce silently.
- The removal is observable: a caller on the old spelling gets a `400` naming `fields`, not a
  successful print with its data dropped.
- The service and its own UI stop disagreeing about the request body, and the UI's type stops
  advertising keys the service refuses.

**Non-Goals:**

- No migration path, no desugaring, no deprecation window. Until 1.0 a behavior change breaks what
  came before, and that is the finished job.
- No new error code, status, `Reason` variant or handler-side validation. This change spends the
  rejection the request layer already produces.
- No change to `/batch`, `/render/label` or `LabelInput`, and no change to `copies` validation, the
  render path, or the `BatchSummary` response.
- `ui/src/api/types.ts:101`'s `option?` on `TemplateInputsRequest.labels[].option` (body of
  `POST /api/templates/{id}/inputs`) is untouched: `/batch` and `/inputs` do not carry
  `deny_unknown_fields`, so nothing about them changes here. Only the `/print` client type is in
  scope.

## Decisions

**Delete `fields` rather than accept-and-warn.** The alternative — keep reading `fields` and log a
deprecation — leaves the UI on the old name, which is exactly the state that produced this issue: a
field read and ignored, or read and grumbled about, is invisible to the client that sends it. The
project's rule is that a dropped spelling becomes a parse error naming the file and the key, which is
what `deny_unknown_fields` gives once the field is gone.

**`deny_unknown_fields` on the struct, not an explicit `fields` check.** The narrower alternative is
to keep a `fields: Option<serde_json::Value>` and return `400` when it is present. That is a second
code path holding a rule the type system can hold instead, and it names only one stray key. With
`deny_unknown_fields` the struct is the contract, the error message enumerates the accepted keys, and
`{"data":{…},"fields":{…}}` fails rather than printing while discarding one map. The cost is a wider
break — `{"template":…,"printer":…,"data":{},"extra":1}` now fails where it used to print — and the
issue accepts it. It is also the same posture the template parser already takes: every `raw.rs`
struct is `deny_unknown_fields`.

**`data` required, not `#[serde(default)]`.** Keeping the default would preserve today's behavior for
a body with no map at all: a label rendered from an empty map. That is the server guessing what the
caller meant, and every template in the fixture set fails to render from an empty map anyway, so the
"working" case it preserves is a `422` dressed up as a request the service accepted. An explicit
`data: {}` still expresses "no parameters" for a template that needs none, so nothing legible is
lost. This also makes `/print` match `LabelInput`, which has required `data` and no default.

**The rejection is `json_malformed`, and that is not a new mapping.** An alternative reading is that
an unknown key deserves its own `reason` — `unknown_field`, say. It does not: `request-error-envelope`
already defines `json_malformed` as "the request body could not be deserialized into the type the
endpoint declares", explicitly covering a body that is valid JSON of the wrong shape, and
`details.error` is stated there as the only thing separating the sub-cases. A new reason would
supersede that requirement to say something it already says. So the delta cites the envelope
capability instead of restating or modifying it.

**A body that will not deserialize is rejected before `copies` is checked.** This falls out of
extraction running before the handler and is not a choice, but it is an observable change worth
pinning: `{"printer":…,"template":…,"copies":0}` returns `copies_invalid` today and `json_malformed`
after. The spec states it and a scenario holds it, so a later reader does not read the change as a
regression in the `copies` contract.

**Remove `option?` from `printLabel`'s body type as well as `fields`.** The issue names
`ui/src/api/client.ts:86-91` and asks for "`data` required, `fields` gone"; lines 86-91 also declare
`option?: Record<string, string>`, which no caller passes and the service has never accepted. Under
`deny_unknown_fields` it becomes a guaranteed `400`. Leaving it would keep a client type that invites
precisely the drift this change exists to delete, in the same six lines, so it goes. **Assumption
recorded:** this is a type-level removal with no runtime effect, since `PrintForm.tsx:177-180` is
`printLabel`'s only call site and passes `template`, `printer`, `fields` and `copies` only.

**Assert the OpenAPI schema rather than hand-write it, including `additionalProperties: false`.**
`PrintRequest` is already registered in `src/openapi.rs:142` and utoipa derives the schema from the
struct, so the document follows the field change with no edit. utoipa 5.5.0 translates a container's
`deny_unknown_fields` into `additional_properties(FreeForm(false))`, which serializes as
`"additionalProperties": false` [verified at `utoipa-gen-5.5.0/src/component/schema.rs:548-552`;
`Cargo.lock` pins 5.5.0]. The planned test therefore asserts three things about the `PrintRequest`
schema — `data` in `required`, no `fields` property, and `additionalProperties` false — because the
first two alone would leave the published document permitting a key the endpoint rejects: an object
schema that merely omits a property still admits it as an additional one. `src/lib.rs` already has
the pattern (`openapi_schema_contains_param_types`).

**Move the existing `/print` tests to `data` rather than duplicating them.** Seven HTTP tests post
`fields` incidentally — they are testing copies, auth, 404, 413, recents — and simply switch spelling.
Two exist only to pin the synonym: `api_print_accepts_data_or_fields` and
`api_print_data_precedes_fields`. They do not become `data`-only duplicates of the happy path; they
are replaced by the refusal tests the new spec calls for (`fields` alone, `fields` beside `data`,
neither key, an unrelated stray key, and the deserialize-before-`copies` ordering), which is where
the removed behavior's coverage belongs. Every refusal test asserts the status **and** the
`details.reason`, because a `400` alone would also pass against a handler rejecting for the wrong
reason.

**Two tests for `data: {}`, because one proves only half of it.** The unknown-template case
(`{"template":"nope",…,"data":{}}` → `404`) proves the body deserialized, and nothing more: the
handler returns at template lookup before `LabelInput` is ever built (`src/api.rs:2551-2556`), so it
cannot witness the empty map being *passed to the template*. The second test posts `data: {}` with a
registered fake printer against `brother_24mm_qr`, whose `message` and `code` have no defaults, and
asserts `422` / `BatchInvalid` with a failure naming a missing parameter. That outcome is reachable
only if the empty map travelled through `run_batch` into the render, which is the half the `404`
leaves open. Every fixture template declares undefaulted parameters, so a `200` from an empty map is
not available without adding a no-parameter fixture; the parameter-validation outcome pins the same
property without one.

**An explicit UI test for the no-input template.** Converting the three existing `body.fields`
assertions covers a form with entered values; none of them exercises an empty submitted map, which is
the case where "send `data: {}`" and "omit `data`" differ and only one of them is now legal. The added
test stubs a `single` template reporting no inputs — `pruneDataForSubmit` returns `{}` when no input
is active (`ui/src/lib/labelInputs.ts:240-257`), and `PrintForm.test.tsx` already stubs
`/api/templates/{id}/inputs` per template id — selects a printer, presses Print, and asserts the
captured `/api/print` body has `data` deep-equal to `{}` and no `fields` key. The negative assertion
matters as much as the positive: without it the test would pass against the pre-change form, which
sends `fields: {}` and no `data` at all.

**Exercise the UI print path end to end at the level the assertions already work.**
`PrintForm.test.tsx` stubs `fetch` and reads the captured request body, so moving 161, 327 and 449 to
`body.data` is itself the end-to-end check the acceptance criterion asks for: it proves what leaves
the form, on the real submit path, through `printLabel`. Adding a negative assertion that the body has
no `fields` key is what makes those tests fail against the pre-change code, so they cannot pass for
the wrong reason.

## Risks / Trade-offs

- **Any existing integration posting `fields` breaks on upgrade** (Grocy recipes, scripts, home
  automation) → Accepted and intended; this is the point of the change. The break is loud: `400` with
  `details.error` naming `fields`, not a silent behavior change. The pre-1.0 rule makes this the
  finished job, with no shim.
- **`deny_unknown_fields` breaks more than `fields`** — a caller sending an extra key it invented,
  or a future key sent by a client newer than the server, now gets a `400` → Accepted by the issue.
  It is also what makes the `fields` removal enforceable rather than advisory. Worth noting for
  whoever adds a field to `PrintRequest` later: clients must be updated before servers, not after.
- **A test could pass against the old code** — e.g. a "`fields` is refused" test asserting only
  `!= 200` would already pass today for a template whose parameters `fields` fails to satisfy →
  Mitigated by asserting `400` with `error.details.reason` `json_malformed` and by choosing payloads
  that print successfully under the current code, so each refusal test is red before the change.
- **The frozen `curl` example at `docs/SPEC.md:281` still spells `fields`** and cannot be edited →
  The delta names both it and the §2.3 row as superseded, which is the project's mechanism for
  exactly this. A reader following the precedence rule reaches the new capability.
- **`error.details.error` carries the serde message**, which for `deny_unknown_fields` enumerates the
  accepted field names → No secret is exposed: the field names are in the published OpenAPI document.
  The log rule in `request-error-envelope` (a rejected body is not echoed to the service log) is
  unaffected, since this change adds no logging.

## Migration Plan

None. There is no stored state, no schema, and no persisted request. The break lands with the
release; `docs/SPEC.md` §2.3 is superseded by the new capability at archive time, and any caller on
`fields` is told so by the `400`.
