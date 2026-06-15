# M4 Integrations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a substitution-based variable/interpolation layer (`{field}`, `{settings.key}`) for text and QR content, a generic settings API, and CSV import that renders one label per row (ZIP download or per-row print jobs).

**Architecture:** Interpolation is a pure helper in `render/helpers.rs` consumed by the text and QR render paths. Settings flow from the SQLite `settings` table through a new `Store::all_settings()` into every render entry point via `RenderContext`. CSV import is a new handler that loops rows through the existing single-render and print-dispatch code.

**Tech Stack:** Rust, axum, Typst (`typst-as-lib`), rusqlite, `csv` crate (parse), `zip` crate (download bundle), utoipa.

**Spec:** `docs/superpowers/specs/2026-06-15-m4-integrations-design.md`. Issues #14, #21. ADR-0010 (new).

---

## File map

- `src/render/helpers.rs` — add `interpolate()` + unit tests.
- `src/models.rs` — `Text`/`Qr`: `name: Option<String>`, add `value: Option<String>`.
- `src/raw.rs` — `TextRaw`/`QrRaw`: `name: Option<String>`, add `value: Option<String>`.
- `src/convert.rs` — enforce exactly-one-of `name`/`value` for Text/Qr.
- `src/render/mod.rs` — thread `settings` through `RenderContext` + entry points; interpolate value-based text/QR; fix test fixtures.
- `src/templates.rs` — `layout_item_name` handles `Option` name; fix test fixtures.
- `src/store.rs` — add `all_settings()`.
- `src/api.rs` — load settings in render handlers; settings routes; CSV import handler.
- `src/models.rs` — add `SettingValue`, `ImportSummary`, `ImportRowError` response models.
- `src/openapi.rs` — register new paths + schemas.
- `Cargo.toml` — add `csv`, `zip`.
- `templates/*.yaml`, `docs/SPEC.md`, `docs/adr/0010-*.md`, `docs/CAPABILITIES.md`, `docs/PLAN-phase-1.md` — docs.

Work on a branch:

```bash
git checkout -b m4-integrations
```

---

## Task 1: Interpolation helper

**Files:**
- Modify: `src/render/helpers.rs`

- [ ] **Step 1: Write the failing tests**

Add at the end of `src/render/helpers.rs` (create a `#[cfg(test)] mod tests` block if none exists; if one exists, add these tests into it):

```rust
#[cfg(test)]
mod interpolate_tests {
    use super::interpolate;
    use serde_json::json;
    use std::collections::{BTreeMap, HashMap};

    fn data() -> HashMap<String, serde_json::Value> {
        HashMap::from([
            ("id".to_string(), json!("A1")),
            ("count".to_string(), json!(3)),
        ])
    }

    fn settings() -> BTreeMap<String, String> {
        BTreeMap::from([("qr_base_url".to_string(), "https://h/i".to_string())])
    }

    #[test]
    fn substitutes_field_and_setting() {
        let out = interpolate("{settings.qr_base_url}/{id}", &data(), &settings()).unwrap();
        assert_eq!(out, "https://h/i/A1");
    }

    #[test]
    fn stringifies_non_string_field() {
        assert_eq!(interpolate("n={count}", &data(), &settings()).unwrap(), "n=3");
    }

    #[test]
    fn literal_braces() {
        assert_eq!(interpolate("{{x}}", &data(), &settings()).unwrap(), "{x}");
    }

    #[test]
    fn missing_field_errors() {
        assert!(interpolate("{nope}", &data(), &settings()).is_err());
    }

    #[test]
    fn missing_setting_errors() {
        assert!(interpolate("{settings.nope}", &data(), &settings()).is_err());
    }

    #[test]
    fn unmatched_brace_errors() {
        assert!(interpolate("a{id", &data(), &settings()).is_err());
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --lib interpolate_tests 2>&1 | tail -20`
Expected: FAIL — `cannot find function interpolate`.

- [ ] **Step 3: Implement `interpolate`**

At the top of `src/render/helpers.rs`, ensure these imports exist (add any missing):

```rust
use crate::errors::AppError;
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, HashMap};
```

Add the function (near `value_to_string`):

```rust
/// Substitution-only interpolation (ADR-0010). `{field}` resolves from `data` via `value_to_string`,
/// `{settings.<key>}` from `settings`; `{{`/`}}` emit literal braces. An unresolved token or an
/// unmatched brace is an error.
pub(super) fn interpolate(
    template: &str,
    data: &HashMap<String, JsonValue>,
    settings: &BTreeMap<String, String>,
) -> Result<String, AppError> {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                if chars.peek() == Some(&'{') {
                    chars.next();
                    out.push('{');
                    continue;
                }
                let mut token = String::new();
                let mut closed = false;
                for tc in chars.by_ref() {
                    if tc == '}' {
                        closed = true;
                        break;
                    }
                    token.push(tc);
                }
                if !closed {
                    return Err(AppError::invalid_request(format!(
                        "unterminated '{{' in template '{template}'"
                    )));
                }
                let resolved = if let Some(key) = token.strip_prefix("settings.") {
                    settings
                        .get(key)
                        .cloned()
                        .ok_or_else(|| AppError::missing_field(&format!("settings.{key}")))?
                } else {
                    value_to_string(
                        data.get(&token)
                            .ok_or_else(|| AppError::missing_field(&token))?,
                    )
                };
                out.push_str(&resolved);
            }
            '}' => {
                if chars.peek() == Some(&'}') {
                    chars.next();
                    out.push('}');
                } else {
                    return Err(AppError::invalid_request(format!(
                        "unmatched '}}' in template '{template}'"
                    )));
                }
            }
            other => out.push(other),
        }
    }
    Ok(out)
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test --lib interpolate_tests 2>&1 | tail -20`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add src/render/helpers.rs
git commit -m "Add substitution interpolation helper (#14)"
```

---

## Task 2: Add `value` field to Text/Qr (model, raw, convert)

**Files:**
- Modify: `src/models.rs:238-255` (Text, Qr variants)
- Modify: `src/raw.rs:35-56` (TextRaw, QrRaw)
- Modify: `src/convert.rs:74-96` (Text/Qr conversion)
- Modify: `src/templates.rs:271-279` (`layout_item_name`)

- [ ] **Step 1: Write the failing test (convert exactly-one-of)**

Add a test module to `src/convert.rs` (end of file):

```rust
#[cfg(test)]
mod tests {
    use crate::raw::TemplateDefinitionRaw;
    use crate::templates::TemplateDefinition;

    fn try_build(layout_yaml: &str) -> Result<TemplateDefinition, String> {
        let yaml = format!(
            "id: t\nname: T\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 10\n  height: 10\nlayout:\n{layout_yaml}"
        );
        let raw: TemplateDefinitionRaw = serde_yaml::from_str(&yaml).map_err(|e| e.to_string())?;
        TemplateDefinition::try_from(raw).map_err(|e| e.to_string())
    }

    #[test]
    fn text_with_value_ok() {
        assert!(try_build("  - type: text\n    value: \"{id}\"\n    at: [0,0]\n    size: [10,5]\n    font_size: 8\n").is_ok());
    }

    #[test]
    fn text_with_name_ok() {
        assert!(try_build("  - type: text\n    name: id\n    at: [0,0]\n    size: [10,5]\n    font_size: 8\n").is_ok());
    }

    #[test]
    fn text_with_both_errors() {
        assert!(try_build("  - type: text\n    name: id\n    value: \"{id}\"\n    at: [0,0]\n    size: [10,5]\n    font_size: 8\n").is_err());
    }

    #[test]
    fn text_with_neither_errors() {
        assert!(try_build("  - type: text\n    at: [0,0]\n    size: [10,5]\n    font_size: 8\n").is_err());
    }
}
```

(Confirm `serde_yaml` is a dev-or-normal dependency: `rg serde_yaml Cargo.toml`. It is used by `parse.rs`, so it is available.)

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --lib convert::tests 2>&1 | tail -20`
Expected: FAIL — compile error (no `value` field yet) or assertion failures.

- [ ] **Step 3: Update `models.rs` Text/Qr**

In `src/models.rs`, change the `Text` and `Qr` variants of `LayoutItem` (lines ~239-255):

```rust
    Text {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<String>,
        #[serde(flatten)]
        placement: Placement,
        font_size: FontSize,
        #[serde(default)]
        multiline: bool,
        #[serde(default)]
        alignment: Alignment,
    },
    Qr {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<String>,
        #[serde(flatten)]
        placement: Placement,
        #[serde(skip_serializing_if = "Option::is_none")]
        params: Option<QrParams>,
    },
```

- [ ] **Step 4: Update `raw.rs` TextRaw/QrRaw**

In `src/raw.rs`:

```rust
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextRaw {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(flatten)]
    pub placement: Placement,
    pub font_size: FontSize,
    #[serde(default)]
    pub multiline: bool,
    #[serde(default)]
    pub alignment: Alignment,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QrRaw {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(flatten)]
    pub placement: Placement,
    #[serde(default)]
    pub params: Option<QrParams>,
}
```

- [ ] **Step 5: Update `convert.rs` with exactly-one-of**

In `src/convert.rs`, add a helper above `impl TryFrom<LayoutItemRaw>`:

```rust
fn require_one_of(
    kind: &str,
    name: Option<String>,
    value: Option<String>,
) -> Result<(Option<String>, Option<String>), TemplateError> {
    match (&name, &value) {
        (Some(_), Some(_)) => Err(TemplateError::Validation {
            path: kind.to_string(),
            msg: format!("{kind} must set exactly one of name or value, not both"),
        }),
        (None, None) => Err(TemplateError::Validation {
            path: kind.to_string(),
            msg: format!("{kind} must set one of name or value"),
        }),
        _ => Ok((name, value)),
    }
}
```

Replace the `Text` and `Qr` arms of `TryFrom<LayoutItemRaw>`:

```rust
            LayoutItemRaw::Text(TextRaw {
                name,
                value,
                placement,
                font_size,
                multiline,
                alignment,
            }) => {
                let (name, value) = require_one_of("text", name, value)?;
                Ok(LayoutItem::Text {
                    name,
                    value,
                    placement,
                    font_size,
                    multiline,
                    alignment,
                })
            }
            LayoutItemRaw::Qr(raw) => {
                let (name, value) = require_one_of("qr", raw.name, raw.value)?;
                Ok(LayoutItem::Qr {
                    name,
                    value,
                    placement: raw.placement,
                    params: raw.params,
                })
            }
```

Update the `use` line in `convert.rs` to import `TextRaw` (already imported) — no change needed; `TextRaw` is in the existing `use crate::raw::{...}`.

- [ ] **Step 6: Update `templates.rs` `layout_item_name`**

In `src/templates.rs`, the Text/Qr arms now hold `Option<String>`:

```rust
fn layout_item_name(item: &LayoutItem) -> Option<&str> {
    match item {
        LayoutItem::Text { name, .. } => name.as_deref(),
        LayoutItem::Qr { name, .. } => name.as_deref(),
        LayoutItem::Image { name, .. } => name.as_deref(),
        LayoutItem::Line { .. } => None,
        LayoutItem::Container { .. } => None,
    }
}
```

- [ ] **Step 7: Build to find all literal sites**

Run: `cargo build 2>&1 | rg "missing field|LayoutItem::Text|LayoutItem::Qr" | head -40`
Expected: errors at every `LayoutItem::Text { name: "...".to_string(), ... }` / `LayoutItem::Qr { ... }` literal that lacks `value`. These are in `src/render/mod.rs` test module and `src/templates.rs` test module.

- [ ] **Step 8: Fix every Text/Qr literal**

For each `LayoutItem::Text { name: "X".to_string(), ... }` change to `name: Some("X".to_string()), value: None,`. For each `LayoutItem::Qr { name: "X".to_string(), ... }` change to `name: Some("X".to_string()), value: None,`. (Locations: `src/render/mod.rs` test fixtures around lines 678, 719, 732, 796, 999; `src/templates.rs` test fixtures — find with the build errors.)

- [ ] **Step 9: Run convert + full build**

Run: `cargo test --lib convert::tests 2>&1 | tail -20`
Expected: PASS (4 tests). Then `cargo build 2>&1 | tail -5` → clean.

- [ ] **Step 10: Commit**

```bash
git add src/models.rs src/raw.rs src/convert.rs src/templates.rs src/render/mod.rs
git commit -m "Add value field to text/qr with exactly-one-of validation (#14)"
```

---

## Task 3: Thread settings through render + interpolate value items

**Files:**
- Modify: `src/store.rs` (add `all_settings`)
- Modify: `src/render/mod.rs` (RenderContext + entry points + text/qr render)
- Modify: `src/api.rs` (load settings in handlers, pass through)

- [ ] **Step 1: Add `Store::all_settings` + test**

In `src/store.rs`, add inside `impl Store` (after `set_setting`):

```rust
    pub async fn all_settings(&self) -> Result<std::collections::BTreeMap<String, String>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare("SELECT key, value FROM settings ORDER BY key")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut out = std::collections::BTreeMap::new();
        for row in rows {
            let (k, v) = row?;
            out.insert(k, v);
        }
        Ok(out)
    }
```

Add to the `settings_and_jobs` test in `store.rs` (after the existing set/get asserts):

```rust
        let all = store.all_settings().await.unwrap();
        assert_eq!(all.get("k").map(String::as_str), Some("v2"));
```

Run: `cargo test --lib store:: 2>&1 | tail -10` → PASS.

- [ ] **Step 2: Add `settings` to `RenderContext`**

In `src/render/mod.rs`, add the field and constructor param:

```rust
struct RenderContext<'a> {
    frame_width_units: f32,
    frame_height_units: f32,
    unit: &'a str,
    data: &'a HashMap<String, JsonValue>,
    selected_option: Option<&'a BTreeMap<String, String>>,
    settings: &'a BTreeMap<String, String>,
    images: &'a RefCell<ImageCollector>,
}
```

Update `RenderContext::new` to accept `settings: &'a BTreeMap<String, String>` (add the parameter after `selected_option`) and store it. Update the import line to include `interpolate`:

```rust
use helpers::{
    assets_root, build_qr_svg, escape_typst_string, fit_text_to_box, format_length, interpolate,
    parse_image_data_uri, resolve_dimension, resolve_image_asset, to_nonbreaking, to_page_coords,
    typst_alignment, typst_font_options, value_to_string,
};
```

- [ ] **Step 3: Update the three `RenderContext::new` call sites**

There are three constructions: in `build_typst_source` (line ~213), in `render_sheet_labels` loop (line ~162), and in `render_container_item` (line ~539). Each must pass `settings`. The container reuses `self.settings`. The other two receive `settings` from their function args (added next step). Example for the container:

```rust
        let context = RenderContext::new(
            inner_width,
            inner_height,
            self.unit,
            self.data,
            self.selected_option,
            self.settings,
            self.images,
        );
```

- [ ] **Step 4: Thread `settings` into entry points**

Add a `settings: &BTreeMap<String, String>` parameter to: `compile_single_doc`, `render_single_label`, `render_single_label_pdf`, `render_sheet_labels`, and `build_typst_source`. Pass it down. New signatures:

```rust
pub fn render_single_label(
    template: &TemplateDefinition,
    data: &HashMap<String, JsonValue>,
    option: Option<&BTreeMap<String, String>>,
    settings: &BTreeMap<String, String>,
) -> Result<Vec<u8>, AppError> { /* pass settings into compile_single_doc */ }
```

`compile_single_doc(template, data, option, settings)` forwards to `build_typst_source(..., selected_option, settings, &images)`, which builds the root `RenderContext::new(..., selected_option, settings, images)`. `render_single_label_pdf` mirrors `render_single_label`. In `render_sheet_labels`, add `settings` param and pass `settings` into the per-label `RenderContext::new`.

- [ ] **Step 5: Interpolate value-based text + QR**

Update `render_text_item` to take both name and value and resolve the raw text. Change its signature and call site. In `render_items`, the `Text` arm becomes:

```rust
                LayoutItem::Text {
                    name,
                    value,
                    placement,
                    font_size,
                    multiline,
                    alignment,
                } => {
                    self.render_text_item(
                        &mut out, name, value, placement, font_size, *multiline, alignment,
                    )?;
                }
```

`render_text_item` signature gains `value: &Option<String>` after `name: &Option<String>`, and computes `raw_text`:

```rust
        let raw_text = match (name, value) {
            (Some(name), _) => value_to_string(
                self.data
                    .get(name)
                    .ok_or_else(|| AppError::missing_field(name))?,
            ),
            (_, Some(template)) => interpolate(template, self.data, self.settings)?,
            (None, None) => {
                return Err(AppError::unsupported_layout_item("text requires name or value"))
            }
        };
```

(The rest of `render_text_item` is unchanged; `name` param type becomes `&Option<String>`.)

Apply the same change to the `Qr` arm and `render_qr_item`: signature gains `value: &Option<String>`; compute `payload`:

```rust
        let payload = match (name, value) {
            (Some(name), _) => value_to_string(
                self.data
                    .get(name)
                    .ok_or_else(|| AppError::missing_field(name))?,
            ),
            (_, Some(template)) => interpolate(template, self.data, self.settings)?,
            (None, None) => {
                return Err(AppError::unsupported_layout_item("qr requires name or value"))
            }
        };
```

- [ ] **Step 6: Fix render entry-point call sites in `api.rs`**

In `src/api.rs`, every call to `render_single_label`, `render_single_label_pdf`, `render_sheet_labels` now needs a settings map. Add to each handler, before rendering:

```rust
    let settings = state.store().all_settings().await?;
```

Pass `&settings` as the new final argument. Update `render_to_format` to take `settings: &std::collections::BTreeMap<String, String>` and forward it. Call sites: `print` (download + both driver-format renders), `render_label` (png + pdf arms), `render_batch` (`render_sheet_labels`).

- [ ] **Step 7: Fix render test call sites**

Every `render_single_label(...)`, `render_single_label_pdf(...)`, `render_sheet_labels(...)` in `src/render/mod.rs` tests needs a trailing settings arg. Add a helper at the top of the test module:

```rust
    fn no_settings() -> BTreeMap<String, String> {
        BTreeMap::new()
    }
```

and pass `&no_settings()` as the final argument to each call.

- [ ] **Step 8: Add a value-interpolation render test**

Add to `src/render/mod.rs` tests:

```rust
    #[test]
    fn render_value_text_and_qr_interpolate() {
        let template = TemplateDefinition {
            id: "interp".to_string(),
            name: "Interp".to_string(),
            description: String::new(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: Dimension::Fixed(40.0),
                height: Dimension::Fixed(20.0),
            },
            options: None,
            layout: Layout::Items(vec![
                LayoutItem::Text {
                    name: None,
                    value: Some("Item {id}".to_string()),
                    placement: Placement {
                        at: Position([0.0, 10.0]),
                        size: Size([SizeValue::Value(40.0), SizeValue::Value(8.0)]),
                        max_w: None,
                        max_h: None,
                        rotate: None,
                    },
                    font_size: FontSize::Fixed(8.0),
                    multiline: false,
                    alignment: Alignment::default(),
                },
                LayoutItem::Qr {
                    name: None,
                    value: Some("{settings.qr_base_url}/{id}".to_string()),
                    placement: Placement {
                        at: Position([0.0, 0.0]),
                        size: Size([SizeValue::Value(10.0), SizeValue::Value(10.0)]),
                        max_w: None,
                        max_h: None,
                        rotate: None,
                    },
                    params: None,
                },
            ]),
            version: None,
        };
        let data = HashMap::from([("id".to_string(), json!("A1"))]);
        let settings = BTreeMap::from([("qr_base_url".to_string(), "https://h/i".to_string())]);
        let png = render_single_label(&template, &data, None, &settings).expect("render interp");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");

        // Missing setting is an error.
        assert!(render_single_label(&template, &data, None, &no_settings()).is_err());
    }
```

- [ ] **Step 9: Run all render + lib tests**

Run: `cargo test --lib 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add src/store.rs src/render/mod.rs src/api.rs
git commit -m "Thread settings into render and interpolate value-based text/qr (#14)"
```

---

## Task 4: Settings API

**Files:**
- Modify: `src/models.rs` (add `SettingValue`)
- Modify: `src/api.rs` (routes + handlers)
- Modify: `src/openapi.rs`
- Modify: `src/lib.rs` (HTTP integration test)

- [ ] **Step 1: Add response model**

In `src/models.rs`:

```rust
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SettingValue {
    pub value: String,
}
```

- [ ] **Step 2: Add handlers + routes**

In `src/api.rs`, add handlers:

```rust
#[utoipa::path(
    get,
    path = "/settings",
    responses((status = 200, description = "All settings", body = std::collections::BTreeMap<String, String>))
)]
pub async fn get_settings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<std::collections::BTreeMap<String, String>>, AppError> {
    Ok(Json(state.store().all_settings().await?))
}

#[utoipa::path(
    put,
    path = "/settings/{key}",
    params(("key" = String, Path, description = "Setting key")),
    request_body = SettingValue,
    responses(
        (status = 200, description = "Setting stored", body = SettingValue),
        (status = 400, description = "Invalid key", body = ErrorResponse)
    )
)]
pub async fn put_setting(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
    Json(body): Json<SettingValue>,
) -> Result<Json<SettingValue>, AppError> {
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.') {
        return Err(AppError::invalid_request(format!(
            "setting key '{key}' must be non-empty and contain only letters, digits, '_', '-' or '.'"
        )));
    }
    let _guard = state.write_lock.lock().await;
    state.store().set_setting(&key, &body.value).await?;
    Ok(Json(body))
}
```

Add `SettingValue` to the `models` import block at the top of `api.rs`. Add routes in `app()`:

```rust
        .route("/settings", get(get_settings))
        .route("/settings/{key}", put(put_setting))
```

(`put` is already imported via `routing::{get, post}` — add `put`: change to `routing::{get, post, put}`.)

- [ ] **Step 3: Register in openapi**

In `src/openapi.rs`, add `api::get_settings, api::put_setting` to `paths(...)` and `SettingValue` to `schemas(...)`. Add `SettingValue` to the `models::{...}` import.

- [ ] **Step 4: HTTP integration test**

Find the integration tests in `src/lib.rs` (`rg -n "async fn .*server|app\(" src/lib.rs`). Add a test that PUTs then GETs a setting, following the existing test style (use the same test harness/helpers as the printer CRUD tests). Minimal shape:

```rust
#[tokio::test]
async fn settings_put_then_get() {
    let app = test_app().await; // use whatever harness the existing tests use
    // PUT /settings/qr_base_url {"value":"https://h/i"}
    // assert 200
    // GET /settings -> body contains "qr_base_url":"https://h/i"
}
```

(Match the actual harness in `lib.rs`; replicate a printer test and adapt the path/body.)

- [ ] **Step 5: Run**

Run: `cargo test 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/models.rs src/api.rs src/openapi.rs src/lib.rs
git commit -m "Add settings API (#14)"
```

---

## Task 5: QR base-URL demo template + docs (#14, ADR-0010)

**Files:**
- Modify: one starter template under `templates/` (pick the QR-bearing one; inspect with `rg -l "type: qr" templates/`)
- Create: `docs/adr/0010-variable-interpolation-layer.md`
- Modify: `docs/SPEC.md` (+ changelog), `docs/CAPABILITIES.md` (§3.1), `docs/PLAN-phase-1.md`

- [ ] **Step 1: Update a starter template to use `value`**

In a QR-bearing starter template, change the QR item from `name: code` (or similar) to:

```yaml
  - type: qr
    value: "{settings.qr_base_url}/{id}"
    at: [..]
    size: [..]
```

Ensure the template's sample still renders: the `starter_tape_templates_render` test in `render/mod.rs` passes `data` with `message`/`code`. If you change a tape template's QR to require `id` + `qr_base_url`, update that test's `data`/settings accordingly, OR add `id` to the test data and pass a settings map with `qr_base_url`. Prefer adding a dedicated demo template (e.g. `templates/homebox-qr.yaml`) so the existing tape tests stay untouched.

- [ ] **Step 2: Verify templates still load + render**

Run: `cargo test --lib starter 2>&1 | tail -10` and `cargo run` then `curl -s localhost:8080/templates | jq '.templates[].id'` (Ctrl-C after). Expected: all templates load; no startup abort.

- [ ] **Step 3: Write ADR-0010**

Create `docs/adr/0010-variable-interpolation-layer.md` (Nygard format) capturing: context (no named-variable layer, §3.1 gap), decision (substitution-only `{field}`/`{settings.key}`, `{{`/`}}` escapes, `value` alternative to `name` on text/qr, exactly-one-of, missing → 422), consequences (subsumes id-field mapping; formulas/conditionals/defaults deferred), alternatives considered (narrow QR `link` flag — rejected).

- [ ] **Step 4: Update SPEC + CAPABILITIES + PLAN**

- `docs/SPEC.md`: document `value` on text/qr, interpolation syntax/errors, and add a changelog entry. Reference ADR-0010.
- `docs/CAPABILITIES.md` §3.1: mark the named-variable gap addressed (substitution tier).
- `docs/PLAN-phase-1.md`: mark #14 DONE (commit hash filled after final merge); note #22 deferred.

- [ ] **Step 5: Commit**

```bash
git add templates docs
git commit -m "Demo qr_base_url interpolation + ADR-0010 and spec updates (#14)"
```

---

## Task 6: CSV import — download mode

**Files:**
- Modify: `Cargo.toml` (add `csv`, `zip`)
- Modify: `src/api.rs` (handler + route + query model)
- Modify: `src/openapi.rs`

- [ ] **Step 1: Add dependencies**

Run (check current latest with `cargo search csv` / `cargo search zip`; pin majors):

```bash
cargo add csv
cargo add zip --no-default-features --features deflate
```

Expected: `Cargo.toml` gains both. (`zip` with only `deflate` keeps the dep light.)

- [ ] **Step 2: Add query model + CSV row parsing helper**

In `src/api.rs`:

```rust
#[derive(serde::Deserialize)]
pub struct ImportCsvQuery {
    pub template: String,
    pub mode: Option<String>,
    pub printer: Option<String>,
    pub format: Option<String>,
}

fn parse_csv_rows(
    body: &str,
) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>, AppError> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .trim(csv::Trim::All)
        .from_reader(body.as_bytes());
    let headers = reader
        .headers()
        .map_err(|err| AppError::invalid_request(format!("invalid CSV header: {err}")))?
        .clone();
    let mut rows = Vec::new();
    for record in reader.records() {
        let record =
            record.map_err(|err| AppError::invalid_request(format!("invalid CSV row: {err}")))?;
        let mut data = std::collections::HashMap::new();
        for (key, val) in headers.iter().zip(record.iter()) {
            data.insert(key.to_string(), serde_json::Value::String(val.to_string()));
        }
        rows.push(data);
    }
    if rows.is_empty() {
        return Err(AppError::invalid_request("CSV has no data rows"));
    }
    Ok(rows)
}
```

- [ ] **Step 3: Add the handler (download branch first)**

```rust
#[utoipa::path(
    post,
    path = "/import/csv",
    params(
        ("template" = String, Query, description = "Template id"),
        ("mode" = Option<String>, Query, description = "download (default) or print"),
        ("printer" = Option<String>, Query, description = "Printer id (required when mode=print)"),
        ("format" = Option<String>, Query, description = "Download format: png (default) or pdf")
    ),
    request_body(content = String, description = "CSV (header row + one row per label)", content_type = "text/csv"),
    responses(
        (status = 200, description = "ZIP of rendered labels (download) or per-row summary (print)"),
        (status = 400, description = "Invalid CSV or request", body = ErrorResponse),
        (status = 404, description = "Template or printer not found", body = ErrorResponse),
        (status = 422, description = "Render/validation error (download is atomic)", body = ErrorResponse),
        (status = 502, description = "Printer/transport failure", body = ErrorResponse)
    )
)]
pub async fn import_csv(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ImportCsvQuery>,
    body: String,
) -> Result<Response, AppError> {
    let registry = state.templates.load_full();
    let template = registry
        .get(&params.template)
        .ok_or_else(|| AppError::template_not_found(params.template.clone()))?;
    let rows = parse_csv_rows(&body)?;
    let settings = state.store().all_settings().await?;

    match params.mode.as_deref().unwrap_or("download") {
        "download" => {
            let width = rows.len().to_string().len();
            let mut cursor = std::io::Cursor::new(Vec::new());
            let mut zip = zip::ZipWriter::new(&mut cursor);
            let opts: zip::write::FileOptions<()> =
                zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            use std::io::Write as _;
            for (idx, data) in rows.iter().enumerate() {
                let (bytes, _ct, ext) =
                    render_to_format(template, data, None, params.format.as_deref(), &settings)
                        .map_err(|err| err.with_row(idx + 1))?;
                let name = format!("{:0width$}.{ext}", idx + 1, width = width);
                zip.start_file(name, opts)
                    .map_err(|err| AppError::render_failed(format!("zip error: {err}")))?;
                zip.write_all(&bytes)
                    .map_err(|err| AppError::render_failed(format!("zip error: {err}")))?;
            }
            zip.finish()
                .map_err(|err| AppError::render_failed(format!("zip error: {err}")))?;
            let bytes = cursor.into_inner();
            Ok(download_response(bytes, "application/zip", &format!("{}.zip", template.id)))
        }
        "print" => Err(AppError::invalid_request("print mode added in next task")),
        other => Err(AppError::invalid_request(format!(
            "unknown mode '{other}'; use download or print"
        ))),
    }
}
```

Add `with_row` to `AppError` in `src/errors.rs` (so the atomic failure carries the row index):

```rust
    pub fn with_row(mut self, row: usize) -> Self {
        let mut details = self.details.take().unwrap_or_else(|| json!({}));
        if let Some(obj) = details.as_object_mut() {
            obj.insert("row".to_string(), json!(row));
        }
        self.details = Some(details);
        self
    }
```

(`details` and `status`/`code`/`message` are private fields in the same module, so this method compiles.)

Update `render_to_format` signature to take `settings: &std::collections::BTreeMap<String, String>` (done in Task 3 Step 6). Register the route in `app()`:

```rust
        .route("/import/csv", post(import_csv))
```

- [ ] **Step 4: Register openapi path**

Add `api::import_csv` to `paths(...)` in `src/openapi.rs`.

- [ ] **Step 5: Add a download test**

In `src/lib.rs` integration tests, add:

```rust
#[tokio::test]
async fn import_csv_download_zips_rows() {
    // POST /import/csv?template=<single-template-id> with body:
    //   header line of the template's fields, then 2 data rows
    // assert 200, content-type application/zip, body starts with b"PK"
}
```

Use an existing single-format template id (inspect `templates/`), and field headers matching its `name`/`value` references. A ZIP starts with `PK\x03\x04`.

Also add a negative test: a row missing a required field → 422 with `details.row` present.

- [ ] **Step 6: Run**

Run: `cargo test 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/api.rs src/errors.rs src/openapi.rs src/lib.rs
git commit -m "Add CSV import download mode (#21)"
```

---

## Task 7: CSV import — print mode

**Files:**
- Modify: `src/models.rs` (summary models)
- Modify: `src/api.rs` (print branch)
- Modify: `src/openapi.rs`

- [ ] **Step 1: Add summary models**

In `src/models.rs`:

```rust
#[derive(Debug, Serialize, ToSchema)]
pub struct ImportRowError {
    pub row: usize,
    pub error: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ImportSummary {
    pub total: usize,
    pub succeeded: usize,
    pub failed: Vec<ImportRowError>,
}
```

- [ ] **Step 2: Implement the print branch**

Replace the `"print"` arm in `import_csv` with:

```rust
        "print" => {
            if !matches!(template.format, crate::models::TemplateFormat::Single { .. }) {
                return Err(AppError::unsupported_format(format!(
                    "template '{}' is a sheet; CSV import prints single-format templates only",
                    template.id
                )));
            }
            let printer_id = params
                .printer
                .as_deref()
                .ok_or_else(|| AppError::invalid_request("mode=print requires a printer"))?;
            if params.format.is_some() {
                return Err(AppError::invalid_request(
                    "format applies only to download; omit it when printing",
                ));
            }
            let printer = state
                .store()
                .get_printer(printer_id)
                .await?
                .ok_or_else(|| AppError::printer_not_found(printer_id.to_string()))?;
            if !printer.enabled {
                return Err(AppError::printer_disabled(printer_id));
            }
            let driver = crate::driver::build_driver(&printer.kind, &printer.config)
                .map_err(|err| AppError::printer_invalid(err.to_string()))?;
            let total = rows.len();
            let mut failed = Vec::new();
            for (idx, data) in rows.iter().enumerate() {
                let row = idx + 1;
                let artifact = match driver.accepted_format() {
                    crate::driver::ArtifactFormat::Pdf => {
                        render_single_label_pdf(template, data, None, &settings)
                    }
                    crate::driver::ArtifactFormat::Png => {
                        render_single_label(template, data, None, &settings)
                    }
                    fmt => Err(AppError::print_failed(format!(
                        "no renderer for artifact format {fmt:?}"
                    ))),
                };
                let result = match artifact {
                    Ok(bytes) => driver
                        .send(&bytes, &crate::driver::PrintOptions::default())
                        .await
                        .map_err(|err| AppError::print_failed(err.to_string())),
                    Err(err) => Err(err),
                };
                match result {
                    Ok(()) => {
                        let _ = state
                            .store()
                            .record_job(&params.template, Some(printer_id), "ok", None)
                            .await;
                    }
                    Err(err) => {
                        let message = err.message_text();
                        let _ = state
                            .store()
                            .record_job(&params.template, Some(printer_id), "failed", Some(&message))
                            .await;
                        failed.push(crate::models::ImportRowError { row, error: message });
                    }
                }
            }
            let summary = crate::models::ImportSummary {
                total,
                succeeded: total - failed.len(),
                failed,
            };
            Ok((axum::http::StatusCode::OK, Json(summary)).into_response())
        }
```

Add a `message_text` accessor to `AppError` in `src/errors.rs` (the field is private):

```rust
    pub fn message_text(&self) -> String {
        self.message.clone()
    }
```

Add `ImportSummary`, `ImportRowError` to the `models` import in `api.rs`.

- [ ] **Step 3: Register openapi schemas**

Add `ImportSummary`, `ImportRowError` to `schemas(...)` and their import in `src/openapi.rs`.

- [ ] **Step 4: Add a print-mode test**

In `src/lib.rs`, register a `fake` printer (the test-only driver kind) via `POST /printers`, then `POST /import/csv?template=...&mode=print&printer=...` with 2 rows where one row is missing a required field. Assert 200 and the JSON summary has `total: 2`, `succeeded: 1`, `failed[0].row == 2` (or whichever row). Mirror the existing `/print` dispatch tests that already use the `fake` kind.

- [ ] **Step 5: Run**

Run: `cargo test 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/models.rs src/api.rs src/errors.rs src/openapi.rs src/lib.rs
git commit -m "Add CSV import print mode with per-row summary (#21)"
```

---

## Task 8: Docs + final verification + merge

**Files:**
- Modify: `docs/SPEC.md`, `docs/PLAN-phase-1.md`, `scripts/` (optional smoke script)

- [ ] **Step 1: Document CSV import + settings in SPEC**

Add `/import/csv` (query params, modes, atomic download, per-row print summary, out-of-scope notes) and `/settings`, `/settings/{key}` to `docs/SPEC.md` with a changelog entry.

- [ ] **Step 2: Update the plan**

In `docs/PLAN-phase-1.md`, mark #14 and #21 DONE (hashes after merge); record #22 deferred out of M4.

- [ ] **Step 3: Full gate**

Run:

```bash
cargo fmt
cargo clippy --all-targets --all-features 2>&1 | tail -20
cargo test 2>&1 | tail -20
```

Expected: clean fmt, zero clippy warnings, all tests pass. Fix root causes of any warning (never `#[allow]`).

- [ ] **Step 4: Adversarial review loop**

Per CLAUDE.md, run the reviewer → fix → review loop against the full M4 diff (`git diff main...m4-integrations`) before merging. Address every meaningful finding with file:line evidence.

- [ ] **Step 5: Commit docs + merge**

```bash
git add docs
git commit -m "Document settings + CSV import in SPEC and plan (#14, #21)"
git checkout main && git merge m4-integrations && git push
```

Use a merge commit message that references `Fixes #14` and `Fixes #21` so both close on push.

---

## Self-review notes

- **Spec coverage:** interpolation syntax/errors (T1, T3), `value` exactly-one-of (T2), settings store+API (T3,T4), QR base-URL demo (T5), CSV download ZIP atomic (T6), CSV print per-row summary (T7), docs/ADR (T5,T8). #22 explicitly deferred. All spec sections map to a task.
- **Type consistency:** `interpolate(template, data, settings)` signature identical across T1/T3; `render_to_format` gains `settings` once (T3 Step 6) and is reused in T6; `Store::all_settings` name consistent; `AppError::with_row` / `message_text` added once and reused.
- **Known follow-up:** confirm `zip` crate `FileOptions<()>` generic and `csv::Trim` API against installed versions during T6 (run a web/docs check per CLAUDE.md before wiring).
