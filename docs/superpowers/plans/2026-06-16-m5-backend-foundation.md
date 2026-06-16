# M5 Backend Foundation Implementation Plan (/api migration + template source)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the REST API under `/api` (so the SPA can own the root), with a JSON 404 for unknown `/api/*`, and add `GET /api/templates/{id}/source` for the raw-YAML view.

**Architecture:** Extract the existing routes into an inner router, `nest("/api", …)` it, point the OpenAPI doc at the `/api` server, add a path-aware fallback (JSON for `/api/*`, a placeholder 404 elsewhere that the frontend plan will replace with the SPA `index.html`), and add a source-reading handler.

**Tech Stack:** Rust, axum, utoipa / utoipa-swagger-ui.

**Spec:** `docs/superpowers/specs/2026-06-15-m5-web-ui-design.md` (§"/api namespacing and static serving", §2 backend touch). This is M5 issue #15's backend half; the frontend scaffold + screens are separate plans.

**Scope note:** This plan does NOT add `ServeDir`/SPA `index.html` serving (no `ui/dist` exists yet); the fallback is structured so the frontend foundation plan drops that in. It is independently shippable: all existing behavior moves to `/api` with green tests.

Work on a branch:

```bash
git checkout -b m5-api-namespacing
```

---

## File map
- `src/api.rs` — split `app()` into `api_router()` + `app()` (nest under `/api`); add fallback; add `template_source` handler + route.
- `src/openapi.rs` — add `servers((url = "/api"))`; register `template_source`.
- `src/errors.rs` — add a `NotFound` code + `AppError::not_found`.
- `src/lib.rs` — migrate HTTP test URIs to `/api`; add fallback + source tests.
- `docs/SPEC.md` — endpoint paths under `/api`; add the source endpoint; changelog.
- `scripts/render_test.sh`, `scripts/render_avery_horizontal.sh` — call `/api/batch`.

---

## Task 1: Path-aware 404 fallback error

**Files:**
- Modify: `src/errors.rs`

- [ ] **Step 1: Add the code constant + constructor**

In `src/errors.rs`, add the constant near the other `const CODE_*`:

```rust
const CODE_NOT_FOUND: &str = "NotFound";
```

Add to the `impl AppError` block:

```rust
    pub fn not_found(path: &str) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            CODE_NOT_FOUND,
            format!("no API route for '{path}'"),
            Some(json!({ "path": path })),
        )
    }
```

- [ ] **Step 2: Build**

Run: `cargo build 2>&1 | tail -5`
Expected: clean (the constructor is used in Task 2; a `dead_code` warning is acceptable until then, do not `#[allow]`).

- [ ] **Step 3: Commit**

```bash
git add src/errors.rs
git commit -m "Add NotFound error for the API fallback (#15)"
```

---

## Task 2: Nest routes under `/api` + fallback

**Files:**
- Modify: `src/api.rs` (the `app()` fn, lines 77-101), imports.

- [ ] **Step 1: Write the failing tests**

In `src/lib.rs` `http_tests`, add (uses the existing `build_app` + `oneshot`/`Request` helpers, mirror a nearby test for the exact request-building style):

```rust
    #[tokio::test]
    async fn api_routes_are_namespaced() {
        let app = build_app();
        // health now lives under /api
        let res = app
            .clone()
            .oneshot(Request::builder().uri("/api/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn root_path_is_not_the_api() {
        let app = build_app();
        // the old root path no longer serves the API
        let res = app
            .clone()
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn unknown_api_route_returns_json_404() {
        let app = build_app();
        let res = app
            .clone()
            .oneshot(Request::builder().uri("/api/nope").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(res.into_body(), 64 * 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "NotFound");
    }
```

(Confirm the imports the test module uses: `axum::http::{Request, StatusCode}`, `axum::body::Body`, `tower::ServiceExt` for `oneshot`. Add any missing to the test module's `use`.)

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --lib http_tests::api_routes_are_namespaced http_tests::unknown_api_route_returns_json_404 2>&1 | tail -20`
Expected: FAIL (routes still at root; `/api/health` 404s, `/api/nope` is a plain 404 without the JSON body).

- [ ] **Step 3: Restructure `app()`**

Replace the `app()` function in `src/api.rs` with an inner `api_router()` + a nesting `app()` + a fallback:

```rust
fn api_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/health", get(health))
        .route("/templates", get(list_templates).post(create_template))
        .route("/templates/reload", post(reload_templates))
        .route(
            "/templates/{id}",
            get(get_template)
                .put(replace_template)
                .delete(delete_template),
        )
        .route("/templates/{id}/source", get(template_source))
        .route("/printers", get(list_printers).post(create_printer))
        .route(
            "/printers/{id}",
            get(get_printer).put(replace_printer).delete(delete_printer),
        )
        .route("/settings", get(get_settings))
        .route("/settings/{key}", put(put_setting))
        .route("/render/label", post(render_label))
        .route("/batch", post(batch))
        .route("/import/csv", post(import_csv))
        .merge(SwaggerUi::new("/docs").url("/api/openapi.json", ApiDoc::openapi()))
}

pub fn app(state: Arc<AppState>) -> Router {
    Router::new()
        .nest("/api", api_router())
        .fallback(fallback)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn fallback(uri: axum::http::Uri) -> Response {
    if uri.path().starts_with("/api/") {
        AppError::not_found(uri.path()).into_response()
    } else {
        // The frontend foundation plan replaces this branch with the SPA index.html.
        (axum::http::StatusCode::NOT_FOUND, "Not Found").into_response()
    }
}
```

Note: `template_source` is added in Task 4; to keep this task compiling, either do Task 4's handler first or temporarily stub `template_source` (a handler returning `AppError::not_found`) and replace it in Task 4. Prefer implementing Task 4's handler now if working top-to-bottom; otherwise add this stub above `app()`:

```rust
// Temporary stub; real implementation in Task 4.
async fn template_source(Path(_id): Path<String>) -> Result<Response, AppError> {
    Err(AppError::internal("not implemented"))
}
```

**Swagger nuance to verify:** with the inner router nested under `/api`, `SwaggerUi::new("/docs")` serves the UI at `/api/docs`. The spec URL is set to `/api/openapi.json` (absolute) so the UI fetches the right path. After building, verify in Task 5 that `/api/docs` loads and `/api/openapi.json` returns the doc; if utoipa-swagger-ui needs the relative `"/openapi.json"` instead, adjust.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test --lib http_tests::api_routes_are_namespaced http_tests::root_path_is_not_the_api http_tests::unknown_api_route_returns_json_404 2>&1 | tail -20`
Expected: the namespacing + 404 tests PASS. (Other http_tests still fail until Task 3 migrates their URIs.)

- [ ] **Step 5: Commit**

```bash
git add src/api.rs src/lib.rs
git commit -m "Nest REST API under /api with a JSON 404 fallback (#15)"
```

---

## Task 3: Migrate existing tests + scripts to `/api`

**Files:**
- Modify: `src/lib.rs` (all `http_tests` request URIs), `scripts/render_test.sh`, `scripts/render_avery_horizontal.sh`

- [ ] **Step 1: Update every test URI**

In `src/lib.rs`, prefix every request path in `http_tests` with `/api`: `/health`→`/api/health`, `/templates`→`/api/templates`, `/templates/{…}`→`/api/templates/{…}`, `/printers…`→`/api/printers…`, `/settings…`→`/api/settings…`, `/render/label`→`/api/render/label`, `/batch`→`/api/batch`, `/import/csv`→`/api/import/csv`. Use a careful find/replace; the `json_req("POST", "/batch", …)` helper calls and the `Request::builder().uri("…")` calls both need it. Do NOT change the two new tests from Task 2 (already `/api`), and leave `root_path_is_not_the_api` pointing at `/health`.

- [ ] **Step 2: Run the full suite**

Run: `cargo test 2>&1 | tail -20`
Expected: all pass (every http test now hits `/api/*`).

- [ ] **Step 3: Update the sample scripts**

In `scripts/render_test.sh` and `scripts/render_avery_horizontal.sh`, change the `curl` target from `localhost:$PORT/batch` to `localhost:$PORT/api/batch` (keep the rest of each script). `rg -n "/batch|/render|/print|/import" scripts/` to confirm none remain at root.

- [ ] **Step 4: Commit**

```bash
git add src/lib.rs scripts/render_test.sh scripts/render_avery_horizontal.sh
git commit -m "Migrate HTTP tests and sample scripts to /api (#15)"
```

---

## Task 4: `GET /api/templates/{id}/source`

**Files:**
- Modify: `src/api.rs` (replace the Task 2 stub with the real handler), `src/openapi.rs`

- [ ] **Step 1: Write the failing test**

In `src/lib.rs` `http_tests` add (pick a real starter template id that exists on disk, e.g. `brother24mm`):

```rust
    #[tokio::test]
    async fn template_source_returns_yaml() {
        let app = build_app();
        let res = app
            .clone()
            .oneshot(Request::builder().uri("/api/templates/brother24mm/source").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let ct = res.headers().get("content-type").unwrap().to_str().unwrap().to_string();
        assert!(ct.contains("yaml") || ct.contains("text/plain"), "got {ct}");
        let body = axum::body::to_bytes(res.into_body(), 256 * 1024).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("id: brother24mm"), "source should contain the template id");
    }

    #[tokio::test]
    async fn template_source_unknown_is_404() {
        let app = build_app();
        let res = app
            .clone()
            .oneshot(Request::builder().uri("/api/templates/does-not-exist/source").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --lib http_tests::template_source 2>&1 | tail -20`
Expected: FAIL (stub returns 500 / not implemented).

- [ ] **Step 3: Implement the handler**

In `src/api.rs`, replace the Task 2 `template_source` stub with the real one. It validates the id with the existing `template_file_path` guard (path-traversal-safe), reads the YAML from the templates dir, and returns it as `text/yaml`. Look up the file path the same way the registry does (the `AppState` already holds `templates_dir`; reuse the private `template_file_path` helper in this file):

```rust
#[utoipa::path(
    get,
    path = "/templates/{id}/source",
    params(("id" = String, Path, description = "Template ID")),
    responses(
        (status = 200, description = "Raw template YAML", content_type = "text/yaml"),
        (status = 400, description = "Invalid id", body = ErrorResponse),
        (status = 404, description = "Template not found", body = ErrorResponse)
    )
)]
pub async fn template_source(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let path = template_file_path(&state.templates_dir, &id)?;
    let yaml = std::fs::read_to_string(&path).map_err(|_| AppError::template_not_found(id))?;
    Ok((
        axum::http::StatusCode::OK,
        [("content-type", "text/yaml; charset=utf-8")],
        yaml,
    )
        .into_response())
}
```

Notes: `template_file_path(dir, id)` already exists in `src/api.rs` (used by create/replace) and returns `<dir>/<id>.yaml`, rejecting bad ids with `400`. Templates on disk are `.yaml` in this repo (the starter set), so reading `<id>.yaml` is correct; if a `.yml` is ever supported, extend here. `state.templates_dir` is the private field, accessible within this module.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test --lib http_tests::template_source 2>&1 | tail -20`
Expected: both PASS.

- [ ] **Step 5: Register in OpenAPI**

In `src/openapi.rs`: add `api::template_source` to `paths(...)`. Add the `/api` server so the doc paths resolve correctly: in the `#[openapi(...)]` attribute add `servers((url = "/api"))` (alongside `paths`, `components`, `tags`).

- [ ] **Step 6: Run**

Run: `cargo test 2>&1 | tail -10` → all pass. `cargo fmt`; `cargo clippy --all-targets --all-features 2>&1 | tail -10` → zero warnings.

- [ ] **Step 7: Commit**

```bash
git add src/api.rs src/openapi.rs
git commit -m "Add GET /api/templates/{id}/source for the raw-YAML view (#15)"
```

---

## Task 5: SPEC + Swagger verification + gate

**Files:**
- Modify: `docs/SPEC.md`

- [ ] **Step 1: Update SPEC endpoints**

In `docs/SPEC.md` §2: prefix every path in the HTTP API table with `/api` (`/api/health`, `/api/templates`, `/api/batch`, …, `/api/openapi.json`, `/api/docs`); add a `GET /api/templates/{id}/source` row (raw YAML). The existing "Planned (ADR-0008)" note about `/api` becomes current, reword it to "the REST API is served under `/api`; the root is reserved for the SPA (served by the frontend build)". Add an unknown-`/api/*`-returns-`NotFound`-404 sentence to the error model. Add a changelog entry dated 2026-06-16 (API moved under `/api`; `template/{id}/source` added; #15).

- [ ] **Step 2: Verify Swagger live**

Run: `cargo run` (background), then:
```bash
curl -s localhost:8080/api/openapi.json | head -c 200; echo
curl -s -o /dev/null -w "%{http_code}\n" localhost:8080/api/docs
curl -s -o /dev/null -w "%{http_code}\n" localhost:8080/api/health
curl -s localhost:8080/api/nope | head -c 120; echo
```
Expected: the openapi JSON starts with `{"openapi"`; `/api/docs` returns `200`; `/api/health` `200`; `/api/nope` is the JSON `NotFound`. If `/api/docs` or the spec URL 404s, adjust the `SwaggerUi::new("/docs").url(...)` argument (try relative `"/openapi.json"`) and re-verify. Stop the server.

- [ ] **Step 3: Full gate**

```bash
cargo fmt
cargo clippy --all-targets --all-features 2>&1 | tail -20
cargo test 2>&1 | tail -20
```
Clean fmt, zero clippy, all tests pass.

- [ ] **Step 4: Adversarial review + merge**

Per CLAUDE.md, run the reviewer → fix loop on `git diff main...m5-api-namespacing` (focus: nothing still served at root, the fallback's `/api/*` vs non-`/api` branch, Swagger paths, no test still hitting a root path). Then:

```bash
git add docs/SPEC.md
git commit -m "Document /api namespacing + template source endpoint (#15)"
git checkout main && git merge m5-api-namespacing && git push
```
Reference `#15` in the merge commit (do not close #15, the shell/SPA half remains).

---

## Self-review notes
- **Spec coverage:** `/api` nesting + Swagger move (T2,T4,T5), JSON 404 for unknown `/api/*` (T2), tests/scripts/SPEC migration (T3,T5), raw-YAML source endpoint (T4). The SPA static-serve + `index.html` fallback is explicitly deferred to the frontend foundation plan (the fallback's non-`/api` branch is the seam).
- **Type/route consistency:** `api_router()` lists `/templates/{id}/source` and `template_source` is defined in T4 (with a T2 stub to keep the build green meanwhile); the fallback path test (`/api/nope`) matches the `starts_with("/api/")` branch; `AppError::not_found` (T1) is used by the fallback (T2).
- **Deferred (next plans):** frontend scaffold + `ServeDir`/SPA fallback; the five screens; the data layer; the reusable grid.
