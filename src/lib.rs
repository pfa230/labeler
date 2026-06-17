pub mod api;
pub mod auth;
pub mod batch;
mod convert;
pub mod driver;
pub mod errors;
pub mod middleware;
pub mod models;
pub mod openapi;
pub mod parse;
mod raw;
pub mod render;
pub mod store;
pub mod templates;

pub use api::{app, AppState};
pub use templates::TemplateRegistry;

#[cfg(test)]
mod tests {
    use super::store::Store;
    use super::{app, AppState, TemplateRegistry};
    use std::future::IntoFuture;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::net::{TcpListener, TcpStream};
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn server_starts_and_accepts_connections() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().expect("local addr");

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let templates = TemplateRegistry::load_from_dir("templates").expect("load templates");
        let store = Store::open_in_memory().expect("store");
        let state = Arc::new(AppState::new(templates, "templates".into(), store));
        let server = axum::serve(listener, app(state)).with_graceful_shutdown(async {
            let _ = shutdown_rx.await;
        });

        let handle = tokio::spawn(server.into_future());

        let connect = TcpStream::connect(addr);
        tokio::time::timeout(Duration::from_millis(250), connect)
            .await
            .expect("server did not accept connections in time")
            .expect("failed to connect to server");

        let _ = shutdown_tx.send(());
        handle
            .await
            .expect("server task failed")
            .expect("server error");
    }
}

#[cfg(test)]
mod http_tests {
    use super::store::Store;
    use super::{app, AppState, TemplateRegistry};
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use serde_json::{json, Value};
    use std::sync::Arc;
    use tower::ServiceExt;

    // These integration tests exercise the protected `/api` routes. The auth middleware now rejects
    // unauthenticated callers with 401, so every app the harness builds seeds a fixed API token and
    // every request the harness sends carries `Authorization: Bearer <TEST_TOKEN>`. This authenticates
    // genuinely (the middleware hashes and looks the token up in the store), with no per-test churn.
    const TEST_TOKEN: &str = "test-token-secret";

    fn seed_token(store: &Store) {
        // The builders run inside the test's tokio runtime, so drive the async seed on a separate OS
        // thread with its own runtime (block_on from within a runtime would panic).
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .build()
                    .expect("seed runtime");
                rt.block_on(async {
                    store
                        .create_token("test", &super::auth::sha256_hex(TEST_TOKEN))
                        .await
                        .expect("seed token");
                });
            });
        });
    }

    /// Inject the bearer header into every request the harness sends, so protected routes authenticate.
    fn with_auth(router: axum::Router) -> axum::Router {
        router.layer(tower::layer::layer_fn(|inner| AuthInject { inner }))
    }

    #[derive(Clone)]
    struct AuthInject<S> {
        inner: S,
    }

    impl<S> tower::Service<Request<Body>> for AuthInject<S>
    where
        S: tower::Service<Request<Body>> + Clone,
    {
        type Response = S::Response;
        type Error = S::Error;
        type Future = S::Future;

        fn poll_ready(
            &mut self,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            self.inner.poll_ready(cx)
        }

        fn call(&mut self, mut req: Request<Body>) -> Self::Future {
            if !req
                .headers()
                .contains_key(axum::http::header::AUTHORIZATION)
            {
                req.headers_mut().insert(
                    axum::http::header::AUTHORIZATION,
                    axum::http::HeaderValue::from_str(&format!("Bearer {TEST_TOKEN}")).unwrap(),
                );
            }
            self.inner.call(req)
        }
    }

    fn build_app() -> axum::Router {
        let templates = TemplateRegistry::load_from_dir("templates").expect("load templates");
        let store = Store::open_in_memory().expect("store");
        seed_token(&store);
        with_auth(app(Arc::new(AppState::new(
            templates,
            "templates".into(),
            store,
        ))))
    }

    fn uniq() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        format!(
            "{}_{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        )
    }

    fn app_with_ui(dir: &std::path::Path) -> axum::Router {
        let templates = TemplateRegistry::load_from_dir("templates").expect("load templates");
        let store = Store::open_in_memory().expect("store");
        seed_token(&store);
        with_auth(app(Arc::new(
            AppState::new(templates, "templates".into(), store).with_ui_dir(dir),
        )))
    }

    async fn json_response(response: axum::response::Response) -> Value {
        let body = response
            .into_body()
            .collect()
            .await
            .expect("collect body")
            .to_bytes();
        serde_json::from_slice(&body).expect("parse json")
    }

    async fn bytes_response(response: axum::response::Response) -> Vec<u8> {
        response
            .into_body()
            .collect()
            .await
            .expect("collect body")
            .to_bytes()
            .to_vec()
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let app = build_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_response(response).await;
        assert_eq!(body["status"], "ok");
    }

    #[tokio::test]
    async fn api_routes_are_namespaced() {
        let app = build_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn root_path_serves_spa_not_api() {
        // empty ui dir (no index.html): the old root API path is gone; /health is not the API.
        let dir = std::env::temp_dir().join(format!("labeler_ui_empty_{}", uniq()));
        std::fs::create_dir_all(&dir).unwrap();
        let app = app_with_ui(&dir);
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND); // not the API; no index.html present
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn spa_fallback_serves_index_for_non_api() {
        let dir = std::env::temp_dir().join(format!("labeler_ui_{}", uniq()));
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(
            dir.join("index.html"),
            "<!doctype html><title>labeler ui</title>",
        )
        .unwrap();
        let app = app_with_ui(&dir);

        // a client-side route falls back to index.html
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/templates/abc")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let ct = res
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(ct.contains("text/html"), "got {ct}");
        let body = axum::body::to_bytes(res.into_body(), 64 * 1024)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&body).contains("labeler ui"));

        // unknown API path still returns the JSON contract
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/nope")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(res.into_body(), 64 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "NotFound");

        // a missing asset is a 404 (NOT the SPA html) — assets must not be shadowed by index.html
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/assets/missing.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let ct = res
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default();
        assert!(
            !ct.contains("text/html"),
            "missing asset must not serve SPA html"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn unknown_api_route_returns_json_404() {
        let app = build_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/nope")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = json_response(response).await;
        assert_eq!(body["error"]["code"], "NotFound");
    }

    #[tokio::test]
    async fn template_source_returns_yaml() {
        let app = build_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/templates/brother24mm/source")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        assert!(
            content_type.contains("yaml"),
            "content-type: {content_type}"
        );
        let body = bytes_response(response).await;
        let body = String::from_utf8(body).expect("utf8 body");
        assert!(body.contains("id: brother24mm"), "body: {body}");
    }

    #[tokio::test]
    async fn template_source_unknown_is_404() {
        let app = build_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/templates/does-not-exist/source")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn templates_lists_available_templates() {
        let app = build_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/templates")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_response(response).await;
        let templates = body["templates"].as_array().expect("templates array");
        assert!(!templates.is_empty());
        let ids: Vec<_> = templates
            .iter()
            .filter_map(|item| item.get("id").and_then(|id| id.as_str()))
            .collect();
        assert!(ids.contains(&"avery5163"));
        assert!(ids.contains(&"brother12mm"));
    }

    #[tokio::test]
    async fn template_detail_unknown_returns_404() {
        let app = build_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/templates/missing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = json_response(response).await;
        assert_eq!(body["error"]["code"], "TemplateNotFound");
    }

    #[tokio::test]
    async fn render_label_unknown_template_returns_404() {
        let app = build_app();
        let payload = json!({ "template": "missing", "data": {} });
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/render/label")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = json_response(response).await;
        assert_eq!(body["error"]["code"], "TemplateNotFound");
    }

    #[tokio::test]
    async fn render_png() {
        let app = build_app();
        let label_payload = json!({
            "template": "brother12mm",
            "data": {
                "message": "Hello",
                "code": "QR-123"
            }
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/render/label")
                    .header("content-type", "application/json")
                    .body(Body::from(label_payload.to_string()))
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        assert!(content_type.starts_with("image/png"));
        let body = bytes_response(response).await;
        assert!(!body.is_empty(), "rendered PNG is empty");
        assert_eq!(&body[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[tokio::test]
    async fn batch_single_download_returns_zip() {
        let app = build_app();
        let payload = json!({
            "template": "brother24mm",
            "mode": "download",
            "labels": [
                { "data": { "message": "Hello", "code": "QR-1" } },
                { "data": { "message": "World", "code": "QR-2" } }
            ]
        });
        let response = app
            .oneshot(json_req("POST", "/api/batch", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        assert_eq!(content_type, "application/zip");
        let body = bytes_response(response).await;
        assert_eq!(&body[..4], b"PK\x03\x04");
    }

    #[tokio::test]
    async fn batch_sheet_download_returns_pdf() {
        let app = build_app();
        let label = json!({
            "option": { "orientation": "horizontal", "outline": "yes" },
            "data": {
                "id": "A1",
                "url": "https://example.com/A1",
                "name": "Floor Grinder",
                "tags": "Power tools",
                "description": "Angle grinder with floor grinding attachment and dust shroud"
            }
        });
        let payload = json!({
            "template": "avery5163",
            "mode": "download",
            "labels": [label.clone(), label]
        });
        let response = app
            .oneshot(json_req("POST", "/api/batch", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        assert!(content_type.starts_with("application/pdf"));
        let body = bytes_response(response).await;
        assert!(body.starts_with(b"%PDF"), "missing PDF header");
    }

    #[tokio::test]
    async fn batch_invalid_label_returns_422() {
        let app = build_app();
        let payload = json!({
            "template": "brother24mm",
            "mode": "download",
            "labels": [
                { "data": { "message": "Hello", "code": "QR-1" } },
                { "data": { "message": "World" } }
            ]
        });
        let response = app
            .oneshot(json_req("POST", "/api/batch", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(response).await;
        assert_eq!(body["error"]["code"], "BatchInvalid");
        assert_eq!(body["error"]["details"]["failures"][0]["index"], 1);
    }

    #[tokio::test]
    async fn batch_print_summary_ok() {
        let app = build_app();
        create_fake_printer(&app, "ok-printer", false).await;
        let payload = json!({
            "template": "brother24mm",
            "mode": "print",
            "printer": "ok-printer",
            "labels": [
                { "data": { "message": "Hello", "code": "QR-1" } },
                { "data": { "message": "World", "code": "QR-2" } }
            ]
        });
        let response = app
            .clone()
            .oneshot(json_req("POST", "/api/batch", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_response(response).await;
        assert_eq!(body["total"], 2);
        assert_eq!(body["succeeded"], 2);
        assert_eq!(body["failed"].as_array().expect("failed array").len(), 0);
    }

    #[tokio::test]
    async fn batch_print_summary_failure() {
        let app = build_app();
        create_fake_printer(&app, "bad-printer", true).await;
        let payload = json!({
            "template": "brother24mm",
            "mode": "print",
            "printer": "bad-printer",
            "labels": [
                { "data": { "message": "Hello", "code": "QR-1" } },
                { "data": { "message": "World", "code": "QR-2" } }
            ]
        });
        let response = app
            .clone()
            .oneshot(json_req("POST", "/api/batch", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_response(response).await;
        assert_eq!(body["succeeded"], 0);
        let failed = body["failed"].as_array().expect("failed array");
        assert_eq!(failed.len(), 2);
        assert_eq!(failed[0]["index"], 0);
        assert_eq!(failed[1]["index"], 1);
    }

    #[tokio::test]
    async fn batch_sheet_print_failure_marks_all() {
        let app = build_app();
        create_fake_printer(&app, "bad-sheet-printer", true).await;
        let label = json!({
            "option": { "orientation": "horizontal", "outline": "yes" },
            "data": {
                "id": "A1",
                "url": "https://example.com/A1",
                "name": "Floor Grinder",
                "tags": "Power tools",
                "description": "Angle grinder with floor grinding attachment and dust shroud"
            }
        });
        let payload = json!({
            "template": "avery5163",
            "mode": "print",
            "printer": "bad-sheet-printer",
            "labels": [label.clone(), label]
        });
        let response = app
            .clone()
            .oneshot(json_req("POST", "/api/batch", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_response(response).await;
        assert_eq!(body["total"], 2);
        assert_eq!(body["succeeded"], 0);
        let failed = body["failed"].as_array().expect("failed array");
        assert_eq!(failed.len(), 2);
        assert_eq!(body["jobs"], 1);
    }

    #[tokio::test]
    async fn batch_sheet_print_success_one_job() {
        let app = build_app();
        create_fake_printer(&app, "ok-sheet-printer", false).await;
        let label = json!({
            "option": { "orientation": "horizontal", "outline": "yes" },
            "data": {
                "id": "A1",
                "url": "https://example.com/A1",
                "name": "Floor Grinder",
                "tags": "Power tools",
                "description": "Angle grinder with floor grinding attachment and dust shroud"
            }
        });
        let payload = json!({
            "template": "avery5163",
            "mode": "print",
            "printer": "ok-sheet-printer",
            "labels": [label.clone(), label]
        });
        let response = app
            .clone()
            .oneshot(json_req("POST", "/api/batch", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_response(response).await;
        assert_eq!(body["total"], 2);
        assert_eq!(body["succeeded"], 2);
        assert_eq!(body["failed"].as_array().expect("failed array").len(), 0);
        assert_eq!(body["jobs"], 1);
    }

    #[tokio::test]
    async fn batch_start_slot_single_400() {
        let app = build_app();
        let payload = json!({
            "template": "brother24mm",
            "mode": "download",
            "start_slot": 1,
            "labels": [
                { "data": { "message": "Hello", "code": "QR-1" } }
            ]
        });
        let response = app
            .oneshot(json_req("POST", "/api/batch", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn render_label_pdf() {
        let app = build_app();
        let payload = json!({
            "template": "brother12mm",
            "data": { "message": "Hello", "code": "QR-123" }
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/render/label?format=pdf")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        assert!(content_type.starts_with("application/pdf"));
        let body = bytes_response(response).await;
        assert!(body.starts_with(b"%PDF"), "missing PDF header");
    }

    #[tokio::test]
    async fn render_label_unknown_format_returns_400() {
        let app = build_app();
        let payload = json!({
            "template": "brother12mm",
            "data": { "message": "Hello", "code": "QR-123" }
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/render/label?format=xml")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = json_response(response).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
    }

    #[tokio::test]
    async fn render_label_pdf_on_sheet_template_returns_422() {
        let app = build_app();
        let payload = json!({ "template": "avery5163", "data": {} });
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/render/label?format=pdf")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(response).await;
        assert_eq!(body["error"]["code"], "UnsupportedFormat");
    }

    #[tokio::test]
    async fn import_csv_download_zips_rows() {
        let app = build_app();
        let csv = "message,code\nHello,QR-1\nWorld,QR-2\n";
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/import/csv?template=brother24mm")
                    .header("content-type", "text/csv")
                    .body(Body::from(csv))
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        assert_eq!(content_type, "application/zip");
        let body = bytes_response(response).await;
        assert!(body.len() > 4, "zip body too small");
        assert_eq!(&body[..4], b"PK\x03\x04");
    }

    #[tokio::test]
    async fn import_csv_strips_leading_bom() {
        let app = build_app();
        let csv = format!("{}message,code\nHello,QR-1\n", '\u{feff}');
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/import/csv?template=brother24mm")
                    .header("content-type", "text/csv")
                    .body(Body::from(csv))
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        assert_eq!(content_type, "application/zip");
        let body = bytes_response(response).await;
        assert_eq!(&body[..4], b"PK\x03\x04");
    }

    #[tokio::test]
    async fn import_csv_duplicate_headers_returns_400() {
        let app = build_app();
        let csv = "message,message\nHello,World\n";
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/import/csv?template=brother24mm")
                    .header("content-type", "text/csv")
                    .body(Body::from(csv))
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = json_response(response).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
    }

    #[tokio::test]
    async fn import_csv_missing_field_is_atomic() {
        let app = build_app();
        // brother24mm needs `message` and `code`. The CSV omits the `code` column, so every row
        // fails to render and the atomic batch aborts with a BatchInvalid before any output.
        let csv = "message\nHello\nWorld\n";
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/import/csv?template=brother24mm")
                    .header("content-type", "text/csv")
                    .body(Body::from(csv))
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(response).await;
        assert_eq!(body["error"]["code"], "BatchInvalid");
        let failures = body["error"]["details"]["failures"]
            .as_array()
            .expect("failures array");
        assert_eq!(failures[0]["index"], 0);
        assert_eq!(failures[0]["code"], "MissingField");
    }

    #[tokio::test]
    async fn import_csv_print_reports_per_row() {
        let app = build_app();
        create_fake_printer(&app, "ok-printer", false).await;
        let csv = "message,code\nHello,QR-1\nWorld,QR-2\n";
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/import/csv?template=brother24mm&mode=print&printer=ok-printer")
                    .header("content-type", "text/csv")
                    .body(Body::from(csv))
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_response(response).await;
        assert_eq!(body["total"], 2);
        assert_eq!(body["succeeded"], 2);
        assert_eq!(body["failed"].as_array().expect("failed array").len(), 0);

        create_fake_printer(&app, "bad-printer", true).await;
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/import/csv?template=brother24mm&mode=print&printer=bad-printer")
                    .header("content-type", "text/csv")
                    .body(Body::from(csv))
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_response(response).await;
        assert_eq!(body["total"], 2);
        assert_eq!(body["succeeded"], 0);
        let failed = body["failed"].as_array().expect("failed array");
        assert_eq!(failed.len(), 2);
        assert_eq!(failed[0]["index"], 0);
        assert_eq!(failed[1]["index"], 1);
        assert!(!failed[0]["error"]
            .as_str()
            .expect("error string")
            .is_empty());
    }

    #[tokio::test]
    async fn import_csv_print_requires_printer() {
        let app = build_app();
        let csv = "message,code\nHello,QR-1\n";
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/import/csv?template=brother24mm&mode=print")
                    .header("content-type", "text/csv")
                    .body(Body::from(csv))
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    fn temp_templates_dir() -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!("labeler_http_tpl_{n}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn build_app_in(dir: &std::path::Path) -> axum::Router {
        let templates = TemplateRegistry::load_from_dir(dir).expect("load templates");
        let store = Store::open_in_memory().expect("store");
        seed_token(&store);
        with_auth(app(Arc::new(AppState::new(
            templates,
            dir.to_path_buf(),
            store,
        ))))
    }

    fn template_yaml(id: &str) -> String {
        format!(
            r#"id: {id}
name: {id}
description: d
unit: mm
dpi: 300
format:
  type: single
  width: 20.0
  height: 10.0
layout:
  - type: text
    name: msg
    at: [0.0, 0.0]
    size: [20.0, 5.0]
    font_size: 10.0
"#
        )
    }

    async fn template_count(app: &axum::Router) -> usize {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/templates")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        let body = json_response(response).await;
        body["templates"].as_array().expect("templates array").len()
    }

    #[tokio::test]
    async fn reload_picks_up_new_template() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("t1.yaml"), template_yaml("t1")).unwrap();
        let app = build_app_in(&dir);
        assert_eq!(template_count(&app).await, 1);

        std::fs::write(dir.join("t2.yaml"), template_yaml("t2")).unwrap();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/templates/reload")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_response(response).await;
        assert_eq!(body["count"], 2);
        assert_eq!(template_count(&app).await, 2);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn reload_invalid_file_keeps_previous_set() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("t1.yaml"), template_yaml("t1")).unwrap();
        let app = build_app_in(&dir);
        assert_eq!(template_count(&app).await, 1);

        std::fs::write(dir.join("bad.yaml"), "id: bad\nunit: nope\n").unwrap();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/templates/reload")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        // The previously-loaded set is still served.
        assert_eq!(template_count(&app).await, 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    fn yaml_post(uri: &str, method: &str, body: String) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "text/yaml")
            .body(Body::from(body))
            .unwrap()
    }

    #[tokio::test]
    async fn template_create_get_replace_delete_roundtrip() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("t1.yaml"), template_yaml("t1")).unwrap();
        let app = build_app_in(&dir);

        let resp = app
            .clone()
            .oneshot(yaml_post("/api/templates", "POST", template_yaml("new1")))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::CREATED);
        assert_eq!(template_count(&app).await, 2);

        // Replace with a changed dpi and confirm it took.
        let body200 = template_yaml("new1").replace("dpi: 300", "dpi: 200");
        let resp = app
            .clone()
            .oneshot(yaml_post("/api/templates/new1", "PUT", body200))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::OK);
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/templates/new1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        let detail = json_response(resp).await;
        assert_eq!(detail["dpi"], 200);

        // Delete and confirm it's gone.
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/templates/new1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/templates/new1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn template_create_duplicate_returns_409() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("dup.yaml"), template_yaml("dup")).unwrap();
        let app = build_app_in(&dir);
        let resp = app
            .clone()
            .oneshot(yaml_post("/api/templates", "POST", template_yaml("dup")))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::CONFLICT);
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "TemplateExists");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn template_create_invalid_yaml_returns_422() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);
        let resp = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates",
                "POST",
                "id: x\nunit: nope\n".to_string(),
            ))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "TemplateInvalid");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn template_create_unsafe_id_returns_400() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);
        let body = template_yaml("ok").replace("id: ok", "id: ../evil");
        let resp = app
            .clone()
            .oneshot(yaml_post("/api/templates", "POST", body))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        // No file escaped the templates dir.
        assert!(!dir.parent().unwrap().join("evil.yaml").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn template_replace_id_mismatch_returns_400() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("a.yaml"), template_yaml("a")).unwrap();
        let app = build_app_in(&dir);
        let resp = app
            .clone()
            .oneshot(yaml_post("/api/templates/a", "PUT", template_yaml("b")))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn template_replace_missing_returns_404() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);
        let resp = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/ghost",
                "PUT",
                template_yaml("ghost"),
            ))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        std::fs::remove_dir_all(&dir).ok();
    }

    // A valid write persists even if an unrelated invalid file makes the post-write reload fail (422).
    #[tokio::test]
    async fn create_persists_but_reload_reports_invalid_sibling() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("t1.yaml"), template_yaml("t1")).unwrap();
        let app = build_app_in(&dir);
        std::fs::write(dir.join("broken.yaml"), "id: broken\nunit: nope\n").unwrap();

        let resp = app
            .clone()
            .oneshot(yaml_post("/api/templates", "POST", template_yaml("new1")))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(dir.join("new1.yaml").exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    fn json_req(method: &str, uri: &str, body: String) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap()
    }

    fn printer_json(id: &str) -> String {
        json!({
            "id": id,
            "name": id,
            "kind": "cups",
            "config": { "uri": format!("ipp://host/printers/{id}") }
        })
        .to_string()
    }

    async fn get_json(app: &axum::Router, uri: &str) -> (StatusCode, Value) {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .expect("request");
        let status = response.status();
        (status, json_response(response).await)
    }

    #[tokio::test]
    async fn printer_crud_roundtrip() {
        let app = build_app();

        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/printers", printer_json("office")))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::CREATED);

        let (_, list) = get_json(&app, "/api/printers").await;
        assert_eq!(list.as_array().unwrap().len(), 1);

        let (status, detail) = get_json(&app, "/api/printers/office").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(detail["kind"], "cups");

        let replace = json!({
            "id": "office", "name": "Front Desk", "kind": "cups",
            "config": { "uri": "ipp://h/p" }
        })
        .to_string();
        let resp = app
            .clone()
            .oneshot(json_req("PUT", "/api/printers/office", replace))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::OK);
        let (_, detail) = get_json(&app, "/api/printers/office").await;
        assert_eq!(detail["name"], "Front Desk");

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/printers/office")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        let (status, _) = get_json(&app, "/api/printers/office").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn settings_put_then_get_roundtrip() {
        let app = build_app();

        let resp = app
            .clone()
            .oneshot(json_req(
                "PUT",
                "/api/settings/qr_base_url",
                json!({ "value": "https://h/i" }).to_string(),
            ))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::OK);

        let (status, settings) = get_json(&app, "/api/settings").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(settings["qr_base_url"], "https://h/i");
    }

    #[tokio::test]
    async fn printer_create_duplicate_returns_409() {
        let app = build_app();
        app.clone()
            .oneshot(json_req("POST", "/api/printers", printer_json("p")))
            .await
            .expect("request");
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/printers", printer_json("p")))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::CONFLICT);
        assert_eq!(json_response(resp).await["error"]["code"], "PrinterExists");
    }

    #[tokio::test]
    async fn printer_create_invalid_kind_returns_422() {
        let app = build_app();
        let body = json!({ "id": "p", "name": "P", "kind": "zebra", "config": {} }).to_string();
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/printers", body))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(json_response(resp).await["error"]["code"], "PrinterInvalid");
    }

    #[tokio::test]
    async fn printer_create_unsafe_id_returns_400() {
        let app = build_app();
        let body =
            json!({ "id": "../evil", "name": "P", "kind": "cups", "config": { "uri": "x" } })
                .to_string();
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/printers", body))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn printer_get_unknown_returns_404() {
        let app = build_app();
        let (status, body) = get_json(&app, "/api/printers/nope").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "PrinterNotFound");
    }

    #[tokio::test]
    async fn printer_replace_id_mismatch_returns_400() {
        let app = build_app();
        app.clone()
            .oneshot(json_req("POST", "/api/printers", printer_json("a")))
            .await
            .expect("request");
        let resp = app
            .clone()
            .oneshot(json_req("PUT", "/api/printers/a", printer_json("b")))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    async fn create_fake_printer(app: &axum::Router, id: &str, fail: bool) {
        let body =
            json!({ "id": id, "name": id, "kind": "fake", "config": { "fail": fail } }).to_string();
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/printers", body))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn printer_replace_missing_returns_404() {
        let app = build_app();
        let resp = app
            .clone()
            .oneshot(json_req(
                "PUT",
                "/api/printers/ghost",
                printer_json("ghost"),
            ))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            json_response(resp).await["error"]["code"],
            "PrinterNotFound"
        );
    }
}

#[cfg(test)]
mod auth_http_tests {
    use super::store::Store;
    use super::{app, AppState, TemplateRegistry};
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use serde_json::Value;
    use std::sync::Arc;
    use tower::ServiceExt;

    fn test_app() -> axum::Router {
        let templates = TemplateRegistry::load_from_dir("templates").expect("load templates");
        let store = Store::open_in_memory().expect("store");
        app(Arc::new(AppState::new(
            templates,
            "templates".into(),
            store,
        )))
    }

    fn req_get(uri: &str) -> Request<Body> {
        Request::builder().uri(uri).body(Body::empty()).unwrap()
    }

    fn req_get_cookie(uri: &str, cookie: &str) -> Request<Body> {
        Request::builder()
            .uri(uri)
            .header("cookie", cookie)
            .body(Body::empty())
            .unwrap()
    }

    fn req_post_json(uri: &str, body: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .header("host", "localhost")
            .header("origin", "http://localhost")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    async fn body_json(res: axum::response::Response) -> Value {
        let bytes = axum::body::to_bytes(res.into_body(), 64 * 1024)
            .await
            .expect("collect body");
        serde_json::from_slice(&bytes).expect("parse json")
    }

    fn cookie_from(res: &axum::response::Response) -> String {
        res.headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string()
    }

    /// Create the first user and log in, returning the session cookie that authorizes protected calls.
    async fn setup_login_cookie(app: &axum::Router) -> String {
        app.clone()
            .oneshot(req_post_json(
                "/api/auth/setup",
                r#"{"username":"a","password":"pw123456"}"#,
            ))
            .await
            .unwrap();
        let res = app
            .clone()
            .oneshot(req_post_json(
                "/api/auth/login",
                r#"{"username":"a","password":"pw123456"}"#,
            ))
            .await
            .unwrap();
        cookie_from(&res)
    }

    fn req_post_json_cookie(uri: &str, body: &str, cookie: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .header("host", "localhost")
            .header("origin", "http://localhost")
            .header("cookie", cookie)
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    fn req_delete_cookie(uri: &str, cookie: &str) -> Request<Body> {
        Request::builder()
            .method("DELETE")
            .uri(uri)
            .header("host", "localhost")
            .header("origin", "http://localhost")
            .header("cookie", cookie)
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn protected_route_requires_auth() {
        let app = test_app();
        let res = app
            .clone()
            .oneshot(req_get("/api/templates"))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn setup_then_login_flow() {
        let app = test_app();
        // setup creates the first user (origin header required for state-changing)
        let res = app
            .clone()
            .oneshot(req_post_json(
                "/api/auth/setup",
                r#"{"username":"a","password":"pw123456"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        // second setup is rejected
        let res = app
            .clone()
            .oneshot(req_post_json(
                "/api/auth/setup",
                r#"{"username":"b","password":"pw123456"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CONFLICT);
        // login returns a session cookie
        let res = app
            .clone()
            .oneshot(req_post_json(
                "/api/auth/login",
                r#"{"username":"a","password":"pw123456"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let cookie = res
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        // the cookie now authorizes a protected GET
        let res = app
            .clone()
            .oneshot(req_get_cookie("/api/templates", &cookie))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        // bad password is 401
        let res = app
            .clone()
            .oneshot(req_post_json(
                "/api/auth/login",
                r#"{"username":"a","password":"nope"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn health_is_open() {
        let app = test_app();
        let res = app.oneshot(req_get("/api/health")).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn setup_rejects_short_password() {
        let app = test_app();
        let res = app
            .oneshot(req_post_json(
                "/api/auth/setup",
                r#"{"username":"a","password":"short"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn me_reports_needs_setup_then_authed() {
        let app = test_app();
        // zero users: me is exempt, 200, authed:false needsSetup:true
        let res = app.clone().oneshot(req_get("/api/auth/me")).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_json(res).await;
        assert_eq!(body["authed"], false);
        assert_eq!(body["needsSetup"], true);
        // after setup + login, me with the cookie is authed:true
        app.clone()
            .oneshot(req_post_json(
                "/api/auth/setup",
                r#"{"username":"a","password":"pw123456"}"#,
            ))
            .await
            .unwrap();
        let res = app
            .clone()
            .oneshot(req_post_json(
                "/api/auth/login",
                r#"{"username":"a","password":"pw123456"}"#,
            ))
            .await
            .unwrap();
        let cookie = res
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        let res = app
            .clone()
            .oneshot(req_get_cookie("/api/auth/me", &cookie))
            .await
            .unwrap();
        let body = body_json(res).await;
        assert_eq!(body["authed"], true);
        assert_eq!(body["me"]["username"], "a");
    }

    #[tokio::test]
    async fn origin_mismatch_rejected_for_cookie_post() {
        let app = test_app();
        app.clone()
            .oneshot(req_post_json(
                "/api/auth/setup",
                r#"{"username":"a","password":"pw123456"}"#,
            ))
            .await
            .unwrap();
        let res = app
            .clone()
            .oneshot(req_post_json(
                "/api/auth/login",
                r#"{"username":"a","password":"pw123456"}"#,
            ))
            .await
            .unwrap();
        let cookie = res
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        // A cookie-authenticated state-changing POST with a foreign Origin is rejected with 403.
        let req = Request::builder()
            .method("POST")
            .uri("/api/auth/logout")
            .header("host", "localhost")
            .header("origin", "http://evil.test")
            .header("cookie", &cookie)
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn create_user_then_duplicate_conflicts() {
        let app = test_app();
        let cookie = setup_login_cookie(&app).await;
        // create a second user
        let res = app
            .clone()
            .oneshot(req_post_json_cookie(
                "/api/users",
                r#"{"username":"bob","password":"pw123456"}"#,
                &cookie,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        // list now shows 2
        let res = app
            .clone()
            .oneshot(req_get_cookie("/api/users", &cookie))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_json(res).await;
        assert_eq!(body.as_array().unwrap().len(), 2);
        // a second POST with the same username is a clean 409, not a 500
        let res = app
            .clone()
            .oneshot(req_post_json_cookie(
                "/api/users",
                r#"{"username":"bob","password":"pw123456"}"#,
                &cookie,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn delete_last_user_conflicts() {
        let app = test_app();
        let cookie = setup_login_cookie(&app).await;
        // there is exactly one user; find its id
        let res = app
            .clone()
            .oneshot(req_get_cookie("/api/users", &cookie))
            .await
            .unwrap();
        let body = body_json(res).await;
        let id = body[0]["id"].as_str().unwrap().to_string();
        let res = app
            .clone()
            .oneshot(req_delete_cookie(&format!("/api/users/{id}"), &cookie))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn change_password_verifies_current() {
        let app = test_app();
        let cookie = setup_login_cookie(&app).await;
        // wrong current password is 401
        let res = app
            .clone()
            .oneshot(req_post_json_cookie(
                "/api/auth/password",
                r#"{"current_password":"nope","new_password":"newpass12"}"#,
                &cookie,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        // correct current password is 200
        let res = app
            .clone()
            .oneshot(req_post_json_cookie(
                "/api/auth/password",
                r#"{"current_password":"pw123456","new_password":"newpass12"}"#,
                &cookie,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn token_create_authorizes_then_revokes() {
        let app = test_app();
        let cookie = setup_login_cookie(&app).await;
        // create a token; the secret is returned once
        let res = app
            .clone()
            .oneshot(req_post_json_cookie(
                "/api/tokens",
                r#"{"name":"ci"}"#,
                &cookie,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let body = body_json(res).await;
        let secret = body["secret"].as_str().unwrap().to_string();
        let id = body["id"].as_str().unwrap().to_string();
        // the token authorizes a protected GET via the bearer header
        let req = Request::builder()
            .uri("/api/templates")
            .header("authorization", format!("Bearer {secret}"))
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        // revoke it
        let res = app
            .clone()
            .oneshot(req_delete_cookie(&format!("/api/tokens/{id}"), &cookie))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        // the revoked token no longer authorizes
        let req = Request::builder()
            .uri("/api/templates")
            .header("authorization", format!("Bearer {secret}"))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }
}
