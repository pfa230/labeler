## ADDED Requirements

### Requirement: Connection management has its own route

Connection management SHALL live at `/connections` and its child routes, and SHALL render nowhere
else.

`/connections` SHALL list the connections in a table showing, for each, its name, connector, base URL,
public URL, whether an API key is set, and whether it is enabled, with `-` for a connection that has
no public URL, in the order `GET /api/connections` returns. Each row SHALL offer **Edit**, which opens
`/connections/{id}`, and the page SHALL offer **Add connection**, which opens `/connections/new`. A
list that is still loading, that failed to load, and that loaded holding no connection SHALL each be
reported distinguishably, because a list that has not answered does not say the installation has no
connections. `/connections` SHALL also carry the **Default connection** control, whose contract is
stated by `default-connection`.

`/connections/new` and `/connections/{id}` SHALL render the connection form, and SHALL NOT render the
connections table beside it. The list and the form are two views of one route, never two panes of one
screen.

Primary navigation SHALL carry a **Connections** item directly after **Connect**.

The Connect page SHALL render no connections table, no connection form and no default-connection
control, and SHALL offer a link to `/connections`. When its connections list has loaded and holds no
connection, the Connect page SHALL offer a call to action linking to `/connections/new`, which is the
state in which the rest of that page has nothing to show. When that list failed to load, the page
SHALL report the failure, and SHALL NOT offer the call to action: a list that did not answer does not
say the installation has no connections. Both of the Connect page's links SHALL record their origin,
which "Where a connection form returns to" carries onward.

The Settings page SHALL render no connections UI: no connections table, no connection form, no
default-connection control, and no link or redirect standing in for any of them.

This requirement supersedes the frozen `docs/SPEC.md` §12 ("Using a connection (UI)") as far as that
paragraph says where connections are added and edited, namely its first sentence. How the API key
behaves is superseded by "The connection form is a page of its own"; the browse table by
`connector-browser`; the opening of the Connect flow by `default-connection`.

#### Scenario: Listing connections

- **WHEN** the operator opens `/connections` and the list loads holding a connection with no public
  URL
- **THEN** the table shows that connection's name, connector, base URL, `-` for its public URL,
  whether its API key is set and whether it is enabled
- **AND** the row offers **Edit**, and the page offers **Add connection**

#### Scenario: The list is never rendered beside the form

- **WHEN** the operator is on `/connections/new` or on `/connections/{id}` for an existing connection
- **THEN** no connections table is rendered

#### Scenario: Connect renders no connections UI

- **WHEN** the operator opens the Connect page
- **THEN** it renders no connections table, no connection form and no default-connection control
- **AND** it offers a link to `/connections`

#### Scenario: Connect with no connections offers a way to create one

- **WHEN** the operator opens the Connect page and the connections list loads holding no connection
- **THEN** the page offers a call to action linking to `/connections/new`

#### Scenario: Connect while the connections list is still loading

- **WHEN** the operator opens the Connect page and the request for the connections list has not
  answered
- **THEN** no call to action for creating a connection is offered, because nothing yet says the
  installation has none

#### Scenario: Connect when the connections list failed to load

- **WHEN** the operator opens the Connect page and the request for the connections list fails
- **THEN** the page reports the failure, and offers no call to action for creating a connection

#### Scenario: Settings has no connections UI

- **WHEN** the operator opens the Settings page
- **THEN** it renders no connections table, no connection form and no default-connection control, and
  requests no connection list for one

#### Scenario: Navigation offers Connections

- **WHEN** the operator looks at the primary navigation
- **THEN** a **Connections** item follows the **Connect** item, and it opens `/connections`

#### Scenario: The connections list fails to load

- **WHEN** the operator opens `/connections` and the request for the list fails
- **THEN** the page reports the failure, and does not report that the installation has no connections

### Requirement: The connection form is a page of its own

`/connections/new` and `/connections/{id}` SHALL render the connection form as a single-column page
form in titled sections: **Details**, then **Field transforms**.

**Details** SHALL carry the connection's **connector**, which is fixed at creation and SHALL NOT be
editable, **name**, **base url**, an optional **public url**, an **api key** and an **enabled**
checkbox.

**Field transforms** SHALL carry, for an existing connection, the ordered field-transform rule editor
that submits the connection's `transforms`; for a new connection it SHALL carry no rule editor and
SHALL say that rules are added after saving. What that editor offers, when it submits `transforms` and
how a rule is previewed belong to `connector-field-transforms`, which this requirement neither
restates nor overrides.

The form SHALL be pre-filled from the stored connection when editing one, and SHALL submit
`public_url` on every save so that clearing the field clears the stored value. A non-blank **base url**
or **public url** SHALL be rejected in the form, without a request, when it is not a parseable
`http`/`https` URL.

The API key SHALL be write-only: the form SHALL present it as a password field, responses expose only
`has_credential`, and saving an edit with the field left blank SHALL keep the stored key. Creating a
connection with the field blank SHALL be rejected in the form, without a request.

The form for an existing connection SHALL offer **Delete**, which SHALL require a confirming action
before any request is sent. `/connections/new` SHALL offer no delete. Deleting, like creating,
editing and enabling or disabling, is performed from the form and not from the list, so the list is a
list and the editor is where a connection is changed.

**Each entry into the form is its own editor.** The form's draft values, its rule editor, its previews
and the request it has in flight belong to the entry the operator opened, and SHALL NOT carry across
to another. Moving between `/connections/{id}` entries — pressing back or forward, or opening another
connection — SHALL present the destination's own stored values with no draft, no preview and no
validation message left from the entry departed, and this SHALL hold when the two entries name the
same connection and when one of them is `/connections/new`. A save still in flight when the operator
moves SHALL NOT reach the destination form: it belongs to the entry that started it, which is no
longer the active view, so it SHALL neither navigate nor write anything into what is now on screen.
It SHALL still maintain the cache, which "A connection write settles before Connect acts on it"
requires of the write and not of the form.

`/connections/{id}` SHALL be resolvable on a cold load, with no earlier visit to `/connections`. When
the connections list has loaded and holds no connection with that id, the route SHALL render an
explicit not-found state naming the id and linking back to `/connections`; it SHALL render neither an
empty form nor a redirect. A list that is still loading SHALL render neither the not-found state nor a
form, and a list that failed to load SHALL report that failure rather than the id being unknown:
neither says the connection does not exist.

This requirement supersedes the frozen `docs/SPEC.md` §12 ("Using a connection (UI)") as far as that
paragraph says how the API key behaves, namely its second sentence. Where connections are added and
edited is superseded by "Connection management has its own route".

#### Scenario: Deep-linking to the editor

- **WHEN** the operator loads `/connections/{id}` for an existing connection, having not visited
  `/connections` first
- **THEN** the form renders pre-filled from that connection, in the sections **Details** and **Field
  transforms**

#### Scenario: An unknown id

- **WHEN** the operator loads `/connections/{id}` for an id no connection has, and the connections
  list loads
- **THEN** the page renders a not-found state naming that id and linking to `/connections`
- **AND** no form is rendered and no redirect is performed

#### Scenario: The connections list has not answered yet

- **WHEN** the operator loads `/connections/{id}` and the request for the connections list has not
  answered
- **THEN** neither the not-found state nor a form is rendered

#### Scenario: The connections list failed to load

- **WHEN** the operator loads `/connections/{id}` and the request for the connections list fails
- **THEN** the page reports that failure and does not report the id as unknown

#### Scenario: Creating carries no rule editor

- **WHEN** the operator opens `/connections/new`
- **THEN** the **Field transforms** section carries no rule editor and says that rules are added after
  saving

#### Scenario: Setting a public URL

- **WHEN** the operator edits a connection, types `https://homebox.example.com` into **public url**,
  and saves
- **THEN** the request body carries `public_url: "https://homebox.example.com"`
- **AND** the row for that connection on `/connections` shows that public URL

#### Scenario: Clearing a public URL

- **WHEN** the operator edits a connection that has a public URL, empties the **public url** field,
  and saves
- **THEN** the request body carries `public_url: null`
- **AND** the row for that connection on `/connections` shows `-`

#### Scenario: Rejecting an invalid public URL in the form

- **WHEN** the operator types `homebox.example.com` into **public url** and saves
- **THEN** the form shows a validation error and sends no request

#### Scenario: Leaving a public URL unset

- **WHEN** the operator adds a connection and leaves **public url** empty
- **THEN** the request body carries `public_url: null` and the connection is created

#### Scenario: Keeping the stored API key

- **WHEN** the operator edits a connection whose `has_credential` is `true`, leaves **api key** blank,
  and saves
- **THEN** the request body carries no credential and the stored key is kept

#### Scenario: Creating a connection with no API key

- **WHEN** the operator adds a connection and leaves **api key** empty
- **THEN** the form shows a validation error and sends no request

#### Scenario: Deleting takes a confirming action

- **WHEN** the operator opens `/connections/{id}` and presses **Delete** once
- **THEN** no delete request is sent until the operator confirms

#### Scenario: The list offers no delete

- **WHEN** the operator looks at a row on `/connections`
- **THEN** it offers **Edit** and no delete

#### Scenario: Moving between editors while a save is in flight

- **WHEN** the operator opens `/connections/{b}`, edits its fields, presses **Save**, and before the
  response arrives navigates back to `/connections/{a}`, which they had opened earlier
- **THEN** the form shows connection `a`'s stored values, carrying none of `b`'s draft, previews or
  validation messages
- **AND** when `b`'s save succeeds it moves the operator nowhere and changes nothing in the form on
  screen
- **AND** it evicts the answers it affects all the same

#### Scenario: Reopening the same connection is a fresh editor

- **WHEN** the operator edits a field on `/connections/{id}` without saving, leaves, and opens
  `/connections/{id}` again
- **THEN** the form shows that connection's stored values, with the abandoned draft gone

### Requirement: Where a connection form returns to

Connection management records where the operator came from, and every link along the path SHALL carry
that origin forward, so that an operator who reached the form from the Connect page lands back on the
Connect page whichever route they took to it.

The origin SHALL be an in-app path carried in router state. It SHALL be honoured only when it is a
string beginning with a single `/`; a value that is absent, is not a string, or begins with anything
else SHALL be ignored and `/connections` used, so a value that reached the state from outside the
application cannot decide where the operator lands. A protocol-relative `//host` is such a value.

Both of the Connect page's links SHALL record `/connect`: the link to `/connections`, and the
empty-list call to action that opens `/connections/new` directly. The `/connections` page SHALL carry
whatever origin it was reached with onto its own **Add connection** and **Edit** links, and SHALL
record nothing when it was reached without one. So there are two complete paths and they agree:
`/connect` → `/connections/new` returns to `/connect`, and `/connect` → `/connections` →
`/connections/{id}` returns to `/connect`, while `/connections` reached from primary navigation or a
bookmark sends its forms back to `/connections`.

On a successful save, and on a cancel, the form SHALL navigate to the recorded origin when there is
an honoured one, and to `/connections` otherwise.

A successful delete SHALL navigate to `/connections` whatever origin was recorded: the origin was
reached for a connection that no longer exists.

**Only a form that is still the active view moves the operator.** The active view is the entry the
write was started from, on the terms "The connection form is a page of its own" sets: another entry on
the same route is a different view, not the same one. A write outlives the form that started it, and
the operator may have gone somewhere of their own choosing while it was in flight; a completion
arriving then SHALL move them nowhere. Returning them to the origin they left, or sending
them to `/connections` after a delete, would overrule a navigation they made themselves and would land
them on the page they had just chosen to leave. What the completion still does everywhere is maintain
the cache, which "A connection write settles before Connect acts on it" requires of the write itself
and not of the form: the two have different lifetimes, and only the navigation is the form's.

#### Scenario: Saving a form reached from Connect's call to action

- **WHEN** the operator follows the Connect page's empty-list call to action into `/connections/new`
  and saves successfully
- **THEN** the application navigates to `/connect`

#### Scenario: Saving a form reached from Connect by way of the list

- **WHEN** the operator follows the Connect page's link to `/connections`, presses **Edit** on a
  connection, and saves successfully
- **THEN** the application navigates to `/connect`

#### Scenario: Cancelling a form reached from Connect by way of the list

- **WHEN** the operator follows the Connect page's link to `/connections`, presses **Add connection**,
  and cancels
- **THEN** the application navigates to `/connect`, and no save request was sent

#### Scenario: Saving a form reached from the list on its own

- **WHEN** the operator opens `/connections` from primary navigation, presses **Add connection** or
  **Edit**, and saves successfully
- **THEN** the application navigates to `/connections`

#### Scenario: Saving a form loaded cold

- **WHEN** the operator loads `/connections/{id}` directly, with no recorded origin, and saves
  successfully
- **THEN** the application navigates to `/connections`

#### Scenario: A recorded origin that is not an in-app path

- **WHEN** the form is reached with a recorded origin that is not a string, or that does not begin
  with a single `/`, such as `https://elsewhere.example.com/` or `//elsewhere.example.com`
- **THEN** a save or a cancel navigates to `/connections`

#### Scenario: Deleting ignores the recorded origin

- **WHEN** the operator follows the Connect page's link into the form and deletes the connection, and
  is still on the form when the delete succeeds
- **THEN** the application navigates to `/connections`, not to `/connect`

#### Scenario: A completion does not move an operator who has already left

- **WHEN** the operator presses **Save** or confirms a delete, leaves the form by **Cancel** or by
  primary navigation before the response arrives, and the request then succeeds
- **THEN** the operator stays where they navigated to, and the completion moves them nowhere
- **AND** the cache maintenance the write requires happens all the same

### Requirement: A connection write settles before Connect acts on it

A **connection-management write** is a create, update or delete of a connection, or a write or clear
of `default_connection_id`. Connection management and the Connect page never render together, so no
such write needs to be delivered into a Connect page that is on screen when it starts. It does not
follow that one cannot *finish* while Connect is on screen: **Cancel** and the primary navigation stay
available while a request is in flight, and a write outlives the view that started it. Two rules
together SHALL make what Connect presents agree with every write this browser has completed.

**A successful write SHALL evict every held answer it affects**, rather than marking it stale. Which
answers a write affects is fixed:

- a create, an update or a delete of a connection affects the connections list;
- an update or a delete affects that connection's connector schema;
- a delete affects the settings, because deleting the connection a stored `default_connection_id`
  names clears that setting;
- a write or a clear of `default_connection_id` affects the settings.

A read of an affected answer SHALL be answered by a request started after the latest successful write
affecting it. Marking stale is not enough, because a held copy is still served while its replacement
is in flight, and a page that resolves the moment data is available resolves from that copy.

Nothing more is required. An answer that no completed write affects MAY be served from the copy
already held: naming a default changes no connection, so the connections list held across it is still
what the server would send. An answer held across a **failed** write MAY be served for the same
reason, a failed write having changed nothing, and a write that fails SHALL evict nothing.

That eviction is a property of the write and SHALL NOT depend on the view that started it still being
on screen, because the case it exists for is precisely the one where that view is gone. It is
therefore separate from the navigation the form performs, which "Where a connection form returns to"
makes conditional on that form still being the active view: the two have different lifetimes and SHALL
NOT be made one.

**While a connection-management write is in flight, the Connect page SHALL act on nothing.** It SHALL
resolve no connection, read no connector schema and browse no rows, and SHALL report that it is
waiting. When the write settles, the page SHALL proceed from reads made afterwards. This SHALL hold
however the operator got there, including leaving the form by **Cancel** or by primary navigation
while the request is still in flight, and SHALL NOT depend on the form still being mounted.

The two rules replace the event that used to push a completed save into the page beside it. What the
Connect page reads when it acts is stated by `default-connection`.

Leaving the Connect page discards its connection-scoped state: the browse rows, the row selection and
the composer do not survive the trip, and rows selected against a connection SHALL NOT reappear on
return.

A return to Connect resolves afresh, and the connection the operator was browsing before the trip is
not restored on their behalf (`default-connection`). The scenarios below therefore name the connection
they are about as the one selected on the return, whether it is what resolved or the operator picked
it in the picker; a save reaching a connection nobody has selected changes nothing on screen, because
the page reads no schema and browses no rows for it.

#### Scenario: A save reaches Connect on the next visit

- **WHEN** the operator edits the connection they were browsing to a different base URL, saves, and
  returns to Connect with that connection selected
- **THEN** the rows shown are browsed from the new upstream

#### Scenario: A save that changes only the credential

- **WHEN** the operator saves the connection they were browsing with a new **api key** and nothing
  else, and returns to Connect with that connection selected
- **THEN** that connection's schema is requested and its rows are browsed again, the page having no
  way to tell that the credential changed

#### Scenario: No pre-write answer is presented

- **WHEN** the operator saves a connection whose schema and whose connections list had already been
  read, and returns to Connect with that connection selected
- **THEN** the schema and the list the page acts on are the answers to requests made after the save,
  and the copies read before it are presented at no point

#### Scenario: Leaving the form by Cancel while the save is still in flight

- **WHEN** the operator presses **Save**, presses **Cancel** before the response arrives, and lands on
  the Connect page
- **THEN** the Connect page resolves nothing, reads no schema and browses no rows until the save
  settles
- **AND** once it succeeds, the page resolves from a connections list read after it, and any schema it
  then reads is read after it too
- **AND** the operator is not moved off the Connect page by the save completing

#### Scenario: Leaving the form by primary navigation while a delete is in flight

- **WHEN** the operator confirms a delete and opens the Connect page from primary navigation before
  the response arrives
- **THEN** the Connect page acts on nothing until the delete settles, and then acts on a connections
  list read after it, in which the deleted connection is absent
- **AND** no request is made for the deleted connection's schema
- **AND** the operator stays on the Connect page: the delete does not send them to `/connections`

#### Scenario: A write that fails changes nothing

- **WHEN** the operator saves a connection, leaves for the Connect page, and the save fails
- **THEN** the Connect page proceeds once the request settles, on the answers held before it, which
  the failed write did not change

#### Scenario: A write does not evict what it does not affect

- **WHEN** the operator names a default on `/connections` and returns to the Connect page
- **THEN** the settings the page resolves from are read after that write
- **AND** the connections list may be the copy already held, that write having changed no connection

#### Scenario: A deleted connection leaves no held schema

- **WHEN** the operator deletes a connection whose schema had been read
- **THEN** no schema for that connection is still held, and no request is made for the deleted id

#### Scenario: A row selection does not survive the trip

- **WHEN** the operator selects rows on Connect, follows the link to connection management, and
  returns to Connect
- **THEN** no row is selected, and none of the earlier rows is listed in a selection summary

## REMOVED Requirements

### Requirement: Connection management on the Connect page

**Reason**: The Connect page no longer holds a **Manage connections** block, so a requirement whose
every clause is about that block's disclosure state, its table and the form inside it describes a page
that does not exist. Six fields plus a repeating rule editor is past every published threshold for an
inline form beside a table, which is why #384 moved it. Its form contract, its table contract and its
"Settings renders no connections UI" clause are all restated at their new home by "Connection
management has its own route" and "The connection form is a page of its own"; nothing it required is
merely dropped.

**Migration**: None. Pre-1.0, a change that alters behavior breaks what came before. An operator who
managed connections from the Connect page uses `/connections`, which the Connect page links to.

### Requirement: Saving or deleting a connection refreshes what the open Connect page holds

**Reason**: It specifies how a save performed in a form on the Connect page propagates to the rest of
that page while both are on screen. The form is now a separate route, so there is no form beside the
page to propagate from. What the guarantee protected does not lapse: "A connection write settles
before Connect acts on it" keeps it, by evicting every answer a successful write can have changed and
by holding the Connect page back while a write is in flight, which is the case this requirement
covered with "Neither refresh SHALL depend on the control that started it still being on screen".

**Migration**: None. The `labeler:connection-saved` and `labeler:connection-deleted` window events
this requirement was implemented with are removed with it; nothing outside the application consumed
them.
