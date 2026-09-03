## MODIFIED Requirements

### Requirement: A parameter is required unless the template declares a default

For one request, the value a `{token}` reads for a declared parameter SHALL come from exactly two
places, tried in this order:

1. the request's `data` map;
2. the parameter's declared `default:`, resolved per the requirement below.

There is no third place. The service SHALL NOT derive the value a token reads from the parameter's
type, from its `values` list, from its `min` or `max`, or from the clock. A parameter that neither
source supplies is **absent**, and absent is a state the render carries rather than an error in itself.

An absent parameter that an **active** layout item reads through a token SHALL be `422 MissingField`
naming the parameter, on the same terms and with the same payload as an absent request field. Whether
an item is active is decided by its `when:` predicate, and an item under an unmatched predicate is
neither measured nor rendered, so a parameter that only an inactive branch reads SHALL NOT be required.

An absent parameter named by a `when:` predicate SHALL make that predicate false. It SHALL NOT be an
error, because a predicate asks what a value is and absence is an answer. A template whose every branch
is gated on an absent parameter therefore renders none of them rather than failing.

This rule holds for every parameter type. A `boolean` with no declared `default:` is not `false`, an
`enum` with no declared `default:` is not its first value, and a `datetime` with no declared `default:`
is not the render instant.

**Two things that look like a third source and are not.** A CSV import's `option.<name>` column is
folded into the row's `data` map before the label is built, and an empty cell is folded nowhere, so it
reaches this rule as a plain omission from `data`. And the renderer's internal option-selection argument
is populated by nothing at all: no request model carries it, so no caller can reach it, and the preview
requirement below supplies none either. No token takes a value through it.

**What this rule does not reach, stated here rather than in a footnote.** A numeric parameter named by a
container's `width`/`height` `ref:` is resolved by *different* mechanisms, which do derive a value when
the parameter has no usable default, and which do not even agree with each other: at load
`load_geometry_values` falls back `min` → `max` → `0.0` (`src/templates.rs:1514-1529`) while
`resolve_f32_default` falls back `min` → `0.0` (`:1531-1544`) and `resolve_u16_default` falls back to
`400` (`:1546-1556`); at render `render_geometry_values` falls back `min` → `0.0` and never consults
`max` (`src/render/mod.rs:927-946`). They carry the same defect this requirement removes, in another
place, and this capability neither governs nor changes them; they are tracked as **#261**. The absolute
sentence above is about the value a token reads.

#### Scenario: An omitted boolean with no default fails

- **WHEN** a template declares `bold: { type: boolean }`, an active `text` item renders `{bold}`, and
  the request omits `bold`
- **THEN** the response is `422 MissingField` naming `bold`

#### Scenario: An omitted enum with no default fails

- **WHEN** a template declares `size: { type: enum, values: [small, large] }`, an active item renders
  `{size}`, and the request omits `size`
- **THEN** the response is `422 MissingField` naming `size`, rather than the label printing `small`

#### Scenario: An omitted enum gates a branch off rather than failing

- **WHEN** a template declares `outline: { type: enum, values: [yes] }`, a container carries
  `when: { outline: yes }`, and the request omits `outline`
- **THEN** the label renders with that container absent, and the response is not an error

#### Scenario: An omitted boolean gates a branch off rather than selecting one

- **WHEN** a container carries `when: { bold: "false" }`, `bold` declares no `default:`, and the
  request omits `bold`
- **THEN** that container is absent, rather than rendered because `bold` was taken as `false`

#### Scenario: A parameter only an inactive branch reads is not required

- **WHEN** an inactive container's `text` item renders `{caption}` and the request omits `caption`
- **THEN** the label renders, and no `MissingField` is raised for `caption`

#### Scenario: A declared default is used

- **WHEN** a template declares `bold: { type: boolean, default: false }` and the request omits `bold`
- **THEN** the label renders with `bold` resolved to `false`

#### Scenario: A filled CSV option cell is an ordinary value

- **WHEN** a CSV import carries an `option.orientation` column whose cell reads `horizontal`
- **THEN** that row's label carries `orientation: horizontal` in its `data`, and the declared default is
  not reached

#### Scenario: A blank CSV option cell is an omission

- **WHEN** a CSV import carries an `option.<name>` column whose cell is empty for a row, and the named
  parameter declares no `default:`
- **THEN** that row's label omits the parameter, and the import fails with `422 MissingField` naming it
  if an active item reads it

### Requirement: A preview invents values, and says which ones, because no caller supplied any

A thumbnail or preview render has no request behind it, so every value it prints is one the service
chose. This is placeholder substitution, it is preview-only, and it never reaches a render a caller
asked for. Every placeholder stands in for a parameter the template declares, and exactly
two rules govern it.

1. **Every declared parameter that a token reads and that the service has no *usable* value of its
   own for** gets a placeholder, chosen to be legal for the kind of control it is. `template-inputs`
   owns both the table of placeholders and the eligibility rule, and this capability does not restate
   either; what matters here is which parameters fall inside it. Eligibility is that a token reads the
   name and that the parameter is **required**, and a parameter is required when it declares no
   `default:` *or* when the default it declares cannot be resolved. A `select` carries one further
   condition and is the only control that does: it is stood in for only where its parameter declares
   no `default:` at all. So an undefaulted `boolean`, `datetime` or `enum` falls inside the rule, where
   the service's own fallback once covered the first two and a preview-only option selection covered
   the third; an undefaulted `enum`'s placeholder is the first of its `values`, which is what makes it
   legal. A parameter whose declared default **resolves** is outside the rule on every control: the
   service has a value for that one, which is why a thumbnail of a template declaring
   `title: { default: Untitled }` prints `Untitled` and not the placeholder `title`. A parameter whose
   declared default **cannot** be resolved is inside it on every control but `select`, so a broken
   default is masked by a placeholder there and propagates as `param_default_unresolvable` on a
   `select`. Whether that split is the right behavior is #344; that it is the behavior is stated here
   and in `template-inputs` alike.
2. **Nothing else is invented.** A parameter rule 1 does not supply is resolved exactly as a render
   resolves it: its declared `default:` if it has one, and absent if it has none. A `boolean` named only
   by a `when:` predicate is the case that changes — with no declared default it is now absent, so that
   predicate is false in a preview where it was previously true against `false`; with one, it resolves
   to it, as it always did. An `enum` named only by a `when:` predicate is the same case and takes the
   same answer.

**Nothing outranks a declared `enum` default.** A preview resolves one exactly as a render does, so a
preview of a template declaring `orientation: { values: [horizontal, vertical], default: vertical }`
shows `vertical`, and one whose
`enum` default cannot be resolved fails there as a render of it fails. The sentence in the frozen
`docs/SPEC.md` §2.0 reading "The default option selection (first allowed value per option key) is used
automatically" is superseded, with the rest of that thumbnail bullet, by `template-inputs`: no
selection is applied, automatically or otherwise, and an `enum` a preview shows is one the template
declared or one rule 1 stood in for.

Rule 2 covers every parameter rule 1 does not stand in for. A preview resolves such a parameter's
declared default whether a `when:` predicate names it or **nothing reads it at all**, because
resolution walks a template's declared parameters rather than the set some layout reads. So a stale
parameter carrying a broken default fails every render and every preview of its template, and it does so
even though no branch would have used it. That is eager where `docs/SPEC.md` §5 and `layout-sizing` are
lazy about *values*, and the reason is that laziness there is about what a request must supply, which a
renderer can decide from the active layout, while this is about what the template itself declares, which
would need the read-set the input derivation computes and this path does not have. A parameter nothing
reads is dead weight an author should delete; this capability makes a broken one say so.

These two rules govern the **server's** preview, which is the thumbnail: the service knows no caller
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
`select` input's placeholder SHALL be one of its `values`. On the server this is so because the
invention table gives a `select` the first of the entry's `values`. A client building its own preview
sends ordinary request data and no preview-only channel, so it SHALL put the first allowed value in the
request `data`, for the entries `template-inputs` names. That is preview data, not a form control and
not a default, and it is not the client-side inference this capability forbids: it is what the service's
own preview does, spelled the only way a request can carry it.

Whether a preview's placeholders are *good* ones is a separate question: #215 asked it and is closed,
and what remains of it is #343, for what a client fills, and #344, for whether a broken default should
be masked. What this capability settles is that a placeholder must at least be a value the parameter
accepts.

#### Scenario: A thumbnail of a template with an undefaulted datetime renders

- **WHEN** a thumbnail is rendered for a template printing `{printed_on:short_date}` where
  `printed_on` declares no `default`
- **THEN** the thumbnail prints the current date and does not fail

#### Scenario: A thumbnail still shows an enum-gated branch

- **WHEN** a thumbnail is rendered for a template whose outline container carries
  `when: { outline: yes }` and `outline` declares `values: [yes]` and `default: yes`
- **THEN** the thumbnail renders with that container, through the declared default

#### Scenario: A thumbnail drops an enum-gated branch whose parameter declares no default

- **WHEN** a thumbnail is rendered for a template whose outline container carries
  `when: { outline: yes }`, `outline` declares `values: [yes]` and no `default`, and no active item
  prints `outline`
- **THEN** the thumbnail renders without that container, because `outline` is absent and an absent
  parameter makes its predicate false

#### Scenario: A thumbnail shows an enum's declared default

- **WHEN** a thumbnail is rendered for a template printing `{orientation}` where `orientation` declares
  `values: [horizontal, vertical]` and `default: vertical`
- **THEN** the thumbnail prints `vertical`, and no placeholder is invented for `orientation`

#### Scenario: A thumbnail stands in for an enum declaring no default

- **WHEN** a thumbnail is rendered for a template printing `{orientation}` where `orientation` declares
  `values: [horizontal, vertical]` and no `default`
- **THEN** the thumbnail prints `horizontal`, the first of its `values`, and does not fail

#### Scenario: A thumbnail stands in for a broken default on a control that is not `select`

- **WHEN** a thumbnail is rendered for a template declaring
  `title: { type: string, default: "{vars.base}" }` whose active `text` item reads `{title}`, and the
  store holds no `base`
- **THEN** the entry is `required`, the thumbnail fills `title` with its own name and renders, while a
  caller's render of the same template omitting `title` is still `422` with
  `param_default_unresolvable`

#### Scenario: A thumbnail drops a boolean-gated branch

- **WHEN** a thumbnail is rendered for a template whose container carries `when: { bold: "false" }` and
  `bold` declares no `default`
- **THEN** the thumbnail renders without that container

#### Scenario: A thumbnail fails on a broken default a token reads

- **WHEN** a thumbnail is rendered for a template declaring
  `orientation: { type: enum, values: [horizontal, vertical], default: "{vars.orient}" }` whose active
  `text` item reads `{orientation}`, and the store holds no `orient`
- **THEN** the thumbnail fails with `param_default_unresolvable` naming `orientation`, because a
  `select` whose parameter declares a default is not stood in for and its default is resolved instead

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
