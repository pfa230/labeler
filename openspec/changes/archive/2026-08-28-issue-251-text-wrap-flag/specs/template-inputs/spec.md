## MODIFIED Requirements

### Requirement: An input list describes the controls one label needs

An **input list** is an ordered list of entries, one per distinct name the operator may be asked for.
Each entry carries everything needed to render one control and to decide whether the label is
complete:

| Field | Meaning |
| --- | --- |
| `name` | The request `data` key the control fills. |
| `control` | `text`, `textarea`, `integer`, `number`, `select`, `checkbox`, `date`, `datetime`, or `image`. |
| `slider` | For `integer` and `number`, whether both bounds are declared so the control is a slider. False otherwise. |
| `required` | Whether the label is incomplete without a value. |
| `default` | The value the service would use if the label omitted this name. Absent when there is none. |
| `values` | For `select`, the allowed values in declared order. Absent otherwise. |
| `min`, `max` | For `integer` and `number`, the declared bounds. Absent otherwise. |
| `unit` | For a `length` parameter, the template's unit, for display beside the control. Absent otherwise. |
| `description` | The parameter's declared description. Absent when it declares none. |
| `interpolated` | Whether some active item reads this name **as a value**: a `text` or `qr` token, an `image` `name:`, or an interpolated `image.src`. False for a name present only because it gates an item or resolves a layout attribute. |
| `truncated_elsewhere` | Whether some `wrap: false` `text` item anywhere in the template, in any branch, reads this name. **The name is historical.** It meant that such an item would render only the value's first line; since #251 every segment of a value enters layout regardless of `wrap`, so the flag no longer reports a loss. It is still computed, still returned, and the print form still renders its note from it — a warning about a truncation that no longer happens. Issue #269 removes the field, the computation and the note together, because deleting a response field is a change of its own. |

An entry SHALL be present for a name the label's render will read, and for no other name. In
particular an entry SHALL be present for a parameter read only as a `when:` key, and for one read
only by a layout attribute, so the operator keeps the control that selects a branch or sizes a box.

A name resolved by the service SHALL NOT appear: a `{vars.<key>}` reference, a `{datetime}` or
`{datetime.<format>}` token, and any name no active item reads.

**`control` is decided by declaration first, use second**, which preserves how the print form renders
a declared parameter today:

- For a name the template declares under `params:`, `control` follows the declared type: `select` for
  `enum`; `checkbox` for `boolean`; `date` or `datetime` for `datetime` according to its `time` flag;
  `integer` for `integer`; `number` for `length` and `number`; `textarea` for a `string` declaring
  `multiline: true`, and `text` for a `string` otherwise. The one override is `image`: a `string`
  parameter that any active `image` item binds through its `name:` gets `image`, since the value it
  carries is a data URI.

  `integer` and `number` are distinct controls, not one numeric control, because they are stepped and
  parsed differently: a client steps an `integer` by 1 and reads a whole number, and steps a `number`
  freely and reads a decimal. Collapsing them would force the client back to the declared type to
  tell them apart. `slider` then says whether the control is presented as a slider, which is true
  exactly when both `min` and `max` are declared.
- For a name the template does not declare, `control` follows use: `image` when an active `image`
  item binds it through `name:`; otherwise `textarea` when an active `wrap: true` `text` item reads
  it; otherwise `text`. That a layout flag decides a text control at all is a leftover this capability
  keeps only until #269: `wrap` says whether the renderer soft-wraps a line, which is not a statement
  about what a caller types into.

These two rules are total and mutually exclusive, so a name with several uses has exactly one
`control`. A declared `string` read by a `wrap: true` text item but declared `multiline: false`
therefore keeps its single-line control. `truncated_elsewhere` still reports the reverse pairing, but
it no longer describes a loss: since #251 every `\n` segment of a value enters layout under either
control and under either flag. Only the authored `overflow` policy may then shorten a line, drop lines,
or — under `overflow: fail` — reject the render with `text_does_not_fit`; a shortened or dropped line is
marked.

`required` SHALL be false for a declared parameter that resolves when omitted, namely one carrying a
`default` and one of type `boolean`, `enum` or `datetime`, and true otherwise, including for every
undeclared name.

`default` SHALL carry the declared `default` when there is one, `false` for a `boolean` without one,
and the first entry of `values` for an `enum` without one. It SHALL be absent for a `datetime`, whose
value is the request's instant and whose control a client seeds from the browser
(`datetime-params`), and for any other name with no fallback.

Entries SHALL be ordered by declared parameters first, in ascending name order, then names the
template does not declare, in the order the layout first reads them. Ascending rather than "as
written" because `params` is an ordered map keyed by name and a template's authoring order is not
retained.

#### Scenario: A gated field is absent from the list

- **WHEN** a template reads `{subtitle}` only inside a container gated on `orientation: horizontal`
  and a label selects `orientation: vertical`
- **THEN** the label's input list holds no entry for `subtitle`

#### Scenario: The parameter that selects a branch is an input

- **WHEN** a template reads `orientation` nowhere except as a `when:` key on two containers
- **THEN** every label's input list holds an entry for `orientation`, with control `select`

#### Scenario: A parameter that only sizes something is an input

- **WHEN** a template declares `width` as a `length` parameter read only by `format.width`
- **THEN** every label's input list holds an entry for `width`, with control `number` and the
  template's unit

#### Scenario: A declared control ignores a conflicting use

- **WHEN** a template declares `title` as a `string` with `multiline: false` and a `wrap: true` `text`
  item reads `{title}`
- **THEN** its entry carries control `text` and `truncated_elsewhere` false, since no `wrap: false`
  item reads it

#### Scenario: An image binding overrides a string declaration

- **WHEN** a template declares `logo` as a `string` and an active `image` item carries `name: "logo"`
- **THEN** its entry carries control `image`

#### Scenario: An undeclared name read by a multiline item gets a textarea

- **WHEN** a `wrap: true` `text` item reads `{body}` and `body` is not declared under `params:`
- **THEN** its entry carries control `textarea` and `required` true

#### Scenario: An interpolated image source is an input

- **WHEN** an `image` item carries `src: "{asset_path}"` and `asset_path` is not declared under
  `params:`
- **THEN** the input list holds an entry for `asset_path` with control `text`, since the value names a
  bundled asset rather than carrying image bytes

#### Scenario: A resolved name is never asked for

- **WHEN** a `qr` item interpolates `{vars.base_url}/{id}` and a `text` item interpolates
  `{datetime.iso_date}`
- **THEN** the input list holds an entry for `id` and none for `base_url` or `datetime`

#### Scenario: A parameter the template never reads is not an input

- **WHEN** a template declares a parameter that no item, condition or attribute reads
- **THEN** no input list holds an entry for it

#### Scenario: An enum carries the value it would resolve to

- **WHEN** a template declares `orientation` with `values: [horizontal, vertical]` and
  `default: vertical`
- **THEN** its entry carries `values: [horizontal, vertical]`, `default: vertical` and
  `required: false`

#### Scenario: Entries are ordered by name, then by first use

- **WHEN** a template declares `zebra` and `alpha` and reads undeclared `{second}` before
  undeclared `{first}`
- **THEN** the list runs `alpha`, `zebra`, `second`, `first`


### Requirement: The template detail carries the lists a client needs before it has a label

`GET /api/templates/{id}` SHALL include:

- `inputs.default`: the input list for a label carrying no `data`. A client renders its first form
  from this without a second round trip.
- `inputs.all`: the union of every entry any label could produce, one per distinct name, ignoring
  every `when:` condition. It is what the thumbnail and the template preview fill their sample values
  from, for the closure reason given in the thumbnail requirement, and what a view describing the
  template rather than a label reads.
- `variables`: the `{vars.<key>}` keys the layout reads, as a list of keys without the prefix,
  ascending.

Every other response carrying the same template-detail body SHALL include them too.

An entry in `inputs.all` SHALL carry the same fields as one in `inputs.default`, decided by the same
declaration-first rule. That rule already yields one `control` per name, so branches cannot disagree
about a declared parameter. For a name the template does not declare, where different branches use it
differently, `image` SHALL win over `textarea`, and `textarea` over `text`, so the union never offers
a control that cannot hold what some branch needs.

#### Scenario: The detail lists inputs from every branch

- **WHEN** a template reads `{subtitle}` only under `orientation: horizontal` and `{tracking_url}`
  only under `orientation: vertical`
- **THEN** `inputs.all` holds both, and `inputs.default` holds only the one the default selection
  reads

#### Scenario: The union prefers the wider control for an undeclared name

- **WHEN** undeclared `{title}` is read by a `wrap: true` `text` item in one branch and a `wrap: false`
  one in another
- **THEN** its `inputs.all` entry carries control `textarea` and `truncated_elsewhere` true

#### Scenario: Variables are listed separately

- **WHEN** a `qr` item interpolates `{vars.base_url}/{id}`
- **THEN** `variables` holds `base_url`, and neither input list holds an entry for it

#### Scenario: The addition does not disturb the rest of the body

- **WHEN** a client reads `GET /api/templates/{id}`
- **THEN** every other field of the response is unchanged, and the response still carries no
  `options` key
