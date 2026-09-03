# Plan review

AUTHOR: opencode
REVIEWER: claude
VERDICT: APPROVE_WITH_CHANGES
ROUNDS: 5

## Plan review: issue-336-option-name-csv-columns-are-a-second-spe

I verified the plan mechanically end to end, not just by reading. Findings below cite file:line and state what I ran.

### What checks out

- `openspec validate --strict` passes on the change as written [verified].
- I copied `openspec/` to a scratch tree and ran `openspec archive` on the change. It applies cleanly (`+3, ~2, -2`), and `openspec validate --specs --strict` passes on the resulting published specs (27/27) [verified].
- The design's two load-bearing mechanism claims are both **true**, which I tested rather than assumed. A `MODIFIED` block with the two scenario headings renamed fails `--strict` with `omits scenario(s) the current spec still has`; a `REMOVED` + `ADDED` pair sharing one title fails with `Requirement present in both ADDED and REMOVED`. The composite titles are therefore forced, not gratuitous [verified].
- Post-move directional references all resolve. In the archived scratch tree `param-resolution:37` still points forward to `A default that cannot be resolved…` (now line 185), and `request-data-keys:29,76` still point forward to the CSV requirement (now 145) while `:157` still points back to `Every key a request sends…` (line 11) [verified].
- The withdrawal requirement matches the shape `scan_canonical_withdrawals` reads (`src/errors.rs:732-765`): the heading contains `withdrawn`, the slug sits in the **first** table cell (`line.split('|').nth(1)`), and `Reason`/`---` are filtered. The `csv_data_column_unknown` contract table in the sibling requirement is not contaminated, because its `### Requirement:` heading resets `in_withdrawn_section` to false [verified].
- Both `MODIFIED` blocks are verbatim copies with exactly one changed line each (`diff` against the published requirements) [verified].
- Code claims hold: `src/api.rs:2263` inserts `String(val)` for every non-`option.` header including empty cells, so "a blank data cell is `\"\"`" restates today's behavior; `:2767` skips empty option cells, which is the semantics change the design flags. `unknown_param_names` (`src/render/mod.rs:173`) uses `Vec<String>::sort`, so `option.size` before `sku_legacy` is correct byte order [verified].
- Frozen-spec coverage is complete. The only normative `docs/SPEC.md` mention is the §10.1 row at `:758`; `:1394` is inside the Changelog section (starts `:1097`), and the normative `## CSV import` section (`:1063-1096`) never mentions `option.` [verified].

### Required changes

**1. Restore the published line wrapping in both `ADDED` blocks.**

The issue names this as trap 2, one of "Three traps, each of which failed a previous attempt at this plan," and `design.md:17` asserts compliance ("`ADDED` blocks follow the file's ~100-column style"). Neither holds. An `ADDED` block lands verbatim exactly as a `MODIFIED` one does, so this permanently rewraps published text the change did not mean to touch.

In `specs/param-resolution/spec.md`, seven prose paragraphs whose wording is unchanged are rewrapped from the published ~100 columns to ~96. Restore the published line breaks byte-for-byte from `openspec/specs/param-resolution/spec.md` for: `There is no third place…` (19-21), `An absent parameter that an **active**…` (23-26), `**A \`repeat:\` key…` (28-34), `**A repeat binds one name…` (36-41), `An absent parameter named by a \`when:\`…` (43-45), `This rule holds for every parameter type…` (47-49), and `**What this rule does not reach…` (57-65). The nine scenarios at 70-116 are already byte-identical; leave them.

In `specs/request-data-keys/spec.md`, restore the published line breaks byte-for-byte from `openspec/specs/request-data-keys/spec.md` for the two unchanged paragraphs at `147-152` (`\`POST /api/import/csv\` SHALL refuse…`) and `186-188` (`**\`csv_data_column_unknown\` is a new entry…`), and for the two unchanged scenarios at `193-200` (`An unrecognized data column is refused`) and `207-211` (`Every unrecognized column is named once`).

For the paragraphs and scenarios whose wording genuinely changed, rewrap to the same ~100-column width the surrounding published text uses, rather than the ~70 to ~85 columns now used (`specs/param-resolution/spec.md:117-131`, `specs/request-data-keys/spec.md:82-87`).

Then correct `design.md:17`: replace the clause "`ADDED` blocks follow the file's ~100-column style" with a statement that `ADDED` blocks reproduce the published line breaks verbatim for unchanged text and wrap changed text to the file's ~100-column width.

**2. Fix the enum aside in the new blank-cell scenario.**

`specs/param-resolution/spec.md:130-131` reads "if the parameter were an `enum` the row would instead fail with `422 InvalidEnumValue`". On `/api/import/csv` that is a per-row failure code, not a top-level status: `src/lib.rs:2611` records this distinction explicitly ("BatchInvalid with a per-row InvalidEnumValue (not a top-level InvalidEnumValue)") and `:2631` asserts `failures[0]["code"] == "InvalidEnumValue"`. Replace that clause with: "and if the parameter were an `enum` the row would instead fail, contributing a `details.failures` entry whose `code` is `InvalidEnumValue` under `422 BatchInvalid`". Do not assert anything about `reason` for that entry.

**3. Add the missed test breakage to Impact.**

`proposal.md:29` names `ui/src/lib/csv.test.ts:5-11` but not `:62-63`, where two expected rows carry `option: {}`. Deleting `CsvRow.option` (`ui/src/lib/csv.ts:10`, named at `proposal.md:29`) breaks those assertions under `toEqual` and fails typecheck. Extend the citation in `proposal.md:29` to `ui/src/lib/csv.test.ts:5-11,62-63`.

### Not findings, recorded so they are not re-raised

- The composite requirement titles (`…: every header is a data column`, `…: CSV data cells are plain values`) read awkwardly, but I confirmed both alternatives are refused by the CLI, and `ANSWERS.md` leaves the mechanism to the author. Accepted.
- `docs/SPEC.md:1394` mentions `option.<name>` columns but sits in the Changelog, which `AGENTS.md` forbids editing and which is not normative. No supersession needed.
- The pre-archive phantom (`cargo test` red between code deletion and sync) is real and is correctly recorded at `design.md:24`; `run-change.sh` runs gates after archive.
- `ui/src/components/LabelGrid.tsx:78,91,212-213` keeps reading `row.option` against a map that will always be empty. `design.md:19` scopes that to #214 with a reason. Accepted.

CHANGES_APPLIED: yes
SPECS_SHA256: 438d6cf42070910dc83f6e6968ba4c1ffac128313e3e9299a3587c0ff4e0105d
