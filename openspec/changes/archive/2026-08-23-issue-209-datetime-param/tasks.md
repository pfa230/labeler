## 1. Template schema

- [x] 1.1 Add `RawParamType::DateTime` to `src/raw.rs`, plus `time` and `format` on `RawParamSpec`. Every field the datetime rules inspect (`format`, `default`, `min`, `max`, `multiline`, `values`, `enum`, `time`), `format` included, parses presence-preserving via the existing `deserialize_present` pattern, so a written-but-null key is distinguishable from an absent one.
- [x] 1.2 Add `ParamType::DateTime { time: bool }` to `src/models.rs`, serialized as `type: "datetime"` with `time` **always** present (not skipped when false), per the GET scenario in the spec.
- [x] 1.3 In `TryFrom<RawParamSpec> for ParamSpec` (`src/convert.rs:219`), map the new type and reject `format`, `default`, `min`, `max`, `multiline`, `values` and `enum` on a `datetime` parameter, and `time` on any other type, each with a message naming the parameter and the attribute. `format` on any type points at the token spelling.
- [x] 1.4 Add the `ParamType::DateTime { .. }` arm to `check_param_ref` (`src/templates.rs:893`) so a `datetime` parameter is refused in every numeric context.
- [x] 1.5 Register the extended `ParamType` in `src/openapi.rs` and add `datetime_param_invalid` to `src/reason.rs`.
- [x] 1.6 Unit tests: each rejected attribute combination loads as a quarantined template with the expected message, including a forbidden attribute written with an explicit YAML null and `time:` written empty; a valid declaration round-trips through parse → convert → validate with `time` defaulting to false and serializing as `time: false`.

## 2. Parsing and formatting

- [x] 2.1 Add the override parser to `src/datetime_fmt.rs`: `%Y-%m-%d` at local midnight, `%Y-%m-%dT%H:%M[:%S]` as local wall-clock, RFC 3339 converted with `with_timezone(&Local)`; trim first. Resolve the naive forms through `Local.from_local_datetime`, taking `Single`, the earlier instant on `Ambiguous`, and erroring on `None`.
- [x] 2.2 Add `DateTimeResolver::resolve_param(token, instants)` formatting the bare ISO date or a named `datetime_formats` pattern, returning `missing_field("<p>.<name>")` for an unknown name.
- [x] 2.3 Unit tests: every accepted form; whitespace trimmed; a DST gap rejected and an ambiguous local time resolved to the earlier instant, over a fixed timezone; unknown format name errors; a non-datetime token returns `None`.

## 3. Render path

- [x] 3.1 Change `resolve_parameters` (`src/render/mod.rs:27`) to take `now: DateTime<Local>` and return `ResolvedParams { data, instants }`; resolve each `datetime` parameter from the request value or `now`, narrowing JSON (`Null` and blank string are omission, non-string is `datetime_param_invalid`) and writing the bare ISO string into `data`.
- [x] 3.2 Update both call sites (`:281`, `:574`) to pass `env.datetime.now`, verifying no third call site and no `Local::now()` remains below `api.rs`.
- [x] 3.3 Add `RenderContext::with_instants`, chain it on every context built from resolved label data (the auto-length measurement probe at `:332`, the final single-label context at `:388`, the per-label sheet context at `:587`), and carry the map into every child container context, on the measure path (`:1112`, `:1225`) and the render path (`:1738`, `:1835`); extend `interpolate` (`src/render/helpers.rs:42`) with the map, consulted after the `datetime` namespace and before `vars.`.
- [x] 3.4 Filter the parameter namespace out of `template_fields` and `placeholder_data` by head-of-token, so neither `{p}` nor `{p.<fmt>}` is advertised as a request field.
- [x] 3.5 Unit tests: bare and dotted token output; an unknown format is `422 MissingField` at render but loads fine; a request key named `p.<fmt>` cannot shadow the namespace; an override moves `{p}` but not `{datetime}`; every label on one sheet shares one instant; `template_fields` omits the namespace; a thumbnail renders a real date; **a dynamic-width (auto-length) template printing `{p.<fmt>}` renders**, which fails if the measurement probe lacks the map.

## 4. HTTP behavior

- [x] 4.1 HTTP integration tests: an omitted parameter prints today; `YYYY-MM-DD`, local `T` and RFC 3339 overrides; `null` behaves as omitted; an unparseable value and a numeric value are `400 InvalidRequest` with `details.reason` `datetime_param_invalid`; a batch with one bad label is `422 BatchInvalid` carrying that index, code and reason and producing no artifact.

## 5. UI

- [x] 5.1 Extend `ParamType` with `"datetime"` and `ParamSpec` with `time?: boolean` in `ui/src/api/types.ts`.
- [x] 5.2 Add the `datetime` branch to `ui/src/components/ParamInput.tsx`: `<input type="date">` or `<input type="datetime-local">` per `time`.
- [x] 5.3 In `ui/src/lib/templateFields.ts`: export the shared `hasServerDefault(spec)` helper including `datetime`; export `datetimeCellError(raw)`; teach the field-collecting functions the head-of-token rule so `{p.<fmt>}` is not a grid column; add the print-form-only seeding helper and leave `defaultParamValues` alone.
- [x] 5.4 Replace the three inlined `hasDefault` expressions with the helper in `ui/src/pages/print/FieldForm.tsx`, `ui/src/pages/Import.tsx` and `ui/src/pages/Connect.tsx`; call `datetimeCellError` from both grids' `validateRow`; seed the picker in `PrintForm`'s initial state.
- [x] 5.5 vitest: the control follows `time`; a blank `datetime` is not flagged required in the form and in both grids; an unparseable cell is flagged and blocks the run; `2026-02-30` and `2026-08-19T25:00` are errors; a cleared control submits no value.

## 6. Templates and visual check

- [x] 6.1 Add a `datetime` parameter to one template under `config-dev/templates/` printing `{p}` and `{p.<name>}`, render it to PNG via `POST /api/render/label?format=png` with `LABELER_NO_AUTH=true`, **open the image** and check the date reads correctly and stays inside the printable area; fix and re-render until it does.
- [x] 6.2 Exercise a sheet end-to-end through `POST /api/batch` with a sheet template carrying a `datetime` parameter (the bundled `scripts/render_avery_sheet.sh` drives `avery5163`, which declares none), rasterise the returned PDF and **open it**: every un-overridden slot shows the same date and an overridden slot shows its own.

## 7. Docs

- [x] 7.1 Write the ADR named in `design.md` under `docs/adr/`, taking the next free number (0066 is claimed by the in-flight #201 and #203 changes), and add its row to `docs/adr/README.md`.
- [x] 7.2 Add the `datetime` parameter to `docs/AUTHORING.md` by worked example, including the `time: false` with `{p.time}` pairing and when to reach for `{datetime}` instead.

## 8. Gates

- [x] 8.1 Run `cargo fmt`, `cargo clippy --all-targets --all-features` and `cargo test`; fix every finding at the root rather than with `#[allow]`.
- [x] 8.2 Run the UI checks (`npm run lint`, `npm run build`, `npm test` in `ui/`).
- [x] 8.3 Adversarial code review of the diff against the acceptance criteria in #209, per `AGENTS.md`; address or refute every finding with file:line evidence, then re-review until a pass surfaces nothing meaningful.
