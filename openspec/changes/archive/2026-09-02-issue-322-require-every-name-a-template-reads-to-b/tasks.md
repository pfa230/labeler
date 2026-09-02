## 1. Site 1: a bare token names a declared parameter

- [x] 1.1 In `validate_interpolated_string` (`src/templates.rs:1424-1481`), refuse a
  `Source::Bare(name)` that is not a key of `params`, with a message naming the token and the
  undeclared name in the family `check_param_ref` already emits. One edit covers `text.value`,
  `qr.value` and an interpolated `image.src`, because all three already call it (design 1).
- [x] 1.2 Re-read `validate_params` (`src/templates.rs:1017-1028`) and confirm it still refuses a bare
  token in a `default:` **before** calling `validate_interpolated_string`, so a `default:` keeps its own
  message and the new check stays unreachable there (design 3).
- [x] 1.3 Unit-test the refusal in `src/templates.rs` for each site: a `text` `value:`, a `qr` `value:`
  and an interpolated `image` `src:` reading `{sku}` with no `sku` declared. Each case must fail before
  the change and pass after, so the assertion proves the check fires (design risks).
- [x] 1.4 Unit-test what the rule does not touch: `{vars.<key>}`, `{sys.now}` and `{sys.now:<fmt>}`
  still load undeclared, a `default: "{message}"` still reports the bare-token-in-default message, and
  a template printing `{datetime}` loads when it declares `datetime` and is quarantined when it does
  not (`interpolation-tokens`, "The retired bare spelling becomes an ordinary field").
- [x] 1.5 Unit-test that existence is the only condition: a template declaring `copies` as `integer`,
  `bold` as `boolean` and `width` as `length` and reading `"{copies} {bold} {width}"` loads
  (`interpolation-tokens`, "A bare token may name a parameter of any type").

## 2. Site 2: an `image` `name:` names a declared `string` parameter

- [x] 2.1 In the `image` arm of `validate_item_references` (`src/templates.rs:1539-1553`), keep the
  charset check and then call `check_param_ref(params, n, "image name", &["string"])`, in that order,
  so an illegal name is never reported as an undeclared one (design 2).
- [x] 2.2 Unit-test the three outcomes: `name: "logo"` with no `logo` declared is refused naming `logo`;
  `logo` declared as `integer` is refused naming `logo` and its type; `name: "my logo"` still reports
  the character class. The first two must fail before the change.
- [x] 2.3 Unit-test that a declared `string` `name:` still binds: the template loads, and a render with
  `logo` supplied as a PNG data URI draws it while a render omitting it is `422 MissingField` naming
  `logo` (`interpolation-tokens`, the two `image` binding scenarios).

## 3. Quarantine and the write path

- [x] 3.1 Add an HTTP-level test in `src/lib.rs`, alongside the existing quarantine tests, that a
  template file reading an undeclared name is quarantined at startup and on
  `POST /api/templates/reload` while the service starts and serves its valid siblings (#175).
- [x] 3.2 Add an HTTP-level test that the same content through a template write is refused with
  `422 TemplateInvalid` and `details.reason` `template_validation_failed`, and that nothing is stored.

## 4. The inputs derivation loses its undeclared branch

- [x] 4.1 Delete `undeclared_specs` and its `InputSpec` construction from `derive_inputs_internal`
  (`src/templates.rs:479-503`), together with the `multiline_text` flag that only fed it and the
  `NameInfo.order` / `next_order` bookkeeping that only ordered it; entries sort by name alone. Keep
  `image_bound`. A retained defensive branch must fail loudly rather than synthesize an entry
  (design 6).
- [x] 4.2 Unit-test the post-change entry rules: entries ordered by name ascending; a declared
  `multiline: true` `string` gets `textarea` while a declared `multiline: false` one read by a
  `wrap: true` item keeps `text`; an `image` `src: "{asset_path}"` over a declared `asset_path` gets an
  entry with control `text` (`template-inputs`, "An input list describes the controls one label
  needs").
- [x] 4.3 Unit-test the union rule: a declared `string` bound by an `image` `name:` in one branch and
  printed by a `text` item in another carries control `image` in `inputs.all` (`template-inputs`, "The
  template detail carries the lists a client needs before it has a label").
- [x] 4.4 Confirm no code change is needed for the `param-resolution` delta: `placeholder_data`
  (`src/templates.rs:165-198`) already fills only from `inputs.all`, so the thumbnail tests stay green
  as the proof (design 7).

## 5. Existing templates and test fixtures

- [x] 5.1 Declare the names that inline YAML in `src/templates.rs` and `tests/` reads, so every test
  template satisfies the new rule; a test that must read an undeclared name becomes a test of this
  rule instead.
- [x] 5.2 Give each inline `type: image` layout in `src/` and `tests/` a declared `string` parameter
  for its `name:` (15 sites).
- [x] 5.3 Confirm the 5 templates under `catalog/` and the 18 under `tests/fixtures/templates/` need no
  edit, which is the blast-radius claim the proposal makes.

## 6. Gates

- [x] 6.1 `cargo fmt`
- [x] 6.2 `cargo clippy --all-targets --all-features`
- [x] 6.3 `cargo test`
- [x] 6.4 Confirm the diff touches production code in `src/templates.rs` only (no changes to
  production code in `src/render/mod.rs`, `src/api.rs`, `src/batch.rs`, `src/convert.rs` or `ui/`),
  plus test modules in `src/templates.rs`, `src/lib.rs`, `src/render/mod.rs`, and
  `tests/acceptance_issue_263.rs` and documentation in `docs/AUTHORING.md`.
