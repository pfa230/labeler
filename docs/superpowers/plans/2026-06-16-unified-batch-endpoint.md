# Unified Batch Endpoint Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `/print` and `/render/batch` with one `POST /batch` that renders/prints a list of resolved labels across the single/sheet × download/print matrix, with multi-page sheet pagination and validate-then-execute error handling.

**Architecture:** A sync `batch` module renders the batch into either a download blob (ZIP for single, PDF for sheet) or a set of print artifacts; the async handler sends print artifacts via the driver and builds a summary. `render_sheet_labels` generalizes to multi-page `render_sheet_pages`. `/import/csv` is refactored to call the same path.

**Tech Stack:** Rust, axum, Typst (`typst-as-lib`), `zip`, `csv`, rusqlite, utoipa.

**Spec:** `docs/superpowers/specs/2026-06-16-unified-batch-endpoint-design.md`. ADR-0011. Issue #30.

---

## File map

- `src/errors.rs` — add `BatchInvalid` (422) + `BatchTooLarge` (413) codes/constructors.
- `src/render/mod.rs` — replace `render_sheet_labels` with multi-page `render_sheet_pages` that collects per-label failures.
- `src/batch.rs` — NEW: `RenderedBatch`, `PrintUnit`, `BatchMode`, `render_batch(...)` (dispatch + validate-then-render).
- `src/models.rs` — `BatchRequest`, `BatchSummary`, `BatchRowError`.
- `src/api.rs` — `POST /batch` handler + route; remove `/print` and `/render/batch`; refactor `/import/csv` onto the batch path; add a max-labels const.
- `src/openapi.rs` — register batch models + path; drop removed paths.
- `src/lib.rs` — batch integration tests; migrate `/print` and `/render/batch` tests.
- `src/main.rs`, `src/api.rs` — module decl `mod batch;` (wherever modules are declared).
- `docs/SPEC.md`, `docs/PLAN-phase-1.md`, `scripts/*.sh`, the design doc — docs.

Work on a branch:

```bash
git checkout -b batch-endpoint
```

---

## Task 1: Error codes — BatchInvalid (422) and BatchTooLarge (413)

**Files:**
- Modify: `src/errors.rs`

- [ ] **Step 1: Add the code constants**

After the existing `const CODE_*` lines in `src/errors.rs`, add:

```rust
const CODE_BATCH_INVALID: &str = "BatchInvalid";
const CODE_BATCH_TOO_LARGE: &str = "BatchTooLarge";
```

- [ ] **Step 2: Add a failure detail struct + constructors**

Add near the other `AppError` constructors:

```rust
/// One label's validation failure within a batch (its 0-based index + the error code/message).
#[derive(Debug, serde::Serialize)]
pub struct BatchFailure {
    pub index: usize,
    pub code: &'static str,
    pub message: String,
}

impl AppError {
    pub fn batch_invalid(failures: Vec<BatchFailure>) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            CODE_BATCH_INVALID,
            "one or more labels in the batch are invalid",
            Some(json!({ "failures": failures })),
        )
    }

    pub fn batch_too_large(count: usize, max: usize) -> Self {
        Self::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            CODE_BATCH_TOO_LARGE,
            format!("batch has {count} labels; the maximum is {max}"),
            Some(json!({ "count": count, "max": max })),
        )
    }

    /// The stable error `code` string (for tests / introspection).
    pub fn code(&self) -> &'static str {
        self.code
    }
}
```

(`code` is a private field; this accessor lets integration tests assert the code. `json!` and `StatusCode` are already imported in this file.)

- [ ] **Step 3: Build**

Run: `cargo build 2>&1 | tail -5`
Expected: clean (these are unused until Task 3/5; a `dead_code` warning on `batch_too_large`/`batch_invalid`/`BatchFailure` is acceptable and consumed later; do not `#[allow]`).

- [ ] **Step 4: Commit**

```bash
git add src/errors.rs
git commit -m "Add BatchInvalid and BatchTooLarge error codes (#30)"
```

---

## Task 2: Multi-page sheet pagination

**Files:**
- Modify: `src/render/mod.rs` (replace `render_sheet_labels`, lines ~125-205, and its test names)

Today `render_sheet_labels` errors when `start_slot + labels.len() > positions.len()`. Generalize it to paginate across pages and to collect per-label render failures by index.

- [ ] **Step 1: Write the failing tests**

In the `tests` module of `src/render/mod.rs`, add (the module already imports the needed types and `json`, `BTreeMap`, `HashMap`, and has `no_settings()`):

```rust
    fn two_slot_sheet() -> TemplateDefinition {
        TemplateDefinition {
            id: "sheet2".to_string(),
            name: "Sheet2".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Sheet {
                paper_width: 20.0,
                paper_height: 10.0,
                label_width: 10.0,
                label_height: 10.0,
                positions: vec![SheetPosition([0.0, 0.0]), SheetPosition([10.0, 0.0])],
            },
            options: None,
            layout: Layout::Items(vec![LayoutItem::Text {
                name: Some("message".to_string()),
                value: None,
                placement: Placement {
                    at: Position([0.0, 0.0]),
                    size: Size([SizeValue::Value(10.0), SizeValue::Value(10.0)]),
                    max_w: None,
                    max_h: None,
                    rotate: None,
                },
                font_size: FontSize::Fixed(8.0),
                multiline: false,
                alignment: Alignment::default(),
            }]),
            version: None,
        }
    }

    fn label(msg: &str) -> LabelInput {
        LabelInput {
            data: HashMap::from([("message".to_string(), json!(msg))]),
            option: None,
        }
    }

    #[test]
    fn sheet_pages_paginate_overflow() {
        // 2-slot sheet, 3 labels => 2 pages.
        let labels = vec![label("a"), label("b"), label("c")];
        let pdf = render_sheet_pages(&two_slot_sheet(), &labels, 0, &no_settings()).expect("render");
        assert!(pdf.starts_with(b"%PDF"));
        // 2 pages: count "/Type /Page" occurrences is brittle; instead assert the doc has 2 pages
        // by re-parsing is overkill — assert non-empty and PDF header here, page-count asserted via
        // the batch test that checks the doc. (Keep this test to the pagination-not-erroring contract.)
    }

    #[test]
    fn sheet_pages_respect_start_slot() {
        // start_slot=1 on a 2-slot sheet with 2 labels => label 0 in slot 1 (page 1), label 1 page 2.
        let labels = vec![label("a"), label("b")];
        let pdf = render_sheet_pages(&two_slot_sheet(), &labels, 1, &no_settings()).expect("render");
        assert!(pdf.starts_with(b"%PDF"));
    }

    #[test]
    fn sheet_pages_collect_bad_label_index() {
        // label 1 is missing "message" => BatchInvalid naming index 1.
        let labels = vec![
            label("a"),
            LabelInput { data: HashMap::new(), option: None },
        ];
        let err = render_sheet_pages(&two_slot_sheet(), &labels, 0, &no_settings()).unwrap_err();
        assert_eq!(err.code(), "BatchInvalid");
    }
```

To assert page count precisely, add a tiny helper to the render module (see Step 3 `page_count`) and use it in the batch tests (Task 3). For this task the PDF-header + no-error contract is enough.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --lib render::tests::sheet_pages 2>&1 | tail -20`
Expected: FAIL — `render_sheet_pages` not found.

- [ ] **Step 3: Implement `render_sheet_pages`**

Replace the whole `render_sheet_labels` function (signature `pub fn render_sheet_labels(template, labels, start_slot, settings)`) with:

```rust
pub fn render_sheet_pages(
    template: &TemplateDefinition,
    labels: &[LabelInput],
    start_slot: u32,
    settings: &BTreeMap<String, String>,
) -> Result<Vec<u8>, AppError> {
    let TemplateFormat::Sheet {
        paper_width,
        paper_height,
        label_width,
        label_height,
        positions,
    } = &template.format
    else {
        return Err(AppError::unsupported_format(
            "render_sheet_pages only supports sheet format",
        ));
    };

    let start_slot = start_slot as usize;
    if start_slot >= positions.len() && !labels.is_empty() {
        return Err(AppError::invalid_request("start_slot is out of range"));
    }

    let page_width_units = *paper_width;
    let page_height_units = *paper_height;
    let unit = &template.unit;
    let items = select_layout_items(template)?;

    // Assign each label a (page, slot): page 0 starts at start_slot, later pages start at 0.
    let slots_per_page = positions.len();
    let mut placements: Vec<(usize, usize)> = Vec::with_capacity(labels.len());
    let mut slot = start_slot;
    let mut page = 0usize;
    for _ in labels {
        if slot >= slots_per_page {
            page += 1;
            slot = 0;
        }
        placements.push((page, slot));
        slot += 1;
    }
    let page_count = placements.last().map(|(p, _)| p + 1).unwrap_or(1);

    let images = RefCell::new(ImageCollector::default());

    // Validation/render pass: render each label's content, collecting failures by index.
    let mut rendered: Vec<String> = Vec::with_capacity(labels.len());
    let mut failures: Vec<crate::errors::BatchFailure> = Vec::new();
    for (idx, lbl) in labels.iter().enumerate() {
        let selected_option = match normalize_option(template, lbl.option.as_ref()) {
            Ok(opt) => opt,
            Err(err) => {
                failures.push(crate::errors::BatchFailure {
                    index: idx,
                    code: err.code(),
                    message: err.message_text(),
                });
                rendered.push(String::new());
                continue;
            }
        };
        let context = RenderContext::new(
            *label_width,
            *label_height,
            unit,
            &lbl.data,
            selected_option,
            settings,
            &images,
        );
        match context.render_items(items) {
            Ok(content) => rendered.push(content),
            Err(err) => {
                failures.push(crate::errors::BatchFailure {
                    index: idx,
                    code: err.code(),
                    message: err.message_text(),
                });
                rendered.push(String::new());
            }
        }
    }
    if !failures.is_empty() {
        return Err(AppError::batch_invalid(failures));
    }

    // Compose `page_count` pages.
    let mut source = String::new();
    let page_w = format_length(page_width_units, unit)?;
    let page_h = format_length(page_height_units, unit)?;
    for p in 0..page_count {
        writeln!(
            source,
            "#set page(width: {page_w}, height: {page_h}, margin: 0{unit})"
        )
        .map_err(|err| AppError::render_failed(format!("failed to build typst source: {err}")))?;
        if p == 0 {
            writeln!(source, "#set text(font: (\"Inter Variable\", \"Inter\"))").map_err(|err| {
                AppError::render_failed(format!("failed to build typst source: {err}"))
            })?;
        } else {
            writeln!(source, "#pagebreak()").map_err(|err| {
                AppError::render_failed(format!("failed to build typst source: {err}"))
            })?;
        }
        for (idx, (lp, ls)) in placements.iter().enumerate() {
            if *lp != p {
                continue;
            }
            let point = positions[*ls].point();
            let top = point.y + *label_height;
            let dx = format_length(point.x, unit)?;
            let dy = format_length(page_height_units - top, unit)?;
            let bw = format_length(*label_width, unit)?;
            let bh = format_length(*label_height, unit)?;
            writeln!(
                source,
                "#place(top + left, dx: {dx}, dy: {dy})[#box(width: {bw}, height: {bh}, clip: true)[{}]]",
                rendered[idx]
            )
            .map_err(|err| AppError::render_failed(format!("failed to build typst source: {err}")))?;
        }
    }

    let doc = compile_paged(source, images.into_inner().files)?;
    typst_pdf::pdf(&doc, &Default::default())
        .map_err(|err| AppError::render_failed(format!("failed to encode pdf: {err:?}")))
}
```

Notes: this reuses `compile_paged`, `select_layout_items`, `normalize_option`, `RenderContext`, `format_length`, `ImageCollector` (all already in this file). The Typst `#pagebreak()` between page preambles creates the additional pages. `BatchFailure`, `AppError::code()`, `message_text()` come from Task 1.

- [ ] **Step 4: Add a test-only `page_count` helper for later tasks**

Add to `src/render/mod.rs` (not behind cfg(test) — the batch tests in a sibling module need it; keep it `pub`):

```rust
/// Count `%PDF` page objects by counting Typst page breaks is unreliable; instead count "/Page"
/// markers. Test helper for asserting pagination.
pub fn count_pdf_pages(pdf: &[u8]) -> usize {
    // Each page is a "/Type /Page" object (not "/Pages"). Count those.
    let needle = b"/Type /Page";
    let mut count = 0usize;
    let mut i = 0;
    while let Some(pos) = pdf[i..].windows(needle.len()).position(|w| w == needle) {
        let at = i + pos;
        // Exclude "/Type /Pages" (the page tree node).
        let after = at + needle.len();
        if pdf.get(after) != Some(&b's') {
            count += 1;
        }
        i = after;
    }
    count
}
```

(If this proves flaky against the Typst PDF output during implementation, assert page count via `typst`'s `PagedDocument.pages.len()` instead by having `render_sheet_pages` expose pages in a test build. Verify empirically.)

- [ ] **Step 5: Run to verify pass**

Run: `cargo test --lib render::tests::sheet_pages 2>&1 | tail -20`
Expected: PASS (3 tests).

- [ ] **Step 6: Fix the old caller + tests**

`render_sheet_labels` is gone. Update its references:
- `src/api.rs` import `render::render_sheet_labels` and its use in the (about-to-be-removed) `render_batch` handler — leave for Task 5, but to keep the build green now, rename the import to `render_sheet_pages` and the call at `src/api.rs:757` to `render_sheet_pages(template, &req.labels, req.start_slot, &settings)`.
- Rename the existing tests `render_sheet_labels_produces_pdf` / `render_sheet_labels_with_image_produces_pdf` calls to `render_sheet_pages(...)`.

Run: `cargo test --lib 2>&1 | tail -15` → all pass. `cargo clippy --all-targets --all-features 2>&1 | tail -10` → no new warnings.

- [ ] **Step 7: Commit**

```bash
git add src/render/mod.rs src/api.rs
git commit -m "Generalize sheet rendering to multi-page pagination (#30)"
```

---

## Task 3: `batch` module — render_batch dispatch + validate

**Files:**
- Create: `src/batch.rs`
- Modify: `src/main.rs` and/or `src/lib.rs` (add `pub mod batch;` next to the other `mod` declarations — check `rg -n "^mod |^pub mod " src/lib.rs src/main.rs`)

- [ ] **Step 1: Write the failing tests**

Create `src/batch.rs` with the implementation stubbed enough to compile only after Step 2; first write the test module at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        Alignment, Dimension, FontSize, LabelInput, Layout, LayoutItem, Placement, Position,
        SheetPosition, Size, SizeValue, TemplateFormat,
    };
    use crate::templates::TemplateDefinition;
    use serde_json::json;
    use std::collections::{BTreeMap, HashMap};

    fn no_settings() -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    fn single_tpl() -> TemplateDefinition {
        TemplateDefinition {
            id: "s".to_string(), name: "S".to_string(), description: String::new(),
            unit: "mm".to_string(), dpi: 200,
            format: TemplateFormat::Single { width: Dimension::Fixed(20.0), height: Dimension::Fixed(10.0) },
            options: None,
            layout: Layout::Items(vec![LayoutItem::Text {
                name: Some("message".to_string()), value: None,
                placement: Placement { at: Position([0.0,0.0]), size: Size([SizeValue::Value(20.0), SizeValue::Value(8.0)]), max_w: None, max_h: None, rotate: None },
                font_size: FontSize::Fixed(8.0), multiline: false, alignment: Alignment::default(),
            }]),
            version: None,
        }
    }

    fn lbl(msg: &str) -> LabelInput {
        LabelInput { data: HashMap::from([("message".to_string(), json!(msg))]), option: None }
    }

    #[test]
    fn single_download_zips_each_label() {
        let labels = vec![lbl("a"), lbl("b")];
        let out = render_batch(&single_tpl(), &labels, BatchMode::Download, Some("png"), 0, &no_settings(), 500).unwrap();
        match out {
            RenderedBatch::Download { bytes, content_type, .. } => {
                assert_eq!(content_type, "application/zip");
                assert_eq!(&bytes[..4], b"PK\x03\x04");
            }
            _ => panic!("expected download"),
        }
    }

    #[test]
    fn single_print_one_unit_per_label() {
        let labels = vec![lbl("a"), lbl("b"), lbl("c")];
        let out = render_batch(&single_tpl(), &labels, BatchMode::Print, None, 0, &no_settings(), 500).unwrap();
        match out {
            RenderedBatch::Print { units } => {
                assert_eq!(units.len(), 3);
                assert_eq!(units[1].indices, vec![1]);
            }
            _ => panic!("expected print"),
        }
    }

    #[test]
    fn bad_label_is_batch_invalid_with_index() {
        let labels = vec![lbl("a"), LabelInput { data: HashMap::new(), option: None }];
        let err = render_batch(&single_tpl(), &labels, BatchMode::Download, Some("png"), 0, &no_settings(), 500).unwrap_err();
        assert_eq!(err.code(), "BatchInvalid");
    }

    #[test]
    fn over_cap_is_too_large() {
        let labels = vec![lbl("a"), lbl("b")];
        let err = render_batch(&single_tpl(), &labels, BatchMode::Download, Some("png"), 0, &no_settings(), 1).unwrap_err();
        assert_eq!(err.code(), "BatchTooLarge");
    }
}
```

- [ ] **Step 2: Implement `src/batch.rs`**

Above the test module:

```rust
//! Unified batch rendering (ADR-0011). Renders a list of resolved labels into either a download blob
//! (ZIP for single templates, PDF for sheet) or a set of print artifacts. Pure/sync; the async print
//! dispatch lives in the `/batch` handler.

use std::collections::BTreeMap;
use std::io::Write as _;

use crate::errors::{AppError, BatchFailure};
use crate::models::{LabelInput, TemplateFormat};
use crate::render::{render_sheet_pages, render_single_label, render_single_label_pdf};
use crate::templates::TemplateDefinition;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchMode {
    Download,
    Print,
}

/// One print job's bytes plus the label indices it covers (single: one label; sheet: all labels).
pub struct PrintUnit {
    pub bytes: Vec<u8>,
    pub indices: Vec<usize>,
}

pub enum RenderedBatch {
    Download {
        bytes: Vec<u8>,
        content_type: &'static str,
        filename: String,
    },
    Print {
        units: Vec<PrintUnit>,
    },
}

pub fn render_batch(
    template: &TemplateDefinition,
    labels: &[LabelInput],
    mode: BatchMode,
    format: Option<&str>, // download only
    start_slot: u32,
    settings: &BTreeMap<String, String>,
    max_labels: usize,
) -> Result<RenderedBatch, AppError> {
    if labels.len() > max_labels {
        return Err(AppError::batch_too_large(labels.len(), max_labels));
    }
    if labels.is_empty() {
        return Err(AppError::invalid_request("batch has no labels"));
    }

    match &template.format {
        TemplateFormat::Single { .. } => render_single_batch(template, labels, mode, format, settings),
        TemplateFormat::Sheet { .. } => render_sheet_batch(template, labels, mode, start_slot, settings),
    }
}

fn render_single_batch(
    template: &TemplateDefinition,
    labels: &[LabelInput],
    mode: BatchMode,
    format: Option<&str>,
    settings: &BTreeMap<String, String>,
) -> Result<RenderedBatch, AppError> {
    let fmt = format.unwrap_or("png");
    let (content_type, ext): (&'static str, &'static str) = match fmt {
        "" | "png" => ("application/zip", "png"),
        "pdf" => ("application/zip", "pdf"),
        other => {
            return Err(AppError::invalid_request(format!(
                "unknown format '{other}'; use png or pdf"
            )))
        }
    };

    // Validate + render every label, collecting failures by index.
    let mut artifacts: Vec<Vec<u8>> = Vec::with_capacity(labels.len());
    let mut failures: Vec<BatchFailure> = Vec::new();
    for (idx, lbl) in labels.iter().enumerate() {
        let res = match fmt {
            "pdf" => render_single_label_pdf(template, &lbl.data, lbl.option.as_ref(), settings),
            _ => render_single_label(template, &lbl.data, lbl.option.as_ref(), settings),
        };
        match res {
            Ok(bytes) => artifacts.push(bytes),
            Err(err) => {
                failures.push(BatchFailure { index: idx, code: err.code(), message: err.message_text() });
                artifacts.push(Vec::new());
            }
        }
    }
    if !failures.is_empty() {
        return Err(AppError::batch_invalid(failures));
    }

    match mode {
        BatchMode::Print => Ok(RenderedBatch::Print {
            units: artifacts
                .into_iter()
                .enumerate()
                .map(|(i, bytes)| PrintUnit { bytes, indices: vec![i] })
                .collect(),
        }),
        BatchMode::Download => {
            let width = labels.len().to_string().len();
            let mut cursor = std::io::Cursor::new(Vec::new());
            let mut zip = zip::ZipWriter::new(&mut cursor);
            let opts: zip::write::SimpleFileOptions =
                zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            for (i, bytes) in artifacts.iter().enumerate() {
                let name = format!("{:0width$}.{ext}", i + 1, width = width);
                zip.start_file(name, opts)
                    .map_err(|e| AppError::render_failed(format!("zip error: {e}")))?;
                zip.write_all(bytes)
                    .map_err(|e| AppError::render_failed(format!("zip error: {e}")))?;
            }
            zip.finish().map_err(|e| AppError::render_failed(format!("zip error: {e}")))?;
            Ok(RenderedBatch::Download {
                bytes: cursor.into_inner(),
                content_type,
                filename: format!("{}.zip", template.id),
            })
        }
    }
}

fn render_sheet_batch(
    template: &TemplateDefinition,
    labels: &[LabelInput],
    mode: BatchMode,
    start_slot: u32,
    settings: &BTreeMap<String, String>,
) -> Result<RenderedBatch, AppError> {
    // render_sheet_pages does the per-label validation (BatchInvalid) and pagination.
    let pdf = render_sheet_pages(template, labels, start_slot, settings)?;
    match mode {
        BatchMode::Download => Ok(RenderedBatch::Download {
            bytes: pdf,
            content_type: "application/pdf",
            filename: format!("{}.pdf", template.id),
        }),
        BatchMode::Print => Ok(RenderedBatch::Print {
            units: vec![PrintUnit { bytes: pdf, indices: (0..labels.len()).collect() }],
        }),
    }
}
```

(`render_single_label`/`_pdf`/`render_sheet_pages` must be `pub` in `src/render/mod.rs` — they already are. `BatchFailure` is `pub` from Task 1.)

- [ ] **Step 3: Declare the module**

Add `pub mod batch;` to `src/lib.rs` (alongside the other `pub mod`/`mod` declarations).

- [ ] **Step 4: Run**

Run: `cargo test --lib batch:: 2>&1 | tail -20` → PASS (4 tests).
Then `cargo clippy --all-targets --all-features 2>&1 | tail -10` → no new warnings.

- [ ] **Step 5: Commit**

```bash
git add src/batch.rs src/lib.rs
git commit -m "Add batch module: render_batch dispatch + validate-then-render (#30)"
```

---

## Task 4: Request/response models + openapi

**Files:**
- Modify: `src/models.rs`, `src/openapi.rs`

- [ ] **Step 1: Add models**

In `src/models.rs`:

```rust
#[derive(Debug, Deserialize, ToSchema)]
pub struct BatchRequest {
    pub template: String,
    pub labels: Vec<LabelInput>,
    pub mode: String, // "download" | "print"
    #[serde(default)]
    pub printer: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub start_slot: u32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BatchRowError {
    pub index: usize,
    pub error: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BatchSummary {
    pub total: usize,
    pub succeeded: usize,
    pub failed: Vec<BatchRowError>,
    pub jobs: usize,
}
```

- [ ] **Step 2: Register in openapi**

In `src/openapi.rs`: add `api::batch` to `paths(...)`, add `BatchRequest, BatchSummary, BatchRowError` to `schemas(...)` and the `models::{...}` import. Remove `api::print` and `api::render_batch` from `paths(...)` (handlers deleted in Task 5) — do this in Task 5 to keep the build green; for now just add the new ones.

- [ ] **Step 3: Build**

Run: `cargo build 2>&1 | tail -5` → clean (models unused until Task 5; acceptable).

- [ ] **Step 4: Commit**

```bash
git add src/models.rs src/openapi.rs
git commit -m "Add batch request/response models (#30)"
```

---

## Task 5: `/batch` handler + route; remove `/print` and `/render/batch`

**Files:**
- Modify: `src/api.rs`, `src/openapi.rs`, `src/lib.rs`

- [ ] **Step 1: Add a max-labels constant + the handler**

In `src/api.rs`, add near the top:

```rust
const MAX_BATCH_LABELS: usize = 500;
```

Add the handler (model the printer lookup on the existing `print`/`import_csv` handlers):

```rust
#[utoipa::path(
    post,
    path = "/batch",
    request_body = BatchRequest,
    responses(
        (status = 200, description = "Download blob (zip/pdf) or print summary"),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 404, description = "Template or printer not found", body = ErrorResponse),
        (status = 409, description = "Printer disabled", body = ErrorResponse),
        (status = 413, description = "Batch too large", body = ErrorResponse),
        (status = 422, description = "One or more labels invalid", body = ErrorResponse),
        (status = 502, description = "Printer transport failure", body = ErrorResponse)
    )
)]
pub async fn batch(
    State(state): State<Arc<AppState>>,
    payload: Result<Json<BatchRequest>, JsonRejection>,
) -> Result<Response, AppError> {
    let Json(req) = payload.map_err(AppError::from)?;
    let registry = state.templates.load_full();
    let template = registry
        .get(&req.template)
        .ok_or_else(|| AppError::template_not_found(req.template.clone()))?;
    let is_single = matches!(template.format, crate::models::TemplateFormat::Single { .. });

    let mode = match req.mode.as_str() {
        "download" => crate::batch::BatchMode::Download,
        "print" => crate::batch::BatchMode::Print,
        other => return Err(AppError::invalid_request(format!("unknown mode '{other}'; use download or print"))),
    };
    if req.start_slot > 0 && is_single {
        return Err(AppError::invalid_request("start_slot applies only to sheet templates"));
    }
    let settings = state.store().all_settings().await?;

    match mode {
        crate::batch::BatchMode::Download => {
            let rendered = crate::batch::render_batch(
                template, &req.labels, mode, req.format.as_deref(), req.start_slot, &settings, MAX_BATCH_LABELS,
            )?;
            let crate::batch::RenderedBatch::Download { bytes, content_type, filename } = rendered else {
                return Err(AppError::internal("batch returned non-download for download mode"));
            };
            Ok(download_response(bytes, content_type, &filename))
        }
        crate::batch::BatchMode::Print => {
            if req.format.is_some() {
                return Err(AppError::invalid_request("format applies only to download; omit it when printing"));
            }
            let printer_id = req.printer.as_deref()
                .ok_or_else(|| AppError::invalid_request("mode=print requires a printer"))?;
            let printer = state.store().get_printer(printer_id).await?
                .ok_or_else(|| AppError::printer_not_found(printer_id.to_string()))?;
            if !printer.enabled {
                return Err(AppError::printer_disabled(printer_id));
            }
            let driver = crate::driver::build_driver(&printer.kind, &printer.config)
                .map_err(|err| AppError::printer_invalid(err.to_string()))?;
            // Render first (validate-then-execute): bad data => 422 before any send.
            let rendered = crate::batch::render_batch(
                template, &req.labels, mode, None, req.start_slot, &settings, MAX_BATCH_LABELS,
            )?;
            let crate::batch::RenderedBatch::Print { units } = rendered else {
                return Err(AppError::internal("batch returned non-print for print mode"));
            };
            // The driver consumes one artifact format; our single units are png/pdf-agnostic bytes
            // (we render to the driver's accepted format below). Re-render single units to the driver
            // format is avoided by rendering here per accepted_format. For simplicity Phase 1 drivers
            // accept Pdf; render units already match (single png path won't be printed). See note.
            let total = req.labels.len();
            let mut failed = Vec::new();
            let jobs = units.len();
            for unit in &units {
                if let Err(err) = driver.send(&unit.bytes, &crate::driver::PrintOptions::default()).await {
                    let msg = err.to_string();
                    for &i in &unit.indices {
                        failed.push(crate::models::BatchRowError { index: i, error: msg.clone() });
                    }
                }
                let _ = state.store().record_job(&req.template, Some(printer_id),
                    if failed.is_empty() { "ok" } else { "failed" }, None).await;
            }
            let summary = crate::models::BatchSummary {
                total, succeeded: total - failed.len(), failed, jobs,
            };
            Ok((axum::http::StatusCode::OK, Json(summary)).into_response())
        }
    }
}
```

**Driver-format note (resolve during implementation):** the CUPS driver accepts PDF. For single+print, `render_batch` currently renders PNG by default. Make print-mode single rendering use the driver's accepted format: pass the driver's `accepted_format()` into `render_batch` for print (e.g. add a `print_format: Option<&str>` arg, or have the handler request `Some("pdf")` for a Pdf driver). Wire this so single+print produces PDF bytes for a CUPS driver. Add a test with the `fake` driver (accepts Pdf) asserting single+print sends PDF-headed bytes. Keep the contract: `render_batch(..., format)` already takes a format; for print pass the driver format.

- [ ] **Step 2: Add the route, remove old routes/handlers**

In `app()`: add `.route("/batch", post(batch))`. Remove `.route("/render/batch", post(render_batch))` and `.route("/print", post(print))`. Delete the `print` and `render_batch` handler fns and their `#[utoipa::path]` blocks. Remove `render_batch`/`print` from `src/openapi.rs` `paths(...)`. Remove now-unused imports (`render_sheet_pages` if only used by deleted code — but batch.rs uses it; the api.rs import may become unused → drop it).

- [ ] **Step 3: Migrate + add integration tests**

In `src/lib.rs`: delete tests that POST `/print` and `/render/batch`; re-express the meaningful ones against `/batch`. Add tests (mirror the existing http_tests harness — `build_app()`, `json_req`, register a `fake` printer for print):
- `batch_single_download_returns_zip`: POST `/batch` {single template, 2 labels, mode download} → 200, `application/zip`, body starts `PK`.
- `batch_sheet_download_returns_pdf`: sheet template, labels → 200 `application/pdf`, `%PDF`.
- `batch_invalid_label_returns_422`: one bad label → 422, body `error.code == "BatchInvalid"`, `details.failures[0].index` present.
- `batch_too_large_returns_413`: labels over a small cap (use the real `MAX_BATCH_LABELS` is 500; instead test the constraint by... keep one test asserting the 413 path via a template+labels of length > 500 is heavy; instead assert the error is produced by unit test in Task 3, and skip the HTTP 413 test) — OR lower-risk: trust Task 3's `over_cap_is_too_large`. Do not add a 500-label HTTP test.
- `batch_print_summary`: register a `fake` printer (succeeds), POST mode print → 200 summary `{total, succeeded, failed:[]}`; then a `fake` printer with `{"fail":true}` → 200 with `failed` populated and correct indices.
- `batch_start_slot_on_single_400`: single template + start_slot=1 → 400.

- [ ] **Step 4: Run**

Run: `cargo test 2>&1 | tail -20` → all pass. `cargo fmt`; `cargo clippy --all-targets --all-features 2>&1 | tail -10` → zero warnings.

- [ ] **Step 5: Commit**

```bash
git add src/api.rs src/openapi.rs src/lib.rs
git commit -m "Add /batch endpoint; remove /print and /render/batch (#30)"
```

---

## Task 6: Refactor `/import/csv` onto the batch path

**Files:**
- Modify: `src/api.rs`

- [ ] **Step 1: Rewrite `import_csv` to build labels + reuse batch**

`/import/csv` should parse the CSV into `Vec<LabelInput>` (data only for now; `option.` columns are #32, out of scope) and then run the exact same logic as `batch`. Extract the shared body of `batch` into a helper both call, e.g.:

```rust
async fn run_batch(
    state: &Arc<AppState>,
    template: &TemplateDefinition,
    labels: &[crate::models::LabelInput],
    mode: crate::batch::BatchMode,
    printer: Option<&str>,
    format: Option<&str>,
    start_slot: u32,
) -> Result<Response, AppError> { /* the match-on-mode body from Task 5, parameterized */ }
```

Then `batch` becomes: parse request → `run_batch(...)`. And `import_csv` becomes: parse CSV → `labels: Vec<LabelInput> = rows.map(|data| LabelInput { data, option: None })` → `run_batch(...)` with `start_slot: 0`. Keep `parse_csv_rows` as-is. Remove the old `import_csv` render/zip/print loop and `render_to_format` if now unused (or keep `render_to_format` only if still referenced; otherwise delete it and `ImportSummary`/`ImportRowError` if `run_batch` returns `BatchSummary`). Prefer returning `BatchSummary` from both, deleting `ImportSummary`/`ImportRowError`.

- [ ] **Step 2: Update CSV tests**

In `src/lib.rs`, the existing `import_csv_*` tests should still pass (download zip, atomic invalid → now 422 `BatchInvalid` with `details.failures` instead of the old `details.row`; update those assertions to the new shape). Print-summary test asserts `BatchSummary` shape.

- [ ] **Step 3: Run**

Run: `cargo test 2>&1 | tail -20` → all pass. `cargo fmt`; `cargo clippy --all-targets --all-features 2>&1 | tail -10` → zero warnings.

- [ ] **Step 4: Commit**

```bash
git add src/api.rs src/lib.rs
git commit -m "Refactor /import/csv onto the shared batch path (#30)"
```

---

## Task 7: Docs, scripts, final gate, merge

**Files:**
- Modify: `docs/SPEC.md`, `docs/PLAN-phase-1.md`, `scripts/*.sh`, the design doc if a detail changed

- [ ] **Step 1: SPEC**

In `docs/SPEC.md`: replace the `/print` and `/render/batch` rows with `POST /batch`; document the request, the dispatch matrix, the validate-then-execute model, `BatchInvalid` (422) and `BatchTooLarge` (413); note `/render/label` stays for preview; update the Printing section and add a changelog entry. Confirm the design doc's "sheet print jobs" wording matches the implemented behavior (one job for the whole sheet PDF → `jobs: 1`); fix the design doc/spec if it diverged.

- [ ] **Step 2: PLAN + scripts**

`docs/PLAN-phase-1.md`: note `/print` + `/render/batch` removed, `/batch` added; rescope #28 (sheet-print + batch delivered here; server-side copies moot). Update `scripts/render_test.sh` / `scripts/render_avery_horizontal.sh` to call `/batch` (or `/render/label`) instead of `/print` / `/render/batch`; run one to confirm it still writes a file (`cargo run` then the script).

- [ ] **Step 3: Full gate**

```bash
cargo fmt
cargo clippy --all-targets --all-features 2>&1 | tail -20
cargo test 2>&1 | tail -20
```
Clean fmt, zero clippy, all tests pass. Fix root causes; never `#[allow]`.

- [ ] **Step 4: Adversarial review loop**

Per CLAUDE.md, run reviewer → fix → review against `git diff main...batch-endpoint` before merging. Focus: the validate-then-execute atomicity (nothing prints on a bad batch), single+print driver-format correctness, pagination math (start_slot + overflow), and that `/import/csv` behavior is preserved through the shared path.

- [ ] **Step 5: Merge**

```bash
git add docs scripts
git commit -m "Document /batch; update plan and scripts (#30)"
git checkout main && git merge batch-endpoint && git push
```
Reference `Fixes #30` in the merge commit so it closes on push.

---

## Self-review notes

- **Spec coverage:** contract + topology (T4,T5), dispatch matrix (T3), pagination (T2), sync + cap (T3,T5), validate-then-execute (T2,T3,T5), responses (T5), `/import/csv` refactor (T6), remove `/print`+`/render/batch` (T5), ADR/docs (T7). All spec sections map to a task.
- **Open implementation decision flagged inline:** single+print must render to the driver's accepted format (T5 driver-format note) — resolve with a test, not a guess.
- **Type consistency:** `render_batch(template, labels, mode, format, start_slot, settings, max_labels)` and `RenderedBatch`/`PrintUnit`/`BatchMode` are used identically in T3 and T5; `BatchSummary`/`BatchRowError` shared by `/batch` and `/import/csv` (T5,T6); `render_sheet_pages` signature consistent T2/T3.
- **Verify during impl:** `count_pdf_pages` heuristic (T2) and the `zip` `SimpleFileOptions` API (already used in the codebase) against installed versions.
