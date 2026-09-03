TREE_SHA256: f581af1f95627256ce9ecee732b8dc45af178143e789dda36250780402b082f5

## Diff review — issue-324

**Scope reviewed.** `src/reason.rs`, `src/render/mod.rs`, `src/api.rs`, `src/batch.rs`, `src/lib.rs`, `tests/acceptance_issue_263.rs` against `proposal.md`, `specs/{request-data-keys,batch-validation,template-inputs}`, `design.md`, `tasks.md` and AGENTS.md.

**Gates, run by me** [verified]: `cargo fmt --check` clean, `cargo clippy --all-targets --all-features` clean (0 warnings), `cargo test` 806 passed / 0 failed / 2 ignored. `openspec validate --strict` reports the change valid. `.workflow/review-gate-check.sh "$PWD" --plan-only` exits 0. `.workflow/specs-digest.sh` recomputes `f813d445…`, matching `review.md`'s `SPECS_SHA256`, so `specs/` is unedited since the plan verdict.

**What I verified as sound.**
- The check reaches every path the contract names and no other: `src/api.rs:2676` (render), `src/batch.rs:94` (single batch), `src/render/mod.rs:999` (sheet loop), `src/api.rs:2742` (CSV columns). Every other call site that renders builds its own data — `thumbnail` uses `placeholder_data` (`src/api.rs:1255`), and no connector or preview path takes caller data into a render.
- Ordering matches the spec on all four paths: query validation precedes the key check (`src/api.rs:2656-2676`, proven by `issue_324_4_8`), the label cap precedes the loop (`src/batch.rs:56`, proven by `issue_324_4_9`), `start_slot` out-of-range precedes the sheet loop (`src/render/mod.rs:958`), and the file's own refusals precede the column check (`src/api.rs:2733-2742`).
- The CSV shortcut of reading `parsed_rows.first()` is safe: the `csv` reader is non-flexible, so an unequal-length record is already `csv_row_invalid` (`src/api.rs:2256`), and an empty file is `csv_empty` (`src/api.rs:2270`), so row 0's non-`option.` keys are exactly the header's data columns [verified against `src/api.rs:2258-2275`].
- Failure handling matches the existing idiom exactly (`push BatchFailure` + placeholder + `continue`), so every label is still visited and `details.failures` stays in index order (`src/batch.rs:94-103` vs `115-124`).
- The round-2 blocking finding is genuinely fixed: 4.6 and 5.2 now assert `GET /api/recent-templates` is empty after the refusal (`src/lib.rs:8452`, `src/lib.rs:8624`), and that assertion can fail — `record_job` fires for both `ok` and `failed` transports with `principal.actor_id()` (`src/api.rs:2459,2466`), and `AuthInject` (`src/lib.rs:160-171`) gives the print and the read the same token actor.
- No existing test now passes for a different reason: I scanned every http test hitting the four paths that asserts 400/422 without a reason assertion; each sends only declared keys [verified].
- No UI change needed: `pruneDataForSubmit` is a whitelist against the reported input list (`ui/src/lib/labelInputs.ts:246-250`) and every submit and preview funnels through it.

### Findings

**1. `tasks.md:41` (5.6) is checked over a test that omits half of what the box claims. (blocking)**

Task 5.6 reads "A file naming only declared parameters, **alongside `option.` columns naming declared ones**, still imports." The test written for it sends `code,message\nQR1,msg1\nQR2,msg2\n` to `brother_24mm_qr` (`src/lib.rs:8700-8710`) — no `option.` column appears anywhere in it, and `brother_24mm_qr` declares no parameter an `option.` column would name. The matching spec scenario (`specs/request-data-keys/spec.md:227-230`) names the option columns as part of the case.

The behaviour is in fact covered, by the pre-existing `import_csv_routes_option_columns` (`src/lib.rs:2337`), which sends `id,url,name,tags,description,option.orientation,option.outline` and asserts `200` — it is green and would fail if the new check judged `option.` columns as data columns. But `tasks.md` records nothing, so the archived record claims coverage from a test that does not provide it while the test that does goes unnamed. This is the round-2 defect class in a third place, and AGENTS.md is explicit: a checked box is a claim the next reader trusts instead of redoing the work. Fix is either adding an `option.` column pair to the 5.6 file against a template that declares them, or recording `import_csv_routes_option_columns` the way 6.2 records its covering tests.

**2. `tasks.md:9` (2.3) claims an order-independence check the unit test does not make. (minor)**

The box claims "the order is the same across repeated runs over a `HashMap` whose iteration order is not". The test loops 50 times over **one** `HashMap` instance (`src/render/mod.rs:10373-10376`); a single instance has a fixed iteration order for a fixed key set within a run, so that loop cannot fail and demonstrates nothing about iteration order. The property itself is genuinely proven two assertions earlier, where `unknown_param_names` is handed an explicitly unsorted iterator and must return `["alpha","mid","zeta"]` (`src/render/mod.rs:10348-10351`) — so this is an overstated box, not a coverage hole.

**3. `tasks.md:49,51` cite stale line numbers. (nit)**

`issue_324_4_9_…` is recorded at `src/lib.rs:8527` but sits at 8558; `issue_324_6_1_…` at 8688 but sits at 8732; `issue_324_6_4_…` at 8778 but sits at 8822. The drift is from the round-3 edits that added the recents assertions. Test names resolve, so nothing is lost, but the archived record points at the wrong lines.

**4. `#[derive(Copy, Clone)]` on the local `RenderFormat` is unnecessary. (nit)**

`src/api.rs:2652` — the enum is bound once and matched by value at `src/api.rs:2678`, so neither trait is needed.

Nothing in the implementation itself needs changing: findings 1-3 are all in `tasks.md`, which lands permanently under `openspec/changes/archive/`, and finding 1 is a claim its test does not support.

VERDICT: REVISE
