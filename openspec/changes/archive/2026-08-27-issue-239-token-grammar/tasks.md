## 1. The grammar, in one place

- [x] 1.1 Add `src/interpolation.rs`: `Source` (`Bare` / `Vars` / `Sys`), `SysValue` (one variant, `Now`), `Token { source, format, raw }`, a `TokenError` carrying the distinct causes (unknown source, unknown system value, malformed name, empty segment, empty or repeated format), and `parse(raw) -> Result<Token, TokenError>`.
- [x] 1.2 Add the scanner that walks an interpolated string yielding well-formed tokens with their offsets, honouring `{{` / `}}` and skipping brace-malformed text (which stays a render-time `interpolation_syntax` error, per design.md).
- [x] 1.3 Unit-test `parse` over every shape the spec names: bare, `vars.<key>` including a key containing dots, `sys.now`, each with and without a format; and the refusals `{}`, `{ id }`, `{my field}`, `{a.b}`, `{vars.}`, `{sys.}`, `{.x}`, `{:fmt}`, `{x:}`, `{x:a:b}`, `{VARS.x}`, `{Sys.now}`, `{sys.nwo}`, `{sys.now.long_date}`. Each refusal asserts the *cause*, not merely that it failed.
- [x] 1.4 Register the module in `src/lib.rs`.

## 2. Load-time validation, and the write path with it

- [x] 2.1 Extend `validate_item_references` (`src/templates.rs`) to walk `text.value`, `qr.value` and `image.src` through the scanner, and to reject: an unknown source, an unknown `sys` value, a malformed or empty segment, and a format attached to a value that is neither `sys.now` nor a declared `type: datetime` parameter.
- [x] 2.2 Make each refusal message name the offending token and state the replacement spelling, as the spec's THENs require (`{datetime.long_date}` and `{sys.now.long_date}` both name `{sys.now:long_date}`).
- [x] 2.3 Bind `image` `name:` to the bare-name rule in the same walk.
- [x] 2.4 Delete the reserved-word branches from `validate_param_name` (`src/templates.rs:766-771`), leaving the character-class rule and its reason in a comment.
- [x] 2.5 Test that a template file carrying each refusal is quarantined and the server still starts and serves the others, and that the same content through `PUT /api/templates/{id}` is `422 TemplateInvalid` / `template_validation_failed` with nothing stored (`src/api.rs:638-644` shares the `validate()`).
- [x] 2.6 Test that a parameter named `vars`, `sys` or `datetime` now loads, and that `{vars}` resolves the parameter while `{vars.<key>}` resolves the store.

## 3. Render path

- [x] 3.1 Rewrite `interpolate` (`src/render/helpers.rs`) to parse each token once and match on the result, deleting the try-each-source fall-through and the fall-through to `data.get` for unrecognised text.
- [x] 3.2 Reduce `DateTimeResolver` (`src/datetime_fmt.rs`) to formatting: it takes an instant and a format name, and reports an unknown name as `422 MissingField` naming the whole token text `<value-path>:<format-name>`. Token splitting leaves this file.
- [x] 3.3 Resolve `sys.now` from the instant the request already captured (`src/render/mod.rs:350`, `:646`), touching neither the capture nor the single-instant guarantee.
- [x] 3.4 Test each render-time error separately: absent bare field, absent `vars` key, unknown format name; and test that one sheet prints one instant.

## 4. Advertised fields and placeholders

- [x] 4.1 Route `collect_data_tokens` (`src/render/mod.rs:2080-2104`), `template_fields` and `placeholder_data` through the parser, filtering down to `Source::Bare` tokens that do not name a declared parameter.
- [x] 4.2 Unit-test that `{datetime}` produces the advertised field `datetime` while `{sys.now}` and `{sys.now:<fmt>}` produce nothing; that `{vars}` produces `vars` while `{vars.<key>}` produces nothing; and that a parameter named `vars`, `sys` or `datetime` is excluded from advertised data fields as every declared parameter is.

## 5. Connector capture-group regex validation

- [x] 5.1 Update the transform-spec validation in `src/connector/mod.rs` (`validate_transforms` / `validate_single_transform`, around `:244-250`, `:1076-1093`): delete the reserved-names check (`datetime`, `vars.`, `datetime.`) and replace it with the bare-name check `^[a-zA-Z0-9_-]+$`.
- [x] 5.2 Update connector tests in `src/connector/mod.rs` to assert that group names matching `^[a-zA-Z0-9_-]+$` (including `datetime`, `vars`, `sys`) are accepted, while names with dots, colons or spaces are rejected.

## 6. UI

- [x] 6.1 Update `ui/src/lib/templateFields.ts`: regex scanner recognises the new grammar (`{value-path}` and `{value-path:format}`), `isDataField` returns `false` for `vars.*` and `sys.*`, `true` for bare `datetime` and every other bare name, `referencedVariables` strips any format suffix, and the test suite covers the new shapes.
- [x] 6.2 Update the format help text in `ui/src/pages/settings/DatetimeFormatsSection.tsx:266` to name `{sys.now:<name>}`.
- [x] 6.3 Update `ui/src/lib/templateFields.test.ts` for the new spellings, and add a case asserting a `sys.` token is not a data field.

## 7. Templates, docs and the decision record

- [x] 7.1 Rewrite `tests/fixtures/templates/homebox-qr.yaml:33` to `{sys.now:iso_date}` and `brother_24mm_printed_on.yaml:28` to `{printed_on:short_date}`, and update the render tests naming those tokens (`src/render/mod.rs:5190`, `:5473-5594`, `:5667`).
- [x] 7.2 Rewrite the datetime section of `docs/AUTHORING.md` (`:540-570`) for the colon and `{sys.now}`, including the two consequences an author will meet: a format applies only to an instant, and a bare name may carry neither a dot nor a colon.
- [x] 7.3 Write the ADR stating the whole grammar, superseding ADR-0028 outright and the token-list portion of ADR-0068. Confirm the next free number against `main` and every live worktree first (`main`'s highest is 0076; the in-flight #226 worktree claims 0076, 0077 and 0078).
- [x] 7.4 Add the ADR's row to `docs/adr/README.md`, and mark ADR-0028 superseded in its own header and index row.

## 8. Gates

- [x] 8.1 `cargo fmt`, then `cargo clippy --all-targets --all-features` with no new lint and no `#[allow]`, then `cargo test`.
- [x] 8.2 Run the UI unit tests.
- [x] 8.3 `openspec validate issue-239-token-grammar --strict`, and `.workflow/review-gate-check.sh` against the worktree root.

<!-- No render-and-look task, deliberately. AGENTS.md ("Templates are visual artifacts") records that
     the loop runs against a running server and a config dir outside the repository, so its only
     evidence is an image no later reader can retrieve, and that no task should claim it (#220). The
     two files touched here are test fixtures whose correctness is the test that reads them, and this
     change alters token spelling only: no coordinate, size, font or layout value moves. -->
