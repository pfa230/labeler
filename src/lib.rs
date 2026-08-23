pub mod api;
pub mod auth;
pub mod batch;
pub mod connector;
mod convert;
pub mod datetime_fmt;
pub mod driver;
pub mod egress;
pub mod errors;
pub mod middleware;
pub mod models;
pub mod openapi;
pub mod parse;
pub mod raw;
pub mod reason;
pub mod render;
pub mod settings;
pub mod store;
pub mod templates;
pub mod ui_freshness;

pub use api::{app, AppState};
pub use templates::TemplateRegistry;

/// Resolve a directory from an optional env value, falling back to a CWD-relative default.
/// Callers pass `std::env::var_os("LABELER_...")`; keeping the env read out of here makes it testable.
pub fn resolve_dir(value: Option<std::ffi::OsString>, default: &str) -> std::path::PathBuf {
    value
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(default))
}

#[cfg(test)]
mod resolve_dir_tests {
    use super::resolve_dir;
    use std::path::PathBuf;

    #[test]
    fn defaults_when_absent() {
        assert_eq!(resolve_dir(None, "fonts"), PathBuf::from("fonts"));
    }

    #[test]
    fn uses_env_value_when_present() {
        assert_eq!(
            resolve_dir(Some("/custom/fonts".into()), "fonts"),
            PathBuf::from("/custom/fonts")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::store::Store;
    use super::{app, AppState};
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
        let (templates, templates_dir) = crate::templates::load_all_for_tests();
        let store = Store::open_in_memory().expect("store");
        let state = Arc::new(AppState::new(templates, templates_dir, store));
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
        build_app_with_state().0
    }

    /// Like `build_app` but also returns the shared `AppState`, so a test can read the store directly
    /// (e.g. to assert a write-only secret persisted, since the API never echoes it back).
    fn build_app_with_state() -> (axum::Router, Arc<AppState>) {
        let (templates, templates_dir) = crate::templates::load_all_for_tests();
        let store = Store::open_in_memory().expect("store");
        seed_token(&store);
        let state = Arc::new(AppState::new(templates, templates_dir, store));
        (with_auth(app(state.clone())), state)
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
        let (templates, templates_dir) = crate::templates::load_all_for_tests();
        let store = Store::open_in_memory().expect("store");
        seed_token(&store);
        with_auth(app(Arc::new(
            AppState::new(templates, templates_dir, store).with_ui_dir(dir),
        )))
    }

    fn loopback_state() -> Arc<AppState> {
        let (templates, templates_dir) = crate::templates::load_all_for_tests();
        let store = Store::open_in_memory().expect("store");
        seed_token(&store);
        Arc::new(AppState::new(templates, templates_dir, store).with_loopback_egress())
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
    async fn connection_crud_endpoints_redact_credential() {
        let app = build_app();
        // create
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/connections")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"homebox","name":"home","base_url":"http://hb.lan:7745","credential":"hb_secret"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let v = json_response(res).await;
        assert_eq!(v["has_credential"], true);
        assert!(
            v.get("credential").is_none(),
            "credential must never be returned"
        );
        let id = v["id"].as_str().unwrap().to_string();
        // list (no credential leaked)
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/connections")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let list = json_response(res).await;
        assert_eq!(list.as_array().unwrap().len(), 1);
        assert!(list[0].get("credential").is_none());
        // update name
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/connections/{id}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"homebox","name":"renamed","base_url":"http://hb.lan:7745"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        // delete
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/connections/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn create_connection_with_valid_public_url() {
        let app = build_app();
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/connections")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"homebox","name":"home","base_url":"http://hb.lan:7745/","public_url":"https://homebox.example.com/","credential":"secret"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let v = json_response(res).await;
        assert_eq!(v["public_url"], "https://homebox.example.com");
        assert_eq!(v["base_url"], "http://hb.lan:7745");
        assert_eq!(v["has_credential"], true);

        // GET /connections/{id} returns the normalized public_url
        let id = v["id"].as_str().unwrap();
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/connections/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let v = json_response(res).await;
        assert_eq!(v["public_url"], "https://homebox.example.com");
    }

    #[tokio::test]
    async fn create_connection_normalizes_empty_public_url_to_none() {
        let app = build_app();
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/connections")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"homebox","name":"home","base_url":"http://hb.lan:7745","public_url":"","credential":"secret"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let v = json_response(res).await;
        assert_eq!(v["public_url"], Value::Null);
    }

    #[tokio::test]
    async fn create_connection_rejects_invalid_public_url_scheme_or_query() {
        let app = build_app();
        // Scheme ftp
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/connections")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"homebox","name":"home","base_url":"http://hb.lan:7745","public_url":"ftp://homebox","credential":"secret"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(body["error"]["details"]["reason"], "public_url_invalid");

        // Query param
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/connections")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"homebox","name":"home","base_url":"http://hb.lan:7745","public_url":"http://homebox?q=1","credential":"secret"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(body["error"]["details"]["reason"], "public_url_invalid");

        // Fragment
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/connections")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"homebox","name":"home","base_url":"http://hb.lan:7745","public_url":"http://homebox#frag","credential":"secret"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(body["error"]["details"]["reason"], "public_url_invalid");

        // Userinfo
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/connections")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"homebox","name":"home","base_url":"http://hb.lan:7745","public_url":"https://user:pass@homebox","credential":"secret"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(body["error"]["details"]["reason"], "public_url_invalid");
    }

    #[tokio::test]
    async fn update_connection_preserves_omitted_public_url() {
        let app = build_app();
        // Create with public_url
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/connections")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"homebox","name":"home","base_url":"http://hb.lan:7745","public_url":"https://homebox.example.com","credential":"secret"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let v = json_response(res).await;
        let id = v["id"].as_str().unwrap();
        assert_eq!(v["public_url"], "https://homebox.example.com");

        // Update omitting public_url preserves existing
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/connections/{id}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"homebox","name":"renamed","base_url":"http://hb.lan:7745"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let v = json_response(res).await;
        assert_eq!(v["name"], "renamed");
        assert_eq!(v["public_url"], "https://homebox.example.com");
    }

    #[tokio::test]
    async fn update_connection_clears_null_public_url() {
        let app = build_app();
        // Create with public_url
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/connections")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"homebox","name":"home","base_url":"http://hb.lan:7745","public_url":"https://homebox.example.com","credential":"secret"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let v = json_response(res).await;
        let id = v["id"].as_str().unwrap();
        assert_eq!(v["public_url"], "https://homebox.example.com");

        // Clear public_url with null
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/connections/{id}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"homebox","name":"home","base_url":"http://hb.lan:7745","public_url":null}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let v = json_response(res).await;
        assert_eq!(v["public_url"], Value::Null);

        // Set to new public_url
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/connections/{id}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"homebox","name":"home","base_url":"http://hb.lan:7745","public_url":"https://hb2.example.com"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let v = json_response(res).await;
        assert_eq!(v["public_url"], "https://hb2.example.com");

        // Clear public_url with empty string ""
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/connections/{id}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"homebox","name":"home","base_url":"http://hb.lan:7745","public_url":""}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let v = json_response(res).await;
        assert_eq!(v["public_url"], Value::Null);
    }

    /// The two URL fields must not share a discriminator. Passing `UrlField::Public` for `base_url`
    /// would still return 400, so only asserting the status proves nothing: it is the slug that says
    /// which field the client has to fix.
    #[tokio::test]
    async fn invalid_base_url_reports_base_url_invalid_not_public_url_invalid() {
        let app = build_app();
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/connections")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"homebox","name":"home","base_url":"ftp://hb.lan","credential":"secret"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(body["error"]["details"]["reason"], "base_url_invalid");

        // Same on update, whose call site is separate.
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/connections")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"homebox","name":"home","base_url":"http://hb.lan:7745","credential":"secret"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let id = json_response(res).await["id"].as_str().unwrap().to_string();
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/connections/{id}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"homebox","name":"home","base_url":"https://user:pass@hb.lan"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(body["error"]["details"]["reason"], "base_url_invalid");
    }

    /// `https://:pass@host` carries a password with an empty username. `url::Url` still parses it as
    /// userinfo, so the username-only check alone would let a secret through onto a printed label.
    #[tokio::test]
    async fn create_connection_rejects_password_only_userinfo() {
        let app = build_app();
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/connections")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"homebox","name":"home","base_url":"http://hb.lan:7745","public_url":"https://:pass@homebox","credential":"secret"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(body["error"]["details"]["reason"], "public_url_invalid");
    }

    #[tokio::test]
    async fn connection_endpoints_report_404_for_an_unknown_id() {
        let app = build_app();
        for (method, body) in [
            ("GET", Body::empty()),
            (
                "PUT",
                Body::from(r#"{"connector":"homebox","name":"x","base_url":"http://hb.lan:7745"}"#),
            ),
            ("DELETE", Body::empty()),
        ] {
            let res = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri("/api/connections/nope")
                        .header("content-type", "application/json")
                        .body(body)
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::NOT_FOUND, "{method} unknown id");
        }
    }

    /// A connection's connector is fixed at creation: an update naming a different one is neither
    /// applied nor refused (#197).
    #[tokio::test]
    async fn update_connection_ignores_the_connector_in_the_payload() {
        let app = build_app();
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/connections")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"homebox","name":"home","base_url":"http://hb.lan:7745","credential":"secret"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let id = json_response(res).await["id"].as_str().unwrap().to_string();

        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/connections/{id}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"not-a-connector","name":"home","base_url":"http://hb.lan:7745"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(json_response(res).await["connector"], "homebox");
    }

    #[tokio::test]
    async fn deleted_connection_no_longer_appears_in_the_list() {
        let app = build_app();
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/connections")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"homebox","name":"home","base_url":"http://hb.lan:7745","credential":"secret"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let id = json_response(res).await["id"].as_str().unwrap().to_string();

        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/connections/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);

        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/connections")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let list = json_response(res).await;
        assert!(list.as_array().unwrap().is_empty());
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

        // a missing asset is a 404 (NOT the SPA html); assets must not be shadowed by index.html
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
                    .uri("/api/templates/brother_24mm_qr/source")
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
        assert!(body.contains("id: brother_24mm_qr"), "body: {body}");
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
    async fn thumbnail_single_returns_png() {
        let app = build_app();
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/templates/brother_24mm_qr/thumbnail")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers().get("content-type").unwrap(), "image/png");
        assert!(res.headers().get("etag").is_some(), "etag header present");
        let body = bytes_response(res).await;
        assert_eq!(&body[1..4], b"PNG", "PNG magic bytes");
    }

    #[tokio::test]
    async fn thumbnail_sheet_returns_png() {
        let app = build_app();
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/templates/avery5163/thumbnail")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers().get("content-type").unwrap(), "image/png");
        assert!(res.headers().get("etag").is_some(), "etag header present");
        let body = bytes_response(res).await;
        assert_eq!(&body[1..4], b"PNG", "PNG magic bytes");
    }

    #[tokio::test]
    async fn thumbnail_unknown_template_is_404() {
        let app = build_app();
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/templates/does-not-exist/thumbnail")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn thumbnail_if_none_match_returns_304() {
        let app = build_app();
        let first = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/templates/brother_24mm_qr/thumbnail")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let etag = first
            .headers()
            .get("etag")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let second = app
            .oneshot(
                Request::builder()
                    .uri("/api/templates/brother_24mm_qr/thumbnail")
                    .header("if-none-match", etag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(second.status(), StatusCode::NOT_MODIFIED);
        assert!(second.headers().get("etag").is_some(), "304 carries etag");
        let body = bytes_response(second).await;
        assert!(body.is_empty(), "304 body must be empty");
    }

    async fn set_variable(app: &axum::Router, key: &str, value: &str) {
        let res = app
            .clone()
            .oneshot(json_req(
                "PUT",
                &format!("/api/variables/{key}"),
                json!({ "value": value }).to_string(),
            ))
            .await
            .expect("request");
        assert!(
            res.status().is_success(),
            "seeding {key} failed: {}",
            res.status()
        );
    }

    async fn thumbnail_etag(app: &axum::Router, id: &str) -> String {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/templates/{id}/thumbnail"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(res.status(), StatusCode::OK);
        res.headers()
            .get("etag")
            .expect("etag header")
            .to_str()
            .expect("ascii etag")
            .to_string()
    }

    /// #129: the ETag keys on the rendered bytes, so every render input is covered — not just the
    /// template YAML. The image also depends on the renderer, on the variables it interpolates and
    /// on the datetime formats; keying on the YAML alone served stale previews forever. Changing an
    /// interpolated variable changes the picture, so it must change the tag.
    #[tokio::test]
    async fn thumbnail_etag_changes_when_an_interpolated_variable_changes() {
        let app = build_app();
        set_variable(&app, "qr_base_url", "https://one.example.com").await;
        let first = thumbnail_etag(&app, "homebox-qr").await;
        set_variable(&app, "qr_base_url", "https://two.example.com").await;
        let second = thumbnail_etag(&app, "homebox-qr").await;
        assert_ne!(
            first, second,
            "same template, different QR target, but the ETag did not move"
        );
    }

    #[tokio::test]
    async fn thumbnail_etag_rotates_on_content_change() {
        // Uses a temp dir + build_app_in so the replace (PUT) writes to a throwaway
        // directory and never mutates the on-disk templates/ fixtures.
        let dir = temp_templates_dir();
        std::fs::write(dir.join("tpl.yaml"), template_yaml("tpl")).unwrap();
        let app = build_app_in(&dir);

        // E1: etag for original template content.
        let res1 = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/templates/tpl/thumbnail")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res1.status(), StatusCode::OK);
        let etag1 = res1
            .headers()
            .get("etag")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        // Replace with a modified version (different font_size changes the content hash).
        let changed = template_yaml("tpl").replace("font_size: 10.0", "font_size: 8.0");
        let resp = app
            .clone()
            .oneshot(yaml_post("/api/templates/tpl", "PUT", changed))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // E2: etag after replace must differ because the YAML content changed.
        let res2 = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/templates/tpl/thumbnail")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res2.status(), StatusCode::OK);
        let etag2 = res2
            .headers()
            .get("etag")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        assert_ne!(
            etag1, etag2,
            "ETag must rotate after template content changes"
        );

        std::fs::remove_dir_all(&dir).ok();
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
        assert!(ids.contains(&"brother_12mm"));
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
            "template": "brother_12mm",
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

    async fn render_png_bytes(app: &axum::Router, template: &str, data: Value) -> Vec<u8> {
        let payload = json!({ "template": template, "data": data });
        let response = app
            .clone()
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
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "render failed for {template}"
        );
        bytes_response(response).await
    }

    #[tokio::test]
    async fn dynamic_tape_is_auto_length() {
        let app = build_app();
        let png_width =
            |bytes: &[u8]| u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let short = render_png_bytes(&app, "brother_12mm", json!({"message": "hi"})).await;
        let long = render_png_bytes(
            &app,
            "brother_12mm",
            json!({"message": "a considerably longer message that grows the tape"}),
        )
        .await;
        assert_eq!(&short[..8], b"\x89PNG\r\n\x1a\n", "short is not a PNG");
        assert_eq!(&long[..8], b"\x89PNG\r\n\x1a\n", "long is not a PNG");
        assert!(
            png_width(&long) > png_width(&short),
            "expected long ({}) > short ({})",
            png_width(&long),
            png_width(&short),
        );
    }

    #[tokio::test]
    async fn multiline_auto_length_tape_returns_png() {
        let app = build_app();
        let payload = json!({
            "template": "brother_24mm_multiline",
            "data": {
                "message": "Long label that should wrap onto two lines on the tape"
            }
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/render/label?format=png")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        let body = bytes_response(response).await;
        assert_eq!(&body[..8], b"\x89PNG\r\n\x1a\n", "expected PNG magic bytes");
    }

    #[tokio::test]
    async fn batch_single_download_returns_zip() {
        let app = build_app();
        let payload = json!({
            "template": "brother_24mm_qr",
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
            "template": "avery5163_asset_tag",
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

    /// The starter Avery is a sheet with no options and one `message` field (#135). Every other
    /// sheet test drives the multi-variant fixture, so without this the simple path is unrendered.
    #[tokio::test]
    async fn batch_sheet_single_field_download_returns_pdf() {
        let app = build_app();
        let label = json!({ "data": { "message": "Kitchen — spare parts" } });
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
        let body = bytes_response(response).await;
        assert!(body.starts_with(b"%PDF"), "missing PDF header");
    }

    #[tokio::test]
    async fn batch_invalid_label_returns_422() {
        let app = build_app();
        let payload = json!({
            "template": "brother_24mm_qr",
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
        // MissingField is outside the four codes that carry a reason, so the key is absent rather
        // than null. That optionality is the contract (ADR-0052, decision 4), not an oversight.
        assert!(
            body["error"]["details"]["failures"][0]
                .get("reason")
                .is_none(),
            "an unreasoned code must omit the key entirely, got {}",
            body["error"]["details"]["failures"][0]
        );
    }

    #[tokio::test]
    async fn batch_print_summary_ok() {
        let app = build_app();
        create_fake_printer(&app, "ok-printer", false).await;
        let payload = json!({
            "template": "brother_24mm_qr",
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
            "template": "brother_24mm_qr",
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
    async fn print_bilevel_profile_renders_and_succeeds() {
        let app = build_app();
        // a fake printer configured bilevel
        let body = json!({
            "id": "bl",
            "name": "bl",
            "kind": "fake",
            "config": { "fail": false, "render": { "color_mode": "bilevel", "resolution": 203 } }
        })
        .to_string();
        let c = app
            .clone()
            .oneshot(json_req("POST", "/api/printers", body))
            .await
            .expect("req");
        assert_eq!(c.status(), StatusCode::CREATED);
        // print a SINGLE template -> bilevel png path runs end-to-end
        let payload = json!({
            "template": "brother_24mm_qr",
            "mode": "print",
            "printer": "bl",
            "labels": [ { "data": { "message": "Hi", "code": "Q" } } ]
        });
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/batch", payload.to_string()))
            .await
            .expect("req");
        assert_eq!(resp.status(), StatusCode::OK);
        let summary = json_response(resp).await;
        // succeeded == 1 is LOAD-BEARING: the fake driver rejects a non-PNG artifact when
        // configured bilevel, so success proves the print path rendered + sent a bilevel PNG.
        assert_eq!(summary["succeeded"], 1);
        assert_eq!(summary["failed"].as_array().unwrap().len(), 0);
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
            "template": "avery5163_asset_tag",
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
            "template": "avery5163_asset_tag",
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
            "template": "brother_24mm_qr",
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
            "template": "brother_12mm",
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
            "template": "brother_12mm",
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
        assert_eq!(body["error"]["details"]["reason"], "format_unknown");
    }

    #[tokio::test]
    async fn malformed_json_body_keeps_its_shape() {
        let app = build_app();
        let response = app
            .oneshot(json_req(
                "POST",
                "/api/render/label",
                "{ not json".to_string(),
            ))
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = json_response(response).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
        assert_eq!(body["error"]["message"], "Malformed JSON body");
        assert!(
            body["error"]["details"]["error"].is_string(),
            "details.error must still carry the parser message, got {body}"
        );
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
                    .uri("/api/import/csv?template=brother_24mm_qr")
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
                    .uri("/api/import/csv?template=brother_24mm_qr")
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
                    .uri("/api/import/csv?template=brother_24mm_qr")
                    .header("content-type", "text/csv")
                    .body(Body::from(csv))
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = json_response(response).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
        // Same code as the unknown-format case above, different reason. That is the whole point.
        assert_eq!(body["error"]["details"]["reason"], "csv_header_invalid");
    }

    /// The contract is scoped to four codes (ADR-0052). Nothing else gains a reason, and `details`
    /// keeps carrying exactly what it carried before.
    #[tokio::test]
    async fn unreasoned_codes_have_no_reason() {
        let app = build_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/templates/does-not-exist")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = json_response(response).await;
        assert_eq!(body["error"]["code"], "TemplateNotFound");
        assert_eq!(body["error"]["details"]["template"], "does-not-exist");
        assert!(
            body["error"]["details"].get("reason").is_none(),
            "TemplateNotFound is outside the migrated set and must not gain a reason, got {}",
            body["error"]["details"]
        );
    }

    #[tokio::test]
    async fn import_csv_missing_field_is_atomic() {
        let app = build_app();
        // brother_24mm_qr needs `message` and `code`. The CSV omits the `code` column, so every row
        // fails to render and the atomic batch aborts with a BatchInvalid before any output.
        let csv = "message\nHello\nWorld\n";
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/import/csv?template=brother_24mm_qr")
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
                    .uri("/api/import/csv?template=brother_24mm_qr&mode=print&printer=ok-printer")
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
                    .uri("/api/import/csv?template=brother_24mm_qr&mode=print&printer=bad-printer")
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
                    .uri("/api/import/csv?template=brother_24mm_qr&mode=print")
                    .header("content-type", "text/csv")
                    .body(Body::from(csv))
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn import_csv_routes_option_columns() {
        let app = build_app();
        // avery5163_asset_tag declares orientation/outline. The `option.orientation` column routes
        // into the per-row option selection, so the horizontal variant renders.
        let csv = "id,url,name,tags,description,option.orientation,option.outline\n\
            A1,https://x,Widget,t,desc,horizontal,yes\n";
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/import/csv?template=avery5163_asset_tag&mode=download")
                    .header("content-type", "text/csv")
                    .body(Body::from(csv))
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn import_csv_undeclared_option_column_returns_400() {
        let app = build_app();
        // avery5163_asset_tag does not declare `bogus`; an undeclared option.<name> column must be
        // rejected (per SPEC section E), not silently ignored.
        let csv = "id,url,name,tags,description,option.bogus\n\
            A1,https://x,Widget,t,desc,whatever\n";
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/import/csv?template=avery5163_asset_tag")
                    .header("content-type", "text/csv")
                    .body(Body::from(csv))
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn import_csv_disallowed_option_value_is_atomic() {
        let app = build_app();
        // A disallowed option value flows through the shared batch path and fails the row as
        // BatchInvalid with a per-row InvalidOptionValue (not a top-level InvalidOptionValue).
        let csv = "id,url,name,tags,description,option.orientation\n\
            A1,https://x,Widget,t,desc,sideways\n";
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/import/csv?template=avery5163_asset_tag&mode=download")
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
        assert_eq!(failures[0]["code"], "InvalidOptionValue");
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
        build_app_in_with_state(dir).0
    }

    /// The app plus its state, for tests that need to install the between-write-and-reload hook.
    fn build_app_in_with_state(dir: &std::path::Path) -> (axum::Router, Arc<AppState>) {
        let templates = TemplateRegistry::load_from_dir(dir).expect("load templates");
        let store = Store::open_in_memory().expect("store");
        seed_token(&store);
        let state = Arc::new(AppState::new(templates, dir.to_path_buf(), store));
        (with_auth(app(state.clone())), state)
    }

    fn template_yaml_for(id: &str, name: &str) -> String {
        template_yaml(id).replace(&format!("name: {id}"), &format!("name: {name}"))
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
    value: "{{msg}}"
    at: [0.0, 0.0]
    size: [20.0, 5.0]
    font_size: 10.0
"#
        )
    }

    #[tokio::test]
    async fn invalid_template_yaml_carries_a_reason() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);
        let response = app
            .oneshot(yaml_post(
                "/api/templates",
                "POST",
                "id: [not a string".to_string(),
            ))
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(response).await;
        assert_eq!(body["error"]["code"], "TemplateInvalid");
        assert_eq!(body["error"]["details"]["reason"], "template_parse_failed");
    }

    /// The point of #151: one code, two causes, told apart without reading the prose.
    #[tokio::test]
    async fn unvalidatable_template_carries_a_different_reason() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);
        let yaml = template_yaml("v1").replace("id: v1", r#"id: """#);
        let response = app
            .oneshot(yaml_post("/api/templates", "POST", yaml))
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(response).await;
        assert_eq!(body["error"]["code"], "TemplateInvalid");
        assert_eq!(
            body["error"]["details"]["reason"],
            "template_validation_failed"
        );
    }

    /// The fourth migrated code at the wire. Most RenderFailed causes are internal invariants a
    /// request cannot provoke (`item_has_no_source` in particular is unreachable, since raw deserialization
    /// requires a mandatory `value` string for text/qr items at parse time). Deleting the templates
    /// directory out from under a built app reaches one without depending on the test user's uid,
    /// which a read-only directory would.
    ///
    /// The reason is `template_registry_io`, not `template_write_failed`: since #184 a create re-reads
    /// the directory before it decides anything, so a directory that cannot be read is reported as
    /// exactly that, before any write is attempted. `template_write_failed` still covers a write that
    /// fails on a readable directory (a full disk, an I/O fault), which no portable test provokes.
    #[tokio::test]
    async fn a_failed_templates_directory_read_carries_a_reason() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);
        std::fs::remove_dir_all(&dir).expect("remove templates dir");

        let response = app
            .oneshot(yaml_post("/api/templates", "POST", template_yaml("wf1")))
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = json_response(response).await;
        assert_eq!(body["error"]["code"], "RenderFailed");
        assert_eq!(body["error"]["details"]["reason"], "template_registry_io");
    }

    async fn template_ids(app: &axum::Router) -> Vec<String> {
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
        body["templates"]
            .as_array()
            .expect("templates array")
            .iter()
            .map(|t| t["id"].as_str().unwrap().to_string())
            .collect()
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
    async fn reload_with_broken_file_succeeds_and_quarantines_it() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("t1.yaml"), template_yaml("t1")).unwrap();
        let app = build_app_in(&dir);
        assert_eq!(template_count(&app).await, 1);

        // Write a bad file alongside the good one.
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
        // Reload succeeds now: bad files are quarantined, not fatal.
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_response(response).await;
        assert_eq!(body["count"], 1);
        assert_eq!(body["broken_count"], 1);

        // The valid template is still served.
        assert_eq!(template_count(&app).await, 1);

        // GET /api/templates lists the valid template and the broken entry.
        let (_, list) = get_json(&app, "/api/templates").await;
        assert_eq!(list["templates"].as_array().unwrap().len(), 1);
        let broken = list["broken"].as_array().unwrap();
        assert_eq!(broken.len(), 1);
        assert_eq!(broken[0]["filename"], "bad.yaml");
        assert!(broken[0]["error"].as_str().unwrap().contains("bad.yaml"));

        std::fs::remove_dir_all(&dir).ok();
    }

    async fn reload(app: &axum::Router) -> Value {
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
        json_response(response).await
    }

    /// A copy-pasted file claiming a live id is refused on its own; the reload still succeeds and
    /// the template already serving the id is untouched (#181).
    #[tokio::test]
    async fn reload_with_duplicate_id_succeeds_and_quarantines_the_collider() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("a.yaml"), template_yaml("dup")).unwrap();
        let app = build_app_in(&dir);
        assert_eq!(template_count(&app).await, 1);

        std::fs::write(dir.join("z.yaml"), template_yaml("dup")).unwrap();
        let body = reload(&app).await;
        assert_eq!(body["count"], 1);
        assert_eq!(body["broken_count"], 1);

        let (_, list) = get_json(&app, "/api/templates").await;
        let templates = list["templates"].as_array().unwrap();
        assert_eq!(templates.len(), 1);
        assert_eq!(templates[0]["id"], "dup");
        let broken = list["broken"].as_array().unwrap();
        assert_eq!(broken.len(), 1);
        assert_eq!(broken[0]["filename"], "z.yaml");
        let error = broken[0]["error"].as_str().unwrap();
        assert!(
            error.contains("dup") && error.contains("a.yaml"),
            "broken entry names the id and the file it collides with: {error}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The operator's fix converges: drop one of the two files, reload, and the collision is gone
    /// while the winner keeps serving (#181).
    #[tokio::test]
    async fn removing_the_colliding_file_clears_the_broken_entry() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("a.yaml"), template_yaml("dup")).unwrap();
        std::fs::write(dir.join("z.yaml"), template_yaml("dup")).unwrap();
        // The app builds at all only because a duplicate id no longer fails the load.
        let app = build_app_in(&dir);
        let (_, list) = get_json(&app, "/api/templates").await;
        assert_eq!(list["broken"].as_array().unwrap().len(), 1);

        std::fs::remove_file(dir.join("z.yaml")).unwrap();
        let body = reload(&app).await;
        assert_eq!(body["count"], 1);
        assert_eq!(body["broken_count"], 0);

        let (_, list) = get_json(&app, "/api/templates").await;
        assert_eq!(list["templates"].as_array().unwrap().len(), 1);
        assert_eq!(list["templates"][0]["id"], "dup");
        assert!(
            list.get("broken").is_none(),
            "an empty broken list is omitted: {list}"
        );

        let (status, _) = get_json(&app, "/api/templates/dup").await;
        assert_eq!(status, StatusCode::OK);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The one fault that can still fail a reload now that every content fault is quarantined: the
    /// directory itself is unreadable. The previously-loaded set survives it (#181).
    #[tokio::test]
    async fn reload_with_unreadable_dir_fails_and_keeps_the_live_set() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("t1.yaml"), template_yaml("t1")).unwrap();
        let app = build_app_in(&dir);
        assert_eq!(template_count(&app).await, 1);

        std::fs::remove_dir_all(&dir).expect("remove templates dir");
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
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = json_response(response).await;
        assert_eq!(body["error"]["code"], "RenderFailed");
        assert_eq!(body["error"]["details"]["reason"], "template_registry_io");

        assert_eq!(template_count(&app).await, 1);
    }

    #[tokio::test]
    async fn template_list_group_filtering_and_exposure() {
        let dir = temp_templates_dir();
        // 1. Grouped template "t_wh1" in "Warehouse"
        std::fs::write(
            dir.join("t_wh1.yaml"),
            "id: t_wh1\nname: Warehouse 1\ngroup: Warehouse\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 10\nlayout: []\n",
        ).unwrap();
        // 2. Grouped template "t_wh2" in "Warehouse"
        std::fs::write(
            dir.join("t_wh2.yaml"),
            "id: t_wh2\nname: Warehouse 2\ngroup: Warehouse\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 10\nlayout: []\n",
        ).unwrap();
        // 3. Grouped template "t_ship" in "Shipping"
        std::fs::write(
            dir.join("t_ship.yaml"),
            "id: t_ship\nname: Shipping\ngroup: Shipping\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 10\nlayout: []\n",
        ).unwrap();
        // 4. Ungrouped template "t_ungrouped"
        std::fs::write(
            dir.join("t_ungrouped.yaml"),
            "id: t_ungrouped\nname: Ungrouped\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 10\nlayout: []\n",
        ).unwrap();
        // 5. Broken template "bad.yaml"
        std::fs::write(dir.join("bad.yaml"), "not valid yaml : [").unwrap();

        let app = build_app_in(&dir);

        // a grouped summary carries `group`; an ungrouped response has no `group` key
        let (_, list) = get_json(&app, "/api/templates").await;
        let tpls = list["templates"].as_array().unwrap();
        assert_eq!(tpls.len(), 4);
        let wh1 = tpls.iter().find(|t| t["id"] == "t_wh1").unwrap();
        assert_eq!(wh1["group"], "Warehouse");
        let ungr = tpls.iter().find(|t| t["id"] == "t_ungrouped").unwrap();
        assert!(
            ungr.get("group").is_none(),
            "ungrouped template summary must omit 'group' key"
        );

        // detail of ungrouped has no group key
        let (_, detail) = get_json(&app, "/api/templates/t_ungrouped").await;
        assert!(
            detail.get("group").is_none(),
            "ungrouped template detail must omit 'group' key"
        );

        // detail of grouped carries group key
        let (_, detail_wh) = get_json(&app, "/api/templates/t_wh1").await;
        assert_eq!(detail_wh["group"], "Warehouse");

        // ?group=Warehouse returns the 2 warehouse templates
        let (_, list_wh) = get_json(&app, "/api/templates?group=Warehouse").await;
        let tpls_wh = list_wh["templates"].as_array().unwrap();
        let ids_wh: Vec<&str> = tpls_wh.iter().map(|t| t["id"].as_str().unwrap()).collect();
        assert_eq!(ids_wh, vec!["t_wh1", "t_wh2"]);
        assert_eq!(list_wh["broken"].as_array().unwrap().len(), 1);

        // ?group= returns only ungrouped
        let (_, list_ungr) = get_json(&app, "/api/templates?group=").await;
        let tpls_ungr = list_ungr["templates"].as_array().unwrap();
        let ids_ungr: Vec<&str> = tpls_ungr
            .iter()
            .map(|t| t["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids_ungr, vec!["t_ungrouped"]);
        assert_eq!(list_ungr["broken"].as_array().unwrap().len(), 1);

        // ?group=Nonexistent returns empty templates list (200 OK)
        let (status, list_none) = get_json(&app, "/api/templates?group=Nonexistent").await;
        assert_eq!(status, StatusCode::OK);
        assert!(list_none["templates"].as_array().unwrap().is_empty());
        assert_eq!(list_none["broken"].as_array().unwrap().len(), 1);

        // ?group=warehouse (case difference) returns none against Warehouse
        let (_, list_case) = get_json(&app, "/api/templates?group=warehouse").await;
        assert!(list_case["templates"].as_array().unwrap().is_empty());
        assert_eq!(list_case["broken"].as_array().unwrap().len(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn template_move_group_http_endpoint() {
        let dir = temp_templates_dir();
        let t1_yaml = "# Template 1 comment\nid: t1\nname: Template 1\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 50\n  height: 18\nlayout: []\n";
        let t1_path = dir.join("t1.yaml");
        std::fs::write(&t1_path, t1_yaml).unwrap();

        let app = build_app_in(&dir);

        // 1. Move ungrouped template into "Warehouse" -> 200 OK
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/templates/t1/group")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"group":"Warehouse"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let detail = json_response(res).await;
        assert_eq!(detail["id"], "t1");
        assert_eq!(detail["group"], "Warehouse");
        let source_after = std::fs::read_to_string(&t1_path).unwrap();
        assert!(source_after.contains("group: Warehouse"));
        assert!(source_after.starts_with(
            "# Template 1 comment\nid: t1\nname: Template 1\ngroup: Warehouse\nunit: mm"
        ));

        // 2. Idempotent set to "Warehouse" -> 200 OK, byte-identical file
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/templates/t1/group")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"group":"Warehouse"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let detail = json_response(res).await;
        assert_eq!(detail["group"], "Warehouse");
        let source_idem = std::fs::read_to_string(&t1_path).unwrap();
        assert_eq!(source_idem, source_after);

        // 3. Move to "Shipping" -> 200 OK
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/templates/t1/group")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"group":"Shipping"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let detail = json_response(res).await;
        assert_eq!(detail["group"], "Shipping");

        // 4. Clear group with null -> 200 OK
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/templates/t1/group")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"group":null}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let detail = json_response(res).await;
        assert!(detail.get("group").is_none());
        let source_cleared = std::fs::read_to_string(&t1_path).unwrap();
        assert_eq!(source_cleared, t1_yaml);

        // 5. Idempotent clear -> 200 OK, byte-identical
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/templates/t1/group")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"group":null}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let detail = json_response(res).await;
        assert!(detail.get("group").is_none());
        assert_eq!(std::fs::read_to_string(&t1_path).unwrap(), t1_yaml);

        // 6. Unknown id -> 404
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/templates/nonexistent_id/group")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"group":"Warehouse"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);

        // 7. Bad request body (non-string/non-null group) -> 400 Bad Request
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/templates/t1/group")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"group":123}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);

        // 7b. A body omitting the key entirely is malformed, not a clear. Before #164's diff review
        // `group` carried a serde default, so `{}` returned 200 and silently ungrouped the
        // template: a destructive edit from a body the contract rejects.
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/templates/t1/group")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"group":"Warehouse"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let grouped_source = std::fs::read_to_string(&t1_path).unwrap();
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/templates/t1/group")
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            std::fs::read_to_string(&t1_path).unwrap(),
            grouped_source,
            "a body without the group key must not touch the file"
        );

        // 8. Invalid group name (empty) -> 422 Unprocessable Entity, file unchanged
        let pre_invalid = std::fs::read_to_string(&t1_path).unwrap();
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/templates/t1/group")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"group":""}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let err = json_response(res).await;
        assert_eq!(err["error"]["details"]["reason"], "template_group_invalid");
        assert_eq!(std::fs::read_to_string(&t1_path).unwrap(), pre_invalid);

        // 9. Unpatchable template (flow mapping) -> 422 Unprocessable Entity, file unchanged
        let flow_yaml = "{id: flow_t, name: Flow, unit: mm, dpi: 200, format: {type: single, width: 50, height: 18}, layout: []}\n";
        let flow_path = dir.join("flow_t.yaml");
        std::fs::write(&flow_path, flow_yaml).unwrap();
        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/templates/reload")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/templates/flow_t/group")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"group":"Warehouse"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let err = json_response(res).await;
        assert_eq!(
            err["error"]["details"]["reason"],
            "template_group_unpatchable"
        );
        assert_eq!(std::fs::read_to_string(&flow_path).unwrap(), flow_yaml);

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
    async fn delete_missing_template_returns_404() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/templates/nope")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "TemplateNotFound");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The read-side registry filter hides a stale favorite, so asserting it is gone right after the
    /// delete would pass with or without the prune. Re-creating the id is what discriminates: an
    /// unpruned row becomes visible again, attached to a template the user never favorited (#140).
    #[tokio::test]
    async fn deleting_a_template_prunes_its_favorites() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("f1.yaml"), template_yaml("f1")).unwrap();
        let app = build_app_in(&dir);

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/favorites/f1")
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
                    .method("DELETE")
                    .uri("/api/templates/f1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        let resp = app
            .clone()
            .oneshot(yaml_post("/api/templates", "POST", template_yaml("f1")))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::CREATED);

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/favorites")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        let body = json_response(resp).await;
        assert_eq!(
            body.as_array().expect("favorites array").len(),
            0,
            "a favorite survived the delete and re-attached to the new template"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A broken sibling no longer blocks a delete reload — the bad file is quarantined.
    /// After the delete the registry excludes the deleted template and keeps the broken file listed.
    #[tokio::test]
    async fn delete_with_broken_sibling_succeeds_and_quarantines_broken() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("s1.yaml"), template_yaml("s1")).unwrap();
        let app = build_app_in(&dir);

        std::fs::write(dir.join("bad.yaml"), "id: bad\nunit: nope\n").unwrap();
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/templates/s1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        // Delete now succeeds: broken sibling is quarantined, not fatal.
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert!(
            !dir.join("s1.yaml").exists(),
            "the unlink should have happened"
        );
        assert_eq!(template_count(&app).await, 0);
        let (_, list) = get_json(&app, "/api/templates").await;
        assert_eq!(list["broken"][0]["filename"], "bad.yaml");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn delete_removes_a_yml_backed_template() {
        let dir = temp_templates_dir();
        // The registry loads *.yml as well as *.yaml, so this template is live — and must be
        // deletable through the API, not only by hand (#140).
        std::fs::write(dir.join("y1.yml"), template_yaml("y1")).unwrap();
        let app = build_app_in(&dir);
        assert_eq!(template_count(&app).await, 1);

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/templates/y1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert!(
            !dir.join("y1.yml").exists(),
            "the .yml file is still on disk"
        );
        assert_eq!(template_count(&app).await, 0);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A template's filename is only conventionally its id: the registry keys on the `id` inside the
    /// YAML. Every file-backed endpoint must therefore act on the file the registry actually loaded.
    #[tokio::test]
    async fn file_endpoints_resolve_a_template_whose_filename_differs_from_its_id() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("custom.yaml"), template_yaml("y2")).unwrap();
        let app = build_app_in(&dir);

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/templates/y2/source")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::OK);

        // PUT must overwrite custom.yaml in place. A y2.yaml sibling would give two files one id,
        // and the reload inside the handler would refuse one of them as a duplicate, leaving a
        // broken entry behind and making which file serves y2 depend on filename order (#181).
        let body200 = template_yaml("y2").replace("dpi: 300", "dpi: 200");
        let resp = app
            .clone()
            .oneshot(yaml_post("/api/templates/y2", "PUT", body200))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(
            !dir.join("y2.yaml").exists(),
            "PUT created a duplicate-id sibling"
        );
        assert_eq!(template_count(&app).await, 1);

        // POST for an id the registry already holds is a conflict whatever the file is called.
        let resp = app
            .clone()
            .oneshot(yaml_post("/api/templates", "POST", template_yaml("y2")))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::CONFLICT);

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/templates/y2")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert!(
            !dir.join("custom.yaml").exists(),
            "the backing file is still on disk"
        );

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

    /// The filename half of the create guard: an unservable file occupying `{id}.yaml` blocks the
    /// create by its name alone, since its content claims no id the registry could serve.
    ///
    /// This does not exercise the no-replace publish, and passes against the pre-change code too:
    /// the destination is planted before the request, so the `exists()` check answers first. The
    /// publish primitive is pinned where the race is actually decidable, in
    /// `publish_new_template_file_refuses_an_occupied_name` (`api.rs`).
    #[tokio::test]
    async fn template_create_is_blocked_by_an_unservable_file_at_its_destination() {
        let dir = temp_templates_dir();
        // Content the registry cannot serve, so only the filename half of the guard can catch it.
        std::fs::write(dir.join("planted.yaml"), "not: a valid template\n").unwrap();
        let app = build_app_in(&dir);

        let resp = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates",
                "POST",
                template_yaml("planted"),
            ))
            .await
            .expect("request");

        assert_eq!(resp.status(), StatusCode::CONFLICT);
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "TemplateExists");
        assert_eq!(
            std::fs::read_to_string(dir.join("planted.yaml")).unwrap(),
            "not: a valid template\n",
            "the other writer's file is untouched"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The registry can be stale relative to disk: templates are installed by copying files in, with
    /// no reload. The guard has to test the directory, not the in-memory set (#184).
    #[tokio::test]
    async fn template_create_sees_a_file_copied_in_since_the_last_reload() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);
        // After the app is built, so the registry does not hold `late`, and under a filename the
        // destination check cannot catch either.
        std::fs::write(dir.join("aaa.yaml"), template_yaml("late")).unwrap();

        let resp = app
            .clone()
            .oneshot(yaml_post("/api/templates", "POST", template_yaml("late")))
            .await
            .expect("request");

        assert_eq!(resp.status(), StatusCode::CONFLICT);
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "TemplateExists");
        assert!(
            !dir.join("late.yaml").exists(),
            "nothing was written for the refused create"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// With the pre-write re-read (#184), a `PUT` for an id whose winner changed on disk edits the
    /// file that currently serves it, and answers with the caller's own content. The stale-registry
    /// path that used to return the *other* template's body is gone: the handler no longer resolves
    /// the id from a set that predates the directory.
    #[tokio::test]
    async fn template_replace_writes_the_current_winner_after_a_collider_appears() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("zzz.yaml"), template_yaml("moved")).unwrap();
        let app = build_app_in(&dir);
        // Sorts before zzz.yaml, so the next load hands `moved` to this file instead.
        std::fs::write(dir.join("aaa.yaml"), template_yaml("moved")).unwrap();

        let edited = template_yaml("moved").replace("name: moved", "name: edited by the caller");
        let resp = app
            .clone()
            .oneshot(yaml_post("/api/templates/moved", "PUT", edited.clone()))
            .await
            .expect("request");

        assert_eq!(resp.status(), StatusCode::OK);
        let body = json_response(resp).await;
        assert_eq!(
            body["name"], "edited by the caller",
            "the response describes the caller's own write, never the other file"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("aaa.yaml")).unwrap(),
            edited,
            "the write went to the file that serves the id now"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("zzz.yaml")).unwrap(),
            template_yaml("moved"),
            "the file that lost the id is left alone"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A duplicate that sorts *after* the written file never displaces it, so the write succeeds
    /// normally and the duplicate is just a refused sibling. Returning 409 here would be a lie about
    /// which file serves the id (#184, round-4 review).
    ///
    /// This passed before the change too, since there was no confirmation to get wrong. It guards
    /// the confirmation against the round-4 defect: keying on "a duplicate exists" rather than on
    /// the written file having lost the id turns this 200 into a 409.
    #[tokio::test]
    async fn template_replace_ignores_a_later_sorting_duplicate() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("aaa.yaml"), template_yaml("kept")).unwrap();
        let app = build_app_in(&dir);
        std::fs::write(dir.join("zzz.yaml"), template_yaml("kept")).unwrap();

        let edited = template_yaml("kept").replace("name: kept", "name: still mine");
        let resp = app
            .clone()
            .oneshot(yaml_post("/api/templates/kept", "PUT", edited.clone()))
            .await
            .expect("request");

        assert_eq!(resp.status(), StatusCode::OK);
        let body = json_response(resp).await;
        assert_eq!(body["name"], "still mine", "the caller's own content");
        assert_eq!(
            std::fs::read_to_string(dir.join("aaa.yaml")).unwrap(),
            edited
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The whole of #184, end to end: a colliding file that sorts earlier lands *between* the write
    /// and the reload, so the id the caller addressed is served from another file by the time the
    /// handler answers. Before this change the handler returned `200` with that other file's body.
    ///
    /// This is the one interleaving a request cannot produce on its own, so it is staged with the
    /// test-only mid-write hook. Without it, no endpoint test fails when a handler stops confirming.
    #[tokio::test]
    async fn template_replace_returns_409_when_the_id_moves_between_write_and_reload() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("zzz.yaml"), template_yaml("moved")).unwrap();
        let (app, state) = build_app_in_with_state(&dir);

        let planted = dir.join("aaa.yaml");
        state.set_mid_write_hook(move || {
            // Sorts before zzz.yaml, so the reload that follows hands `moved` to this file.
            std::fs::write(&planted, template_yaml_for("moved", "planted")).unwrap();
        });

        let edited = template_yaml("moved").replace("name: moved", "name: edited by the caller");
        let resp = app
            .clone()
            .oneshot(yaml_post("/api/templates/moved", "PUT", edited.clone()))
            .await
            .expect("request");

        assert_eq!(resp.status(), StatusCode::CONFLICT);
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "TemplateIdCollision");
        let mut files: Vec<&str> = body["error"]["details"]["files"]
            .as_array()
            .expect("files array")
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        files.sort_unstable();
        assert_eq!(files, vec!["aaa.yaml", "zzz.yaml"]);
        assert_eq!(
            std::fs::read_to_string(dir.join("zzz.yaml")).unwrap(),
            edited,
            "the caller's write is kept in the file it addressed"
        );

        let listed = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/templates")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        let listed = json_response(listed).await;
        let broken: Vec<&str> = listed["broken"]
            .as_array()
            .expect("broken array")
            .iter()
            .map(|b| b["filename"].as_str().unwrap())
            .collect();
        assert_eq!(broken, vec!["zzz.yaml"], "the caller's file is quarantined");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A body without the `group` key is a bad request whatever the directory holds: it is judged
    /// before the id is resolved, so an unknown id cannot answer `404` in its place
    /// (`template-groups` spec, response table).
    #[tokio::test]
    async fn template_group_update_rejects_a_bodiless_request_before_resolving_the_id() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/templates/does-not-exist/group")
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .expect("request");

        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "the malformed body decides, not the unknown id"
        );
        let body = json_response(resp).await;
        assert_eq!(body["error"]["details"]["reason"], "request_body_invalid");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The create must publish with the no-replace primitive, not a rename. Staged with the
    /// pre-publish hook: the destination appears once the guard has already passed, which is the one
    /// state `exists()` cannot catch and `rename` would silently overwrite (#184).
    #[tokio::test]
    async fn template_create_does_not_overwrite_a_destination_that_appears_after_its_guard() {
        let dir = temp_templates_dir();
        let (app, state) = build_app_in_with_state(&dir);

        let planted = dir.join("racer.yaml");
        state.set_pre_publish_hook(move || {
            std::fs::write(&planted, "someone else's file\n").unwrap();
        });

        let resp = app
            .clone()
            .oneshot(yaml_post("/api/templates", "POST", template_yaml("racer")))
            .await
            .expect("request");

        assert_eq!(resp.status(), StatusCode::CONFLICT);
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "TemplateExists");
        assert_eq!(
            std::fs::read_to_string(dir.join("racer.yaml")).unwrap(),
            "someone else's file\n",
            "the other writer's file was not overwritten"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The same interleaving through `POST`: the pre-write re-read finds the id free, the collider
    /// lands while the file is being published, and the create must not answer `201` describing it.
    #[tokio::test]
    async fn template_create_returns_409_when_the_id_moves_between_write_and_reload() {
        let dir = temp_templates_dir();
        let (app, state) = build_app_in_with_state(&dir);

        let planted = dir.join("aaa.yaml");
        state.set_mid_write_hook(move || {
            std::fs::write(&planted, template_yaml_for("late", "planted")).unwrap();
        });

        let resp = app
            .clone()
            .oneshot(yaml_post("/api/templates", "POST", template_yaml("late")))
            .await
            .expect("request");

        assert_eq!(resp.status(), StatusCode::CONFLICT);
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "TemplateIdCollision");
        assert_eq!(
            std::fs::read_to_string(dir.join("late.yaml")).unwrap(),
            template_yaml("late"),
            "the caller's file keeps what it submitted"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// And through the group update, whose writing branch makes the same claim.
    #[tokio::test]
    async fn template_group_update_returns_409_when_the_id_moves_between_write_and_reload() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("zzz.yaml"), template_yaml("grouped")).unwrap();
        let (app, state) = build_app_in_with_state(&dir);

        let planted = dir.join("aaa.yaml");
        state.set_mid_write_hook(move || {
            std::fs::write(&planted, template_yaml_for("grouped", "planted")).unwrap();
        });

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/templates/grouped/group")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"group":"Warehouse"}"#))
                    .unwrap(),
            )
            .await
            .expect("request");

        assert_eq!(resp.status(), StatusCode::CONFLICT);
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "TemplateIdCollision");
        assert!(
            std::fs::read_to_string(dir.join("zzz.yaml"))
                .unwrap()
                .contains("group: Warehouse"),
            "the patch stays in the file it was applied to"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The group update shares the write -> reload -> detail shape, so it gets the same pre-write
    /// re-read and the same post-write confirmation; here that means it patches the file serving the
    /// id now, and answers for that file (#184).
    #[tokio::test]
    async fn template_group_update_writes_the_current_winner() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("zzz.yaml"), template_yaml("grouped")).unwrap();
        let app = build_app_in(&dir);
        std::fs::write(dir.join("aaa.yaml"), template_yaml("grouped")).unwrap();

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/templates/grouped/group")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"group":"Warehouse"}"#))
                    .unwrap(),
            )
            .await
            .expect("request");

        assert_eq!(resp.status(), StatusCode::OK);
        let body = json_response(resp).await;
        assert_eq!(body["group"], "Warehouse");
        assert!(
            std::fs::read_to_string(dir.join("aaa.yaml"))
                .unwrap()
                .contains("group: Warehouse"),
            "the file serving the id was the one patched"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A `PUT` for an id whose file is gone by request time is a `404`, decided by the pre-write
    /// re-read before anything is written: the registry no longer holds the id.
    ///
    /// The write-then-vanish `500` arm is a different case and is not reachable from here, because
    /// it needs the file to disappear *between* the write and the reload. It is pinned in
    /// `confirm_written_template_reports_a_renamed_file_as_a_lost_write` (`api.rs`).
    #[tokio::test]
    async fn template_replace_for_a_vanished_file_returns_404() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("gone.yaml"), template_yaml("gone")).unwrap();
        let app = build_app_in(&dir);
        // Stand in for the file being removed between the write and the reload: the registry then
        // serves the id from nothing at all.
        std::fs::remove_file(dir.join("gone.yaml")).unwrap();

        let resp = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/gone",
                "PUT",
                template_yaml("gone"),
            ))
            .await
            .expect("request");

        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "the id is gone from the re-read registry"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Deleting the winner promotes the collider, so the id survives a 204 from different content
    /// with its favorites already pruned. Refuse instead, naming the file to fix (#183).
    #[tokio::test]
    async fn template_delete_is_refused_while_the_id_collides() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("aaa.yaml"), template_yaml("contested")).unwrap();
        let app = build_app_in(&dir);
        // Added after the app is built, so the refusal has to read the directory to see it, and so
        // that reading must not become the served set: `unrelated` is on disk but not yet served,
        // and a refused delete must leave it that way (#183).
        std::fs::write(dir.join("zzz.yaml"), template_yaml("contested")).unwrap();
        std::fs::write(dir.join("unrelated.yaml"), template_yaml("unrelated")).unwrap();
        let served_before = template_ids(&app).await;
        assert_eq!(
            served_before,
            vec!["contested".to_string()],
            "the late files are not served yet"
        );

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/favorites/contested")
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
                    .method("DELETE")
                    .uri("/api/templates/contested")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");

        assert_eq!(resp.status(), StatusCode::CONFLICT);
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "TemplateIdCollision");
        assert_eq!(body["error"]["details"]["template"], "contested");
        let mut files: Vec<&str> = body["error"]["details"]["files"]
            .as_array()
            .expect("files array")
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        files.sort_unstable();
        assert_eq!(
            files,
            vec!["aaa.yaml", "zzz.yaml"],
            "exactly the files declaring the id, and no others"
        );
        assert!(
            !body["error"]["details"]
                .as_object()
                .expect("details object")
                .contains_key("reason"),
            "a 409 carries no details.reason key at all (ADR-0052)"
        );
        assert!(dir.join("aaa.yaml").exists(), "nothing was unlinked");
        assert!(dir.join("zzz.yaml").exists(), "nothing was unlinked");

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/favorites")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        let favorites = json_response(resp).await;
        assert_eq!(
            favorites,
            serde_json::json!(["contested"]),
            "a refused delete prunes no favorites"
        );
        assert_eq!(
            template_ids(&app).await,
            served_before,
            "a refused delete leaves the served set unchanged"
        );

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/templates/contested")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::OK, "the id is still served");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The live set can outlive the files it names: an earlier delete unlinks the file, its reload
    /// then fails on an unreadable directory, and the id stays served. A retry must converge, which
    /// means the reading that proves the id is gone has to become the served set, not just decide
    /// this one response (#183, round-3 diff review).
    #[tokio::test]
    async fn template_delete_of_an_already_unlinked_file_converges() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("ghost.yaml"), template_yaml("ghost")).unwrap();
        let app = build_app_in(&dir);
        // Stand in for the earlier delete whose reload failed: the file is gone, the registry is not.
        std::fs::remove_file(dir.join("ghost.yaml")).unwrap();

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/templates/ghost")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/templates/ghost")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "the service stopped serving a template it just called missing"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The refusal is scoped to the id being deleted: a file refused for some *other* id is a
    /// pre-existing condition of the directory and must not block an unrelated delete (#183).
    ///
    /// Passes against the pre-change code as well, where nothing blocked a delete at all; it exists
    /// to keep the new refusal from over-reaching.
    #[tokio::test]
    async fn template_delete_succeeds_beside_an_unrelated_refused_file() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("target.yaml"), template_yaml("target")).unwrap();
        std::fs::write(dir.join("aaa.yaml"), template_yaml("other")).unwrap();
        std::fs::write(dir.join("zzz.yaml"), template_yaml("other")).unwrap();
        let app = build_app_in(&dir);

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/templates/target")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert!(!dir.join("target.yaml").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Once the operator removes the collider the delete goes through, so the refusal converges
    /// instead of stranding the id (#183). Like the test above it also passes pre-change; its job is
    /// to prove the refusal is not permanent.
    #[tokio::test]
    async fn template_delete_succeeds_once_the_collider_is_gone() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("aaa.yaml"), template_yaml("fixable")).unwrap();
        std::fs::write(dir.join("zzz.yaml"), template_yaml("fixable")).unwrap();
        let app = build_app_in(&dir);

        std::fs::remove_file(dir.join("zzz.yaml")).unwrap();
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/templates/fixable")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert!(!dir.join("aaa.yaml").exists());
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

    /// A rejected edit must not touch the stored template: `parse_and_validate` runs before the
    /// write, so both the served source and the file on disk stay as they were.
    #[tokio::test]
    async fn template_replace_invalid_yaml_leaves_the_stored_template_unchanged() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("inv.yaml"), template_yaml("inv")).unwrap();
        let app = build_app_in(&dir);

        // 40mm wide inside a 20mm frame: parses fine, fails validate_bounds.
        let bad = template_yaml("inv").replace("size: [20.0, 5.0]", "size: [40.0, 5.0]");
        let resp = app
            .clone()
            .oneshot(yaml_post("/api/templates/inv", "PUT", bad))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/templates/inv/source")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(
            String::from_utf8(body.to_vec()).unwrap(),
            template_yaml("inv"),
            "a rejected edit rewrote the file"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// PUT writes the file and reloads — with a broken sibling the reload still succeeds (quarantine),
    /// so the edited template is live immediately.
    #[tokio::test]
    async fn template_replace_with_broken_sibling_succeeds_and_live_immediately() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("p1.yaml"), template_yaml("p1")).unwrap();
        let app = build_app_in(&dir);

        std::fs::write(dir.join("bad.yaml"), "id: bad\nunit: nope\n").unwrap();
        let edited = template_yaml("p1").replace("dpi: 300", "dpi: 200");
        let resp = app
            .clone()
            .oneshot(yaml_post("/api/templates/p1", "PUT", edited.clone()))
            .await
            .expect("request");
        // Succeeds: broken sibling is quarantined, not fatal.
        assert_eq!(resp.status(), StatusCode::OK);
        // The edit is live.
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/templates/p1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        let detail = json_response(resp).await;
        assert_eq!(detail["dpi"], 200);

        std::fs::remove_dir_all(&dir).ok();
    }

    // A valid write with a broken sibling succeeds; the new template is live and broken is listed.
    #[tokio::test]
    async fn create_with_broken_sibling_succeeds_and_quarantines_broken() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("t1.yaml"), template_yaml("t1")).unwrap();
        let app = build_app_in(&dir);
        std::fs::write(dir.join("broken.yaml"), "id: broken\nunit: nope\n").unwrap();

        let resp = app
            .clone()
            .oneshot(yaml_post("/api/templates", "POST", template_yaml("new1")))
            .await
            .expect("request");
        // Succeeds now that broken files are quarantined.
        assert_eq!(resp.status(), StatusCode::CREATED);
        assert!(dir.join("new1.yaml").exists());
        assert_eq!(template_count(&app).await, 2);

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
                "/api/variables/qr_base_url",
                json!({ "value": "https://h/i" }).to_string(),
            ))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::OK);

        let (status, variables) = get_json(&app, "/api/variables").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(variables["qr_base_url"], "https://h/i");
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
    async fn probe_ok_returns_capabilities() {
        let app = build_app();
        let body = json!({
            "kind": "fake",
            "config": { "capabilities": {
                "bilevel": true, "color_known": true, "accepts_png": true,
                "resolution": 180, "model": "Brother PT-2730"
            } }
        })
        .to_string();
        let resp = app
            .oneshot(json_req("POST", "/api/printers/probe", body))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::OK);
        let v = json_response(resp).await;
        assert_eq!(v["status"], "ok");
        assert_eq!(v["capabilities"]["color"], "bilevel");
        assert_eq!(v["capabilities"]["model"], "Brother PT-2730");
        assert_eq!(v["capabilities"]["resolution_dpi"], 180);
    }

    #[tokio::test]
    async fn probe_reports_unknown_color_when_printer_is_silent() {
        let app = build_app();
        let body =
            json!({ "kind": "fake", "config": { "capabilities": { "bilevel": false, "color_known": false } } })
                .to_string();
        let resp = app
            .oneshot(json_req("POST", "/api/printers/probe", body))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::OK);
        let v = json_response(resp).await;
        assert_eq!(v["status"], "ok");
        assert_eq!(v["capabilities"]["color"], "unknown");
    }

    #[tokio::test]
    async fn probe_unreachable_returns_status() {
        let app = build_app();
        let body = json!({ "kind": "fake", "config": { "probe": "unreachable" } }).to_string();
        let resp = app
            .oneshot(json_req("POST", "/api/printers/probe", body))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::OK);
        let v = json_response(resp).await;
        assert_eq!(v["status"], "unreachable");
        assert!(v["detail"].is_string());
    }

    #[tokio::test]
    async fn probe_missing_uri_is_422() {
        let app = build_app();
        let body = json!({ "kind": "cups", "config": {} }).to_string();
        let resp = app
            .oneshot(json_req("POST", "/api/printers/probe", body))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn probe_malformed_uri_is_422() {
        let app = build_app();
        let body = json!({ "kind": "cups", "config": { "uri": "ipp://" } }).to_string();
        let resp = app
            .oneshot(json_req("POST", "/api/printers/probe", body))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
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

    #[tokio::test]
    async fn browse_endpoint_returns_rows_e2e() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let hb = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [{"id":"e1","name":"Drill","entityType":{"name":"item"}}], "total": 1
            })))
            .mount(&hb)
            .await;
        let state = loopback_state();
        let c = state
            .store()
            .create_connection(crate::store::NewConnection {
                connector: "homebox",
                name: "h",
                base_url: &hb.uri(),
                public_url: None,
                credential: "hb_key",
                enabled: true,
                transforms: &[],
            })
            .await
            .unwrap();
        let router = with_auth(app(state.clone()));
        let res = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/connections/{}/browse", c.id))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"resource":"entities"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["rows"][0]["id"]["key"], "e1");
    }

    #[tokio::test]
    async fn browse_endpoint_public_url_and_fallback_e2e() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let hb = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [{"id":"e1","name":"Drill","entityType":{"name":"item"}}], "total": 1
            })))
            .mount(&hb)
            .await;
        let state = loopback_state();
        let c = state
            .store()
            .create_connection(crate::store::NewConnection {
                connector: "homebox",
                name: "h",
                base_url: &hb.uri(),
                public_url: Some("https://public.homebox.domain"),
                credential: "hb_key",
                enabled: true,
                transforms: &[],
            })
            .await
            .unwrap();
        let router = with_auth(app(state.clone()));

        // Browse uses public_url for row links
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/connections/{}/browse", c.id))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"resource":"entities"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            v["rows"][0]["url"],
            "https://public.homebox.domain/entity/e1"
        );

        // Clear public_url via PUT
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/connections/{}", c.id))
                    .header("content-type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"connector":"homebox","name":"h","base_url":"{}","public_url":null}}"#,
                        hb.uri()
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // Browse falls back to base_url
        let res = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/connections/{}/browse", c.id))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"resource":"entities"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let v: Value = serde_json::from_slice(&body).unwrap();
        let expected_url = format!("{}/entity/e1", hb.uri().trim_end_matches('/'));
        assert_eq!(v["rows"][0]["url"], expected_url);
    }

    #[tokio::test]
    async fn schema_endpoint_e2e() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let hb = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities/fields"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!(["SKU"])))
            .mount(&hb)
            .await;
        let state = loopback_state();
        let c = state
            .store()
            .create_connection(crate::store::NewConnection {
                connector: "homebox",
                name: "h",
                base_url: &hb.uri(),
                public_url: None,
                credential: "hb_key",
                enabled: true,
                transforms: &[],
            })
            .await
            .unwrap();
        let router = with_auth(app(state.clone()));
        let res = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/connections/{}/schema", c.id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert!(v["resources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == "entities"));
    }

    #[tokio::test]
    async fn materialize_endpoint_e2e() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let hb = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities/e1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id":"e1","name":"Drill","manufacturer":"Acme"
            })))
            .mount(&hb)
            .await;
        let state = loopback_state();
        let c = state
            .store()
            .create_connection(crate::store::NewConnection {
                connector: "homebox",
                name: "h",
                base_url: &hb.uri(),
                public_url: None,
                credential: "hb_key",
                enabled: true,
                transforms: &[],
            })
            .await
            .unwrap();
        let router = with_auth(app(state.clone()));
        let res = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/connections/{}/materialize", c.id))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"rows":[{"resource":"entities","key":"e1"}],"fields":["name","manufacturer"],"expansion":"as_listed"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v[0]["data"]["manufacturer"], "Acme");
    }

    #[tokio::test]
    async fn browse_requires_auth() {
        let state = loopback_state();
        let c = state
            .store()
            .create_connection(crate::store::NewConnection {
                connector: "homebox",
                name: "h",
                base_url: "http://hb.lan:7745",
                public_url: None,
                credential: "hb_key",
                enabled: true,
                transforms: &[],
            })
            .await
            .unwrap();
        let router = app(state.clone()); // NO with_auth
        let res = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/connections/{}/browse", c.id))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"resource":"entities"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn browse_unknown_connection_404() {
        let state = loopback_state();
        let router = with_auth(app(state.clone()));
        let res = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/connections/does-not-exist/browse")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"resource":"entities"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn browse_bad_cursor_400() {
        let state = loopback_state();
        let c = state
            .store()
            .create_connection(crate::store::NewConnection {
                connector: "homebox",
                name: "h",
                base_url: "http://hb.lan:7745",
                public_url: None,
                credential: "hb_key",
                enabled: true,
                transforms: &[],
            })
            .await
            .unwrap();
        let router = with_auth(app(state.clone()));
        let res = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/connections/{}/browse", c.id))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"resource":"entities","cursor":"garbage.token"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn schema_with_transforms_includes_derived_fields() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let hb = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities/fields"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&hb)
            .await;
        let state = loopback_state();
        let rules = vec![crate::connector::FieldTransform {
            resource: "entities".into(),
            source: "location".into(),
            pattern: r"^(?<location_id>[^|]+?)\s*\|\s*(?<location_name>.*)$".into(),
        }];
        let c = state
            .store()
            .create_connection(crate::store::NewConnection {
                connector: "homebox",
                name: "h",
                base_url: &hb.uri(),
                public_url: None,
                credential: "hb_key",
                enabled: true,
                transforms: &rules,
            })
            .await
            .unwrap();
        let router = with_auth(app(state.clone()));
        let res = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/connections/{}/schema", c.id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let v: Value = serde_json::from_slice(&body).unwrap();
        let entities = v["resources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == "entities")
            .unwrap();
        let loc_id_col = entities["columns"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["key"] == "location_id")
            .expect("location_id column present");
        assert_eq!(loc_id_col["ty"], "text");
        assert_eq!(loc_id_col["tier"], "derived");

        let locations = v["resources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == "locations")
            .unwrap();
        assert!(!locations["columns"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["key"] == "location_id"));
    }

    #[tokio::test]
    async fn browse_with_transforms_populates_derived_cells() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let hb = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {"id":"e1","name":"Drill","parent":{"id":"loc1","name":"BOX.123 | Garage"}}
                ],
                "total": 1
            })))
            .mount(&hb)
            .await;
        let state = loopback_state();
        let rules = vec![crate::connector::FieldTransform {
            resource: "entities".into(),
            source: "location".into(),
            pattern: r"^(?<location_id>[^|]+?)\s*\|\s*(?<location_name>.*)$".into(),
        }];
        let c = state
            .store()
            .create_connection(crate::store::NewConnection {
                connector: "homebox",
                name: "h",
                base_url: &hb.uri(),
                public_url: None,
                credential: "hb_key",
                enabled: true,
                transforms: &rules,
            })
            .await
            .unwrap();
        let router = with_auth(app(state.clone()));
        let res = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/connections/{}/browse", c.id))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"resource":"entities"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let v: Value = serde_json::from_slice(&body).unwrap();
        let row = &v["rows"][0];
        assert_eq!(row["cells"]["location_id"], "BOX.123");
        assert_eq!(row["cells"]["location_name"], "Garage");
    }

    #[tokio::test]
    async fn materialize_derived_field_alone_returns_it_without_source() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let hb = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities/e1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id":"e1","name":"Drill","parent":{"id":"loc1","name":"BOX.123 | Garage"}
            })))
            .mount(&hb)
            .await;
        let state = loopback_state();
        let rules = vec![crate::connector::FieldTransform {
            resource: "entities".into(),
            source: "location".into(),
            pattern: r"^(?<location_id>[^|]+?)\s*\|\s*(?<location_name>.*)$".into(),
        }];
        let c = state
            .store()
            .create_connection(crate::store::NewConnection {
                connector: "homebox",
                name: "h",
                base_url: &hb.uri(),
                public_url: None,
                credential: "hb_key",
                enabled: true,
                transforms: &rules,
            })
            .await
            .unwrap();
        let router = with_auth(app(state.clone()));
        let res = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/connections/{}/materialize", c.id))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"rows":[{"resource":"entities","key":"e1"}],"fields":["location_id"],"expansion":"as_listed"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let v: Value = serde_json::from_slice(&body).unwrap();
        let data = &v[0]["data"];
        assert_eq!(data["location_id"], "BOX.123");
        assert!(
            data.get("location").is_none(),
            "source must not be returned when unrequested"
        );
    }

    #[tokio::test]
    async fn materialize_non_matching_row_omits_key() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let hb = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities/e1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id":"e1","name":"Drill","parent":{"id":"loc1","name":"Simple Garage"}
            })))
            .mount(&hb)
            .await;
        let state = loopback_state();
        let rules = vec![crate::connector::FieldTransform {
            resource: "entities".into(),
            source: "location".into(),
            pattern: r"^(?<location_id>BOX\.\d+)\s*\|\s*(?<location_name>.*)$".into(),
        }];
        let c = state
            .store()
            .create_connection(crate::store::NewConnection {
                connector: "homebox",
                name: "h",
                base_url: &hb.uri(),
                public_url: None,
                credential: "hb_key",
                enabled: true,
                transforms: &rules,
            })
            .await
            .unwrap();
        let router = with_auth(app(state.clone()));
        let res = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/connections/{}/materialize", c.id))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"rows":[{"resource":"entities","key":"e1"}],"fields":["location_id"],"expansion":"as_listed"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let v: Value = serde_json::from_slice(&body).unwrap();
        let data = &v[0]["data"];
        assert!(
            data.get("location_id").is_none(),
            "non-matching row must omit key entirely"
        );
    }

    #[tokio::test]
    async fn rejected_connection_save_leaves_stored_connection_untouched() {
        let state = loopback_state();
        let original_rules = vec![crate::connector::FieldTransform {
            resource: "entities".into(),
            source: "location".into(),
            pattern: r"^(?<location_id>[^|]+?)\s*\|\s*(?<location_name>.*)$".into(),
        }];
        let c = state
            .store()
            .create_connection(crate::store::NewConnection {
                connector: "homebox",
                name: "h",
                base_url: "http://hb.lan:7745",
                public_url: None,
                credential: "hb_key",
                enabled: true,
                transforms: &original_rules,
            })
            .await
            .unwrap();
        let router = with_auth(app(state.clone()));

        // PUT with invalid regex pattern
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/connections/{}", c.id))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"homebox","name":"Renamed","base_url":"http://hb.lan:7745","transforms":[{"resource":"entities","source":"location","pattern":"(?<bad>[0-9+"}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(
            body["error"]["details"]["reason"],
            "connection_transform_invalid"
        );
        assert!(body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("rule 0"));

        // Verify stored connection is unchanged
        let stored = state.store().get_connection(&c.id).await.unwrap().unwrap();
        assert_eq!(stored.name, "h");
        assert_eq!(stored.transforms, original_rules);
    }

    #[tokio::test]
    async fn create_connection_rejects_invalid_transforms() {
        let state = loopback_state();
        let router = with_auth(app(state.clone()));
        let res = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/connections")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"homebox","name":"New","base_url":"http://hb.lan:7745","credential":"key","transforms":[{"resource":"entities","source":"unknown_col","pattern":"(?<out>.*)"}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(
            body["error"]["details"]["reason"],
            "connection_transform_invalid"
        );
        assert!(body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("rule 0"));
    }

    #[tokio::test]
    async fn inert_transform_for_unsupported_resource() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let hb = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities/fields"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&hb)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities/e1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id":"e1","name":"Drill"
            })))
            .mount(&hb)
            .await;
        let state = loopback_state();
        let inert_rule = vec![crate::connector::FieldTransform {
            resource: "retired_resource".into(),
            source: "name".into(),
            pattern: r"^(?<retired_id>.*)$".into(),
        }];
        let c = state
            .store()
            .create_connection(crate::store::NewConnection {
                connector: "homebox",
                name: "h",
                base_url: &hb.uri(),
                public_url: None,
                credential: "hb_key",
                enabled: true,
                transforms: &inert_rule,
            })
            .await
            .unwrap();
        let router = with_auth(app(state.clone()));

        // Schema succeeds and does not include retired_id
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/connections/{}/schema", c.id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert!(!v["resources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == "retired_resource"));

        // Materialize succeeds
        let res = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/connections/{}/materialize", c.id))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"rows":[{"resource":"entities","key":"e1"}],"fields":["name"],"expansion":"as_listed"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn datetime_preview_returns_sample_and_rejects_bad_pattern() {
        let app = build_app();
        // valid pattern => 200 with a non-empty sample
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/datetime-formats/preview")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"pattern":"%Y"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = json_response(res).await;
        assert!(
            body["sample"].as_str().is_some_and(|s| !s.is_empty()),
            "expected a non-empty sample"
        );
        // invalid pattern (%!) => 400
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/datetime-formats/preview")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"pattern":"%!"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn print_webhook_ok_single_template_jobs_equal_copies() {
        let app = build_app();
        create_fake_printer(&app, "ok-printer", false).await;
        let payload = json!({
            "template": "brother_24mm_qr",
            "printer": "ok-printer",
            "fields": { "message": "Hello", "code": "QR-1" },
            "copies": 2
        });
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/print", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::OK);
        let body = json_response(resp).await;
        assert_eq!(body["total"], 2);
        assert_eq!(body["succeeded"], 2);
        assert_eq!(body["jobs"], 2); // single/tape template: one send per copy
    }

    #[tokio::test]
    async fn print_webhook_defaults_to_one_copy() {
        let app = build_app();
        create_fake_printer(&app, "ok-printer", false).await;
        let payload = json!({
            "template": "brother_24mm_qr",
            "printer": "ok-printer",
            "fields": { "message": "Hi", "code": "Q" }
        });
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/print", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(json_response(resp).await["total"], 1);
    }

    #[tokio::test]
    async fn print_webhook_copies_out_of_range_is_400() {
        let app = build_app();
        create_fake_printer(&app, "ok-printer", false).await;
        for bad in [0u32, 101] {
            let payload = json!({"template":"brother_24mm_qr","printer":"ok-printer","fields":{"message":"x","code":"y"},"copies":bad});
            let resp = app
                .clone()
                .oneshot(json_req("POST", "/api/print", payload.to_string()))
                .await
                .expect("request");
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "copies={bad}");
            assert_eq!(json_response(resp).await["error"]["code"], "InvalidRequest");
        }
    }

    #[tokio::test]
    async fn print_webhook_unknown_template_is_404() {
        let app = build_app();
        create_fake_printer(&app, "ok-printer", false).await;
        let payload = json!({"template":"nope","printer":"ok-printer","fields":{}});
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/print", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn print_webhook_malformed_json_is_400() {
        let app = build_app();
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/print", "{not json".to_string()))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn print_webhook_oversized_body_is_413() {
        let app = build_app();
        // > 64 KiB body via a huge field value.
        let big = "x".repeat(80 * 1024);
        let payload = json!({"template":"brother_24mm_qr","printer":"p","fields":{"message":big}});
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/print", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(
            json_response(resp).await["error"]["code"],
            "PayloadTooLarge"
        );
    }

    #[tokio::test]
    async fn api_templates_detail_exposes_params_schema() {
        let app = build_app();
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/templates/brother_18mm")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(res.status(), StatusCode::OK);
        let json = json_response(res).await;
        assert!(json.get("params").is_some());
    }

    #[tokio::test]
    async fn api_print_accepts_data_or_fields() {
        let app = build_app();
        create_fake_printer(&app, "ok-printer", false).await;
        let payload = json!({
            "template": "brother_18mm",
            "printer": "ok-printer",
            "data": {
                "message": "Printed via data",
                "target_width": 70
            }
        });
        let res = app
            .clone()
            .oneshot(json_req("POST", "/api/print", payload.to_string()))
            .await
            .expect("request");
        assert_ne!(res.status(), StatusCode::BAD_REQUEST);
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn api_print_data_precedes_fields() {
        let app = build_app();
        create_fake_printer(&app, "ok-printer", false).await;
        let payload = json!({
            "template": "brother_18mm",
            "printer": "ok-printer",
            "data": {
                "message": "From data"
            },
            "fields": {
                "message": "From fields"
            }
        });
        let res = app
            .clone()
            .oneshot(json_req("POST", "/api/print", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[test]
    fn openapi_schema_contains_param_types() {
        use utoipa::OpenApi;
        let doc = crate::openapi::ApiDoc::openapi();
        let schemas = doc.components.as_ref().unwrap().schemas.clone();
        assert!(
            schemas.contains_key("ParamSpec"),
            "ParamSpec missing in openapi schemas"
        );
        assert!(
            schemas.contains_key("ParamType"),
            "ParamType missing in openapi schemas"
        );
        assert!(
            schemas.contains_key("ParamValue"),
            "ParamValue missing in openapi schemas"
        );
    }

    // Verify the API-wide behavior: oversized bodies on non-/print JSON endpoints also return 413.
    // axum's global DefaultBodyLimit (~2 MiB) triggers the same JsonRejection->PayloadTooLarge path.
    #[tokio::test]
    async fn batch_oversized_body_is_413() {
        let app = build_app();
        // ~2.1 MiB body; exceeds the global ~2 MiB DefaultBodyLimit.
        let big = "x".repeat(2 * 1024 * 1024 + 100 * 1024);
        let payload = json!({"labels":[{"template":"brother_24mm_qr","data":{"message":big}}]});
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/batch", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(
            json_response(resp).await["error"]["code"],
            "PayloadTooLarge"
        );
    }

    #[tokio::test]
    async fn printer_password_is_redacted_in_responses() {
        let app = build_app();
        let create = json!({
            "id": "sec", "name": "Sec", "kind": "cups",
            "config": { "uri": "ipps://h/q", "username": "u", "password": "s3cret" }
        });
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/printers", create.to_string()))
            .await
            .expect("req");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = json_response(resp).await;
        assert!(
            body["config"].get("password").is_none(),
            "create response must omit password"
        );
        assert_eq!(body["config"]["username"], "u");

        let g = app
            .clone()
            .oneshot(json_req("GET", "/api/printers/sec", String::new()))
            .await
            .expect("req");
        let gb = json_response(g).await;
        assert!(
            gb["config"].get("password").is_none(),
            "GET must omit password"
        );
        assert_eq!(gb["config"]["username"], "u");

        // list must redact too
        let l = app
            .clone()
            .oneshot(json_req("GET", "/api/printers", String::new()))
            .await
            .expect("req");
        let lb = json_response(l).await;
        let entry = lb
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == "sec")
            .expect("listed");
        assert!(
            entry["config"].get("password").is_none(),
            "list must omit password"
        );

        // PUT omitting password must succeed (keep); response still omits password.
        let upd = json!({ "id": "sec", "name": "Sec2", "kind": "cups", "config": { "uri": "ipps://h/q", "username": "u" } });
        let p = app
            .clone()
            .oneshot(json_req("PUT", "/api/printers/sec", upd.to_string()))
            .await
            .expect("req");
        assert_eq!(p.status(), StatusCode::OK);
        assert!(json_response(p).await["config"].get("password").is_none());
    }

    // End-to-end guard on the security-critical merge->upsert wiring: the API never echoes the
    // password, so a regression (redact-before-upsert, or merging the wrong object) would silently
    // wipe the STORED secret without any response-body test noticing. Read the store directly.
    #[tokio::test]
    async fn printer_password_persists_across_update_and_clears_on_null() {
        let (app, state) = build_app_with_state();

        let create = json!({
            "id": "persist", "name": "Persist", "kind": "cups",
            "config": { "uri": "ipps://h/q", "username": "u", "password": "s3cret" }
        });
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/printers", create.to_string()))
            .await
            .expect("req");
        assert_eq!(resp.status(), StatusCode::CREATED);

        // PUT omitting password (change only the name): the stored secret must be KEPT.
        let upd = json!({ "id": "persist", "name": "Renamed", "kind": "cups", "config": { "uri": "ipps://h/q", "username": "u" } });
        let p = app
            .clone()
            .oneshot(json_req("PUT", "/api/printers/persist", upd.to_string()))
            .await
            .expect("req");
        assert_eq!(p.status(), StatusCode::OK);

        let stored = state
            .store()
            .get_printer("persist")
            .await
            .expect("store read")
            .expect("printer exists");
        assert_eq!(stored.name, "Renamed");
        assert_eq!(
            stored.config["password"], "s3cret",
            "password must persist across a password-omitting update"
        );

        // PUT with password: null: the stored secret must be CLEARED.
        let clr = json!({ "id": "persist", "name": "Renamed", "kind": "cups", "config": { "uri": "ipps://h/q", "username": "u", "password": null } });
        let c = app
            .clone()
            .oneshot(json_req("PUT", "/api/printers/persist", clr.to_string()))
            .await
            .expect("req");
        assert_eq!(c.status(), StatusCode::OK);

        let stored = state
            .store()
            .get_printer("persist")
            .await
            .expect("store read")
            .expect("printer exists");
        assert!(
            stored.config.get("password").is_none(),
            "explicit null must clear the stored password"
        );
    }

    #[tokio::test]
    async fn render_bilevel_png_is_pure_black_white() {
        let app = build_app();
        let body =
            json!({ "template": "brother_24mm_qr", "data": { "message": "Hi", "code": "Q" } });
        // bilevel: every pixel pure black or white
        let resp = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png&color_mode=bilevel",
                body.to_string(),
            ))
            .await
            .expect("req");
        assert_eq!(resp.status(), StatusCode::OK);
        let png = bytes_response(resp).await;
        let img = image::load_from_memory(&png).expect("decode").to_rgba8();
        assert!(
            img.pixels().all(|p| {
                let (r, g, b) = (p[0], p[1], p[2]);
                (r, g, b) == (0, 0, 0) || (r, g, b) == (255, 255, 255)
            }),
            "bilevel output must be pure B/W"
        );

        // default (color) render of the same template HAS intermediate grays (anti-aliasing)
        let resp2 = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png",
                body.to_string(),
            ))
            .await
            .expect("req");
        let png2 = bytes_response(resp2).await;
        let img2 = image::load_from_memory(&png2).expect("decode").to_rgba8();
        assert!(
            img2.pixels().any(|p| {
                let (r, g, b) = (p[0], p[1], p[2]);
                (r, g, b) != (0, 0, 0) && (r, g, b) != (255, 255, 255)
            }),
            "color render should contain anti-aliased grays (proves bilevel changed something)"
        );
    }

    #[tokio::test]
    async fn render_bilevel_rejects_pdf_and_bad_params() {
        let app = build_app();
        let body =
            json!({ "template": "brother_24mm_qr", "data": { "message": "Hi", "code": "Q" } });
        for q in [
            "format=pdf&color_mode=bilevel",
            "color_mode=bogus",
            "resolution=99999",
            "resolution=abc",
        ] {
            let resp = app
                .clone()
                .oneshot(json_req(
                    "POST",
                    &format!("/api/render/label?{q}"),
                    body.to_string(),
                ))
                .await
                .expect("req");
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "query: {q}");
            assert_eq!(
                json_response(resp).await["error"]["code"],
                "InvalidRequest",
                "query: {q}"
            );
        }
        // valid resolution override succeeds
        let ok = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png&color_mode=bilevel&resolution=203",
                body.to_string(),
            ))
            .await
            .expect("req");
        assert_eq!(ok.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn print_render_profile_precedence() {
        let app = build_app();
        async fn print_ok(app: &axum::Router, id: &str, cfg: serde_json::Value) {
            let body = json!({ "id": id, "name": id, "kind": "fake", "config": cfg }).to_string();
            let c = app
                .clone()
                .oneshot(json_req("POST", "/api/printers", body))
                .await
                .expect("req");
            assert_eq!(c.status(), StatusCode::CREATED, "create {id}");
            let payload = json!({
                "template": "brother_24mm_qr",
                "mode": "print",
                "printer": id,
                "labels": [ { "data": { "message": "Hi", "code": "Q" } } ]
            });
            let resp = app
                .clone()
                .oneshot(json_req("POST", "/api/batch", payload.to_string()))
                .await
                .expect("req");
            assert_eq!(resp.status(), StatusCode::OK, "print {id}");
            assert_eq!(json_response(resp).await["succeeded"], 1, "succeeded {id}");
        }
        // 1. no config.render + caps bilevel+png -> negotiated bilevel -> PNG
        print_ok(
            &app,
            "neg",
            json!({ "fail": false, "capabilities": { "bilevel": true, "accepts_png": true, "resolution": 203 } }),
        )
        .await;
        // 2. config color + caps bilevel -> configured wins -> PDF (color/pdf)
        print_ok(
            &app,
            "sup",
            json!({ "fail": false, "render": { "color_mode": "color" }, "capabilities": { "bilevel": true, "accepts_png": true } }),
        )
        .await;
        // 3. no config + no caps -> default Color -> PDF
        print_ok(&app, "def", json!({ "fail": false })).await;
        // 4. config bilevel + no caps -> configured bilevel -> PNG
        print_ok(
            &app,
            "cfg",
            json!({ "fail": false, "render": { "color_mode": "bilevel" } }),
        )
        .await;
    }

    #[tokio::test]
    async fn print_media_gate() {
        let app = build_app();

        async fn mk(app: &axum::Router, id: &str, caps: serde_json::Value) {
            let body = json!({
                "id": id, "name": id, "kind": "fake",
                "config": { "fail": false, "capabilities": caps }
            })
            .to_string();
            let c = app
                .clone()
                .oneshot(json_req("POST", "/api/printers", body))
                .await
                .expect("req");
            assert_eq!(c.status(), StatusCode::CREATED);
        }

        // Superset of fields: covers brother_24mm (message) and homebox-qr (id, message).
        async fn print_resp(
            app: &axum::Router,
            template: &str,
            printer: &str,
        ) -> axum::response::Response {
            let data = json!({
                "message": "Hi", "code": "Q", "id": "A1",
                "url": "https://x/A1", "name": "N", "tags": "T", "description": "D"
            });
            let payload = json!({
                "template": template,
                "mode": "print",
                "printer": printer,
                "labels": [{ "data": data }]
            });
            app.clone()
                .oneshot(json_req("POST", "/api/batch", payload.to_string()))
                .await
                .expect("req")
        }

        // 1. brother_24mm (media_width 24) + loaded 12mm -> mismatch -> 409 MediaMismatch
        mk(&app, "wrong", json!({ "loaded_media_width": 12 })).await;
        let r = print_resp(&app, "brother_24mm", "wrong").await;
        assert_eq!(r.status(), StatusCode::CONFLICT);
        assert_eq!(json_response(r).await["error"]["code"], "MediaMismatch");

        // 2. brother_24mm + loaded 24mm -> match -> 200
        mk(&app, "match", json!({ "loaded_media_width": 24 })).await;
        assert_eq!(
            print_resp(&app, "brother_24mm", "match").await.status(),
            StatusCode::OK
        );

        // 3. loaded width unknown -> gate inert -> 200
        mk(&app, "unknown", json!({})).await;
        assert_eq!(
            print_resp(&app, "brother_24mm", "unknown").await.status(),
            StatusCode::OK
        );

        // 4. homebox-qr (no media_width) + loaded 12mm -> gate inert -> 200
        // Seed the vars.qr_base_url variable that homebox-qr requires.
        let seed = app
            .clone()
            .oneshot(json_req(
                "PUT",
                "/api/variables/qr_base_url",
                json!({ "value": "https://lab.example/items" }).to_string(),
            ))
            .await
            .expect("seed var");
        assert_eq!(seed.status(), StatusCode::OK);
        mk(&app, "nomw", json!({ "loaded_media_width": 12 })).await;
        assert_eq!(
            print_resp(&app, "homebox-qr", "nomw").await.status(),
            StatusCode::OK
        );
    }
}

#[cfg(test)]
mod auth_http_tests {
    use super::store::Store;
    use super::{app, AppState};
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use serde_json::Value;
    use std::sync::Arc;
    use tower::ServiceExt;

    fn test_app() -> axum::Router {
        let (templates, templates_dir) = crate::templates::load_all_for_tests();
        let store = Store::open_in_memory().expect("store");
        app(Arc::new(AppState::new(templates, templates_dir, store)))
    }

    fn test_app_no_auth() -> axum::Router {
        let (templates, templates_dir) = crate::templates::load_all_for_tests();
        let store = Store::open_in_memory().expect("store");
        app(Arc::new(
            AppState::new(templates, templates_dir, store).with_no_auth(true),
        ))
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

    #[tokio::test]
    async fn auth_me_is_no_store() {
        let app = test_app();
        let res = app.oneshot(req_get("/api/auth/me")).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            res.headers()
                .get("cache-control")
                .map(|v| v.to_str().unwrap()),
            Some("no-store"),
            "auth/me must be no-store so browser/proxy never serve stale auth state"
        );
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

    fn req_put_json_cookie(uri: &str, body: &str, cookie: &str) -> Request<Body> {
        Request::builder()
            .method("PUT")
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

    fn req_put_json(uri: &str, body: &str) -> Request<Body> {
        Request::builder()
            .method("PUT")
            .uri(uri)
            .header("content-type", "application/json")
            .header("host", "localhost")
            .header("origin", "http://localhost")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    fn req_delete(uri: &str) -> Request<Body> {
        Request::builder()
            .method("DELETE")
            .uri(uri)
            .header("host", "localhost")
            .header("origin", "http://localhost")
            .body(Body::empty())
            .unwrap()
    }

    /// A minimal valid `fake`-kind printer body (mirrors `create_fake_printer`).
    fn default_test_printer_json(id: &str, is_default: bool) -> String {
        serde_json::json!({
            "id": id,
            "name": id,
            "kind": "fake",
            "config": { "fail": false },
            "is_default": is_default,
        })
        .to_string()
    }

    /// Fetch `/api/printers` and return the `is_default` flag for `id` (panics if absent).
    async fn printer_is_default(app: &axum::Router, id: &str) -> bool {
        let res = app.clone().oneshot(req_get("/api/printers")).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let list = body_json(res).await;
        list.as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == id)
            .unwrap_or_else(|| panic!("printer {id} not found in list"))["is_default"]
            .as_bool()
            .unwrap()
    }

    #[tokio::test]
    async fn setting_default_is_exclusive_and_guarded() {
        let app = test_app_no_auth();
        for id in ["p1", "p2"] {
            let res = app
                .clone()
                .oneshot(req_post_json(
                    "/api/printers",
                    &default_test_printer_json(id, false),
                ))
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::CREATED);
        }

        // Set p1 default.
        let res = app
            .clone()
            .oneshot(req_post_json("/api/printers/p1/default", ""))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        assert!(printer_is_default(&app, "p1").await);
        assert!(!printer_is_default(&app, "p2").await);

        // Set p2 default -> exclusive (p1 cleared).
        let res = app
            .clone()
            .oneshot(req_post_json("/api/printers/p2/default", ""))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        assert!(printer_is_default(&app, "p2").await);
        assert!(!printer_is_default(&app, "p1").await);

        // Unknown id -> 404 AND p2 still default (rollback, not clear-then-fail).
        let res = app
            .clone()
            .oneshot(req_post_json("/api/printers/nope/default", ""))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert!(printer_is_default(&app, "p2").await);

        // Clear p2 -> zero defaults.
        let res = app
            .clone()
            .oneshot(req_delete("/api/printers/p2/default"))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        assert!(!printer_is_default(&app, "p1").await);
        assert!(!printer_is_default(&app, "p2").await);
    }

    #[tokio::test]
    async fn create_and_replace_never_set_default() {
        let app = test_app_no_auth();

        // Create ignores incoming is_default:true (response AND stored value are false).
        let res = app
            .clone()
            .oneshot(req_post_json(
                "/api/printers",
                &default_test_printer_json("p1", true),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        assert_eq!(body_json(res).await["is_default"], false);
        assert!(!printer_is_default(&app, "p1").await);

        // Make it the default via the endpoint.
        let res = app
            .clone()
            .oneshot(req_post_json("/api/printers/p1/default", ""))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        assert!(printer_is_default(&app, "p1").await);

        // Replace with is_default:false in the body -> stored value preserved (still true).
        let res = app
            .clone()
            .oneshot(req_put_json(
                "/api/printers/p1",
                &default_test_printer_json("p1", false),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(body_json(res).await["is_default"], true);
        assert!(printer_is_default(&app, "p1").await);
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
    async fn print_webhook_requires_auth() {
        let app = test_app();
        let payload =
            serde_json::json!({"template":"brother_24mm_qr","printer":"ok-printer","fields":{}});
        let resp = app
            .clone()
            .oneshot(req_post_json("/api/print", &payload.to_string()))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
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
    async fn setup_rejects_empty_password_but_allows_short() {
        // empty password is rejected
        let app = test_app();
        let res = app
            .oneshot(req_post_json(
                "/api/auth/setup",
                r#"{"username":"a","password":""}"#,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);

        // a short (non-empty) password is now accepted (no 8-char floor)
        let app = test_app();
        let res = app
            .oneshot(req_post_json(
                "/api/auth/setup",
                r#"{"username":"a","password":"x"}"#,
            ))
            .await
            .unwrap();
        assert_ne!(res.status(), StatusCode::BAD_REQUEST);
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
    async fn delete_own_account_conflicts() {
        let app = test_app();
        let cookie = setup_login_cookie(&app).await;
        // add a second user so the last-user guard does not fire; the self-delete guard must be what 409s
        app.clone()
            .oneshot(req_post_json_cookie(
                "/api/users",
                r#"{"username":"b","password":"pw123456"}"#,
                &cookie,
            ))
            .await
            .unwrap();
        // resolve my own id via /auth/me, then try to delete it
        let res = app
            .clone()
            .oneshot(req_get_cookie("/api/auth/me", &cookie))
            .await
            .unwrap();
        let me = body_json(res).await;
        let my_id = me["me"]["id"].as_str().unwrap().to_string();
        let res = app
            .clone()
            .oneshot(req_delete_cookie(&format!("/api/users/{my_id}"), &cookie))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CONFLICT);
        let body = body_json(res).await;
        assert_eq!(body["error"]["message"], "cannot delete your own account");
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

    #[tokio::test]
    async fn settings_resolve_override_and_reset() {
        let app = test_app();
        let cookie = setup_login_cookie(&app).await;

        // GET shows the in-code default, flagged is_default
        let res = app
            .clone()
            .oneshot(req_get_cookie("/api/settings", &cookie))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_json(res).await;
        assert_eq!(body["job_log_retention_days"]["value"], 90);
        assert_eq!(body["job_log_retention_days"]["is_default"], true);
        assert_eq!(body["max_label_dimension_mm"]["value"], 1000.0);
        assert_eq!(body["max_label_dimension_mm"]["is_default"], true);

        // PUT an override
        let res = app
            .clone()
            .oneshot(req_put_json_cookie(
                "/api/settings/job_log_retention_days",
                r#"{"value":30}"#,
                &cookie,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_json(res).await;
        assert_eq!(body["value"], 30);
        assert_eq!(body["is_default"], false);

        let res = app
            .clone()
            .oneshot(req_put_json_cookie(
                "/api/settings/max_label_dimension_mm",
                r#"{"value":500.5}"#,
                &cookie,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_json(res).await;
        assert_eq!(body["value"], 500.5);
        assert_eq!(body["is_default"], false);

        // GET now reflects the override
        let res = app
            .clone()
            .oneshot(req_get_cookie("/api/settings", &cookie))
            .await
            .unwrap();
        let body = body_json(res).await;
        assert_eq!(body["job_log_retention_days"]["value"], 30);
        assert_eq!(body["job_log_retention_days"]["is_default"], false);
        assert_eq!(body["max_label_dimension_mm"]["value"], 500.5);
        assert_eq!(body["max_label_dimension_mm"]["is_default"], false);

        // DELETE resets to default and is 204
        let res = app
            .clone()
            .oneshot(req_delete_cookie(
                "/api/settings/job_log_retention_days",
                &cookie,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        let res = app
            .clone()
            .oneshot(req_get_cookie("/api/settings", &cookie))
            .await
            .unwrap();
        let body = body_json(res).await;
        assert_eq!(body["job_log_retention_days"]["is_default"], true);

        // DELETE again is still 204 (idempotent, registry-keyed)
        let res = app
            .clone()
            .oneshot(req_delete_cookie(
                "/api/settings/job_log_retention_days",
                &cookie,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn settings_reject_bad_value_and_unknown_key() {
        let app = test_app();
        let cookie = setup_login_cookie(&app).await;

        // float, string, negative all 400
        for bad in [r#"{"value":90.0}"#, r#"{"value":"90"}"#, r#"{"value":-1}"#] {
            let res = app
                .clone()
                .oneshot(req_put_json_cookie(
                    "/api/settings/job_log_retention_days",
                    bad,
                    &cookie,
                ))
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::BAD_REQUEST, "bad value {bad}");
            assert_eq!(body_json(res).await["error"]["code"], "InvalidRequest");
        }

        // unknown key on PUT and DELETE is 404 SettingNotFound
        let res = app
            .clone()
            .oneshot(req_put_json_cookie(
                "/api/settings/nope",
                r#"{"value":1}"#,
                &cookie,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert_eq!(body_json(res).await["error"]["code"], "SettingNotFound");

        let res = app
            .clone()
            .oneshot(req_delete_cookie("/api/settings/nope", &cookie))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert_eq!(body_json(res).await["error"]["code"], "SettingNotFound");
    }

    #[tokio::test]
    async fn no_auth_opens_protected_data_route() {
        // default mode: a protected route without credentials is 401
        let app = test_app();
        let res = app.oneshot(req_get("/api/variables")).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        // no-auth mode: the same route is open
        let app = test_app_no_auth();
        let res = app.oneshot(req_get("/api/variables")).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn no_auth_disables_credential_management() {
        // every POST on the credential surface (including login/logout) is 403
        let posts = [
            ("/api/auth/setup", r#"{"username":"a","password":"x"}"#),
            ("/api/auth/login", r#"{"username":"a","password":"x"}"#),
            ("/api/auth/logout", r#"{}"#),
            (
                "/api/auth/password",
                r#"{"current_password":"a","new_password":"b"}"#,
            ),
            ("/api/users", r#"{"username":"a","password":"x"}"#),
            ("/api/tokens", r#"{"name":"t"}"#),
        ];
        for (path, body) in posts {
            let app = test_app_no_auth();
            let res = app.oneshot(req_post_json(path, body)).await.unwrap();
            assert_eq!(res.status(), StatusCode::FORBIDDEN, "POST {path}");
            assert_eq!(body_json(res).await["error"]["code"], "Forbidden");
        }
        // GET reads of the credential surface are also blocked
        for path in ["/api/users", "/api/tokens"] {
            let app = test_app_no_auth();
            let res = app.oneshot(req_get(path)).await.unwrap();
            assert_eq!(res.status(), StatusCode::FORBIDDEN, "GET {path}");
        }
        // DELETE on the credential sub-paths is blocked (no cookie needed: is_auth_managed fires first)
        for path in ["/api/users/someid", "/api/tokens/someid"] {
            let app = test_app_no_auth();
            let req = Request::builder()
                .method("DELETE")
                .uri(path)
                .body(Body::empty())
                .unwrap();
            let res = app.oneshot(req).await.unwrap();
            assert_eq!(res.status(), StatusCode::FORBIDDEN, "DELETE {path}");
        }
    }

    #[tokio::test]
    async fn no_auth_me_reports_local_even_with_stale_cookie() {
        // no cookie
        let app = test_app_no_auth();
        let res = app.oneshot(req_get("/api/auth/me")).await.unwrap();
        let body = body_json(res).await;
        assert_eq!(body["authed"], true);
        assert_eq!(body["needsSetup"], false);
        assert_eq!(body["me"]["id"], "local");
        assert_eq!(body["noAuth"], true);
        // a stale/bogus cookie must NOT change the reported identity (branch runs before resolve_optional)
        let app = test_app_no_auth();
        let res = app
            .oneshot(req_get_cookie("/api/auth/me", "labeler_session=bogus"))
            .await
            .unwrap();
        let body = body_json(res).await;
        assert_eq!(body["me"]["id"], "local");
        assert_eq!(body["noAuth"], true);
    }

    /// A second user's session cookie (created via A's cookie), for per-user isolation checks.
    async fn login_second_user(app: &axum::Router, cookie_a: &str, username: &str) -> String {
        let res = app
            .clone()
            .oneshot(req_post_json_cookie(
                "/api/users",
                &format!(r#"{{"username":"{username}","password":"pw123456"}}"#),
                cookie_a,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let res = app
            .clone()
            .oneshot(req_post_json(
                "/api/auth/login",
                &format!(r#"{{"username":"{username}","password":"pw123456"}}"#),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        cookie_from(&res)
    }

    #[tokio::test]
    async fn favorites_crud_and_per_user_isolation() {
        let app = test_app();
        let cookie = setup_login_cookie(&app).await;

        // PUT a favorite -> 204, then GET lists it
        let res = app
            .clone()
            .oneshot(req_put_json_cookie(
                "/api/favorites/brother_24mm_qr",
                "{}",
                &cookie,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        let res = app
            .clone()
            .oneshot(req_get_cookie("/api/favorites", &cookie))
            .await
            .unwrap();
        assert_eq!(body_json(res).await, serde_json::json!(["brother_24mm_qr"]));

        // PUT again is idempotent (still exactly one)
        let res = app
            .clone()
            .oneshot(req_put_json_cookie(
                "/api/favorites/brother_24mm_qr",
                "{}",
                &cookie,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        let res = app
            .clone()
            .oneshot(req_get_cookie("/api/favorites", &cookie))
            .await
            .unwrap();
        assert_eq!(body_json(res).await, serde_json::json!(["brother_24mm_qr"]));

        // PUT an unknown template -> 404
        let res = app
            .clone()
            .oneshot(req_put_json_cookie("/api/favorites/nope", "{}", &cookie))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);

        // user B sees an empty list (per-user isolation)
        let cookie_b = login_second_user(&app, &cookie, "bob").await;
        let res = app
            .clone()
            .oneshot(req_get_cookie("/api/favorites", &cookie_b))
            .await
            .unwrap();
        assert_eq!(body_json(res).await, serde_json::json!([]));

        // DELETE as A -> 204, then GET empty
        let res = app
            .clone()
            .oneshot(req_delete_cookie("/api/favorites/brother_24mm_qr", &cookie))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        let res = app
            .clone()
            .oneshot(req_get_cookie("/api/favorites", &cookie))
            .await
            .unwrap();
        assert_eq!(body_json(res).await, serde_json::json!([]));
    }

    #[tokio::test]
    async fn recents_are_recorded_with_local_actor() {
        // no-auth app: the print actor is "local", so recents are visible to the local caller.
        let app = test_app_no_auth();
        let res = app
            .clone()
            .oneshot(req_post_json(
                "/api/printers",
                r#"{"id":"ok-printer","name":"ok-printer","kind":"fake","config":{"fail":false}}"#,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);

        // recents empty before any print
        let res = app
            .clone()
            .oneshot(req_get("/api/recent-templates"))
            .await
            .unwrap();
        assert_eq!(body_json(res).await, serde_json::json!([]));

        // print one label
        let res = app
            .clone()
            .oneshot(req_post_json(
                "/api/print",
                r#"{"template":"brother_24mm_qr","printer":"ok-printer","fields":{"message":"x","code":"y"},"copies":1}"#,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // the print is now attributed to "local" and surfaces in recents
        let res = app
            .clone()
            .oneshot(req_get("/api/recent-templates"))
            .await
            .unwrap();
        assert_eq!(body_json(res).await, serde_json::json!(["brother_24mm_qr"]));
        // limit clamps to at least one result
        let res = app
            .clone()
            .oneshot(req_get("/api/recent-templates?limit=0"))
            .await
            .unwrap();
        assert_eq!(body_json(res).await, serde_json::json!(["brother_24mm_qr"]));
    }

    #[tokio::test]
    async fn no_auth_relaxed_origin_check() {
        // state-changing request with NO Origin succeeds (non-browser caller)
        let app = test_app_no_auth();
        let req = Request::builder()
            .method("POST")
            .uri("/api/templates/reload")
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        // same request with a MISMATCHED Origin is rejected
        let app = test_app_no_auth();
        let req = Request::builder()
            .method("POST")
            .uri("/api/templates/reload")
            .header("host", "localhost")
            .header("origin", "http://evil.example")
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }
}
