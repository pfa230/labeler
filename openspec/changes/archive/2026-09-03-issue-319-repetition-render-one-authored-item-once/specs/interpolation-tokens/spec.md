## MODIFIED Requirements

### Requirement: A colon attaches a reader: a format to an instant, or a join to a list

A `:` separates a value from a **reader** applied to it, and carries no other meaning. There are exactly
two readers, and a token carries at most one:

- a **format name**, written bare, an entry of the `datetime_formats` app setting, whose strftime pattern
  is applied to the instant the `value-path` resolves to;
- a **join**, written `join('<separator>')`, which renders the elements of a value the template declares
  as `type: list`, separated by the literal.

**The parenthesized argument is what distinguishes them, not the word.** A reader written bare is a
format name, whatever it spells; a reader written with an argument is a join, and `join` is the only
spelling an argument may follow. No word is reserved in the reader position, so `{sys.now:join}` still
names a `datetime_formats` entry called `join` and resolves exactly as it does today. This matters
because `datetime_formats` is stored operator configuration (`docs/SPEC.md:1054-1058`) whose entry names
are the operator's to choose: a rule that made one of them unreachable would strand stored user data,
which is the one thing this project does not break without a migration.

*This requirement replaces "A colon attaches a format name, and only an instant takes one", which this
change removes. It restates that requirement's complete contract and adds the `join` reader.*

**A format SHALL be attached only where the value is an instant.** Exactly two value paths are instants:

- `sys.now`, and
- a bare token naming a parameter the template declares as `type: datetime`.

A token attaching a format to any other value path SHALL fail validation at load, with a message naming
the token and stating that a format applies to an instant only. This is decidable from the template's own
text, because `params:` is part of the file.

An instant written with no reader SHALL render as ISO `%Y-%m-%d`.

Because `datetime_formats` is runtime state, a format name that the setting does not hold SHALL NOT be an
error at template load. It SHALL be `422 MissingField` when the label renders, naming the field as the
whole token text `<value-path>:<format-name>`.

**A join SHALL be attached only to a bare token naming a parameter the template declares as
`type: list`.** A join on any other value path (an undeclared name, a parameter of any other type,
`sys.now`, or a `vars.<key>`) SHALL fail validation at load, naming the token. An undeclared name is
refused with the rest, because the template says nothing about it and a rule that waited for a request to
find out would make the same template load and fail per caller. The consequence is exact and intended:
**an array is printable only through a parameter declared `type: list`.**

**A parameter declared `type: list` SHALL be read only through a join.** A bare `{tags}` naming one, and
`{tags:<name>}` naming one for any bare reader name, SHALL each fail validation at load, naming the
token: the first because a list is not a value a scalar slot can print, the second because a bare reader
is a format name and a list is not an instant. That second rule is what refuses `{tags:join}` written
with no argument, and its message SHALL say that a list is read through `join('<separator>')` rather than
only that a format applies to an instant, because the token an author meant to write is one character
class away. This is what replaces printing a JSON array as its JSON text.

**Both rules are written for a name that denotes the declared list, which is everywhere outside a
repeat scope.** A `container` carrying `repeat: tags` binds `tags` to one element throughout the subtree
it creates, so within that subtree the name denotes a string (`repetition`) and neither rule reaches it:

- a **bare** `{tags}` there is an ordinary bare token on a string and SHALL render the bound element,
  which is what makes the repeat readable at all;
- what a reader attached to that name does there, and what the refusal reports, is stated by
  `repetition`. It is not stated here, and the difference is one of ownership rather than of taste:
  without a `repeat:` neither question exists, both are decided from the repeat's structure together
  with the declaration, and `repetition` reports every refusal a `repeat:` brings into existence the
  same way. The refusals **this** capability owns are decided from `params:` alone and keep the reason
  it publishes.

Outside every such scope both rules are unchanged, and one template may hold both readings of one
parameter: a strip repeating `tags` and a caption joining it.

**An argument may follow only the word `join`.** A bare reader name other than `join` carrying a
parenthesized argument SHALL fail validation at load, naming the token, whatever value path it is
attached to. `{sys.now:long_date(', ')}` and `{sys.now:join(', ')}` are both refused there: the first
because `long_date` takes no argument, the second because a join reads a declared list and `sys.now` is
not one.

**The separator is a single-quoted literal**, because the token's own string is double-quoted in YAML. It
runs from the first `'` after the `(` to the next `'`, and:

- it MAY be empty, which concatenates the elements;
- it MAY contain a `:`, because the token is parsed by structure rather than by counting colons;
- it MAY NOT contain a `'`. There is no escape and no doubling: a further `'` before the `)` SHALL fail
  validation at load, naming the token. Admitting an escape later turns a refusal into an accepted value,
  which is additive; guessing one now would fix a spelling nothing asks for;
- it MAY NOT contain `{` or `}`, and neither is refused by a rule of its own: both are already decided
  before this grammar is reached, because a token ends at the first `}` and is abandoned at a `{`.
  `{{` and `}}` are not escapes inside a token, because no token has yet been recognised where they
  appear.

  **A separator carrying a brace never renders, and the refusal it gets is one of two.** A brace
  re-pairs the braces around it, and which refusal fires follows from that and from nothing else:

  - where the re-pairing still yields a token, that token SHALL fail validation at load, naming it. A
    `}` closes the token early, so `{tags:join('}')}` is refused as the token `{tags:join('}`; a single
    `{` makes the scanner abandon and restart, so `{tags:join('{')}` is refused as the token `{')}`.
  - where it yields no token, the string is left carrying an unmatched brace, and it SHALL be refused by
    the brace-balance rule that already governs the site it is written at: at load, naming the
    parameter, in a `default:`, and at render as `400 InvalidRequest` with `details.reason`
    `interpolation_syntax` in a `text` or `qr` `value:` or an `image` `src:`. `{tags:join('{{')}` is that
    case, because the doubled brace is skipped before any token is looked for.

  A load message here names a token the author did not write, and this capability states that rather than
  implying otherwise: the scanner reports the token it found, and a brace is what moved the boundary.
  Making every one of these a load refusal would mean applying the brace-balance rule to a `text`, `qr`
  or `image src` value at load, which the requirement below explicitly declines to do; changing that is a
  separate decision about three sites this change does not otherwise touch.

The call SHALL be written exactly `join('<separator>')`: no whitespace between `join`, the parentheses and
the quotes, and nothing after the closing `)`. Anything else SHALL fail validation at load, naming the
token.

A token SHALL carry at most one reader, and a format name SHALL NOT be empty. A second colon outside a
`join`'s separator is part of no valid token. `{x:a:b}`, `{x:}` written with a trailing colon and no name, and `{:long_date}` written with
no value path SHALL each fail validation at load, naming the token. `{x:}` in particular SHALL NOT be read
as the bare value `x`: a colon that is written is a reader that is claimed, and a claim with no name is a
mistake worth reporting rather than silently printing an unformatted value.

**What a `join` renders.** The elements of the resolved list SHALL be concatenated in order with the
separator between consecutive elements: no separator before the first or after the last. A list of one
element renders that element, and a list of zero elements renders the empty string. The result is then
ordinary interpolated text and is escaped for the renderer exactly as any other resolved value is. A
`list` parameter that is absent when an active item joins it SHALL be `422 MissingField` naming the
parameter, under `param-resolution`, on the same terms as every other absent parameter.

Because a `default:`'s `value-path` SHALL be dotted, and `join` attaches only to a bare name, no
parameter default can carry a `join`.

#### Scenario: A format renders the system instant

- **WHEN** a template renders `"Printed {sys.now:long_date}"` with the default `long_date` pattern
  `%B %-d, %Y` on 2026-08-23
- **THEN** the label reads `Printed August 23, 2026`

#### Scenario: A format renders a declared datetime parameter

- **WHEN** a template declaring `printed_on: { type: datetime }` renders `"{printed_on:short_date}"`
  with `printed_on` set to `2026-08-19` and the default `short_date` pattern `%m/%d/%Y`
- **THEN** the label reads `08/19/2026`

#### Scenario: An instant with no reader prints an ISO date

- **WHEN** the same template renders `"{printed_on}"` and `"{sys.now}"`
- **THEN** both print their instant as `YYYY-MM-DD`

#### Scenario: A join renders a declared list

- **WHEN** a template declaring `tags: { type: list, default: [CONSUMABLE, KIDS] }` renders
  `"{tags:join(', ')}"` with no `tags` in the request
- **THEN** the label reads `CONSUMABLE, KIDS`

#### Scenario: An empty separator concatenates

- **WHEN** the same template renders `"{tags:join('')}"`
- **THEN** the label reads `CONSUMABLEKIDS`

#### Scenario: A separator may contain a colon

- **WHEN** the same template renders `"{tags:join(' : ')}"`
- **THEN** the label reads `CONSUMABLE : KIDS`, because the token is parsed by structure and the second
  colon is inside the literal

#### Scenario: A one-element and a zero-element list

- **WHEN** requests send `tags: ["ONE"]` and `tags: []` for a template printing `{tags:join(', ')}`
- **THEN** the first prints `ONE` and the second prints nothing, and neither is an error

#### Scenario: A bare token on a declared list is refused when the template loads

- **WHEN** a template declaring `tags: { type: list }` contains `{tags}`
- **THEN** the file fails validation naming the token, and is quarantined, rather than loading and
  printing the value's JSON text

#### Scenario: A format on a declared list is refused

- **WHEN** the same template contains `{tags:long_date}`
- **THEN** the file fails validation with a message naming the token and stating that a format applies to
  an instant only

#### Scenario: Inside a repeat scope the bare token is the spelling

- **WHEN** a template declaring `tags: { type: list }` holds a packed container carrying `repeat: tags`
  whose `text` reads `{tags}`, and a request sends `tags: ["A", "B"]`
- **THEN** the template loads, and two instances print `A` and `B`

#### Scenario: Inside a repeat scope a reader on the repeated name is `repetition`'s to refuse

- **WHEN** that `text` instead reads `{tags:join(', ')}`, and when it instead reads `{tags:long_date}`
- **THEN** each is refused when the template loads, on the terms `repetition` states, and neither is
  refused by the two rules above, which read the name as the declared list and do not reach into the
  scope

#### Scenario: A join on the same parameter outside the scope is unchanged

- **WHEN** the same template holds a `text` outside every repeating container reading
  `{tags:join(', ')}`
- **THEN** it loads and prints the joined list, exactly as it does for a template carrying no `repeat:`

#### Scenario: A join on a value that is not a declared list is refused

- **WHEN** a template contains `{title:join(', ')}` for a `string` parameter, `{sys.now:join(', ')}`, or
  `{items:join(', ')}` for a name the template does not declare
- **THEN** each fails validation at load naming the token, and the file is quarantined

#### Scenario: A join with no argument is refused

- **WHEN** a template declaring `tags: { type: list }` contains `{tags:join}`
- **THEN** the file fails validation naming the token, in a message saying a list is read through
  `join('<separator>')`, because a bare reader is a format name and a list is not an instant

#### Scenario: A bare `join` on an instant is still a format name

- **WHEN** an operator's `datetime_formats` holds an entry named `join` and a template contains
  `{sys.now:join}`
- **THEN** the template loads and the label prints that entry's pattern applied to the instant, exactly
  as it does today, because no word is reserved in the reader position

#### Scenario: An argument on a bare reader name that is not join is refused

- **WHEN** a template contains `{sys.now:long_date(', ')}`
- **THEN** the file fails validation naming the token

#### Scenario: A join on the system instant is refused

- **WHEN** a template contains `{sys.now:join(', ')}`
- **THEN** the file fails validation naming the token, because a join reads a declared list

#### Scenario: A quote inside the separator is refused

- **WHEN** a template declaring `tags: { type: list }` contains `{tags:join(''')}` or
  `{tags:join('it''s')}`
- **THEN** each fails validation naming the token, because the literal admits no escape and no doubling

#### Scenario: A brace inside the separator is refused when the template loads

- **WHEN** a template declaring `tags: { type: list }` contains `{tags:join('}')}` or
  `{tags:join('{')}` in a `text` item's `value`
- **THEN** the file fails validation naming the token the scanner produced, `{tags:join('}` and `{')}`
  respectively, and the file is quarantined

#### Scenario: A doubled brace inside the separator is refused when the label renders

- **WHEN** the same template instead contains `{tags:join('{{')}`
- **THEN** the template loads, because the doubled brace is skipped and no token is recognised, and
  rendering it returns `400 InvalidRequest` with `details.reason` `interpolation_syntax` for the
  unmatched `{`
- **AND** no label prints the separator or any part of the token text

#### Scenario: A malformed call is refused

- **WHEN** a template contains `{tags:join(', ')x}`, `{tags:join( ', ' )}` or `{tags:join(a)}`
- **THEN** each fails validation at load naming the token

#### Scenario: A format on a string is refused when the template loads

- **WHEN** a template declaring `title: { type: string }` contains `{title:long_date}`
- **THEN** the file fails validation with a message naming the token and stating that a format applies
  to an instant only, and the file is quarantined

#### Scenario: A format on a variables key is refused when the template loads

- **WHEN** a template contains `{vars.qr_base_url:long_date}`
- **THEN** the file fails validation for the same reason and is quarantined

#### Scenario: A token carrying two colons is refused

- **WHEN** a template file contains `{sys.now:long_date:short_date}`
- **THEN** the file fails validation naming the token, and the file is quarantined

#### Scenario: An empty format name is refused rather than ignored

- **WHEN** a template file contains `{printed_on:}`
- **THEN** the file fails validation naming the token, rather than loading and printing the parameter's
  bare ISO date

#### Scenario: An unknown format name fails at render, not at load

- **WHEN** a template contains `{sys.now:no_such_format}`
- **THEN** the template loads successfully, and rendering it returns `422 MissingField` naming
  `sys.now:no_such_format`

#### Scenario: A request cannot reach a formatted token through its data

- **WHEN** a request sends `data: { "printed_on:long_date": "whatever" }` for a template declaring
  `printed_on` as a `datetime` parameter and printing `{printed_on:long_date}`
- **THEN** the label prints the parameter's instant through the `long_date` pattern, because a bare
  token cannot contain a colon and no data key is reachable under that name

#### Scenario: A default cannot carry a join

- **WHEN** a template declares `caption: { type: string, default: "{tags:join(', ')}" }`
- **THEN** the file fails validation naming `caption` and the token, because a default's value path must
  be dotted
