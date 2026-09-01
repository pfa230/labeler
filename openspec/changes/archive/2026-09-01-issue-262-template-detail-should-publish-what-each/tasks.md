## 1. One resolution and one failure payload

- [x] 1.1 In `src/render/mod.rs`, introduce the structured failure the resolution raises, carrying the parameter, the message, and `token` or `value` where one exists (at most one of the two), built where `resolve_and_coerce_default` fails today.
- [x] 1.2 In `src/errors.rs`, have `AppError::param_default_unresolvable` construct from that payload: keep the message in `error.message`, and add `param`, plus `token` or `value` where one exists, to `details` alongside `reason` through `reasoned`'s `extra` map. Do not duplicate the message into `details`.
- [x] 1.3 Add a public per-parameter wrapper in `src/render/mod.rs` that resolves one declared default in `ResolveMode::Strict` against a supplied variables map and `DateTimeResolver`, using a scratch `resolved`/`instants` pair, and returns the coerced value or that payload. Do not change how a render resolves.
- [x] 1.4 Add the conversion from a coerced `serde_json::Value` to `ParamValue`: `Number::as_i64` → `Integer`, else `as_f64` → `Float`, `Bool` → `Boolean`, `String` → `String`.
- [x] 1.5 Add `resolve_declared_defaults(template, variables, datetime) -> ResolvedDefaults`, a name-keyed map holding one entry per parameter that declares a `default:` and none for any other, each entry `Resolved(ParamValue)` or `Failed(ParamDefaultError)` so exactly one of the two is representable.

## 2. API models

- [x] 2.1 In `src/models.rs`, add `ParamDefaultError` serializing as `{ reason, message, token?, value? }` with no `param` field, and `ParamDefaultReport` serializing as `{"resolved": <value>}` or `{"error": {...}}`.
- [x] 2.2 Add `InputSpec.default_error: Option<ParamDefaultError>`, omitted when absent.
- [x] 2.3 Add `TemplateDetail.param_defaults`, a map from parameter name to `ParamDefaultReport`. Leave `TemplateSummary` and `ParamSpec` untouched.
- [x] 2.4 Register every new model in `src/openapi.rs`.

## 3. Derivation reads the map

- [x] 3.1 In `src/templates.rs`, give `derive_inputs_internal` a `&ResolvedDefaults` parameter and read each entry's `default`, `default_error` and `required` from it: `default` is the resolved coerced value, `default_error` the failure, `required` true exactly when the entry publishes no `default`. Delete the contains-`{`-or-`}` omission and the `spec.default.is_none()` rule.
- [x] 3.2 Thread the same reference through `inputs_all`, `inputs_default` and `placeholder_data` without re-resolving, so a parameter whose default failed is `required` and therefore invented for by the thumbnail's existing rule.
- [x] 3.3 Give `derive_inputs_for_label` the variables and the `DateTimeResolver` and pass them to `resolve_parameters_mode` in lenient mode, so a gate naming a tokened default is evaluated as a render evaluates it and a default that fails resolution leaves the parameter absent.
- [x] 3.4 Replace `impl From<&TemplateDefinition> for TemplateDetail` with a builder taking the resolution context, serializing the map as `param_defaults` and handing the same reference to both input lists; update `TemplateRegistry::detail` to take that context.

## 4. Handlers

- [x] 4.1 In `src/api.rs`, have `get_template` read `state.store().all_variables()` and `crate::settings::resolve_datetime_formats(state.store())`, capture one instant, build the map once, and return the detail; map a settings-store failure to `AppError::internal`.
- [x] 4.2 Have `template_inputs` build the map once per request and pass the same reference into every label's derivation, so one body's labels publish one resolution.
- [x] 4.3 On the four paths that write a template and return its detail (`save_template`'s three sites and the group move), read the variables and formats and capture the instant **before** the first mutating call, and build the report from that captured context after the reload. Add no store read after a mutation.
- [x] 4.4 Confirm `GET /api/templates` still returns `TemplateSummary` with no report and no resolved default.

## 5. Catalog index

- [x] 5.1 In `src/bin/catalog-index.rs`, build the map from an empty variables set, `settings::resolve_datetime_formats_from(None)` and one instant captured for the run, and keep filtering `inputs_all()` by `required`.

## 6. Client

- [x] 6.1 In `ui/src/api/types.ts`, add `InputSpec.default_error`, `TemplateDetail.param_defaults` and the two report types.
- [x] 6.2 In `PrintForm.tsx`, seed and re-seed from the published `InputSpec.default`, and add the one seeding adaptation: widen a bare `YYYY-MM-DD` to `YYYY-MM-DDT00:00` when the control is `datetime`, in both initial seeding and re-deferral.
- [x] 6.3 In `FieldForm.tsx`, offer the "use default" checkbox only for an entry publishing a `default`, keep its label naming that published value, and for an entry carrying `default_error` render no checkbox and surface the error's `message` against the entry.
- [x] 6.4 In `ParamInput.tsx`, stop substituting a default of its own for an absent value in the checkbox, select and slider branches; the entry's published default reaches it through form state.
- [x] 6.5 In `Import.tsx` and `Connect.tsx`, read requiredness from `InputSpec.required` rather than re-deriving it from the presence of a default, and flag a row whose entry carries `default_error` as needing a value.
- [x] 6.6 In `pages/TemplateDetail.tsx`, show the declared default as authored **and**, from `param_defaults`, either the resolved value or the failure's message; show neither for a parameter declaring no default.

## 7. Tests

- [x] 7.1 Update the existing derivation and HTTP tests in `src/templates.rs` and `src/lib.rs` whose signatures or expectations change.
- [x] 7.2 HTTP: a `{vars.<key>}` default the store holds — `param_defaults` reports `resolved`, `InputSpec.default` matches it, `required` is false; and the same template with the key absent — `200`, an `error` naming the token, no `default`, `required` true, `default_error` present.
- [x] 7.3 HTTP: a `{sys.now:<format>}` default resolving against the store's formats map.
- [x] 7.4 HTTP: a parameter declared with a broken default that the layout never references — present in `param_defaults` with its `error`, absent from `inputs`, and the render still fails.
- [x] 7.5 HTTP: a `boolean` default of `"yes"` reported as an `error`, and a `length` default of `"80mm"` reported as `resolved` `80`, compared numerically.
- [x] 7.6 HTTP: `POST /api/templates/{id}/inputs` and `GET /api/templates/{id}` agree on the same parameter under one snapshot, and every label in one inputs body carries the same published default.
- [x] 7.7 HTTP: `GET /api/templates` is unchanged.
- [x] 7.8 The render path's structured `422`: `error.details` carries `reason`, `param` and `token` or `value`, and the read-only report carries the same strings with no `param`.
- [x] 7.9 A store read failing on a write path returns `500` **and** leaves no template file written, moved or replaced; a successful write returns a detail body carrying `param_defaults` for the template as written.
- [x] 7.10 A thumbnail for a template whose default cannot resolve renders with a placeholder, while a render of the same template omitting that parameter still returns `param_default_unresolvable`.
- [x] 7.11 The catalog derivation with no install lists a `{vars.…}`-defaulted parameter as a field and does not list a `{sys.…}`-defaulted one.
- [x] 7.12 UI: `TemplateDetail` shows declared and resolved side by side and the diagnostic in place of a resolved value; `PrintForm`/`FieldForm` seed from a resolved default, offer no deferral for a `default_error` entry, surface its message, and widen a bare date into a `datetime-local` control; `ParamInput` substitutes no default of its own; `Import` and `Connect` read requiredness from `required` and flag a row whose entry carries `default_error`.

## 8. Decision record

- [~] 8.1 Write ADR-0093, "A declared default is published as it resolves". **Done, then dropped on merge.**
  `main` froze `docs/adr/` at ADR-0091 in `8100b4f` while this change was in flight, and `AGENTS.md`
  now reads "Do not write ADRs and do not add rows". The ADR was written and reviewed under the rule
  in force when this change was planned, and removed when the branch merged. Its rationale, which the
  freeze names as the better record, is `proposal.md` and `design.md` in this folder: resolving a
  default in `GET /templates/{id}` supersedes in part ADR-0068's rejection of it on cacheability
  grounds, and the response sets no `ETag` and no `Cache-Control`, so the concrete cost is one store
  read.
- [~] 8.2 Add its row to `docs/adr/README.md`. **Done, then dropped on merge**, with 8.1.

## 9. Gates

- [x] 9.1 Run `cargo fmt`, `cargo clippy --all-targets --all-features` and `cargo test`, and fix what they flag without silencing a lint.
- [x] 9.2 Run `npm run lint`, `npm run test` and `npm run build` in `ui/`.
