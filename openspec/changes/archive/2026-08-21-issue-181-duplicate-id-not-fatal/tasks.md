## 1. Registry loading

- [x] 1.1 Collect the `read_dir` entries into a `Vec` and sort them by file name before `TemplateRegistry::load_from_dir`'s load loop, keeping the existing `TemplateRegistryError::Io` mapping for both the `read_dir` call and each failed entry. (Task 5.1 later moved this into `sorted_dir_paths`.)
- [x] 1.2 Replace the `return Err(TemplateRegistryError::DuplicateId { .. })` at `src/templates.rs:132` with the quarantine path used by `Parse` and `Validation`: build the same `DuplicateId` value, render its `Display` into the `BrokenTemplate { filename, error }`, `tracing::warn!` it, and `continue` without touching the already-accepted id.
- [x] 1.3 Confirm `TemplateRegistryError::DuplicateId` and the `src/errors.rs:417` match arm both stay, and that `Reason::TemplateDuplicateId` is still declared (the frozen `docs/SPEC.md` §10.1 table requires it).
- [x] 1.4 Update the stale comment at `src/lib.rs:2184` ("the reload inside the handler would fail the duplicate-id check") to say what now happens.

## 2. Tests

- [x] 2.1 Registry test: two valid files sharing an id load with the lexicographically first filename served and the other in `broken()` with a message naming the id and the winning file; assert the same outcome when the files are created in the opposite order.
- [x] 2.2 Registry test: an unrelated third template in the same directory is served normally alongside the collision.
- [x] 2.3 HTTP test: with a duplicate on disk, `POST /api/templates/reload` returns `200` with `count` for the served set and `broken_count` including the refused file, and `GET /api/templates` lists it under `broken`.
- [x] 2.4 HTTP test: removing the colliding file and reloading clears the entry from `broken` and leaves the winner served.
- [x] 2.5 Confirm `spec_documents_every_reason_and_invents_none` (`src/errors.rs:554`) still passes.

## 3. Decision record

- [x] 3.1 Write `docs/adr/0058-duplicate-template-id-refuses-the-file.md` (Nygard format, `Status: Accepted`) covering the non-fatal rule, the filename-order tie-break, and the alternatives rejected in `design.md`.
- [x] 3.2 Add the ADR-0058 row to the index table in `docs/adr/README.md`.

## 4. Verification

- [x] 4.1 Run `cargo fmt`, `cargo clippy --all-targets --all-features`, and `cargo test`; all clean.
- [x] 4.2 Manual check against the issue's acceptance criteria: start the server against a config dir holding two files with one id, confirm it starts, the first is served, and the second is reported in `GET /api/templates` `broken[]`.
- [x] 4.3 Run the adversarial code-review loop over the diff (see `openspec/config.yaml` apply guidance). Three passes: pass 1 found the `POST`/`PUT` regression (verified by probe, filed as #184) and five smaller items; passes 2 and 3 found documentation-accuracy and test-completeness items only, all addressed. Pass 3 closed with no blocking issues.

## 5. Review-loop follow-ups

- [x] 5.1 Extract the entry collection and sort into `sorted_dir_paths` (`src/templates.rs`) and test its output directly, so the sort is pinned on a filesystem that enumerates out of order (the CI runner) rather than only implied by the collision test.
- [x] 5.2 Add the two uncovered normative scenarios as tests: reload against an unreadable directory returns `500` with `template_registry_io` and keeps the live set, and non-YAML files are ignored while an uppercase `.YAML` is loaded.
- [x] 5.3 Correct the now-false documentation the change created: `AGENTS.md`'s registry bullet, the `broken[]` doc comments in `src/models.rs` and `src/templates.rs` (they ship in the OpenAPI document), the create-guard rationale in `src/api.rs`, the `delete_template` comments citing a deleted test, and the `422` references in `ui/src/api/queries.ts` and `ui/src/pages/TemplateDetail.test.tsx`.
- [x] 5.4 Remove the `422` responses documented on `POST /templates/reload` and `DELETE /templates/{id}`, which this change makes unreachable, and add the delta-spec requirement that supersedes the frozen `docs/SPEC.md` sentences describing them.
- [x] 5.5 File the out-of-scope consequences as issues and cite them in the ADR and design: [#183](https://github.com/pfa230/labeler/issues/183) (delete promotes the collider) and [#184](https://github.com/pfa230/labeler/issues/184) (`POST`/`PUT` can answer `2xx` describing a different template).
- [x] 5.6 Correct the two user-facing docs that still promised the removed behavior (`README.md`, `docs/AUTHORING.md` §10), and assert `broken` in `delete_with_broken_sibling_succeeds_and_quarantines_broken` so it covers the whole delete scenario.
