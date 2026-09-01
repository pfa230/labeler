## Context

See proposal.md — Why. What shapes the approach is where the resolution already lives and what
currently cannot reach it.

`resolve_and_coerce_default` (`src/render/mod.rs:317-391`) is the whole resolution: it interpolates a
string default through `helpers::interpolate`, coerces the result with `coerce_param_value`, and in
`ResolveMode::Strict` returns `AppError::param_default_unresolvable` carrying the failing token or the
rejected value. It is private, and it is reached only from `resolve_parameters_mode`, which resolves a
whole label rather than one parameter and writes into a `resolved` map and an `instants` map.

The derivation that builds an input list is synchronous and has no application state:
`derive_inputs_internal` and its two entry points `inputs_all` / `inputs_default` are called from
`TemplateDetail::from` (`src/templates.rs:2207`), from `placeholder_data` (`:157`), and from
`src/bin/catalog-index.rs:97`, which is a build-time binary with no store at all.
`derive_inputs_for_label` (`:140`) does call `resolve_parameters_mode`, but passes `None` for both
`variables` and `datetime`, which is exactly why a tokened default is dropped there today.

Three call sites already read what resolution needs. The thumbnail handler reads
`state.store().all_variables()` and `crate::settings::resolve_datetime_formats(state.store())` and
captures one `chrono::Local::now()` (`src/api.rs:1206-1216`); the render and batch paths read the same
three. The two endpoints this change touches read none of them.

`AppError::param_default_unresolvable` (`src/errors.rs:288-310`) formats the parameter, the token and
the value into its `message` and puts none of them in `details`, which carries `reason` alone.

## Goals / Non-Goals

**Goals:**

- One resolution per request, reached through one function, projected into `param_defaults`,
  `inputs.default` and `inputs.all`.
- Both read endpoints resolve from the same three inputs, so no entry depends on which call produced it.
- A resolution failure is data on a `200`, structurally identical to the payload the render path's `422`
  carries.

**Non-Goals:**

- Changing how a render resolves a default. The render path is untouched; this change adds a read-only
  caller of its resolver and one structured payload to its error.
- Caching. `GET /api/templates/{id}` sets no `ETag` and no `Cache-Control` today and gains neither here.
- Fixing #270. A `boolean` declaring `default: 1` still fails, and this change makes that visible in the
  response rather than only at print. Whatever #270 does to the authored scalar's kind will change what
  is published, through the same one resolution.
- Removing `truncated_elsewhere` (#269) or touching the grids' deferral gap (#242).

## Decisions

**One resolver, reached through a per-parameter wrapper.** Add a public function in `src/render/mod.rs`
that resolves one declared default in `ResolveMode::Strict` against a supplied variables map and
`DateTimeResolver`, returning the coerced `serde_json::Value` or the structured failure the render path
would raise. It is a thin wrapper over `resolve_and_coerce_default` with a scratch `resolved`/`instants`
pair, so nothing about how a default resolves is restated. *Alternative rejected:* a second resolution
written for publication. A default that resolved differently here from how it resolves at render time is
worse than publishing nothing, because the operator is then shown a value the printer will not use, and
nothing would detect the drift.

**One request-scoped result, built once and passed by reference.** The wrapper is a per-parameter call,
which is not by itself the "one resolution projected into three places" the contract demands, so define
the result explicitly:

```rust
pub type ResolvedDefaults = BTreeMap<String, ParamDefaultReport>;   // keyed by parameter name

pub fn resolve_declared_defaults(
    template: &TemplateContent,
    variables: &BTreeMap<String, String>,
    datetime: &DateTimeResolver,
) -> ResolvedDefaults;
```

It calls the wrapper once for each parameter that declares a `default:`, and holds no entry for one that
does not, so its key set is `param_defaults`' key set. Each value is `Resolved(ParamValue)` or
`Failed(ParamDefaultError)`, which is what makes "exactly one of `resolved` and `error`" a property of
the type rather than a rule to remember.

The map is **the publication contract, not a claim about how many times the resolver runs.** Precisely:
one `ResolvedDefaults` map is computed once per request, and every *published* field is read from it —
`param_defaults`, and each entry's `default`, `default_error` and `required` in every input list on that
response. `derive_inputs_internal` takes `&ResolvedDefaults` and reads it for those three fields;
`inputs_all`, `inputs_default` and `placeholder_data` pass through the same reference; the detail builder
builds the map once and hands the same reference to both input lists while serializing it as
`param_defaults`; and `template_inputs` builds it once per request and passes the same reference into
every label's derivation, so fifty labels publish one resolution rather than fifty independent ones.
*Alternative rejected:* threading `(variables, datetime)` down and letting each projection call the
wrapper for what it publishes. It gives the same values today, and it is exactly the shape in which the
three projections could later drift apart without anything failing.

Separately, and outside that map, the **lenient per-label walk** that `derive_inputs_for_label` runs
through `resolve_parameters_mode` invokes the same resolver again, against the same captured context, to
decide which entries are active. That is a second invocation and the plan says so rather than pretending
otherwise: the walk is the render's own function and takes `(variables, datetime)` rather than a
precomputed map, and changing `resolve_parameters_mode`'s signature to accept one would edit the render
path this change deliberately leaves alone. It cannot produce a different value — same function, same
snapshot, deterministic — so what it decides is which entries exist, never what they publish. The cost is
one extra interpolation per declared default per label, on a path that already walks the whole layout per
label.

**The failure payload becomes structured, and both consumers project it.** Introduce one type carrying
the parameter, the message, and `token`/`value` where each exists; build it where
`resolve_and_coerce_default` fails today, and construct both consumers from it:

- `AppError::param_default_unresolvable` keeps its message where the envelope already puts it —
  `error.message`, a sibling of `error.details` (`src/models.rs:11-16`) — and adds `param`, `token` and
  `value` to `details` alongside `reason`, through `reasoned`'s existing `extra` map, which writes
  `reason` plus extras and nothing else (`src/errors.rs:80-99`). `details` must name the parameter,
  because nothing else in the envelope does. The message is **not** duplicated into `details`.
- `ParamDefaultError`, the serialized read-only shape, carries `reason`, `message`, `token?` and
  `value?` and **no** `param`. It has no envelope, so it holds the message as a field of its own; and it
  is only ever reached as the value of a `param_defaults` key or as an entry's `default_error`, both of
  which name the parameter by position, so a `param` field would restate its own key and could be made
  to contradict it.

So the two shapes differ in two structural places, not one: where the message sits, and whether the
parameter is named. The strings themselves are shared, which is what the single payload type enforces.

*Alternative rejected:* leaving `details` as it is and having the report parse `token` and `value` back
out of the English message. That is a second, weaker statement of the same failure, and it breaks the
first time the message is reworded. *Alternative rejected:* carrying `param` in the read-only shape for
symmetry. Symmetry is not the goal; a field that duplicates the key it hangs under is a field that can
disagree with it.

**Resolution context is threaded, not stored.** `derive_inputs_internal`, `derive_inputs_for_label`,
`inputs_all`, `inputs_default`, `placeholder_data` and `TemplateRegistry::detail` take
`(&BTreeMap<String, String>, &DateTimeResolver)`; `TemplateDetail::from` is replaced by a builder taking
the same. The instant lives in the `DateTimeResolver` the callers already build, so nothing captures a
second clock read. *Alternative rejected:* holding the resolved report on the registry, refreshed at
load. It would go stale the moment a variable is edited, which is the failure this change exists to make
visible, and it would make a template's published contract depend on when the process last reloaded.

**Every `TemplateDetail` site carries the report, and every write path reads its context before it
writes.** All six `detail()` call sites (`src/api.rs:837`, `:867`, `:921`, `:1003`, `:1056`, `:1148`) are
in async handlers holding `state`, so each reads the two sources and captures one instant. A
settings-store failure is `AppError::internal`, as at `:1206-1216`.

Five of the six are on paths that mutate: `save_template`'s three (`:837`, `:867`, `:921`) build the
detail after `stage_and_replace`/`stage_and_publish_new` and `state.reload()`, and the group move builds
it after `move_template_file` (`:1056`). Reading the store *there* would let a store failure return
`500` for a template that had already been written or moved. So each of those handlers reads
`all_variables()` and `resolve_datetime_formats()` and captures its instant **before** the first
mutating call — before `state.before_publish()` on the save paths and before `move_template_file` on the
move path — and builds `ResolvedDefaults` from that captured context after the reload. The store reads
are the only fallible part; once captured, resolution is pure and cannot fail. This costs the two reads
on a request that later fails for an unrelated reason, which is a wasted read and not a wrong answer.

*Alternative rejected:* an empty report on the write paths. A client that just created a template seeds
a form from exactly that response, and a report present on one path and absent on another is a field
whose absence has two meanings. *Alternative rejected:* accepting the post-mutation `500` and
documenting it. It makes a successful write indistinguishable from a refused one for the caller, which
is a worse failure than the one the report exists to surface.

**`param_defaults` is keyed on `template.params`, and each entry holds exactly one of `resolved` and
`error`.** The Rust type is constructed only from the wrapper's `Result`, so the two cannot both be
present. The wire shape is `{"resolved": <value>}` or `{"error": {...}}` per key. *Alternative
rejected:* keying on either input list. `resolve_parameters_mode` iterates `template.params` and resolves
**every** declared parameter before any `when:` is evaluated (`src/render/mod.rs:186`), so a broken
default fails a render whether or not any item reads that parameter, and whether or not the branch that
reads it is active. `derive_inputs_internal` emits an entry only for a name the layout walk collected,
so a report keyed on inputs would omit exactly the parameters whose failure is hardest to find: one no
branch reads, and one only an inactive branch reads. Resolution must not be keyed to layout usage
either — `resolve_declared_defaults` iterates the declared parameters, not the collected names.

**The published value is the render path's, and the client adapts where a control cannot hold it.**
`coerce_param_value` renders a datetime to `BARE_DATETIME_FORMAT` (`%Y-%m-%d`), which
`<input type="date">` holds and `<input type="datetime-local">` does not, so the client's seeding helper
widens a bare date to `YYYY-MM-DDT00:00`. *Alternative rejected:* publishing a per-control shape from the
server. The point of the change is that the operator sees what will print; a value reshaped per control
stops matching that, and the shape would then depend on a `control` field that is itself derived.

**The coerced value crosses the wire as `ParamValue`.** The wrapper returns `serde_json::Value`, which
after coercion is only a string, a bool, or a number; the mapping to `ParamValue` is
`Number::as_i64 → Integer`, else `as_f64 → Float`, `Bool → Boolean`, `String → String`. A `length`
declaring `"80mm"` coerces through `serde_json::json!(f32)` and therefore publishes the JSON number
`80.0`, not `80`; a test comparing it must compare numerically rather than by token.

**The catalog index resolves against an empty install.** `src/bin/catalog-index.rs` passes an empty
variables map, `settings::resolve_datetime_formats_from(None)` and one captured instant. *Alternative
rejected:* a no-context derivation path that keeps today's `default.is_none()` rule. That is a second
definition of `required` living in a binary nobody reviews against the endpoints, and it would drift.
Nothing under `catalog/` declares a default today, so the index is byte-identical after this change.

**A parameter whose default fails to resolve is `required`, so the thumbnail invents for it.**
`placeholder_data` fills every entry that is `interpolated` and `required`, so a preview that returns
`422` today renders with a placeholder from this change. *Alternative rejected:* excluding a failed
default from the invention rule so the preview keeps failing. That is a carve-out defended by "it
preserves current behavior", which is a cost rather than a proof; the preview has never claimed a
caller's render would succeed, and `param_defaults` rides on the same response the grid already reads.

**ADR-0093, "A declared default is published as it resolves".** It supersedes, in part, ADR-0068's
consequence rejecting resolution in `GET /templates/{id}` on cacheability grounds
(`docs/adr/0068-datetime-parameter-type.md:82-84`), records that the response was never cacheable so the
concrete cost is one store read, and relates to ADR-0088 and ADR-0090, neither of which moves: resolution
stays at request time and a declared default stays deferred rather than copied. Confirm the number
against `docs/adr/` on `main` before writing it, and add its row to `docs/adr/README.md` in the same
commit.

## Risks / Trade-offs

- **Six paths gain a store read, and two of them gain a new failure mode.** `GET /api/templates/{id}` and
  `POST /api/templates/{id}/inputs` can now return `500` where they previously could not fail. →
  The reads are the two the thumbnail handler already takes on every catalog grid paint, against a local
  SQLite store; the failure maps to `AppError::internal` exactly as it does there.

- **The four write paths could have gained a worse failure mode: `500` after a successful write.** They
  build their detail after the file is published or moved, so a store read placed there would report
  failure for work that had landed. → The reads are hoisted before the first mutating call, as decided
  above, so a store failure refuses the request while nothing has changed; after that point the report is
  pure and cannot fail. Two scenarios in `template-inputs` fix this as contract rather than leaving it to
  the implementation's ordering. What is **not** claimed is that a write becomes transactional: those
  paths already run `state.reload()`, `confirm_written_template` and a read-back after the file lands
  (`src/api.rs:828-842`), and any of those can still fail after a successful mutation. This change adds
  nothing to that set and removes nothing from it.

- **`InputSpec.default` and `required` change meaning for existing clients.** A caller reading `default`
  as the declared text, or `required` as "declares no default", gets different answers. → Both are
  breaking and named as such in the proposal; the repository's only client is the bundled UI, which this
  change updates, and the field's published contract already said what it now carries.

- **The input-list endpoint can now report a different branch than it did.** Resolving a tokened default
  changes which gates pass. → That is the divergence `template-inputs` recorded as a known cost and
  reserved to this issue; the new answer is the render's answer, which is the one a screen needs.

- **A time-dependent default makes an entry time-dependent.** Two reads of the same template can publish
  different defaults, and a screen open across midnight can name a value the print will not use. →
  Stated in the spec rather than engineered around: the alternative is withholding `{sys.now}`-shaped
  defaults, which is the gap this change closes. What the contract promises is the rule and the sources,
  and equality *within* one request; it deliberately does not promise a value frozen across requests,
  which would be a promise the render path itself does not keep.

- **The report and the input lists are three projections that could drift.** → They read one
  `ResolvedDefaults` map by reference rather than each calling the resolver, so drift would need the map
  to be built twice in one request, which no path does. A test asserts a parameter's `param_defaults`
  entry, its `inputs.default` entry and its `inputs.all` entry agree in one response, and that the two
  endpoints agree under one snapshot.

## Migration Plan

No data migration and no config change: the resolution is per request and nothing is persisted. The
change is one commit, and rollback is reverting it — a client seeded from a resolved default degrades to
one seeded from the declared text, which is today's behavior. Nothing under `catalog/` or
`tests/fixtures/templates/` declares a tokened default, so `catalog/index.json` is unchanged and the
fixtures needed for the new HTTP tests are added by them.
