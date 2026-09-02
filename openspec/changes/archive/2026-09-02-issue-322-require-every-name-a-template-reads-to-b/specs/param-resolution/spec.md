## MODIFIED Requirements

### Requirement: A preview invents values, and says which ones, because no caller supplied any

A thumbnail or preview render has no request behind it, so every value it prints is one the service
chose. This is placeholder substitution, it is preview-only, and it never reaches a render a caller
asked for. Every placeholder stands in for a parameter the template declares, and exactly
three rules govern it.

1. **Every declared parameter that a token reads and that the service has no value
   of its own for** gets a placeholder, chosen to be legal for the kind of control it is.
   `template-inputs` owns that table and this capability does not restate it; what matters here is that
   a parameter this change makes required — an undefaulted `boolean` or `datetime` — now falls inside it,
   where before the service's own fallback covered it. An undefaulted `enum` does not: rule 2 covers it. A parameter that declares a `default:`
   is **not** in it: the service has a value for that one, so it resolves rather than being stood in for,
   which is why a thumbnail of a template declaring `title: { default: Untitled }` prints `Untitled` and
   not the placeholder `title`.
2. **Every declared `enum` parameter** additionally gets the first of its `values` as a preview-only
   selection, whether or not a token reads it, as `docs/SPEC.md` §2.0 documents ("The default option
   selection (first allowed value per option key) is used automatically"). That sentence is not
   superseded and its behavior is not changed: a preview that dropped it would render a template's gated
   branches away and show an operator a label nobody will print.
3. **Nothing else is invented.** A parameter that neither rule supplies is resolved exactly as a render
   resolves it: its declared `default:` if it has one, and absent if it has none. A `boolean` named only
   by a `when:` predicate is the case that changes — with no declared default it is now absent, so that
   predicate is false in a preview where it was previously true against `false`; with one, it resolves
   to it, as it always did.

Rule 2 outranks a declared `default:`, and that is the one place a preview and a render disagree. The
option selection is merged into the request data before any default is consulted, so a preview of a
template declaring `orientation: { values: [horizontal, vertical], default: vertical }` shows
`horizontal`. A declared `enum` default is therefore never resolved in a preview, and a broken one never
fails there, while a render of the same template fails. This is the behaviour the frozen §2.0 sentence
already produces and this capability does not change it; it is written down because rule 3 would
otherwise be read as covering every type.

Rule 3 covers every type rule 2 does not. A preview resolves a declared default for such a parameter
whether a token reads it, a `when:` predicate names it, or **nothing reads it at all**, because
resolution walks a template's declared parameters rather than the set some layout reads. So a stale
parameter carrying a broken default fails every render and every preview of its template, and it does so
even though no branch would have used it. That is eager where `docs/SPEC.md` §5 and `layout-sizing` are
lazy about *values*, and the reason is that laziness there is about what a request must supply, which a
renderer can decide from the active layout, while this is about what the template itself declares, which
would need the read-set the input derivation computes and this path does not have. A parameter nothing
reads is dead weight an author should delete; this capability makes a broken one say so.

These three rules govern the **server's** preview, which is the thumbnail: the service knows no caller
supplied data and substitutes its own. A client's live preview is a different thing wearing the same
name — it builds placeholder data itself and POSTs an ordinary render, which the service cannot
distinguish from a real one. Such a client SHALL supply a legal value for every input its
preview references that the service reports as required, and SHALL NOT omit one on the assumption that
the service will fill it in. Nothing on the render path fills it in any more. Two inputs this change
newly makes required are the ones a client gets wrong by default: an undefaulted `datetime`, whose
name-as-placeholder is not a parseable instant, and an undefaulted `enum`, whose name is not one of its
`values`.

A placeholder SHALL be legal for the parameter it stands in for, so that making a parameter required does
not turn a preview into a coercion failure. That binds the `enum` case rather than deferring it: a
`select` input's placeholder SHALL be one of its `values`. On the server this is already so, because the
thumbnail's option selection supplies every declared `enum` and the invention table never reaches one. A
client building its own preview has no option map to send — no request model carries one — so it SHALL
put the first allowed value in the request `data` instead. That is preview data, not a form control and
not a default, and it is not the client-side inference this capability forbids: it is what the service's
own preview does, spelled the only way a request can carry it.

#215 remains the question of whether a preview's placeholders are *good* ones. What this capability
settles is that they must at least be values the parameter accepts.

#### Scenario: A thumbnail of a template with an undefaulted datetime renders

- **WHEN** a thumbnail is rendered for a template printing `{printed_on:short_date}` where
  `printed_on` declares no `default`
- **THEN** the thumbnail prints the current date and does not fail

#### Scenario: A thumbnail still shows an enum-gated branch

- **WHEN** a thumbnail is rendered for a template whose outline container carries
  `when: { outline: yes }` and `outline` declares `values: [yes]` and no `default`
- **THEN** the thumbnail renders with that container, through the preview-only option selection

#### Scenario: A thumbnail drops a boolean-gated branch

- **WHEN** a thumbnail is rendered for a template whose container carries `when: { bold: "false" }` and
  `bold` declares no `default`
- **THEN** the thumbnail renders without that container

#### Scenario: A thumbnail fails on a broken default a token reads

- **WHEN** a thumbnail is rendered for a template declaring `url: { type: string, default: "{vars.base}" }`
  whose active `qr` item reads `{url}`, and the store holds no `base`
- **THEN** the thumbnail fails with `param_default_unresolvable` naming `url`, because a parameter that
  declares a default is not stood in for and its default is resolved instead

#### Scenario: A thumbnail of a template reading an undefaulted boolean renders

- **WHEN** a thumbnail is rendered for a template whose active `text` item reads `{bold}` and `bold`
  declares no `default:`
- **THEN** the thumbnail renders with a legal boolean placeholder, rather than failing to coerce one

#### Scenario: A thumbnail fails on a broken default only a predicate reads

- **WHEN** a thumbnail is rendered for a template declaring `mode: { type: string, default: "{vars.mode}" }`
  named only by a container's `when:`, and the store holds no `mode`
- **THEN** the thumbnail fails with `param_default_unresolvable` naming `mode`, exactly as a real render
  of that template would

#### Scenario: A client's live preview supplies its own instant

- **WHEN** a client renders its live preview of a template printing `{printed_on:short_date}` where
  `printed_on` declares no `default`
- **THEN** the request it posts carries a legal value for `printed_on`, and the preview renders rather
  than returning `422 MissingField`

#### Scenario: A declared default is used rather than stood in for

- **WHEN** a thumbnail is rendered for a template declaring `title: { type: string, default: Untitled }`
  and printing `{title}`
- **THEN** the thumbnail prints `Untitled`, and no placeholder is invented for `title`
