# Diff review

AUTHORS: agy
REVIEWER: claude
VERDICT: APPROVE
ROUNDS: 4
TREE_SHA256: 55b9834beb2707ee01dd60f81944320666c50359b283ca9ff28910d7cccbb0db
SPECS_SHA256: f813d445bfdb1545308394fc78fb941d645aefc00da01260e6012bdd5a046a90

## Diff review, round 4: issue-324

**Scope reviewed.** `git diff` across `src/api.rs`, `src/batch.rs`, `src/lib.rs`, `src/reason.rs`, `src/render/mod.rs`, `tests/acceptance_issue_263.rs`, against `proposal.md`, `design.md`, the three spec deltas, `tasks.md` and AGENTS.md.

### Verification performed

- **All three gates pass** [verified]: `cargo fmt --check` clean, `cargo clippy --all-targets --all-features` clean (no warnings emitted), `cargo test` 806 passed / 0 failed / 2 ignored, plus `acceptance_issue_263` 2/2 and `adr_index` 1/1.
- **`SPECS_SHA256` recomputes to `f813d445…`** [verified], matching the approving `review.md:21`, so `specs/` was not edited after the plan verdict.
- **Four call sites match the plan.** Key check after query validation and before the render on `POST /api/render/label` (`src/api.rs:2676`), at the top of `render_single_batch`'s loop (`src/batch.rs:94-103`) and of `render_sheet_pages`' loop (`src/render/mod.rs:999-1008`), each pushing a `BatchFailure` and `continue`ing exactly as the existing arms do (`src/batch.rs:115-124`, `src/render/mod.rs:1059-1067`). CSV check sits after the `option.` check and before labels are built (`src/api.rs:2741-2765`).
- **Admission ordering holds on every path** [verified]: label cap and empty batch precede the loops (`src/batch.rs:55-64`), `start_slot` out of range precedes the sheet loop (`src/render/mod.rs:956-961`), `start_slot` on a single template precedes `render_batch` (`src/api.rs:2331-2336`), `parse_batch_mode` and `parse_csv_rows` precede the CSV column check (`src/api.rs:2724-2741`).
- **Reading only `parsed_rows.first()` is sound.** The `csv` reader is non-flexible, so an unequal-length record is already `csv_row_invalid` (`src/api.rs:2255-2258`), and every header column is inserted whatever the cell value (`src/api.rs:2260-2266`), so an empty first-row cell does not hide a column.
- **No unchecked render path from caller data remains.** The only production render entries are `src/api.rs:2679/2683` (checked) and `crate::batch::render_batch` at `src/api.rs:2353/2428` (both loops checked); `render_thumbnail_png` and the template preview build their own data.
- **No image-name regression.** #322 (`114be99`) already requires image item `name:` to be a declared string parameter, so the `params` added to the hand-built test templates (`src/batch.rs:218-227`, `src/render/mod.rs:4948-4956`, `5202-5210`, `5308-5316`) restore consistency with a rule the loader already enforces rather than papering over a behavior change.
- **No UI change needed** [verified]: `pruneDataForSubmit` is a whitelist against the reported input list (`ui/src/lib/labelInputs.ts:239-256`), and every submit and preview funnels through it (`Import.tsx:180,274`, `Connect.tsx:208,257`, `print/PrintForm.tsx:84,121,124`). `scripts/render_avery_sheet.sh:27-29` sends only `message`, which `catalog/sheet/avery/avery5163.yaml:5-8` declares.
- **`spec_documents_every_reason_and_invents_none` survives archive** (`src/errors.rs:684-694`): it scans active deltas *and* `openspec/specs/`, excluding only `changes/archive/`, so the two new slugs stay documented after the sync.
- **Every prior round's findings are fixed** [verified]: 5.6 now sends `option.orientation,option.outline` (`src/lib.rs:8700`); 4.6 and 5.2 assert `/api/recent-templates` is empty and that assertion can fail, since `record_job` fires for both `ok` and `failed` sends (`src/api.rs:2459,2466`) and `render_batch` errors before any `driver.send`; 4.7's negative half asserts `request_body_invalid` positively; 6.4 exists; `unreachable!()` is gone, replaced by a local `RenderFormat` with no derives; 2.3 now builds four distinct maps; every line number cited in `tasks.md` resolves correctly.
- **Spec-scenario coverage is complete.** I mapped all 26 scenarios across the three deltas to tests; none is unbacked.

### Findings

**1. `src/api.rs:2676` — the key check now shadows `422 UnsupportedFormat` on `POST /api/render/label`. (minor)**

`validate_label_data_keys` runs at `src/api.rs:2676`, before `render_single_label_image` at `2679`, and the sheet-template rejection lives inside the render call (`compile_single_doc`, `src/render/mod.rs:662-665`). So `POST /api/render/label` for a sheet template carrying an undeclared key now returns `400 InvalidRequest / data_key_unknown` where it returned `422 UnsupportedFormat` before. `docs/SPEC.md:143` states that rule without qualification, `batch-validation` says it "supersedes nothing else", and the caller is told about a stale key rather than about the endpoint mismatch, which is the larger error.

I am not raising this as blocking: `request-data-keys` explicitly authorizes the key check to replace "a render failure", and `UnsupportedFormat` is returned by the render call, so the implementation conforms to the contract the plan review approved. Worth recording because no test pins it in either direction.

**2. `tasks.md:52` — 7.2's guard-test list misclassifies 6.3. (nit)**

7.2 lists "6.3 inputs endpoint leniency" among the tests that "assert preserved behavior", but `issue_324_6_3_inputs_endpoint_remains_lenient` (`src/lib.rs:8779`) also asserts `POST /api/render/label` returns `400 data_key_unknown` (`src/lib.rs:8814-8818`), which fails pre-change. `issue_324_5_1_to_5_6_csv_import_tests` similarly mixes new-refusal assertions (5.1, 5.2, 5.3, 5.5) with preservation ones (5.4, 5.6) and is listed in neither group. At test-function granularity the claim's substance holds: 4.3, 4.8 and 4.9 are the only added functions that pass pre-change. The classification understates rather than overstates coverage, so no reader is misled into skipping work.

**3. `src/api.rs:2741` — the CSV check's `if let Some(first_row)` has a silent empty branch. (nit)**

`parse_csv_rows` returns `csv_empty` for a rowless file (`src/api.rs:2269-2274`), so the `None` arm is unreachable today. If that ever changes, the data-column check disappears with no signal, which is the shape AGENTS.md's "no silent fallbacks" rule warns about. Dead defensiveness, not a live fallback.

Nothing in the implementation needs changing. Findings 2 and 3 are cosmetic, and finding 1 is a defensible reading of the approved contract that I am recording rather than demanding.

