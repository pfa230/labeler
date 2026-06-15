pub mod api;
mod convert;
pub mod errors;
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

    fn build_app() -> axum::Router {
        let templates = TemplateRegistry::load_from_dir("templates").expect("load templates");
        let store = Store::open_in_memory().expect("store");
        app(Arc::new(AppState::new(
            templates,
            "templates".into(),
            store,
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
                    .uri("/health")
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
    async fn templates_lists_available_templates() {
        let app = build_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/templates")
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
                    .uri("/templates/missing")
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
                    .uri("/render/label")
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
    async fn render_batch_unknown_template_returns_404() {
        let app = build_app();
        let payload = json!({ "template": "missing", "labels": [] });
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/render/batch")
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
                    .uri("/render/label")
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
    async fn render_pdf() {
        let app = build_app();
        let sheet_payload = json!({
            "template": "avery5163",
            "labels": [
                {
                    "option": {
                        "orientation": "horizontal",
                        "outline": "yes"
                    },
                    "data": {
                        "id": "A1",
                        "url": "https://example.com/A1",
                        "name": "Floor Grinder",
                        "tags": "Power tools",
                        "description": "Angle grinder with floor grinding attachment and dust shroud"
                    }
                }
            ]
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/render/batch")
                    .header("content-type", "application/json")
                    .body(Body::from(sheet_payload.to_string()))
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
        assert!(!body.is_empty(), "rendered PDF is empty");
        assert!(body.starts_with(b"%PDF"), "missing PDF header");
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
                    .uri("/render/label?format=pdf")
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
                    .uri("/render/label?format=xml")
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
                    .uri("/render/label?format=pdf")
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
        app(Arc::new(AppState::new(templates, dir.to_path_buf(), store)))
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
                    .uri("/templates")
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
                    .uri("/templates/reload")
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
                    .uri("/templates/reload")
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
            .oneshot(yaml_post("/templates", "POST", template_yaml("new1")))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::CREATED);
        assert_eq!(template_count(&app).await, 2);

        // Replace with a changed dpi and confirm it took.
        let body200 = template_yaml("new1").replace("dpi: 300", "dpi: 200");
        let resp = app
            .clone()
            .oneshot(yaml_post("/templates/new1", "PUT", body200))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::OK);
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/templates/new1")
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
                    .uri("/templates/new1")
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
                    .uri("/templates/new1")
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
            .oneshot(yaml_post("/templates", "POST", template_yaml("dup")))
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
                "/templates",
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
            .oneshot(yaml_post("/templates", "POST", body))
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
            .oneshot(yaml_post("/templates/a", "PUT", template_yaml("b")))
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
            .oneshot(yaml_post("/templates/ghost", "PUT", template_yaml("ghost")))
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
            .oneshot(yaml_post("/templates", "POST", template_yaml("new1")))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(dir.join("new1.yaml").exists());

        std::fs::remove_dir_all(&dir).ok();
    }
}
