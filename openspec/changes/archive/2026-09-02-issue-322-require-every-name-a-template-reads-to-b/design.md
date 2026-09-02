## Context

See `proposal.md` for motivation and #322 for the scope. This is a **load-time** change: the render
path is not touched, and what a request may carry is #324's. The constraints that shape it:

- **The rule and its message already exist.** `check_param_ref` (`src/templates.rs:1376-1401`) returns
  `undeclared parameter '<name>' referenced in <context>` and also type-checks against a list of
  allowed types; `validate_when_references` (`:1403-1422`) does the same for a `when:` key. Both are
  reached from `validate_item_references`, which walks every item.
- **One validator already sees all three interpolated sites.** `validate_interpolated_string`
  (`src/templates.rs:1424-1481`) is called for a `text` `value:` (`:1497`), a `qr` `value:` (`:1524`)
  and an `image` `src:` (`:1558`). It scans tokens and today decides only syntax, unknown sources and
  whether a `:format` is applied to an instant.
- **It is also called for a `default:`, but the new check cannot reach one there.**
  `validate_params` (`:1028`) calls it, but only after refusing any bare token in a `default:` outright
  (`:1017-1026`), so a `default:` keeps its own message and its own rule.
- **An `image` item's `name:` is checked for charset and nothing else** (`src/templates.rs:1539-1553`),
  while the renderer resolves it out of the resolved data map (`src/render/mod.rs:1636-1641` and
  `:2119-2126`).
- **An `image` item's source count is settled elsewhere and earlier.** `LayoutItemRaw::Image` refuses
  both `src` and `name`, and neither, in `convert.rs:408-421`, so `docs/SPEC.md` §4.1's "exactly one of
  `src` or `name`" is already a load-time refusal, at the parse stage before validation runs.
- **The input-list walk collects names from the layout, not from the request**
  (`derive_inputs_internal`, `src/templates.rs:199-503`). Once every name the layout reads is declared,
  its `undeclared_specs` branch, the `multiline_text` flag that only feeds it, and the `NameInfo.order`
  / `next_order` bookkeeping that only orders it are all unreachable.
- **No endpoint accepts `multipart/form-data`.** `multer`, which axum's `Multipart` extractor needs, is
  absent from `Cargo.lock`, and no source file mentions multipart. The frozen §8 phrase "a multipart
  upload / data URI via `name`" describes a spelling that was never built.

## Goals / Non-Goals

**Goals:**

- One load-time rule for every name a template reads, with no site left exempt.
- The two new refusals carry the message the rest of the model already emits.
- The inputs contract carries no undeclared branch, in the spec and in the code.

**Non-Goals:**

- What a request may carry. #324 owns it. A `data` key naming no declared parameter is still cloned
  into the render data and still read by nothing, and no requirement here says otherwise.
- Anything at render time. No new status, `error.code` or `details.reason`; no change to coercion,
  defaults, `when:` evaluation or `MissingField`.
- Changing how many sources an `image` item may carry, or when that is decided. §4.1 owns it.
- Adding a multipart upload path. The superseded §8 bullet mentions one; the service has none, and the
  delta removes the phrase rather than building the feature (decision 4).
- Fixing the geometry-`ref:` fallbacks (#261) or removing `truncated_elsewhere` (#269), both of which
  live in code this change touches.
- Any UI change. Every name the service reports as an input is now a declared parameter, which narrows
  what the screens already render.

## Decisions

### 1. Both sites land in the validators that already exist, and nowhere else

`validate_interpolated_string` gains one check on a `Source::Bare(name)`: the name must be a key of
`params`. That single edit covers `text.value`, `qr.value` and an interpolated `image.src`, because all
three already call it, and it puts the refusal in the same pass that already reports an unknown source
or a format on a non-instant, so a template with two faults reports through one mechanism.

The `image` arm of `validate_item_references` keeps its charset check and then calls
`check_param_ref(params, n, "image name", &["string"])`, so the undeclared message is the one the rest
of the model emits and the wrong-type message is the one `check_param_ref` already writes.

*Alternative rejected:* a new validation pass over the layout that collects every read name and
compares it against `params` once. It would duplicate the walk `validate_item_references` already does,
and it would have to re-derive which context each name was read in to write the message.

*Alternative rejected:* checking at render instead. The information is entirely in the template file,
and this project's rule is that a template's own text is decided at load; deciding it at render would
report a template author's mistake as the caller's, on every request.

### 2. Existence only, and the charset check goes first

A bare token stringifies whatever it names, so there is no type to restrict: the check is `params`
membership and nothing more. `image` `name:` is the exception and takes `["string"]`, because the value
it binds is a data URI, which is what a `string` carries and what the `image` control publishes for it.

Within the `image` arm the charset check SHALL precede the declaration check. An illegal name like
`my logo` can never be declared, so the declaration check would fire first and report "undeclared
parameter", sending the author to `params:` instead of to the space in the name.

### 3. `default:` keeps its own rule, and the implementer must confirm the order

`validate_params` refuses a bare token in a `default:` before it calls `validate_interpolated_string`,
so the new check is unreachable there and the existing message survives. That is an ordering the
implementer should re-read rather than assume: if the two were ever swapped, a `default: "{message}"`
would start reporting "undeclared parameter" instead of "bare token not allowed in a default", which is
the wrong advice — the fix is to use `{vars.…}`, not to declare `message`.

### 4. The first-touch image requirement carries three things it must not do

The `ADDED` requirement supersedes the §8 `image` binding bullet, so it must carry that bullet's
complete post-change contract. Three traps, all confirmed against the code:

- **It must not restate the rest of §8.** That section's `{datetime}` and `{datetime.<name>}` tokens are
  already retired by `interpolation-tokens`; copying them back would resurrect them.
- **It must not restate §4.1's "exactly one of `src` or `name`".** `convert.rs:408-421` already refuses
  both and neither at parse time. Restating it would create a second home for a rule this change does
  not touch, and an earlier draft that did so got it backwards, describing a precedence where there is
  a refusal.
- **It must drop "multipart upload" in words.** A requirement claiming to be a complete restatement
  cannot quietly describe only the data URI. The delta says the phrase is removed and why: no endpoint
  accepts `multipart/form-data`, `multer` is not in `Cargo.lock`, and `name:` has only ever resolved
  against the request `data` map. Nothing that works today stops working, and frozen §4.1 already
  describes `name:` as a data key holding a base64 data URI, so the two frozen sections stop
  disagreeing.

### 5. The mapping requirement keeps two kinds of name apart

"A value a bare token cannot name is bound by mapping, not by spelling" previously said a request
`data` key, a CSV header **and** a connector field key must each be a legal bare name to be read
directly. Adding "and declared" to that sentence as written would contradict the requirement's own
exception, whose whole point is that `custom:Internal SKU` stays reachable while being neither.

The restatement splits them: a name a template reads **directly** (a `data` key, a CSV header) must be
a legal bare name and must be declared; a connector key is read by no template, reaches a label only
through the mapping, and is subject to neither rule. The mapping's *template* side is the declared
name. This is also why the requirement's scenario now declares `internal_sku`.

### 6. `derive_inputs_internal`'s undeclared branch is deleted, and `NameInfo.order` with it

With every collected name declared, `undeclared_specs` and its `InputSpec` construction
(`src/templates.rs:479-503`) have no reachable input, and the `multiline_text` flag that only feeds
them goes too. The ordering rule becomes "by name, ascending" alone, so `NameInfo.order` and
`next_order` have no consumer either. The issue leaves this to the implementer; the delta forces it,
because the spec now states one ordering rule and a second one in code would have nothing to produce.

If the implementer prefers to keep a defensive branch, it must fail loudly — an `AppError::internal`
class of failure — and not silently fall back to a synthesized entry, per the no-silent-fallbacks rule.
(Implementation note: `derive_inputs_internal` uses `panic!` for this defensive branch because
validation guarantees every collected name is a subset of validated parameters, avoiding signature
changes to `derive_inputs_internal` and its callers).

`image_bound` stays: it is the `image` control override, which is decided by use and applies to a
declared `string`.

### 7. The preview requirement loses its undeclared wording, and nothing else

`param-resolution`'s preview requirement is written around a case this change deletes: it calls a
preview "the one place the service supplies a value the template does not declare", and builds a
placeholder for "every request field or declared parameter" a token reads. After this change a template
reading an undeclared name is quarantined, so no preview of it is ever derived and neither clause can
describe anything. Left as written, the main specs would carry a contradiction of the token rule, and
the tooling requires a `MODIFIED` delta for a requirement that already exists.

The delta changes those two clauses and nothing else: every rule, every paragraph after them and all
eight scenarios are restated verbatim. No behaviour moves, because `placeholder_data` already fills
only from `inputs.all` (`src/templates.rs:165-198`), whose entries come from the layout walk and are
now all declared.

The neighbouring case stays as it is, and the delta does not touch it: a request `data` key naming no
declared parameter is still accepted and still read by nothing. That is #324's, and a preview never had
such a key to begin with, since it builds its own data.

### 8. Scenario names the tooling pins

`openspec validate` refuses a `MODIFIED` requirement that drops any scenario name the current spec has,
so three scenarios whose subject this change deletes keep their names and carry restated bodies:
"An undeclared name read by a multiline item gets a textarea", "Entries are ordered by name, then by
first use" and "The union prefers the wider control for an undeclared name". Each body states the
post-change truth, and the first and third are useful in their own right, because they pin the new
refusal at exactly the site the old rule lived.

*Alternative rejected:* `REMOVED` plus `ADDED` under a new requirement name, which is how #291 shed
scenarios. It would rename three requirements whose subject has not changed, and requirement names here
are cross-referenced by other capabilities' prose. Renaming a contract to satisfy a validator rule is a
worse trade than three stale scenario titles that their own bodies correct.

### 9. The spec says what a now-unreadable request key does, and stops there

The token requirement records one consequence in a sentence: a `data` key naming no declared parameter
is read by nothing, because no token can reach it. It then says explicitly that it decides nothing
about whether such a key may be sent, and names #324.

Saying nothing at all was the alternative, and it is worse: a reader of the post-change token rule will
ask what happens to the key, and an unanswered question in a contract gets answered by whoever
implements next. Saying it is refused would be this change quietly doing #324's job.

## Risks / Trade-offs

- **A template in the wild that reads an undeclared name stops loading.** → That is the change, and
  pre-1.0 it ships without a migration. It is a quarantine, not a crash: the file is skipped with a
  message naming it and the name, every other template still serves, and the server still starts
  (#175).
- **The same content arriving through `POST`/`PUT` is now refused.** → Consistent with every other
  load-time rule in this capability, and it stops an operator saving a template the loader would
  quarantine.
- **Inline test YAML across `src/templates.rs` and `tests/` leans on undeclared names.** → Expected;
  declaring the name is the fix. The 15 inline `type: image` layouts additionally need a declared
  `string` for their `name:`. A test that must read an undeclared name is now a test *of this rule*.
- **A test could assert the refusal without proving it fires.** → Each new refusal needs a case that
  fails before the change: a template file that loads today and is quarantined after. Asserting only
  that a well-formed template still loads would pass against an implementation that checks nothing.
- **The thumbnail and preview paths could break if any placeholder name were undeclared.** →
  `placeholder_data` fills only from `inputs.all`, every entry of which is a declared parameter after
  this change, so nothing there can name an undeclared key. Keeping the thumbnail tests green is the
  proof.
- **`truncated_elsewhere` and `interpolated` bookkeeping is now partly vestigial.** → Out of scope;
  #269 removes `truncated_elsewhere`, its computation and the print form's note together.
