## MODIFIED Requirements

### Requirement: A transform is validated when the connection is saved

The service SHALL validate every transform at save time and SHALL NOT defer any of these faults to
browse, materialize, or render. A `POST` or `PUT` carrying a rejected transform SHALL fail with
`400 InvalidRequest` and `details.reason` `connection_transform_invalid`, SHALL name the offending
rule by its zero-based index and state the cause in `message`, and SHALL leave the stored connection
untouched.

A transform SHALL be rejected when any of the following holds:

- `pattern` is not a valid regular expression, or its compiled form exceeds the compiled-size budget
  of the bounds requirement below;
- `pattern` declares no named capture group;
- `resource` is not a resource id offered by that connection's connector;
- `source` is not a text-valued field of that resource;
- a capture-group name equals a field key the connector already declares for that resource;
- a capture-group name is repeated within the same rule, or is produced by another rule on the same
  resource;
- a capture-group name is not a legal bare interpolation token name under the `interpolation-tokens`
  capability, which is to say it does not match `^[a-zA-Z0-9_-]+$`. A derived field is reachable from
  a template only as a bare `{name}` token, so a name carrying a dot or a colon could be advertised by
  the schema and mapped in the UI and still never reach a label. This replaces the earlier rejection
  of `datetime` and of names beginning `datetime.` or `vars.`: no word is reserved any more, so
  `datetime` is an ordinary derived name, while `datetime.short_date` and `vars.site` are refused for
  carrying a separator rather than for the word they start with.

A connector MAY declare that a resource carries text fields under a key prefix whose names it
discovers from the upstream at runtime; Homebox's per-item custom fields, keyed `custom:<name>`, are
the case that exists. A `source` under such a prefix SHALL be accepted without proving that the
upstream carries that field, because validation does not contact the upstream. The cost is bounded
and deliberate: a rule sourcing a custom field that does not exist is not an error, it simply never
matches, and falls under the non-match requirement below.

Because a derived name may never equal a field the connector declares, no rule can read another rule's
output: transforms are a single flat pass over what the connector returned, and chaining is
unreachable rather than merely discouraged.

Validation SHALL NOT contact the upstream system. A connection whose upstream is unreachable, or whose
credential is wrong, SHALL still be able to save and correct its transforms.

This requirement supersedes the `docs/SPEC.md` §12 sentence "`POST` rejects an unknown `connector`, a
missing `credential`, or an invalid `base_url` with `400`" by adding transform validation to it; the
three faults it names are unchanged.

#### Scenario: A pattern that does not compile is refused

- **WHEN** a connection is saved with a transform whose `pattern` is not a valid regular expression
- **THEN** the response is `400` with `details.reason` `connection_transform_invalid`
- **AND** the message names the rule's index
- **AND** nothing is stored

#### Scenario: A pattern with no named group is refused

- **WHEN** a transform's pattern matches but declares only unnamed groups
- **THEN** the save is refused, because the rule can name no field

#### Scenario: A derived name that shadows an upstream field is refused

- **WHEN** a transform on a resource that already declares a `name` field derives a group called `name`
- **THEN** the save is refused

#### Scenario: Two rules on one resource cannot derive the same name

- **WHEN** two transforms on the same resource both derive `location_id`
- **THEN** the save is refused

#### Scenario: The same derived name on two different resources is allowed

- **WHEN** one transform on `entities` and one on `locations` both derive `location_id`
- **THEN** the save succeeds, because each rule applies only to the resource it names

#### Scenario: A derived name in a reserved namespace is refused

- **WHEN** a transform derives a group named `datetime.short_date`, `vars.site`, or `printed_on:long_date`
- **THEN** the save is refused, because the name carries a token separator, so no bare token can name
  it and it could never reach a label

#### Scenario: A derived name that was once a reserved word is accepted

- **WHEN** a transform derives a group named `datetime`
- **THEN** the save succeeds, because the name is a legal bare token name and no word is reserved

#### Scenario: A source under a connector's dynamic prefix is accepted unproven

- **WHEN** a transform sources `custom:Internal SKU` on a connector that declares `custom:` as a
  dynamic text prefix, and the upstream is never contacted
- **THEN** the save succeeds
- **AND** if no such custom field exists upstream, the rule never matches and no error is raised

#### Scenario: An unknown resource or source is refused

- **WHEN** a transform names a resource the connector does not offer, or a field that resource does not declare as text
- **THEN** the save is refused

#### Scenario: An unreachable upstream does not block saving

- **WHEN** a connection whose `base_url` cannot be reached is saved with valid transforms
- **THEN** the save succeeds

