## 1. The `repeat:` key in the template model

- [x] 1.1 Add `repeat: Option<Option<String>>` to `ContainerRaw` (`src/raw.rs`), read through
      `deserialize_present_typed` so a key written and left empty is distinguishable from an absent one,
      exactly as `shape`, `stroke`, `background`, `rounded` and `flow` are read.
- [x] 1.2 Add `repeat: Option<String>` to `LayoutItem::Container` (`src/models.rs`) with
      `skip_serializing_if = "Option::is_none"`, so a container carrying no `repeat:` serializes as it
      does today.
- [x] 1.3 Carry the key across in `try_into_container` (`src/convert.rs`), and refuse the `Some(None)`
      case there, naming `repeat` and the container's layout path, the way that function already refuses
      a null `stroke`.
- [x] 1.4 Refuse a repeating container whose parent does not arrange by flow, in the same place, from
      the `is_packed` flag `try_from_raw` already threads. The root call passes "not packed", which is
      what refuses `repeat:` on a root-level item.
- [x] 1.5 Test that `repeat` on a `text`, `qr`, `image` or `line` is refused as an unknown field naming
      that item's layout path. No code: `deny_unknown_fields` already gives this, and the test is what
      says so.
- [x] 1.6 Test that a template carrying a repeating container round-trips through
      `GET /api/templates/{id}`: the response holds the container once, carrying `repeat:` and neither
      `at` nor `to`, and resubmitting the returned document unchanged is accepted.

## 2. The load-time refusals, decided in the conversion

- [x] 2.1 Add a pass at the end of `TryFrom<TemplateDefinitionRaw>` (`src/convert.rs`) that walks the
      converted layout with the converted `params:` and the set of names currently repeated. It runs
      last, after every existing conversion error has had its chance to fire, so no template's current
      message changes.
- [x] 2.2 In that pass, refuse a `repeat:` naming a parameter the template does not declare, naming the
      key, the name and the container's layout path.
- [x] 2.3 In that pass, refuse a `repeat:` naming a declared parameter of any type but `list`, naming
      the key, the parameter, its declared type and the container's layout path.
- [x] 2.4 In that pass, refuse a nested `repeat:` over a parameter an enclosing `repeat:` already
      repeats, naming the parameter and the inner container's layout path. A nested repeat over a
      different declared `list` parameter is accepted.
- [x] 2.5 In that pass, refuse `{p:join('<sep>')}` inside a subtree repeating `p`, naming the token and
      the offending item's layout path.
- [x] 2.6 In that pass, refuse `{p:<name>}` written with a bare reader inside that subtree, naming the
      token and the item's layout path, in the message that says a format applies to an instant only,
      not the one naming `join('<separator>')`.
- [x] 2.7 Test each of the eight refusals (1.3, 1.4, 1.5, 2.2, 2.3, 2.4, 2.5, 2.6): the file is
      quarantined, the service still starts and still serves every other template, and the message names
      the offending item's layout path.
- [x] 2.8 Test at the HTTP level that a `PUT /api/templates/{id}` carrying each of those eight is `422`
      with `error.code` `TemplateInvalid` and `error.details.reason` `template_parse_failed`, that an
      existing template at that id is left byte-for-byte unchanged, and that a create-only write creates
      no file.
- [x] 2.9 Test that a `when:` key naming the declared list, on the repeating container itself, is
      refused exactly as `conditional-visibility` refuses it on a template carrying no `repeat:` at all.
      This change does not move that refusal.

## 3. The load-time permissions, in validation

- [x] 3.1 Thread the set of currently repeated names through `validate_item_references`
      (`src/templates.rs`), pushing a name when the walk descends through a container carrying
      `repeat:`. The root call passes "nothing repeated".
- [x] 3.2 In `validate_interpolated_string`, permit a bare token naming a repeated name inside its
      scope, which the list rules refuse today, and leave every other token rule untouched.
- [x] 3.3 In `validate_when_references`, permit a `when:` key naming a repeated name inside its scope,
      and keep the refusal everywhere else: on the repeating container's own `when:`, on a sibling, and
      on every item outside a repeat scope.
- [x] 3.4 Test that inside a repeat scope a `size: ["{p}", 4]`, a `color: "{p}"` and an `image` carrying
      `name: "p"` are each still refused naming the parameter and the context, exactly as outside a
      repeat. No code: `check_param_ref` and the `image` `name:` check already refuse a `list`, and the
      test is what pins that the scope did not open them.
- [x] 3.5 Test that outside every repeat scope nothing changed: a bare `{p}` on a declared list is still
      a load refusal, `{p:join(', ')}` still loads and prints the joined list, and a `when:` key naming
      the list is still refused.

## 4. Rendering: expansion and binding

- [x] 4.1 Add one expansion function in `src/render/mod.rs` that takes the authored children and the
      resolving context and returns the sequence to walk: an authored index, an optional zero-based
      element index, and the binding to walk it under. It decides activity, so a `when:` inside an
      instance is evaluated under that instance's binding.
- [x] 4.2 Call it from `measure_items` in place of the `is_item_active` filter, so the pre-pass produces
      one `Measured` per instance.
- [x] 4.3 Call it from the render walk in the same way, so the sequence it zips against those nodes
      cannot differ from the one measured.
- [x] 4.4 Walk each instance under a `RenderContext` whose `data` is the enclosing map with the repeated
      name overwritten by the element as a JSON string, so tokens and `when:` inside the subtree read the
      element and a nested repeat over a different list composes.
- [x] 4.5 Raise `422 MissingField` naming the parameter when an active repeating container's list is
      absent, and read nothing when its `when:` gate does not match.
- [x] 4.6 Name an instance in render-time failures as the authored layout path with the element index
      appended after a `#` (`layout[0].items[0]#3`), with items nested inside it extending that path as
      they otherwise would.
- [x] 4.7 Test the render, at the HTTP level, for: three elements drawn in request order; a declared
      `default:` supplying the elements; siblings keeping their places before and after the instances;
      `[]` and `default: []` drawing the strip with no instances and no error; an absent list being
      `422 MissingField`; a gated-off repeat requiring nothing; and the gate being evaluated once rather
      than per element.
- [x] 4.8 Test that each instance is sized on its own, by rendering three elements of different lengths
      into `size: [content, content]` instances and asserting each instance's drawn geometry rather than
      that a PNG came back.
- [x] 4.9 Test the arrangement cases: instances that overrun fail with `UnsupportedLayoutItem` and
      `details.reason` `item_out_of_frame` naming `layout[0].items[0]#2` for the third; the same
      container under `overflow: trim` draws the first two and succeeds; and under `wrap: true` the
      third begins a second line.
- [x] 4.10 Test that a repeating container written with neither `size` nor `to` renders one instance for
      a one-element list and fails with `item_out_of_frame` naming the second instance for a
      two-element one, which is `[fill, fill]` and `flow-layout` unchanged.
- [x] 4.11 Test the scope: a bare `{p}` inside prints one element; a `when:` inside compares the bound
      element; the same parameter joined outside the strip prints the joined list unchanged; and nested
      repeats over two lists draw the four combinations in order.

## 5. Input derivation and previews

- [x] 5.1 Record a `repeat:` key as an **interpolated** read of its parameter in the input derivation
      (`record_ref`, `src/templates.rs`), so the entry is present with `interpolated: true` whether or
      not the subtree prints the element.
- [x] 5.2 Expand a repeat in the per-label derivation, the way the render does, so a repeat over an
      absent or empty list contributes only its own name and the subtree's other reads appear once a
      label carries elements.
- [x] 5.3 Walk the subtree of every `repeat:` exactly once for `inputs.all`, whatever the parameter
      would resolve to for a label carrying no `data`, as that union already ignores every `when:`.
- [x] 5.4 Test the derivation: a repeat-only template reports `tags` with control `list`,
      `required: true` and `interpolated: true`; a `when:` read stays `interpolated: false`;
      `inputs.all` holds the subtree's other reads while `inputs.default` for a label with no data holds
      only `tags`; and `POST /api/templates/{id}/inputs` for a label carrying `tags: ["A"]` holds both.
- [x] 5.5 Test that the thumbnail of a repeat-only template draws one instance from the invented
      `["tags"]` rather than failing with `422 MissingField` or drawing an empty strip. No change to
      `placeholder_data`: its `list` arm already fills a required, interpolated entry.
- [x] 5.6 Add the missing `list` arm to `sampleData` (`ui/src/lib/preview.ts`), filling a one-element
      list holding the entry's own name, so a client preview sends a JSON array where the API requires
      one. This closes a pre-existing #213 defect, so write its test in `ui/src/lib/preview.test.ts`
      against the join spelling, an undefaulted `list` read by `{tags:join(', ')}`.

## 6. Gates

- [x] 6.1 Run `cargo fmt` and commit no formatting drift.
- [x] 6.2 Run `cargo clippy --all-targets --all-features` and fix the root cause of anything it flags,
      never with `#[allow(clippy::...)]`.
- [x] 6.3 Run `cargo test`.
- [x] 6.4 Run `npm --prefix ui run lint`, `npm --prefix ui run test` and `npm --prefix ui run build`,
      which is what CI runs for the one UI file this change touches.
