## Context

See `proposal.md` for motivation and `specs/datetime-params/spec.md` for the contract.

What already exists and shapes the approach:

- `DateTimeResolver` (`src/datetime_fmt.rs`) holds `formats: &BTreeMap<String, String>` plus one
  captured `now: DateTime<Local>`, and answers `{datetime}` / `{datetime.<name>}`. It is built once
  per request in `api.rs` (`:948`, `:1952`, `:2261`) and reaches the renderer inside
  `RenderEnv { settings, datetime }`.
- `interpolate` (`src/render/helpers.rs:42`) resolves a token in the order datetime → `vars.` →
  request `data`.
- `resolve_parameters` (`src/render/mod.rs:27`) coerces and defaults every declared parameter into a
  `HashMap<String, JsonValue>`. It is called on the single-label path (`:281`) and once per label on
  the sheet path (`:574`), both of which already hold a `RenderEnv`.
- Parameters parse through the two-stage path `raw.rs` → `convert.rs` → `models.rs`, with
  `RawParamSpec` carrying `deny_unknown_fields`, and validate in `templates.rs`.
- The UI decides "is a blank value an error?" in three places that each inline the same expression:
  `ui/src/pages/print/FieldForm.tsx:70`, `ui/src/pages/Import.tsx:150`, `ui/src/pages/Connect.tsx:150`.

The whole design follows from one constraint the issue did not have: **the format lives in the token,
so the parameter's resolved value must stay an instant until interpolation.** It cannot be flattened
to a formatted string in `resolve_parameters`.

## Goals / Non-Goals

**Goals:**

- One instant per request, reached by `{datetime}` and by every un-overridden `datetime` parameter.
- A parameter namespace that is the same grammar as `{datetime.<name>}`, so an author who knows one
  knows the other.
- Keep the new plumbing off the 30-odd existing `RenderContext` construction sites.

**Non-Goals:**

- No per-template or per-request timezone. Server-local (`TZ`) only, as today.
- No arithmetic (`{printed_on+1d}`), no new format syntax inside a token, no per-parameter strftime
  pattern. A template that wants a format that `datetime_formats` lacks adds it in Settings.
- No deprecation of `{datetime}` / `{datetime.<name>}`.
- No change to how `datetime_formats` is stored, validated, or edited.

## Decisions

### The parameter carries the instant; the token carries the format

Rejected the issue's sketch of `format:` on the parameter. A parameter is an input; a format is
presentation, and presentation already has a home in the token. Putting it on the parameter would
have created a second place a format name is resolved, a second `422 MissingField` surface, and a
template that prints one format per parameter rather than per use. With this split
`{printed_on}` and `{printed_on.long_date}` can both appear on one label.

Consequence: `format:` on any parameter, of any type, is rejected at load with a message pointing at
the token spelling. `RawParamSpec` gains a `format` field for the sole purpose of producing that
directed error instead of serde's bare `unknown field` from `deny_unknown_fields`. Like every other
field the datetime rules inspect, it parses presence-preserving (see "Rejections live in
`convert.rs`"), so `format:` written and left empty is rejected too.

### `time:` is an explicit flag, not an inference

Chosen over inferring the control from whether the strftime pattern behind the token contains time
specifiers. `datetime_formats` is runtime state an operator edits in Settings; inference would make
an operator's format edit silently change the input control of every template using it, and a token
naming a format that does not resolve would have nothing to infer from. The cost accepted: a template
can pair `time: false` with `{p.time}` and print `00:00`. That is visible in the render → inspect
loop, and no `datetime_formats` edit can cause it.

### `resolve_parameters` is given the instant and returns two maps

New signature:

```rust
pub struct ResolvedParams {
    pub data: HashMap<String, JsonValue>,
    pub instants: BTreeMap<String, DateTime<Local>>,
}

pub fn resolve_parameters(
    template: &TemplateDefinition,
    data: &HashMap<String, JsonValue>,
    option: Option<&BTreeMap<String, String>>,
    now: DateTime<Local>,
) -> Result<ResolvedParams, AppError>
```

Both call sites pass `env.datetime.now`, so nothing new reaches `api.rs`: the instant it already
captures for the resolver is the instant parameters resolve against. That is what makes a sheet
spanning midnight print one date, and it is why the function must never call `Local::now()` itself.

`data` still gets an entry for each `datetime` parameter, the instant rendered as bare ISO
`%Y-%m-%d`. It is what `when:` compares against and what a preview echoes; interpolation never reads
it, because the namespace is checked first.

The incoming `JsonValue` is narrowed rather than coerced: `Null` and an all-whitespace string join
"absent" and take the request instant; a `String` is parsed; every other variant is an error. The
other parameter types accept a number where a string is expected (`ParamType::Integer` parses
`"400"`, `Length` strips a `mm` suffix), and copying that habit here would mean inventing an epoch or
a serial-date convention on the caller's behalf. Refusing is cheaper to live with than guessing wrong
about a date.

### Instants ride on `RenderContext` through a builder, not a new positional argument

`RenderContext::new` has 33 call sites, nearly all tests, and `RenderEnv` has 26; adding a required
field to either is mechanical churn across the file for a value that is meaningless in every one of
those tests. Instead:

```rust
let ctx = RenderContext::new(/* unchanged */).with_instants(&resolved.instants);
```

The field is `Option<&'a BTreeMap<String, DateTime<Local>>>`, `None` by default, which is the honest
state of a context for a template with no `datetime` parameter. Child contexts for containers copy it
from `self`, like `data` and `env` today.

Every context built from resolved label data chains the call, not just the one that emits the final
markup. `compile_label_doc` builds two, the auto-length measurement probe
(`src/render/mod.rs:332`) and the final context (`:388`), and the sheet path builds one per label (`:587`); container children
inherit on both the measure path (`:1112`, `:1225`) and the render path (`:1738`, and `:1835` for a
rotated container). A probe without the map would fail interpolation with `MissingField` on
`{p.<fmt>}` and take a dynamic-width template down before the final context ever ran, so the
measurement path is where this gets tested, not just where it gets set.

`interpolate` gains the map as a fifth argument and consults it after the `datetime` namespace and
before `vars.`. Ordering between the parameter namespace and `vars.` is not observable, because a
parameter name can contain neither a dot nor the reserved prefixes.

### Formatting and parsing stay in `datetime_fmt.rs`

`DateTimeResolver` gains one method, so no caller learns strftime:

```rust
pub fn resolve_param(
    &self,
    token: &str,
    instants: &BTreeMap<String, DateTime<Local>>,
) -> Option<Result<String, AppError>>
```

It splits `token` at the first `.`, looks the head up in `instants`, and formats with
`BARE_DATETIME_FORMAT` or with `self.formats[<name>]`, returning `AppError::missing_field("<p>.<name>")`
for an unknown name. The existing `resolve` is untouched.

Parsing is a free function tried in order: `%Y-%m-%d` (midnight local), `%Y-%m-%dT%H:%M:%S`,
`%Y-%m-%dT%H:%M`, then `DateTime::parse_from_rfc3339` converted with `with_timezone(&Local)`. The
naive forms go through `Local.from_local_datetime`: `Single` is taken, `Ambiguous` takes the earlier
instant, and `None` (a spring-forward gap) is an error. Deciding this explicitly matters because
`.single().unwrap()` on a DST gap is a panic, and a panic in a render handler is a 500.

`YYYY-MM-DDTHH:MM` is not RFC 3339 and the issue did not list it, but it is exactly what
`<input type="datetime-local">` submits, so accepting it is what makes `time: true` work at all.

### Rejections live in `convert.rs`, not `validate_param_spec`

`multiline`, `values` and `enum` are folded into the `ParamType` variant during conversion and are
invisible afterwards, so `templates.rs::validate_param_spec` cannot see them. All datetime attribute
rules (`format`, `default`, `min`, `max`, `multiline`, `values`, `enum` refused on `datetime`; `time`
refused off `datetime`) therefore go in `TryFrom<RawParamSpec> for ParamSpec`, which already returns
`TemplateError::Validation` and already gets the `params.<name>` path prefix from its caller.

Those rules key off **presence**, and a plain `Option<T>` with `#[serde(default)]` cannot express it:
`default:` written with no value deserializes to `None`, indistinguishable from the key being absent,
so a forbidden attribute would slip through and `time:` would silently become `false`. Every field the
rules inspect therefore parses presence-preserving, the way `raw.rs::deserialize_present` and
`models.rs::deserialize_present_group` already do it in this codebase: `#[serde(default,
deserialize_with = "deserialize_present")]` over an outer `Option`, where `None` is absent and
`Some(null)` is written-and-empty. `time` becomes `Option<Option<bool>>`: absent is `false`, a written
bool is taken, and a written null is an error.

`ParamType::DateTime { time }` serializes `time` **always**, unlike `String { multiline }` which skips
a false value. The API scenario in the spec asserts `time: false` on `GET /templates/{id}`, the UI
branches on it, and an always-present boolean is one less `?? false` on the client. The asymmetry with
`multiline` is deliberate and not worth changing `multiline` for.

`templates.rs::check_param_ref` gains a `ParamType::DateTime { .. }` arm. No caller passes
`"datetime"` in `allowed_types`, so every numeric context rejects the parameter without a new list.

### The advertised field list drops the namespace

`collect_data_tokens` is a free function over a string and cannot know the parameter table, so
`template_fields` and `placeholder_data` filter after collecting: a token whose head, up to the first
`.`, names a declared `datetime` parameter is dropped. Both bare `{p}` and `{p.<fmt>}` disappear from
the catalog index (`src/bin/catalog-index.rs:87`) and from thumbnail placeholder data, so a thumbnail
prints a real date instead of the literal `printed_on.short_date`.

The UI mirrors this in `templateFields.ts`: the four field-collecting functions take the parameter map
and apply the same head-of-token rule, so the CSV grid does not grow a required `printed_on.long_date`
column.

### The print form seeds; the grids do not

`defaultParamValues` is shared by the print form, the import grid and the connector grid, and both
grids merge it into every submitted row. Seeding a browser date there would push that date onto every
CSV row and quietly defeat "blank means the server's instant". So the seed is a separate helper used
only by `PrintForm`'s initial state; `defaultParamValues` leaves a `datetime` parameter alone.

The three inlined `hasDefault` expressions collapse into one exported helper that adds
`type === "datetime"`. Three copies of a rule that now has a fourth clause is how one of them gets
missed.

Accepted consequence, recorded because it is externally visible: a seeded value is the *browser's*
date, so an operator whose timezone straddles a date boundary with the server sees, and prints, the
browser's date. Clearing the control restores the server as the authority. The alternative,
`GET /templates/{id}` returning a resolved default, makes a cacheable response time-dependent, which
is worse.

### Grid cells validate grammar on the client, timezone on the server

Both grids keep `LabelGrid`'s plain text `DataEditCell` for a `datetime` parameter: a picker inside a
`react-data-grid` cell fights the grid's own edit lifecycle, and a CSV column is text anyway.

One exported helper in `ui/src/lib/templateFields.ts` implements the check, and both
`Import.tsx::validateRow` and `Connect.tsx::validateRow` call it from the loop they already run over
`requiredForRow`, so there is one grammar and one message:

```ts
// "" is valid (the server fills it). Otherwise one of:
//   YYYY-MM-DD | YYYY-MM-DDTHH:MM | YYYY-MM-DDTHH:MM:SS | RFC 3339 with offset or Z
export function datetimeCellError(raw: string): string | undefined
```

It matches the shape with a regex and then confirms the parts name a real calendar date and time
(month, day-in-month including leap years, hour, minute, second), which is what makes `2026-02-30`
and `2026-08-19T25:00` errors rather than something the server has to catch.

The division of labour is deliberate and is in the spec: the client judges **shape and calendar
validity**, the server judges **the timezone**. A browser cannot know the server's `TZ`, so whether a
well-formed local instant exists is not a client question, and duplicating a DST table in TypeScript
would be a second source of truth that drifts.

A value the client accepts and the server rejects (a spring-forward gap) still lands on its row, with
no new plumbing: both grids already catch `422 BatchInvalid`, read `details.failures`, map each
failure index back through `idForExpandedIndex` and write the message onto that row's annotation
(`ui/src/pages/Import.tsx:335-345`, `ui/src/pages/Connect.tsx:243-251`). A `datetime_param_invalid`
failure is reported through `AppError` like every other per-label failure, so it inherits that path.
The client-side check is therefore an early gate that keeps a doomed batch from being submitted at
all, not the only thing standing between a bad cell and a legible error.

### ADR

This change adds **ADR-0068, "A `datetime` parameter names an instant; the token names the format"**,
recording the parameter/token split, `time:` over inference, the render instant as the default, and
the rule that a new template reaches for a `datetime` parameter when the caller must be able to choose
the instant and for `{datetime}` when it must not. It supersedes nothing; ADR-0028 (datetime tokens)
and ADR-0056 (parameterized templates) both stand.

The number is provisional: the in-flight changes for #201 and #203 both claim ADR-0066. Take the next
free number at apply time and update `docs/adr/README.md` in the same commit.

## Risks / Trade-offs

- **A template can declare `time: false` and print `{p.time}` as `00:00`.** → Accepted, and preferred
  over inference tying the control to editable settings. `docs/AUTHORING.md` shows the pairing.
- **`resolve_parameters` is public and its signature changes.** → Only two callers plus tests, all in
  `src/render/mod.rs`; the compiler finds every one. Taking `now` as an argument is the point, so a
  future caller cannot reintroduce a second clock read.
- **`Option<&BTreeMap>` on `RenderContext` is a nullable dependency.** → It encodes a real state (no
  `datetime` parameters), it is set by a builder next to `new`, and the alternative is 33 edited call
  sites carrying an empty map.
- **DST gaps and ambiguity.** → Explicitly decided (reject the gap, take the earlier instant when
  ambiguous) with unit tests over a fixed zone, rather than left to `.single()`.
- **The browser-seeded date can differ from the server's.** → Recorded above; clearing the field is
  the escape hatch, and the API default is unaffected.
- **`time:` becomes a known key on `RawParamSpec` for every type.** → Rejected explicitly on other
  types rather than ignored, so the error names the parameter instead of the template failing to
  parse with a serde message.

## Migration Plan

None. `type: datetime` is new syntax with no existing user: no template can currently declare it, no
stored data changes, and `datetime_formats` keeps its existing entries and defaults. Rollback is
reverting the commit; a template written against the new type fails to load on an older build and is
quarantined, which is the existing behavior for unknown template syntax.
