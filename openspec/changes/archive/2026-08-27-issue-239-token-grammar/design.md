## Context

See `proposal.md` — Why. The constraint that shapes everything below is where knowledge of a token's
shape currently lives: in five places, none of which is a parser.

| Place | What it knows |
| --- | --- |
| `src/render/helpers.rs:75-90` (`interpolate`) | try the datetime resolver, then declared instants, then `strip_prefix("vars.")`, then the data map |
| `src/datetime_fmt.rs:102`, `:116` | `token == "datetime"`, `strip_prefix("datetime.")`, `split_once('.')` |
| `src/render/mod.rs:2080` (`collect_data_tokens`) | a token is a field unless it is `datetime`, starts `datetime.`, or starts `vars.` |
| `src/render/mod.rs:2139`, `:2165` (two `is_datetime_param` closures) | `token.split('.').next()` is the head |
| `ui/src/lib/templateFields.ts:5`, `:210`, `:275` | the same three prefixes, in TypeScript |

Each is a partial reimplementation of the same grammar, and the dot is the reason: because `a.b` could
be either axis, every site has to guess which by prefix-matching raw text. `CLAUDE.md` already names
the identical failure mode in size resolution ("duplicated between compile-time validation and
render-time resolution; keep the two in sync"). Adding a third axis to five copies is the thing to
avoid, not a sixth copy.

Two facts decide what can be checked when:

- `params:` is part of the template file, so whether a value path is an instant is known at load.
- `datetime_formats` and the variables store are mutable app settings, so whether a *name* exists in
  them is known only at render.

Load-time validation already exists and already quarantines: `TemplateContent::validate_references`
(`src/templates.rs:513`) recurses the layout through `validate_item_references` (`:875`). It inspects
`font_weight`, size refs and `when:` keys, and never looks at a `value:` string. That is the hook.

## Goals / Non-Goals

**Goals:**

- One parser. Every Rust caller that needs to know what a token means calls it and matches on a typed
  result, rather than prefix-matching raw text.
- Every failure the grammar can decide from the file alone is decided at load, so the operator hears
  about it when the service starts and not when someone prints.
- Adding a second `sys` value is one enum variant and one match arm, touching no validation of names.

**Non-Goals:**

- Any filter, argument, chain, default-value or expression syntax. The colon takes one format name and
  nothing else (ADR-0010, ADR-0055).
- Renaming `type: datetime` or the `datetime_formats` app setting. See `proposal.md` — What Changes.
- A compatibility window, a warning mode, or a rewrite tool. The change is loud on purpose.
- Validating format *names* at load. They live in mutable settings; coupling template validity to
  settings state is what ADR-0028 rejected, and that reasoning is unchanged.

## Decisions

### One `interpolation` module owns the grammar

A new `src/interpolation.rs` exports the token type, the parser, and a scanner that walks a template
string yielding well-formed tokens with their byte offsets. Roughly:

```rust
pub enum Source<'a> { Bare(&'a str), Vars(&'a str), Sys(SysValue) }
pub enum SysValue { Now }
pub struct Token<'a> { pub source: Source<'a>, pub format: Option<&'a str>, pub raw: &'a str }
pub enum TokenError { UnknownSource(String), UnknownSysValue(String), MalformedName(String), … }
pub fn parse(raw: &str) -> Result<Token<'_>, TokenError>;
```

`parse` is total over well-formed brace content and knows nothing about a specific template, so it
cannot decide "is this an instant" — that needs `params:` and stays in `templates.rs`. Everything that
does not need `params:` (which root, which key, which format name, is the name legal) lives here and is
decided once.

*Alternative considered: extend `interpolate` in place and leave the four other sites prefix-matching.*
Rejected: the sites disagree today (the UI walker at `templateFields.ts:210` still lists `datetime` as a
prefix that the backend will stop honouring), and adding the colon to five hand-rolled scanners is five
chances to disagree again.

### Load-time validation hangs off `validate_references`, and raises no new error kind

Token checks join `validate_item_references`, extended to read `text.value`, `qr.value` and `image.src`
(the three interpolated strings; `image.name` is a plain data key, not a token). A failure returns the
same `Err(String)` every other reference check returns, so it becomes a `TemplateError`, quarantines the
file, and leaves startup alone. No new `AppError` variant, no new `code` string, no API surface.

The check needs `&self.params` to decide instant-ness, which `validate_item_references` already carries.

The same `validate()` is what a template write calls (`src/api.rs:638-644`, `parse_and_validate`), so the
write path inherits every load-time refusal for free and returns `422 TemplateInvalid` with reason
`template_validation_failed`. That is why the spec states one rule reached by two paths rather than two
rules: an operator cannot `PUT` a template the loader would quarantine.

`image` `name:` is bound by the same bare-name rule, checked in the same walk. It is a request data key
written directly instead of through braces, and exempting it would leave a field an `image` can bind but
no `{token}` can name. No template in the repository or the catalog declares an `image` item, so the
tightening costs nothing in-tree.

*Alternative considered: a separate validation pass over the raw YAML text.* Rejected: it would see
strings the layout never renders and would duplicate the container recursion that already exists.

### Malformed braces stay a render-time error

`{unterminated` and a stray `}` remain `400 InvalidRequest` / `interpolation_syntax` at render, exactly
as today. The load-time scanner skips anything it cannot read as a token rather than reporting it.

This is a deliberate seam, not an oversight: moving brace syntax to load time is a second behavior
change to the same file, it is not what either issue asks for, and it would turn a request-shaped error
into a template-shaped one. If it is wanted, it is an issue of its own.

### `sys` is a closed enum, resolved by the renderer

`SysValue` is a Rust enum with one variant. An unknown `sys.<name>` fails in `parse`, which is what lets
the message say "unknown system value" instead of "missing field" — #240's acceptance criterion — and
what makes the closed set structural rather than a list someone must remember to extend.

`sys.now` resolves from the instant `RenderEnv` already captures once per request
(`src/render/mod.rs:350`, `:646`), so the single-instant guarantee is unchanged and untouched.

*Alternative considered: `sys` values in a map, so a future one is data.* Rejected: the value set is
compiled-in by definition (a request cannot supply one), and an enum makes "did you handle the new
variant" a compile error at every site.

### `DateTimeResolver` stops parsing tokens

`resolve` and `resolve_param` currently take raw token text and split it. After the change they take an
instant and a format name and answer one question: format this instant by this name, or report the name
missing. Token shape leaves `datetime_fmt.rs` entirely.

### Field discovery uses the parser too

`collect_data_tokens`, `template_fields` and `placeholder_data` (`src/render/mod.rs:2080-2185`) call
`parse` and keep only `Source::Bare` names that are not declared `type: datetime` parameters. The two
copies of the `split('.')` head-finding closure go away, and with them the class of bug where the field
list and the renderer disagree about what a token is.

### The UI mirrors the grammar; it does not enforce it

`ui/src/lib/templateFields.ts` keeps its own scanner (different language, no shared build), updated for
the colon and the two roots. It stays best-effort: it decides which form controls to show, and the
backend decides what is valid. Its unit tests are the guard that the mirror has not drifted.

### One ADR, superseding ADR-0028 outright

The new ADR states the whole grammar: a dot navigates, a colon formats, `vars` and `sys` are the roots,
no word is reserved, and what is decided at load versus at render. It **supersedes ADR-0028**
(`{datetime.*}`) rather than extending it, because the token ADR-0028 defines no longer exists in any
form, and it **supersedes the token-list portion of ADR-0068** while leaving that ADR's parameter-type
decision standing.

Numbering is contested: `main`'s highest is `0076-the-filesystem-answers-the-case-question.md`, and the
in-flight `#226` worktree claims 0076, 0077 and 0078 — its `0076-unify-size-resolution.md` already
collides with `main`'s 0076, so whichever lands second renumbers. Confirm the next free number against
`main` and every live worktree when the ADR is written, and add its row to `docs/adr/README.md` in the
same commit.

## Risks / Trade-offs

- **An operator's templates stop loading on upgrade.** → Intended, and the loud half of the change: the
  message names the token and the colon that fixes it, the file is quarantined, and every other template
  still serves. The alternative was a window that has to be tested and later removed.
- **A bare `{datetime}` does not fail loudly; it becomes a request field.** → Not fixable without the
  reserved word this change deletes, so it is stated in the spec, in the ADR, and in `AUTHORING.md`
  instead of being papered over. It fails at print with `422 MissingField` naming `datetime`, and the
  print form starts showing a `datetime` input, so it is visible in two places before anyone is misled.
  It prints wrong output only if a caller happens to send a `datetime` key.
- **Requiring a bare token to match `^[a-zA-Z0-9_-]+$` is a second breaking change.** → It follows from
  the grammar (a name written bare cannot contain a separator), and it closes the split where a data key
  could be spelled in ways a parameter never could. No template in the repository or the catalog uses
  such a key. It carries its own **BREAKING** bullet in `proposal.md` — What Changes, alongside the
  CSV-header and connector-mapping consequences in Impact — rather than living only in this list.
- **A connector field key carrying a separator stops being nameable by a template.** → Homebox's
  `custom:<name>` keys (`src/connector/homebox.rs:511`) are the shipping case. Nothing becomes
  unreachable: the connector grid already maps a template field to any connector key, so the operator
  names the field legally and maps it. What is lost is the same-key auto-prefill
  (`ui/src/lib/connectorRows.ts:9-13`), which for these keys could only ever have prefilled a field name
  that the new grammar rejects. The alternative — rewriting an illegal key into a legal one — is refused
  in the spec, because two distinct keys can rewrite to the same name.
- **A format can no longer be attached to an undeclared data field.** → Correct under the grammar (a
  format applies to an instant), and it was never possible before either; the failure just moves from
  render to load. Worth a line in `AUTHORING.md` so it does not read as a regression.
- **The parser becomes a single point of failure for five call sites.** → That is the goal, and it is
  why the parser gets direct unit tests over the grammar's edges rather than only being exercised
  through render tests.
- **`connector-field-transforms` keeps a scenario titled "A derived name in a reserved namespace is
  refused" whose body no longer mentions a reserved namespace.** → `openspec validate` refuses a
  MODIFIED requirement that drops a scenario name, and there is no scenario-rename operation. The title
  is stale by one word; renaming it is an issue for whenever that capability is next touched.

## Migration Plan

1. Land the parser, the load-time check, the render path, field discovery and the UI walker together.
   Splitting them leaves a commit where the field list and the renderer disagree.
2. Rewrite `tests/fixtures/templates/homebox-qr.yaml:33` (`{datetime.iso_date}` → `{sys.now:iso_date}`)
   and `brother_24mm_printed_on.yaml:28` (`{printed_on.short_date}` → `{printed_on:short_date}`). No
   catalog template uses a datetime token.
3. Update `docs/AUTHORING.md` and add the ADR.
4. Operator action after deploy: read the startup log for quarantined files and rewrite those templates.
   There is no scripted rewrite, by decision.
5. Rollback is `git revert` of the one commit. It is symmetric: templates written in the new spelling
   then name `sys` as an unknown source and quarantine, with a message saying so.
