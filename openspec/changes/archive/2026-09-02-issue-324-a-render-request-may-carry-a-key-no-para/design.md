## Context

See `proposal.md` — Why. The relevant current state:

- `resolve_parameters_mode` (`src/render/mod.rs:176`) starts from `data.clone()` and then iterates
  `template.params`, so an undeclared key survives resolution and is read by nothing.
- It takes `&TemplateContent`, which carries no `id` (`src/templates.rs:61-65`). `TemplateDefinition`
  carries the id and `Deref`s to the content.
- The four checked paths reach the labels through three loops and one column list: the
  `POST /api/render/label` handler (`src/api.rs:2590`), `render_single_batch`'s loop
  (`src/batch.rs:93`), `render_sheet_pages`' loop (`src/render/mod.rs:951`), and `import_csv`'s column
  handling (`src/api.rs:2714-2728`). `render_sheet_batch` (`src/batch.rs:167`) delegates the whole
  slice and has no loop of its own.
- `POST /api/batch`, `POST /api/print` and `POST /api/import/csv` all funnel through `run_batch`
  (`src/api.rs:2315`) into `render_batch`, which dispatches on the template format.

## Goals / Non-Goals

**Goals:**

- One check, one message shape, four call sites.
- Every failing label still reported, on every path, with the failure attributed to its index.
- The check reachable only from the request boundary, so nothing the service builds itself is judged
  by it.

**Non-Goals:**

- Changing what `resolve_parameters_mode` does or what it takes. It stays on `&TemplateContent`.
- Any UI change. Every screen already prunes to the input list before submitting.
- Any change to how an *unrecognized value* for a declared parameter is handled.

## Decisions

**The shared piece returns names. Each call site builds its own error.**
The two refusals are two contracts, not one contract with two spellings. `data_key_unknown` names
*keys* of one label's `data` map and is reported per label, inside a `422 BatchInvalid` envelope on
three of the four paths; `csv_data_column_unknown` names *columns* of a file and is reported once, as a
whole-request `400`. A single function returning an `AppError` cannot produce both from
`(&TemplateDefinition, &HashMap<…>)`, because neither argument says which contract is in force.

So the shared piece is only the part that is genuinely shared:

```
fn unknown_param_names<'a>(
    template: &TemplateContent,
    names: impl Iterator<Item = &'a str>,
) -> Vec<String>
```

It returns the names that no declared parameter matches, sorted ascending by code point, empty when
there are none. It reads only `params`, so it takes `&TemplateContent` and needs no id. It knows no
`Reason`, builds no `AppError`, and decides no status.

Each call site turns a non-empty result into its own error:

- the three label sites, through one thin wrapper taking `&TemplateDefinition` and a `&HashMap<String,
  JsonValue>` and returning `Result<(), AppError>`, which formats
  `AppError::invalid_request(Reason::DataKeyUnknown, …)` naming the keys and the template id. They
  share a wrapper because their reason and their wording are identical and only the id differs by
  request;
- `import_csv`, which passes the header's non-`option.` column names and formats
  `AppError::invalid_request(Reason::CsvDataColumnUnknown, …)` naming the columns and the template id.

The wrapper lives in `src/render/mod.rs`, which already imports `AppError` and is already a dependency
of both `src/batch.rs` and `src/api.rs`. `src/templates.rs` is not an option for it: it deals in
`TemplateError` and does not import `AppError` at all. `unknown_param_names` may live beside it.

*Alternative rejected — one function taking the `Reason` and a message format.* It saves four lines of
formatting at the cost of a parameter whose only job is to say which of two contracts applies, and it
puts both slugs' wording in one place where an edit meant for one silently reaches the other.

*Alternative rejected — put the check inside `resolve_parameters_mode`.* That is where the key is
currently swallowed, so it looks like the natural home, but resolution is shared by the thumbnail, by
`derive_inputs_for_label` and by the lenient `/inputs` path. Two of those must keep ignoring the key,
so the check would need a mode flag threaded through every caller, and `derive_inputs_for_label` calls
it from a bare `TemplateContent` that has no id to name. Keeping the check at the request boundary
makes the `/inputs` leniency fall out of where the code lives rather than out of a flag someone must
remember to pass.

**`render_sheet_pages` widens to `&TemplateDefinition`.**
The message names the template id, and the sheet loop is the only place on the sheet path that both
visits every label and can attribute a failure to one. Its body reaches `format`, `unit`, `dpi` and
`select_layout_items` through the `Deref`, so the body needs no edit. Its one production caller
(`src/batch.rs:167`) already holds a `&TemplateDefinition`.

*Cost accepted:* the unit tests in `src/render/mod.rs` and `tests/acceptance_issue_263.rs:354` that
call it with a bare `TemplateContent` must wrap it in a `TemplateDefinition`. That is mechanical and it
is the price of the message naming what the caller sent.

*Alternative rejected — pass the id as a separate `&str` argument.* It keeps the signature narrower but
invites two callers to disagree about which id they pass, and the id and the params it is checked
against then travel separately.

**The check runs per label inside each loop, never as a whole-request preflight.**
A preflight would return at the first offending label and drop the frozen guarantee that
`details.failures` lists every failing label. Inside the loop, an offending label pushes a
`BatchFailure` and `continue`s, exactly as a resolution failure already does, so every label is still
visited and every failure still reported.

What a preflight would buy is that no valid label is rasterized before the request fails. That is
unobservable: the failing request emits no bytes, dispatches no job and writes nothing either way. The
contract is written to match, and says so explicitly rather than by omission — `batch-validation`
requires that a failing label is not rendered and that a failing request produces nothing, and states
that the order of internal work is not constrained. An earlier draft of that requirement also forbade
rasterizing a passing label first, which the existing loop does at `src/batch.rs:91-118` and which
`render_single_label_image` reaches at `src/render/mod.rs:858`. That clause has been removed: it
promised an internal ordering no caller can test, and it would have forced exactly the preflight this
decision rejects.

**Within a label, the key check runs first, and it can replace the reason that label reports today.**
It is the cheapest check and the one whose failure is about the request rather than the template, so a
label carrying an unrecognized key reports `data_key_unknown` in place of an omitted required
parameter, an uncoercible value for a declared parameter (which today fails during resolution,
`src/render/mod.rs:243-290`), an unresolvable declared default, or a render failure. This produces one
entry per label, matching what every other per-label failure already produces. The relabeling is
deliberate and is written into `request-data-keys`: a stale key and a bad value are two defects in one
label, and naming the one visible in the caller's own request is more use than naming a consequence of
it. A label carrying no unrecognized key is untouched.

**A check that judges the request keeps its reason.**
This is the narrower claim, and it is the one the plan makes. Every check applied to the request as a
whole, before any label is validated, is unchanged: on `POST /api/render/label` the key check goes
after the `format` / `color_mode` / `resolution` query validation and before the render call, so a
request with a bad `format` still reports `format_unknown`; on the batch paths admission (the label
cap, an empty batch, `start_slot`, an unknown template id) still precedes every label; on the CSV path
the file's own refusals still precede the column checks. It is only *within* a label that a reason
changes, which the decision above states outright rather than leaving the two claims to collide.

**On the CSV path, parsing is one phase and column validation is the next.**
`import_csv` already runs in that shape and the check joins the end of it, at
`src/api.rs:2714-2729`:

1. `parse_csv_rows(&body)` — parses the header, refuses an empty or duplicate column name, parses every
   row, and refuses a file with no data rows. It returns `Vec<ParsedCsvRow>`, splitting each row's cells
   into `data` and `option` by the `option.` prefix;
2. the existing `option.` column check, unchanged;
3. **the new data-column check**;
4. only then the `Vec<LabelInput>` is built and handed to `run_batch`.

So the data-column check runs after the file is fully parsed and before any label exists, which is why
the spec claims "before any label is built" and not "before any row is built". Hoisting it above step 1
would need the header before the parser has accepted it, and would re-label every file that today
reports `csv_row_invalid` or `csv_empty` — the opposite of the rule above. A `ParsedCsvRow` is an
intermediate of step 1, not a label: nothing is rendered or dispatched until step 4.

Steps 2 and 3 are adjacent and both read the same header-derived key set, because `parse_csv_rows`
builds every row's `data` from the one header (`src/api.rs:2261-2267`). The existing check reaches that
set by iterating rows; the new one may do the same or read the first row's keys, provided it reports
each unrecognized column exactly once however many rows the file holds.

**Sorting is by Unicode code point, on the collected key names.**
`LabelInput.data` is a `HashMap` (`src/models.rs:1234-1236`), so iteration order differs between two
runs of the same request. Collecting the unrecognized names and sorting them makes the message a
function of the request alone. `str`'s `Ord` is code-point order; no locale-aware collation is
introduced.

**Message shape.** Two sentences, one per contract, formatted at their own call sites: `data keys
'alpha', 'zeta' are not declared parameters of template 'shelf'` for the label paths, and `CSV columns
'alpha', 'zeta' are not declared parameters of template 'shelf'` for the file. The `message`
is prose and the spec binds only what it must name; the slug is the contract.

## Risks / Trade-offs

- **A `POST /api/print` with `copies: 3` reports the same failure three times.** → That is what the
  endpoint does today for every label failure, because `copies` expands into three labels
  (`src/api.rs:2555-2556`). `batch-validation` states the count and the indices rather than leaving a
  reader to infer them; changing the expansion would change the cardinality of every existing print
  failure and is outside this issue.
- **A direct API caller sending a stale field name breaks at once.** → That is the change, and it is
  deliberate (`proposal.md` — Impact). Pre-1.0 there is no deprecation window and none is added.
- **`POST /api/import/csv` refuses spreadsheets that partly worked.** → Raised and overruled when the
  issue was scoped. The operator trims the export or declares the column; the alternative is silence
  about a column nobody reads.
- **The `/inputs` divergence could read as an exception that erodes the rule.** → It is written into
  both specs, from both sides, with the reason: `request-data-keys` scopes its rule to the four paths
  that render or print, and `template-inputs` names the endpoint that ignores what they refuse and why
  the Import screen needs it to.
- **A client that built its own preview by posting a superset of a template's parameters now fails.** →
  The web UI does not (it prunes), and `param-resolution`'s preview requirement already told such a
  client to send legal values for the inputs the service reports. The failure names every offending
  key, so the fix is mechanical.
- **Widening `render_sheet_pages` touches test call sites.** → Mechanical, compile-time, and caught by
  `cargo test` rather than at runtime.

## Migration Plan

None. No stored data, no template content and no configuration changes. The service is stateless with
respect to this rule: a request either carries only declared keys or it does not.
