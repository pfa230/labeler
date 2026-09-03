## Context

See proposal.md for motivation. The constraints that shape the approach are all in code that already
exists, and three of them were misread in the first draft of this design.

- **`scan_tokens` decides where a token ends** (`src/interpolation.rs:180-215`), and it is shared by the
  render path, by the load-time validators, and by `validate_default_syntax`, whose contract depends on
  its skipping a malformed brace sequence rather than reporting one. Its behaviour is not "abandon on a
  brace": it skips a doubled `{{` or `}}` **before** looking for a token, ends a token at the first `}`,
  and abandons and restarts one character later at a nested `{`. Anything a separator literal may contain
  is bounded by that function before any parser sees it, and *which* refusal a brace produces follows
  from it.
- **`parse` counts colons** (`:110-113`): more than one is `InvalidFormat`. A `join(': ')` has two, so a
  separator holding a colon is decided by how the token is split, not by a policy.
- **`Token.format` is `Option<&str>`** and every consumer branches on it: `validate_interpolated_string`
  (`src/templates.rs:1424-1481`) for the load refusals, and `interpolate`
  (`src/render/helpers.rs:78-144`) for resolution.
- **A parameter type moves three files together**: `raw.rs` (`RawParamType`, `RawParamSpec`, whose
  forbidden attributes are already `Option<Option<T>>` so presence is visible), `models.rs`
  (`ParamType`, `ParamValue`), `convert.rs` (`TryFrom<RawParamSpec>`, `convert_raw_default`).
- **`value_to_string`'s catch-all is the defect** (`src/render/helpers.rs:29-36`) and it has **six** live
  consumers, not four: interpolation (`:131`), the render path's `when` evaluation
  (`src/render/mod.rs:1295`), the input derivation's own `when` evaluation (`src/templates.rs:275`),
  `coerce_param_value`'s non-numeric arms (`src/render/mod.rs:54,71,143`), and the two `image` `name:`
  bindings that stringify a `data` value straight into `parse_image_data_uri`
  (`src/render/mod.rs:1636-1642` in the measure pass and `:2120-2125` in the render pass). The last pair
  is the one scalar slot a template writes with no `{token}` in it.
- **`validate_when_references` already refuses every `when:` key absent from `params:`**
  (`src/templates.rs:1403-1419`), reached for every item type including nested container children
  through `validate()` -> `validate_references()` (`:1107-1108`, `:1583-1616`). An undeclared `when:` key
  is a load refusal today.
- **The input list is one derivation** serving the print form, the CSV and connector grids, the
  thumbnail's placeholder data (`src/templates.rs:165-197`) and the catalog index. Five consumers read
  it, so a type that is absent from it is absent five times, and a type that is present needs no
  consumer to be told about it.

## Goals / Non-Goals

**Goals:**

- One rule for where a list may be read, decidable at load wherever the template's own text decides it.
- No path on which a JSON array reaches a scalar slot and prints something, where "scalar slot" covers
  the `image` `name:` binding as well as every token.
- Symmetry between a declared `default:` and a request value, which `param-resolution` already requires.
- No stored operator configuration loses its meaning.

**Non-Goals:**

- Any repetition construct. `{tags:join(', ')}` is one text run; one item per element is #319.
- The screens that draw the new control (#318), and the CSV and connector spellings (#320, #321). This
  change adds the `list` control to what the service **reports**; it builds no editor for it. What it
  does owe the UI is small and is not an editor: the declared API types are corrected, and every screen
  that renders a control per reported input skips the one it cannot draw. That second half is required
  by the control-table requirement and is the one UI-visible obligation here; without it a screen falls
  through to a text input whose value is guaranteed a `400`, which is drawing a control it cannot draw
  rather than omitting it.
- Element typing, a per-element value set, or a `contains` operator on `when`.
- Legalising `when:` keys that name nothing the template declares.
- Fixing the JSON-text stringification of a JSON **object**, which is a different shape and outside #213.

## Decisions

### Parse the token by structure, not by counting colons

`parse` splits at the **first** `:`; everything after it is the reader, which is either a bare name
matching `^[a-zA-Z0-9_-]+$` or a `join('<sep>')` call. `Token.format: Option<&str>` becomes
`Token.reader: Option<Reader>` with `Reader::Format(&str) | Reader::Join(&str)`, and the two consumers
that branch on it gain a third case each.

This preserves every refusal the colon count produced: `{x:a:b}` splits to the reader `a:b`, which is
neither a legal bare name nor a call, so it is still `InvalidFormat`.

*Alternative: keep the colon count and forbid `:` inside a separator.* Rejected. It buys nothing the
structural parse does not, and it adds a second rule about the separator's contents whose only
justification would be the implementation, which is the shape of exception this repo refuses without a
proved cost.

### The parenthesized argument makes a join, not the word `join`

A reader written bare is a format name, whatever it spells. A reader written with an argument is a join,
and `join` is the only spelling an argument may follow. `{sys.now:join}` therefore still resolves a
`datetime_formats` entry named `join`, exactly as it does today.

*Alternative, and the first draft's choice: reserve `join` as a reader name everywhere,* so it is never
looked up in `datetime_formats`. Rejected on the repo's stored-data rule. `datetime_formats` is stored
settings whose entry names are the operator's to choose (`docs/SPEC.md:1054-1058`, `src/settings.rs`
seeds five and an operator may replace the map wholesale), so reserving the word would strand a stored
value that has no author to fix it, and this project breaks stored user data only with a migration.
There is no migration worth writing for it either: the alternative that costs nothing is simply not
reserving the word.

The discriminator is now syntactic rather than semantic, which is what makes it a rule and not a
special case: `join` and `join(...)` are different tokens, and no rule anywhere asks what a value is in
order to decide what its reader means. It also disposes of `{tags:join}` without a rule of its own, since
a bare reader is a format name and a list is not an instant; only the *message* is special-cased, to
name the spelling the author meant.

### A join attaches only to a bare token naming a declared `list`

Not to an undeclared name, and not to any other declared type. The consequence is the tightest statement
this change can make: **an array is printable only through a parameter declared `type: list`.**

*Alternative: allow `{items:join(', ')}` on an undeclared name and decide at render.* Rejected. The
template would load and then fail for one caller and work for another, which is the failure mode the
"decidable at load" rule exists to remove; and it would give an array two ways to reach a slot, one of
which the load-time refusal for declared lists forbids.

### Elements are strings, and neither side coerces

A list holds strings. A `default:` element that is not a YAML string scalar, and a request element that
is not a JSON string, are both refused, naming the parameter and the offending element's position.

*Alternative, and the first draft's choice: stringify any scalar element* (`1` to `"1"`, `null` to `""`).
Rejected. #213 defines an ordered list of strings and says outright that an author wanting numbers writes
`["1", "2"]`, so coercing would widen the accepted domain past the scope, and it would make the declared
type disagree with the type table, which publishes the request value as a JSON array of strings.
Quoting is the whole of what an author or a caller does about it, and `["1", "2"]` joins identically to
what `[1, 2]` would have.

Refusing on **both** sides is what keeps `param-resolution`'s rule that a default may not carry a value
the request could not have carried. That same rule is why the change also refuses a **sequence
`default:` on every other type**: refusing an array from a request while `convert_raw_default` kept
accepting one from a declaration (holding it as `format!("{other:?}")`, `src/convert.rs:527`) would open
exactly that gap, and would leave a template able to print `Sequence [String("a")]` on a `string`
parameter.

### The separator admits no escape, and a brace in one is decided by the scanner

The literal runs from the first `'` after the `(` to the next `'`. A further `'` before the `)` is a load
refusal. An apostrophe therefore cannot be a separator.

*Alternatives: SQL-style doubling (`''`), or a backslash escape.* Both rejected. A backslash inside a
double-quoted YAML scalar is itself a YAML escape, so `"{tags:join('\\'')}"` is a spelling nobody gets
right twice. Doubling is a second spelling for one character, and the whole point of refusing is that
adding it later turns a load error into an accepted value, which is additive and breaks nothing.

`scan_tokens` is **not** changed, and the delta says what that costs rather than promising more than it
delivers. Traced against the real algorithm, a separator carrying a brace produces one of two outcomes
and never renders:

- `{tags:join('}')}` scans to the token `{tags:join('}` and `{tags:join('{')}` scans to the token
  `{')}`; both fail to parse, so both are load refusals, each naming a token boundary the brace moved
  rather than the separator the author wrote. The delta states that plainly.
- `{tags:join('{{')}` produces **no token**, because the doubled brace is skipped before any token is
  looked for. The string then carries an unmatched `{` and is refused by the brace-balance rule that
  already governs its site: at load for a `default:`, at render as `400 InvalidRequest` with
  `interpolation_syntax` for a `text`, `qr` or `image src` value.

*Alternative: make the scanner quote-aware so every brace case is a load refusal.* Rejected twice over.
A stray `'` anywhere in a template would change how braces are found in text that has nothing to do with
lists, and `validate_default_syntax` is built on the current skipping behaviour. *Alternative: apply the
brace-balance rule to `text`, `qr` and `image src` values at load.* Rejected because a standing
`interpolation-tokens` requirement explicitly declines to extend it to those three sites; changing that
is a decision about three sites this change does not otherwise touch.

### An `image` `name:` is a scalar slot and is covered on both halves

`name:` binds a `data` field with no token, and both binding sites stringify it into
`parse_image_data_uri`. Two rules close it:

- naming a declared `list` there is a **load** refusal, alongside the numeric and colour `ref:` refusals,
  because a data URI is one string and a list is never one;
- an array arriving under an **undeclared** name it binds is the same render-time refusal a token gets,
  `field_value_not_scalar`, decided *before* the data-URI parse. Left alone it would surface as
  `image_data_invalid`, which reports a malformed data URI to a caller who never wrote one.

Without both, the "only through a join" guarantee would have a hole in the one slot a template can write
without a `{token}`.

### The undeclared-array refusal is `422 UnsupportedLayoutItem` / `field_value_not_scalar`

#213 specifies `422` and names no code. `MissingField` is wrong because the field is present.
`UnsupportedLayoutItem` is the code a caller's `data` value already fails with when an item cannot use
it: `image_data_invalid` and `image_format_unsupported` are exactly this shape, decided at render, about
the caller's data, naming the field in the message. Using it also means the `image` `name:` case above is
a reason swap rather than a code change.

*Alternative: `400 InvalidRequest` with the same slug,* which is what the declared-parameter coercion
path returns for a value it cannot use. Rejected because it contradicts the `422` #213 states, and
because the two are different questions decided at different times: a declared parameter is judged
against its declaration before any layout is walked, and an undeclared field is judged only where an
item reads it. That split predates this change.

### Each declared type keeps its own refusal for an array; only `string` changes

`coerce_param_value`'s `String` arm stops stringifying and refuses, with the `400 InvalidRequest` /
`request_body_invalid` shape its numeric siblings already use. `boolean`, `integer`, `number` and
`length` already refuse an array; `enum` already refuses it as a value outside `values`; `datetime`
already refuses it as `datetime_param_invalid`. Each is pinned with a test rather than rewritten.

*Alternative: one array check at the top of `coerce_param_value`.* Rejected: it would move `enum` from
`InvalidOptionValue` to `InvalidRequest` for an input #213 never mentions, which
`openspec/config.yaml`'s apply guidance names directly ("Widening a shared parser... is out of scope
even when it makes the new code simpler").

### A layout path is new plumbing, and only the new refusals ask for it

#213 wants the `when:`-over-a-list refusal to name "the key and the item's layout path", and the same
reading applies to the `image` `name:` refusal. Nothing at this stage of validation can do that today:
`validate_when_references` is handed the `when` map and the params and reports a parameter name
(`src/templates.rs:1403-1419`), and `validate_item_references` recurses through containers carrying no
index and no path (`:1483-1616`). The path that does exist belongs to parsing: `convert.rs` attaches
`layout[<i>]` through `TemplateError::with_prefix`, and `serde_path_to_error` gives an unknown-field
refusal its JSON path. That is why `conditional-visibility`'s existing `option` requirement can promise
a layout path and this one cannot: `option` is refused by serde, and a `when:` key is refused after
parsing succeeded.

So the path is threaded: `validate_item_references` takes the path of the item it is walking and extends
it for each child, and `validate_when_references` takes it as an argument. Two new messages consume it,
the `when`-over-a-list refusal and the `image` `name:` refusal.

**No existing message changes.** The undeclared-`when` refusal keeps the text it has, and so does every
other message these two functions raise. That leaves two `when` refusals side by side with different
message shapes, which is an inconsistency, and it is the deliberate one: `openspec/config.yaml`'s apply
guidance forbids changing behaviour for inputs the issue never mentions, a diagnostic is what a test
asserts on, and #213 asks for a path on its own refusals and says nothing about anyone else's. Widening
it later is a one-line change per message once the argument is in place, which is the point of threading
it rather than reaching for the item some other way.

### Undeclared `when:` keys stay refused, and no rule is written for a state no template reaches

`validate_when_references` refuses every `when:` key absent from `params:` at load, so a template
carrying one never loads and no request reaches the comparison. An earlier draft of #213 said an array
under such a key makes the predicate false; the issue has withdrawn that sentence for this reason. The
`conditional-visibility` delta restates the existing refusal, giving it a spec home for the first time,
and writes no rule for the unreachable state.

*Alternative: legalise undeclared `when:` keys so that clause governs something.* Rejected on the
scoping decision, and the reasons are worth keeping because they are why the clause was withdrawn rather
than honoured. It cannot be scoped to arrays: making a key legal makes it legal for scalar, object and
absent values too, so it would owe a complete contract for each, plus the `template-inputs` consequence
that every such key becomes a reported input with control `text` and `required: true`. It would change
gate evaluation for templates that use no lists at all, which #213's own Scope section ("what every
*existing* consumer does when it meets a list") excludes. And it would cut against **#322**, which owns
the general question of whether every name a template reads must be declared and cites
`validate_when_references` as one of the two helpers already enforcing that rule correctly, arguing to
extend it rather than relax it.

The paragraph's stated purpose in #213, keeping the door open for a future `contains` operator, is served
entirely by the declared-list refusal, which is untouched by any of this.

### The `when:` map's shape rules are stated, and §5 is superseded one bullet deep

The `conditional-visibility` requirement is the first spec home for rules the service already enforces
and nothing wrote down: a `when:` map that is present may not be empty, a key or value may not be blank
or whitespace (`validate_when`, `src/templates.rs:2240-2251`), a condition value is a YAML string,
boolean, integer or float held as its textual form while a null, sequence or mapping fails to parse
(`deserialize_when_map`, `src/raw.rs:117-145`), and an `enum` condition's literal must be one of that
parameter's `values`.

One of those needed care, and a draft got it wrong. `when:` written with an explicit YAML null is
**absence**, not an empty map: `deserialize_when_map` reads it through `Option::<BTreeMap<_,_>>`, so it
arrives as `None` (`src/raw.rs:117-145`, `:224-225`) and `validate_when` never sees it
(`src/templates.rs:2240-2251`), leaving the item unconditional. A draft wrote "a `when:` key that is
written SHALL carry at least one condition", which, read against this change's own convention that a
`params:` attribute counts as written when it carries an explicit null, refuses `when: null` and changes
behaviour the requirement claims to preserve. The rule is now written over a map that is **present**,
with the null case stated separately. The two keys differ in what they hold rather than in a rule about
nulls: a `when:` key holds a container, so its null is no container, while a parameter attribute holds a
value, so its null is a key an author wrote and left out. They
are stated because the requirement touches the subject, and an account that named some and not others
would read as exhaustive and quietly legalise the rest.

The supersession is exactly one bullet of frozen §5, "Evaluation against resolved parameters". An
earlier draft also claimed §5's "Lazy missing-field evaluation" and "Enum parameter validation" bullets
were already superseded by `param-resolution`. That was wrong on both counts: `param-resolution`'s only
supersession note names a row of the §10 error-code table (`openspec/specs/param-resolution/spec.md:321`)
and never mentions §5, and the enum bullet governs the response to a **request** value outside `values`,
which is a different rule from the load-time check on a `when:` literal that this requirement states.
Under the repo's precedence rule both frozen bullets stay authoritative, and the delta now says so.

### `param-resolution` owns a rule this change contradicts, so it gets a `MODIFIED`

"A declared default is resolved against one request-scoped snapshot" says a non-string `default:`
"carries no token and SHALL be used as written", and that a default with no interpolation syntax keeps
exactly the load-time checks it has today. The sequence-default refusal contradicts both readings, so
the refusal is written **there**, next to the sentence it qualifies, and derived from that requirement's
own rule that a default may not carry a value the request could not have carried. Putting it only in
`list-params` would have left the general statement about defaults false and the specific one
unreachable by anyone reading about defaults.

`template-inputs`' lenient-resolution requirement is **not** modified, and an earlier draft's reason for
modifying it went away with the exception. Its rule is written over "its declared type", so it absorbs a
new type's coercion failures without amendment; a `list` now has an entry like every other type, so
nothing about how it is absorbed or reported is special; and the render answers it lists as examples are
the render path's, which `list-params` states. Restating it would have republished clauses this change
has no reason to touch.

### `json_to_param_value` is a wildcard the compiler will not catch

`param_defaults` publishes a resolved default by converting the resolved JSON back into a `ParamValue`
through `json_to_param_value` (`src/render/mod.rs:364-378`, called at `:490`). Its last arm is
`other => ParamValue::String(other.to_string())`, so a JSON array becomes the **string**
`["CONSUMABLE"]` today, and adding `ParamValue::List` does not change that: a wildcard over
`serde_json::Value` still matches, so nothing fails to compile and the contract that
`param_defaults.tags.resolved` is `["CONSUMABLE"]` would be quietly false on the wire.

This is the counter-example to the mechanism the `Token.format` decision relies on. Making a type an
enum turns its consumers into compile errors; adding a variant to a type that is *produced* by a
wildcard turns nothing into anything. The function needs an explicit `Value::Array` arm mapping to
`ParamValue::List`, with a non-string element being the same refusal a request's is, and the arms of
that match are worth reading rather than trusting.

Because nothing about it fails to compile, nothing about it fails a unit test on the model either. The
coverage that proves it has to read the **serialized** response: a `GET /api/templates/{id}` for a
template declaring `tags: { type: list, default: [CONSUMABLE] }`, asserting `param_defaults.tags.resolved`
is the JSON array `["CONSUMABLE"]` and not a string. An assertion one layer below the serializer would
pass against exactly the defect this paragraph describes.

### `null` is an omission and `[]` is a value

`null` reaches `param-resolution`'s omission rule, giving a caller a spelling for "use the declared
default", as a `datetime` already has. `[]` is a list of zero elements and resolves to the empty string
through a join. Folding `[]` into omission would make an empty tag set a `422`.

### The no-`InputSpec` exception is withdrawn: a `list` is reported like every other parameter

`InputControl` gains a `list` variant and the input list carries an ordinary `InputSpec` for a `list`
parameter. This reverses the plan's own earlier decision, and the reversal is the single largest thing
that happened to this plan, so it is recorded rather than quietly absorbed.

**What the exception was.** A `list` produces no entry, so no screen is asked to draw a control that
does not exist, and the UI stays untouched. Its whole proof was the absence of an `InputControl`
variant.

**What it cost.** Six consecutive `REVISE` rounds, each finding another unchanged requirement the
exception contradicted, because the corpus assumes every readable parameter has an entry:
`template-inputs`' thumbnail invents a placeholder for every token-read parameter; its CSV Import screen
offers every parameter a template can read; its one-derivation rule feeds the thumbnail and the catalog
index from the input list; its `param_defaults` requirement projects each resolved default into
`inputs.default` and `inputs.all`. Rounds 1 to 3 grew a `template-inputs` delta to reconcile them, and
round 4 then found the grown delta self-inconsistent. That is a plan being patched outward from a bad
root.

**Why the uniform rule wins.** CLAUDE.md's Exceptions section decides it as written: an exception needs
the concrete failure the uniform rule causes, and "no control exists yet" is a cost rather than a proof.
Six rounds is evidence the exception cost more than it bought. Taking the uniform rule leaves all four
of those requirements holding **unchanged**, and the `template-inputs` delta falls from 995 lines across
six `MODIFIED` requirements to two: a `list` row in the control table, and a `list` fill in the
thumbnail's invention table.

**What it costs instead, stated plainly.** A client is told a `list` input exists and, until #318 lands,
has no widget to draw for it. That obligation is written into the control-table requirement: a screen
skips a control it cannot draw and does not fail. The print screen still cannot supply a `list`, so a
template reading one without a resolvable `default:` still submits to `422 MissingField`; that is the
one consequence #213 named all along, and it is unchanged by which way this decision goes.

**What the reversal deletes.** The thumbnail no longer needs a fill set drawn from the walk ahead of the
input-list projection, because a `list` now has an entry and a `control` to be filled by. The catalog
index now lists an undefaulted `list` as a required field, like every other required name. Neither is a
special case any more.

### A defect this change found in `param-resolution` and deliberately does not carry

`param-resolution`'s preview requirement says a parameter that declares a `default:` is **not** stood in
for, "so it resolves rather than being stood in for", and its scenario "A thumbnail fails on a broken
default a token reads" requires a thumbnail to fail with `param_default_unresolvable` when a token-read
parameter's default cannot resolve.

Three things say otherwise. `template-inputs`' thumbnail requirement states that a parameter whose
declared default fails to resolve is `required: true` and "the thumbnail SHALL invent for it on exactly
the terms it invents for a parameter declaring no default at all". `datetime-params` says the same for a
`datetime` whose default fails. And the service does the latter: `src/templates.rs:433-438` reports a
failed default as `required: true`, and `placeholder_data` (`:171-194`) invents for every entry that is
`interpolated` and `required`.

So the preview requirement is wrong against the code and against two capabilities, and it was wrong
before this change. Modifying it would have made that contradiction part of this change's proposed
normative truth, and correcting it would have been a spec fix #213 never asked for. Not modifying it
leaves the defect where it already is, and **#325** now owns it: reconcile the three toward what the
service does, which is placeholder-and-render. This change does not touch that behaviour, and this
paragraph records why the requirement it would otherwise have restated is left alone.

### `default: []` is the author's escape hatch, and what it does not do

A `list` an active item reads and that declares no resolvable `default:` is `required: true`, so a print
screen that cannot draw its control submits without it and the render is `422 MissingField`. A
resolvable `default:`, `default: []` included, removes that: the label prints, the thumbnail prints the
default rather than a placeholder, and the catalog index stops listing the name.

What it does not do is create a widget. Until #318 there is no control to draw whatever the parameter
declares, and a default only means nobody has to supply one.

`param_defaults` follows the rule that governs every type: it holds an entry for each parameter that
**declares a `default:`**, so a `list` declaring one is reported there and a `list` declaring none is
not. An earlier draft said it reports a list "whether or not a default exists", which contradicts the
requirement that keys that report to declared defaults.

### The UI's declared API types are corrected, and nothing else in the UI moves

Two things were bundled together in the first draft and one of them was a scope violation. Separated:

- **A template-detail display rule is cut, and staying cut.** #213 says the UI is untouched, the issue
  body is the scope, and a proposal cannot authorise its own expansion. Nothing is broken by cutting it:
  the page renders a declared default with `String(spec.default)`
  (`ui/src/pages/TemplateDetail.tsx:306`, and `:310` for the resolved value), and JavaScript renders an
  array through `String` as its elements separated by commas, so a list default reads `CONSUMABLE,KIDS`
  with no change at all.
- **`ui/src/api/types.ts` is corrected.** `ParamValue` gains `string[]`, `ParamSpec["type"]` gains
  `"list"`, and the `InputControl` union gains `"list"` (`ui/src/api/types.ts:7,21,30-39`). This is not a feature and not the list form control: those
  declarations are the checked-in statement of what the API returns, and after this change the API
  returns `type: "list"` in `params` and an array-valued `default` in `params` and `param_defaults`, so
  leaving them alone would leave a file in the repository asserting that a shape the server now sends
  cannot occur. The first draft argued this was safe because nothing narrows on `ParamSpec["type"]`
  exhaustively, which is true and is beside the point: a type declaration is wrong or right on its own
  terms, and the next person to widen a narrowing there would inherit the lie.

`InputControl` gains `"list"` in that same file, for the same reason and on the same terms: the service
now reports that control, so the declaration that enumerates them has to say so.

Beyond the declarations, one behaviour is owed and it is an omission rather than an editor. Every
surface that renders a control per reported input falls through to a plain text input for a control it
does not know: `ParamInput.tsx` returns one from its final branch, and the CSV and connector grids build
a text cell per column. For a `list` that text would submit a string and be refused with `400`, so the
fall-through is not tolerance. Each of those surfaces skips a `list` entry instead, which is what the
control-table requirement asks for and all it asks for. No screen collects or renders a list; #318
builds the editor.

## Risks / Trade-offs

- **Changing `Token.format` touches every reader of it.** -> The type change is the mechanism: making it
  an enum turns each site into a compile error, so no consumer is missed silently. The two that matter
  are `validate_interpolated_string` and `interpolate`.
- **`value_to_string` has six consumers and the first inventory found four.** -> The two that were missed
  are the `image` `name:` bindings, and they are now specified on both halves. The remaining consumers
  are enumerated in Context, and each is either changed here or left alone deliberately: the object
  catch-all stays, and it is the only shape this change does not judge.
- **Two `when` evaluations exist** (`src/render/mod.rs:1295` and `src/templates.rs:275`) and they do
  **not** agree by design: `template-inputs` requires the input-list path to resolve leniently, absorbing
  a value it cannot coerce and evaluating a gate naming it as absent, where a render rejects that value,
  and it claims branch parity only for defaults that resolve. -> An earlier draft of the
  `conditional-visibility` delta asserted that every evaluator behaves identically and that an input list
  reports the branch a render takes, which would have made the two main specs mutually unsatisfiable.
  The delta now claims only what is true of the *template*: a file refused at load is refused on every
  path, because no path is served a quarantined template. It claims nothing about which branch a path
  selects, and says so outright. Neither evaluator can meet an array under either answer to the open
  question, because a declared list cannot gate and every other declared type refuses an array during
  coercion; a test that a `string` parameter sent an array is refused during resolution, before any gate
  is evaluated, is what pins that.
- **The input-list derivation reads bare token names before it knows the type** (`bare_token_names`,
  `src/templates.rs:20-31`). A join token must not record its parameter as an input. -> The declared-list
  skip belongs at entry-building time, where the `ParamSpec` is in hand, not in the token walker, so the
  walker keeps one job.
- **A load message can name a token the author did not write**, when a brace moved the token boundary.
  -> Stated in the delta rather than papered over. The alternative is a quote-aware scanner, which is a
  larger and riskier change to a function three other contracts depend on.
- **The reason-completeness test** in `src/errors.rs` accepts a slug documented in an OpenSpec delta.
  `field_value_not_scalar` appears in the `interpolation-tokens` delta as required.
- **`ParamValue` is `#[serde(untagged)]`.** Adding `List(Vec<String>)` is unambiguous, because no other
  variant deserializes a JSON array; but the variant order still matters for anything added later.
- **An exception to a rule the corpus reads everywhere is a plan that grows without converging.** ->
  Six requirements were being modified to reconcile one; withdrawing the exception left two. The rule
  learned twice over: enumerate every requirement that reads a rule before deciding to bend it, and
  restate none that needs no change, because a `MODIFIED` republishes every clause it copies, including
  a wrong one (which is how #325's defect nearly landed here).
- **A `list` is invisible to the catalog index, and no default changes that.** -> The index publishes the
  `required` names of `inputs.all` (`src/bin/catalog-index.rs:106-111`), and a `list` is in no input
  list, so it can never be filtered in. An earlier draft claimed an undefaulted list would be *listed*
  there and that a default would remove it, which is impossible in both halves. Whether the index should
  advertise a name no screen can collect is #318's question, not this change's.

## Migration Plan

None, and this time the claim is checked against the stored data rather than around it. `store.rs` holds
printers, tokens, variables and settings. The one stored value this change could have touched is the
`datetime_formats` setting, whose entry names are operator-chosen; the decision to let a bare reader stay
a format name means no entry becomes unreachable and no template using one stops loading. Nothing else in
the store carries a parameter type, a token or a format name.

Per the repo's pre-1.0 rule, the three breaking changes land with no second spelling, no desugaring and
no deprecation window. A template that printed a JSON array now fails to load, or fails its render where
the key is undeclared, and the error names the token or the field.
