pub mod api;
pub mod auth;
pub mod batch;
pub mod connector;
mod convert;
pub mod datetime_fmt;
pub mod driver;
pub mod egress;
pub mod errors;
pub mod extract;
pub mod fs_safe;
pub mod interpolation;
pub mod middleware;
pub mod models;
pub mod openapi;
pub mod parse;
pub mod raw;
pub mod reason;
pub mod render;
pub mod resolver;
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
    use rustix::fd::AsFd;
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

        // Set to new public_url. Sent with a trailing slash, so the update path is what proves the
        // normalization, not the create path or the shared helper.
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/connections/{id}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"homebox","name":"home","base_url":"http://hb.lan:7745","public_url":"https://hb2.example.com/"}"#,
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
            (
                "PUT",
                Body::from(
                    r#"{"connector":"mismatched","name":"x","base_url":"http://hb.lan:7745"}"#,
                ),
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

    /// A connection's connector is fixed at creation: an update naming a different one is rejected
    /// with 400 and reason connector_immutable (#197).
    #[tokio::test]
    async fn update_connection_rejects_mismatched_connector() {
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
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
        assert_eq!(body["error"]["details"]["reason"], "connector_immutable");
    }

    /// Updating a connection sending the stored connector returns 200 and updates fields (#197).
    #[tokio::test]
    async fn update_connection_with_matching_connector_succeeds() {
        let app = build_app();
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/connections")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"homebox","name":"old-name","base_url":"http://hb.lan:7745","credential":"secret"}"#,
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
                        r#"{"connector":"homebox","name":"new-name","base_url":"http://hb-updated.lan:7745"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = json_response(res).await;
        assert_eq!(body["connector"], "homebox");
        assert_eq!(body["name"], "new-name");
        assert_eq!(body["base_url"], "http://hb-updated.lan:7745");
    }

    /// A rejected PUT with a mismatched connector changes nothing in the stored connection (#197).
    #[tokio::test]
    async fn update_connection_rejected_mismatched_connector_leaves_state_unchanged() {
        let app = build_app();
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/connections")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"connector":"homebox","name":"orig-name","base_url":"http://hb.lan:7745","public_url":"http://pub.lan","credential":"secret","enabled":true}"#,
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
                        r#"{"connector":"other-connector","name":"mutated-name","base_url":"http://other.lan:7745","public_url":"http://mutated-pub.lan","enabled":false}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);

        // Read it back: all fields must remain as originally created
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
        let body = json_response(res).await;
        assert_eq!(body["connector"], "homebox");
        assert_eq!(body["name"], "orig-name");
        assert_eq!(body["base_url"], "http://hb.lan:7745");
        assert_eq!(body["public_url"], "http://pub.lan");
        assert_eq!(body["enabled"], true);
    }

    /// A connector mismatch outranks every field the update itself validates: the check runs before
    /// URL and transform validation, so the client is told which connection it is editing before it
    /// is told which field is malformed. It cannot outrank deserialization, which happens before the
    /// handler runs; the test below pins that boundary (#197).
    #[tokio::test]
    async fn update_connection_connector_mismatch_outranks_other_invalid_fields() {
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

        for (field, payload) in [
            (
                "base_url",
                r#"{"connector":"mismatched-connector","name":"home","base_url":"not a url"}"#,
            ),
            (
                "public_url",
                r#"{"connector":"mismatched-connector","name":"home","base_url":"http://hb.lan:7745","public_url":"ftp://nope"}"#,
            ),
            (
                "transforms",
                r#"{"connector":"mismatched-connector","name":"home","base_url":"http://hb.lan:7745","transforms":[{"resource":"nope","source":"x","pattern":"y"}]}"#,
            ),
        ] {
            let res = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri(format!("/api/connections/{id}"))
                        .header("content-type", "application/json")
                        .body(Body::from(payload))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::BAD_REQUEST, "bad {field}");
            let body = json_response(res).await;
            assert_eq!(body["error"]["code"], "InvalidRequest", "bad {field}");
            assert_eq!(
                body["error"]["details"]["reason"], "connector_immutable",
                "bad {field}"
            );
        }
    }

    /// A body that never deserializes is rejected by the request layer, before the handler and so
    /// before the `connector` comparison, which cannot precede reading the payload that carries it.
    /// What that rejection reports is the request layer's own contract, not this one's: since #225
    /// the crate's `Json<T>` extractor maps every deserialization failure to `400 InvalidRequest`
    /// with `json_malformed` (ADR-0075). Out of scope for #197; what matters here is only that the
    /// rejection is not `connector_immutable` (#197).
    #[tokio::test]
    async fn update_connection_undeserializable_body_is_rejected_before_the_connector_check() {
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

        for (case, expected, payload) in [
            (
                "not json",
                StatusCode::BAD_REQUEST,
                r#"{"connector":"nope","#,
            ),
            (
                "connector of the wrong type",
                StatusCode::BAD_REQUEST,
                r#"{"connector":42,"name":"home","base_url":"http://hb.lan:7745"}"#,
            ),
            (
                "required key missing",
                StatusCode::BAD_REQUEST,
                r#"{"connector":"nope","base_url":"http://hb.lan:7745"}"#,
            ),
        ] {
            let res = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri(format!("/api/connections/{id}"))
                        .header("content-type", "application/json")
                        .body(Body::from(payload))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(res.status(), expected, "{case}");
            let body = String::from_utf8(bytes_response(res).await).expect("utf-8 body");
            assert!(
                !body.contains("connector_immutable"),
                "{case}: rejected before the connector check, got {body}"
            );
        }
    }

    /// The comparison is byte equality, not a case-insensitive one: `ConnectorRegistry::get` matches
    /// ids literally, so a connector differing only in case is a different connector (#197).
    #[tokio::test]
    async fn update_connection_rejects_a_connector_differing_only_in_case() {
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
                        r#"{"connector":"Homebox","name":"home","base_url":"http://hb.lan:7745"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(body["error"]["details"]["reason"], "connector_immutable");
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
        assert!(
            body.contains("name: Brother 24mm Continuous Label (QR + text)"),
            "body: {body}"
        );
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

    /// #209: the datetime parameter's HTTP contract. A render-level test cannot see the status
    /// code or `details.reason`, and those are what a caller switches on.
    #[tokio::test]
    async fn render_label_datetime_param_defaults_and_overrides() {
        for data in [
            json!({ "message": "Hi" }),
            json!({ "message": "Hi", "printed_on": "" }),
            json!({ "message": "Hi", "printed_on": null }),
        ] {
            let payload = json!({ "template": "brother_24mm_printed_on", "data": data });
            let response = build_app()
                .oneshot(json_req(
                    "POST",
                    "/api/render/label?format=png",
                    payload.to_string(),
                ))
                .await
                .expect("request");
            assert_eq!(
                response.status(),
                StatusCode::UNPROCESSABLE_ENTITY,
                "{data} should fail with 422 MissingField"
            );
            let body = json_response(response).await;
            assert_eq!(body["error"]["code"], "MissingField");
            assert_eq!(body["error"]["details"]["field"], "printed_on");
        }

        for data in [
            json!({ "message": "Hi", "printed_on": "2026-08-19" }),
            json!({ "message": "Hi", "printed_on": "2026-08-19T14:30" }),
            json!({ "message": "Hi", "printed_on": "2026-08-19T14:30:00" }),
            json!({ "message": "Hi", "printed_on": "2026-08-19T23:15:00+02:00" }),
            json!({ "message": "Hi", "printed_on": "2026-08-19T23:15:00Z" }),
        ] {
            let payload = json!({ "template": "brother_24mm_printed_on", "data": data });
            let response = build_app()
                .oneshot(json_req(
                    "POST",
                    "/api/render/label?format=png",
                    payload.to_string(),
                ))
                .await
                .expect("request");
            assert_eq!(
                response.status(),
                StatusCode::OK,
                "{data} should render; got {:?}",
                json_response(response).await
            );
        }
    }

    #[tokio::test]
    async fn render_label_datetime_param_rejects_unparseable_values() {
        for bad in [
            json!("yesterday"),
            json!("19-08-2026"),
            json!("2026-02-30"),
            json!(20260819),
            json!(true),
            json!(["2026-08-19"]),
        ] {
            let payload = json!({
                "template": "brother_24mm_printed_on",
                "data": { "message": "Hi", "printed_on": bad }
            });
            let response = build_app()
                .oneshot(json_req(
                    "POST",
                    "/api/render/label?format=png",
                    payload.to_string(),
                ))
                .await
                .expect("request");
            assert_eq!(
                response.status(),
                StatusCode::BAD_REQUEST,
                "{bad} should be refused"
            );
            let body = json_response(response).await;
            assert_eq!(body["error"]["code"], "InvalidRequest");
            assert_eq!(body["error"]["details"]["reason"], "datetime_param_invalid");
            assert!(
                body["error"]["message"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("printed_on"),
                "message should name the parameter: {body}"
            );
        }
    }

    /// A batch is all-or-nothing: the bad label is named by index and no ZIP comes back.
    #[tokio::test]
    async fn batch_datetime_param_failure_names_its_label_and_returns_no_artifact() {
        let payload = json!({
            "template": "brother_24mm_printed_on",
            "mode": "download",
            "labels": [
                { "data": { "message": "one", "printed_on": "2026-08-18" } },
                { "data": { "message": "two", "printed_on": "not a date" } },
                { "data": { "message": "three", "printed_on": "2026-08-19" } }
            ]
        });
        let response = build_app()
            .oneshot(json_req("POST", "/api/batch", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(response).await;
        assert_eq!(body["error"]["code"], "BatchInvalid");
        let failures = body["error"]["details"]["failures"]
            .as_array()
            .expect("failures array");
        assert_eq!(
            failures.len(),
            1,
            "only the second label is invalid: {body}"
        );
        assert_eq!(failures[0]["index"], 1);
        assert_eq!(failures[0]["code"], "InvalidRequest");
        assert_eq!(failures[0]["reason"], "datetime_param_invalid");
    }

    /// The template advertises `message` and not the datetime parameter or its namespace: the
    /// caller supplies the first and never has to supply the second.
    #[tokio::test]
    async fn template_detail_reports_a_datetime_param_and_not_its_namespace() {
        let response = build_app()
            .oneshot(
                Request::builder()
                    .uri("/api/templates/brother_24mm_printed_on")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_response(response).await;
        assert_eq!(body["params"]["printed_on"]["type"], "datetime");
        assert_eq!(
            body["params"]["printed_on"]["time"], false,
            "time is always published, so the form never has to guess: {body}"
        );
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

    fn template_yaml_for(name: &str, alt_name: &str) -> String {
        template_yaml(name).replace(&format!("name: {name}"), &format!("name: {alt_name}"))
    }

    fn template_yaml(name: &str) -> String {
        format!(
            r#"name: {name}
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
                "/api/templates/bad",
                "PUT",
                "name: [not a string".to_string(),
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
        let yaml = template_yaml("v1").replace("size: [20.0, 5.0]", "size: [40.0, 5.0]");
        let response = app
            .oneshot(yaml_post("/api/templates/v1", "PUT", yaml))
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

    #[tokio::test]
    async fn template_put_rejects_invalid_token_with_validation_failed() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);
        let yaml = r#"
name: Bad Token
unit: mm
dpi: 200
format:
  type: single
  height: 12
  width: 50
layout:
  - type: text
    value: "{datetime.long_date}"
    at: [0, 0]
    size: [content, 10]
    font_size: 10
"#;
        let response = app
            .oneshot(yaml_post("/api/templates/bad_tok", "PUT", yaml.to_string()))
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(response).await;
        assert_eq!(body["error"]["code"], "TemplateInvalid");
        assert_eq!(
            body["error"]["details"]["reason"],
            "template_validation_failed"
        );
        assert!(
            !dir.join("bad_tok.yaml").exists(),
            "nothing should be stored"
        );
    }

    #[tokio::test]
    async fn template_put_rejects_unmigrated_multiline_text() {
        for (i, multiline_spec) in [
            "multiline: true",
            "multiline: false",
            "multiline: \"yes\"",
            "multiline:",
        ]
        .iter()
        .enumerate()
        {
            let dir = temp_templates_dir();
            let app = build_app_in(&dir);
            let id = format!("bad_multiline_{i}");
            let yaml = format!(
                r#"
name: Unmigrated Put
unit: mm
dpi: 180
format:
  type: single
  width: 60
  height: 20
layout:
  - type: text
    value: "test"
    at: [0, 0]
    size: [60, 20]
    font_size: 10
    {multiline_spec}
"#
            );
            let response = app
                .oneshot(yaml_post(&format!("/api/templates/{id}"), "PUT", yaml))
                .await
                .expect("request");
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
            let body = json_response(response).await;
            assert_eq!(body["error"]["code"], "TemplateInvalid");
            let msg = body["error"]["message"].as_str().unwrap_or("");
            assert!(
                msg.contains("layout[0].multiline"),
                "error must name layout path: {msg}"
            );
            assert!(
                msg.contains("wrap"),
                "error must name rename to wrap: {msg}"
            );
            assert!(
                !dir.join(format!("{id}.yaml")).exists(),
                "nothing should be written to disk"
            );
        }
    }

    #[tokio::test]
    async fn load_time_put_default_rules() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);

        // 1. Bare token in default is rejected on PUT
        let yaml_bare = r#"
name: Bad Bare Token
unit: mm
dpi: 200
params:
  val:
    type: string
    default: "{bare_token}"
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "{val}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let res = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/bare_def",
                "PUT",
                yaml_bare.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "TemplateInvalid");
        assert_eq!(
            body["error"]["details"]["reason"],
            "template_validation_failed"
        );

        // 2. Datetime accepting literal and {sys.now}
        let yaml_dt_sys = r#"
name: Valid DT Sys
unit: mm
dpi: 200
params:
  dt:
    type: datetime
    default: "{sys.now}"
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "{dt}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let res = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/dt_sys",
                "PUT",
                yaml_dt_sys.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);

        // 3. Explicit null default: null loads as absent default
        let yaml_null = r#"
name: Null Default
unit: mm
dpi: 200
params:
  str_val:
    type: string
    default: null
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "{str_val}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let res = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/null_def",
                "PUT",
                yaml_null.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);

        // 4. Non-string datetime default is refused
        let yaml_non_str_dt = r#"
name: Non String DT
unit: mm
dpi: 200
params:
  dt:
    type: datetime
    default: 12345
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "{dt}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let res = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/non_str_dt",
                "PUT",
                yaml_non_str_dt.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "TemplateInvalid");
        assert_eq!(
            body["error"]["details"]["reason"],
            "template_validation_failed"
        );

        // 5. Unescaped brace in default is refused with template_validation_failed
        let yaml_unescaped = r#"
name: Unescaped Brace DT
unit: mm
dpi: 200
params:
  dt:
    type: datetime
    default: "{sys.now"
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "{dt}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let res = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/unescaped_dt",
                "PUT",
                yaml_unescaped.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "TemplateInvalid");
        assert_eq!(
            body["error"]["details"]["reason"],
            "template_validation_failed"
        );
    }

    #[tokio::test]
    async fn template_put_rejects_invalid_color_literal_and_ink() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);

        // 1. Shape background unreadable colour is refused naming layout path and field, no file written
        let shape_bg_yaml = r#"
name: BadShapeBg
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 20
layout:
  - type: container
    at: [0, 0]
    size: [50, 20]
    background: chartreuse
    items: []
"#;
        let res1 = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/bad_shape_bg",
                "PUT",
                shape_bg_yaml.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res1.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body1 = json_response(res1).await;
        assert_eq!(body1["error"]["code"], "TemplateInvalid");
        let msg1 = body1["error"]["message"].as_str().unwrap();
        assert!(
            msg1.contains("layout[0]")
                && msg1.contains("background")
                && msg1.contains("chartreuse"),
            "expected error naming layout path, background field and invalid color, got: {msg1}"
        );
        assert!(
            !dir.join("bad_shape_bg.yaml").exists(),
            "no file should be written on rejection of bad shape background"
        );

        // 2. Shape stroke unreadable colour is refused naming layout path and field, no file written
        let shape_stroke_yaml = r#"
name: BadShapeStroke
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 20
layout:
  - type: line
    at: [0, 0]
    to: [50, 20]
    stroke:
      thickness: 0.5
      color: chartreuse
"#;
        let res2 = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/bad_shape_stroke",
                "PUT",
                shape_stroke_yaml.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res2.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body2 = json_response(res2).await;
        assert_eq!(body2["error"]["code"], "TemplateInvalid");
        let msg2 = body2["error"]["message"].as_str().unwrap();
        assert!(
            msg2.contains("layout[0]") && msg2.contains("stroke") && msg2.contains("chartreuse"),
            "expected error naming layout path, stroke field and invalid color, got: {msg2}"
        );
        assert!(
            !dir.join("bad_shape_stroke.yaml").exists(),
            "no file should be written on rejection of bad stroke color"
        );

        // 3. Text unreadable colour is refused naming layout path and field, no file written
        let text_yaml = r#"
name: BadTextColor
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 20
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    color: chartreuse
"#;
        let res3 = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/bad_text_color",
                "PUT",
                text_yaml.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res3.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body3 = json_response(res3).await;
        assert_eq!(body3["error"]["code"], "TemplateInvalid");
        let msg3 = body3["error"]["message"].as_str().unwrap();
        assert!(
            msg3.contains("layout[0]") && msg3.contains("color") && msg3.contains("chartreuse"),
            "expected error naming layout path, color field and invalid color, got: {msg3}"
        );
        assert!(
            !dir.join("bad_text_color.yaml").exists(),
            "no file should be written on rejection of bad text color"
        );

        // 4. Task 2.2: ink: on text item is refused with unknown field error naming ink and layout path, no file written
        let ink_yaml = r#"
name: InkText
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 20
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    ink: red
"#;
        let res4 = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/unmigrated_ink",
                "PUT",
                ink_yaml.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res4.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body4 = json_response(res4).await;
        assert_eq!(body4["error"]["code"], "TemplateInvalid");
        let msg4 = body4["error"]["message"].as_str().unwrap();
        assert!(
            msg4.contains("layout[0]") && msg4.contains("unknown field `ink`"),
            "expected error naming layout path and unknown field ink, got: {msg4}"
        );
        assert!(
            !dir.join("unmigrated_ink.yaml").exists(),
            "no file should be written on rejection of unmigrated ink field"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn template_detail_readback_preserves_padded_literal_and_canonical_reference() {
        let dir = temp_templates_dir();
        let yaml = r#"
name: ColorReadback
unit: mm
dpi: 200
params:
  brand:
    type: string
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 10]
    font_size: 10
    color: " red "
  - type: container
    at: [0, 10]
    size: [50, 10]
    background: " {brand} "
    items:
      - type: text
        value: "Escaped"
        at: [0, 0]
        size: [50, 10]
        font_size: 10
        color: "\u0062lue"
"#;
        let tpl_path = dir.join("color_readback.yaml");
        std::fs::write(&tpl_path, yaml).unwrap();

        let app = build_app_in(&dir);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/templates/color_readback")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_response(response).await;

        assert_eq!(body["layout"][0]["color"], " red ");
        assert_eq!(body["layout"][1]["background"], "{brand}");
        assert_eq!(body["layout"][1]["items"][0]["color"], "blue");

        let source_res = app
            .oneshot(
                Request::builder()
                    .uri("/api/templates/color_readback/source")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(source_res.status(), StatusCode::OK);
        let source_body = axum::body::to_bytes(source_res.into_body(), usize::MAX)
            .await
            .unwrap();
        let source_str = String::from_utf8(source_body.to_vec()).unwrap();
        assert!(source_str.contains(r#""\u0062lue""#) || source_str.contains(r#"\u0062lue"#));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn startup_quarantines_unreadable_shape_and_text_colors_and_serves_valid_sibling() {
        let dir = temp_templates_dir();
        let valid_yaml = r#"
name: ValidSibling
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 20
layout:
  - type: container
    at: [0, 0]
    size: [50, 20]
    background: red
    items:
      - type: text
        value: "Good"
        at: [0, 0]
        size: [50, 20]
        font_size: 10
        color: blue
"#;
        let bad_shape_yaml = r#"
name: BadShapeSibling
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 20
layout:
  - type: container
    at: [0, 0]
    size: [50, 20]
    background: chartreuse
    items: []
"#;
        let unmigrated_ink_yaml = r#"
name: UnmigratedInkSibling
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 20
layout:
  - type: text
    value: "Unmigrated"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    ink: red
"#;
        std::fs::write(dir.join("valid.yaml"), valid_yaml).unwrap();
        std::fs::write(dir.join("bad_shape.yaml"), bad_shape_yaml).unwrap();
        std::fs::write(dir.join("unmigrated_ink.yaml"), unmigrated_ink_yaml).unwrap();

        let app = build_app_in(&dir);
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/templates")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = json_response(res).await;
        let templates = body["templates"].as_array().unwrap();
        let broken = body["broken"].as_array().unwrap();

        // Valid template is served
        assert_eq!(templates.len(), 1);
        assert_eq!(templates[0]["id"], "valid");

        // Both broken templates are quarantined
        assert_eq!(broken.len(), 2);

        let shape_broken = broken
            .iter()
            .find(|b| b["path"] == "bad_shape.yaml")
            .expect("bad_shape.yaml in broken");
        let shape_err = shape_broken["error"].as_str().unwrap();
        assert!(
            shape_err.contains("layout[0]")
                && shape_err.contains("background")
                && shape_err.contains("chartreuse"),
            "expected broken shape error naming layout path, background field and chartreuse, got: {shape_err}"
        );

        let ink_broken = broken
            .iter()
            .find(|b| b["path"] == "unmigrated_ink.yaml")
            .expect("unmigrated_ink.yaml in broken");
        let ink_err = ink_broken["error"].as_str().unwrap();
        assert!(
            ink_err.contains("layout[0]") && ink_err.contains("unknown field `ink`"),
            "expected broken ink error naming layout path and unknown field ink, got: {ink_err}"
        );

        // GET /api/templates/valid serves 200, broken templates are 404
        let res_valid = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/templates/valid")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_valid.status(), StatusCode::OK);

        let res_bad_shape = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/templates/bad_shape")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_bad_shape.status(), StatusCode::NOT_FOUND);

        let res_bad_ink = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/templates/unmigrated_ink")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_bad_ink.status(), StatusCode::NOT_FOUND);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn template_put_paint_refusals_report_correct_reasons() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);

        let validation_cases = [
            // 1. Non-positive stroke thickness
            (
                "stroke_zero",
                "name: T\nunit: mm\ndpi: 200\nformat: { type: single, width: 20, height: 20 }\nlayout:\n  - type: container\n    at: [0,0]\n    stroke:\n      thickness: 0\n    items: []\n",
            ),
            // 2. Sub-0.0001 stroke thickness
            (
                "stroke_too_small",
                "name: T\nunit: mm\ndpi: 200\nformat: { type: single, width: 20, height: 20 }\nlayout:\n  - type: container\n    at: [0,0]\n    stroke:\n      thickness: 0.00001\n    items: []\n",
            ),
            // 3. Zero rounded
            (
                "rounded_zero",
                "name: T\nunit: mm\ndpi: 200\nformat: { type: single, width: 20, height: 20 }\nlayout:\n  - type: container\n    at: [0,0]\n    rounded: 0\n    items: []\n",
            ),
            // 4. Sub-0.0001 rounded
            (
                "rounded_too_small",
                "name: T\nunit: mm\ndpi: 200\nformat: { type: single, width: 20, height: 20 }\nlayout:\n  - type: container\n    at: [0,0]\n    rounded: 0.00001\n    items: []\n",
            ),
            // 5. Line non-positive stroke thickness
            (
                "line_stroke_zero",
                "name: T\nunit: mm\ndpi: 200\nformat: { type: single, width: 20, height: 20 }\nlayout:\n  - type: line\n    at: [0,0]\n    to: [10,10]\n    stroke:\n      thickness: 0\n",
            ),
        ];

        for (id, yaml) in validation_cases {
            let response = app
                .clone()
                .oneshot(yaml_post(
                    &format!("/api/templates/{id}"),
                    "PUT",
                    yaml.to_string(),
                ))
                .await
                .expect("request");
            assert_eq!(
                response.status(),
                StatusCode::UNPROCESSABLE_ENTITY,
                "case: {id}"
            );
            let body = json_response(response).await;
            assert_eq!(body["error"]["code"], "TemplateInvalid", "case: {id}");
            assert_eq!(
                body["error"]["details"]["reason"], "template_validation_failed",
                "case: {id}, got: {:?}",
                body["error"]["details"]
            );
        }

        // parse_cases: reason mapping is #289's to settle; this table characterizes current behaviour and is expected to move with it.
        let parse_cases = [
            // Null stroke
            (
                "stroke_null",
                "name: T\nunit: mm\ndpi: 200\nformat: { type: single, width: 20, height: 20 }\nlayout:\n  - type: container\n    at: [0,0]\n    stroke:\n    items: []\n",
            ),
            // Null background
            (
                "bg_null",
                "name: T\nunit: mm\ndpi: 200\nformat: { type: single, width: 20, height: 20 }\nlayout:\n  - type: container\n    at: [0,0]\n    background:\n    items: []\n",
            ),
            // Line with background (unknown field)
            (
                "line_bg",
                "name: T\nunit: mm\ndpi: 200\nformat: { type: single, width: 20, height: 20 }\nlayout:\n  - type: line\n    at: [0,0]\n    to: [10,10]\n    stroke:\n      thickness: 0.2\n    background: red\n",
            ),
            // Bad color name
            (
                "bad_color",
                "name: T\nunit: mm\ndpi: 200\nformat: { type: single, width: 20, height: 20 }\nlayout:\n  - type: container\n    at: [0,0]\n    background: chartreuse\n    items: []\n",
            ),
            // Legacy frame spelling
            (
                "legacy_frame",
                "name: T\nunit: mm\ndpi: 200\nformat: { type: single, width: 20, height: 20 }\nlayout:\n  - type: container\n    at: [0,0]\n    frame:\n      thickness: 0.02\n    items: []\n",
            ),
        ];

        for (id, yaml) in parse_cases {
            let response = app
                .clone()
                .oneshot(yaml_post(
                    &format!("/api/templates/{id}"),
                    "PUT",
                    yaml.to_string(),
                ))
                .await
                .expect("request");
            assert_eq!(
                response.status(),
                StatusCode::UNPROCESSABLE_ENTITY,
                "case: {id}"
            );
            let body = json_response(response).await;
            assert_eq!(body["error"]["code"], "TemplateInvalid", "case: {id}");
            assert_eq!(
                body["error"]["details"]["reason"], "template_parse_failed",
                "case: {id}, got: {:?}",
                body["error"]["details"]
            );
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn template_put_with_top_level_options_is_rejected_before_write() {
        let dir = temp_templates_dir();
        let original = template_yaml("keep_me");
        std::fs::write(dir.join("keep_me.yaml"), &original).unwrap();
        let app = build_app_in(&dir);

        let body = r#"
name: Has Options
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 30
options:
  orientation: [vertical, horizontal]
layout:
  - type: text
    value: "hello"
    at: [0, 0]
    size: [10, 5]
    font_size: 8
"#;
        let response = app
            .clone()
            .oneshot(yaml_post("/api/templates/keep_me", "PUT", body.to_string()))
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let json = json_response(response).await;
        assert_eq!(json["error"]["code"], "TemplateInvalid");
        assert_eq!(json["error"]["details"]["reason"], "template_parse_failed");
        let msg = json["error"]["message"].as_str().unwrap_or("");
        assert!(
            msg.contains("unknown field `options`"),
            "expected 'unknown field `options`' in error message, got: {msg}"
        );

        let stored = std::fs::read_to_string(dir.join("keep_me.yaml")).expect("read stored");
        assert_eq!(
            stored, original,
            "stored file must remain byte-for-byte unchanged"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn template_put_with_container_option_is_rejected_before_write() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);

        let body = r#"
name: Has Container Option
unit: mm
dpi: 200
params:
  orientation:
    type: enum
    values: [vertical, horizontal]
format:
  type: single
  width: 50
  height: 30
layout:
  - type: container
    at: [0, 0]
    option:
      orientation: vertical
    items: []
"#;
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/templates/new_tpl")
                    .header("content-type", "text/yaml")
                    .header("if-none-match", "*")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let json = json_response(response).await;
        assert_eq!(json["error"]["code"], "TemplateInvalid");
        assert_eq!(json["error"]["details"]["reason"], "template_parse_failed");
        let msg = json["error"]["message"].as_str().unwrap_or("");
        assert!(
            msg.contains("layout[0]") && msg.contains("unknown field `option`"),
            "expected layout[0] and 'unknown field `option`' in error message, got: {msg}"
        );
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            0,
            "create-only write must leave no file"
        );

        std::fs::remove_dir_all(&dir).ok();
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
            .oneshot(yaml_post("/api/templates/wf1", "PUT", template_yaml("wf1")))
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
        std::fs::write(dir.join("bad.yaml"), "unit: nope\n").unwrap();
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
        assert_eq!(broken[0]["path"], "bad.yaml");
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
        std::fs::write(dir.join("dup.yaml"), template_yaml("dup")).unwrap();
        let app = build_app_in(&dir);
        assert_eq!(template_count(&app).await, 1);

        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/dup.yaml"), template_yaml("dup")).unwrap();
        let body = reload(&app).await;
        assert_eq!(body["count"], 1);
        assert_eq!(body["broken_count"], 1);

        let (_, list) = get_json(&app, "/api/templates").await;
        let templates = list["templates"].as_array().unwrap();
        assert_eq!(templates.len(), 1);
        assert_eq!(templates[0]["id"], "dup");
        let broken = list["broken"].as_array().unwrap();
        assert_eq!(broken.len(), 1);
        assert_eq!(broken[0]["path"], "sub/dup.yaml");
        let error = broken[0]["error"].as_str().unwrap();
        assert!(
            error.contains("dup") && error.contains("dup.yaml"),
            "broken entry names the id and the file it collides with: {error}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The operator's fix converges: drop one of the two files, reload, and the collision is gone
    /// while the winner keeps serving (#181).
    #[tokio::test]
    async fn removing_the_colliding_file_clears_the_broken_entry() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("dup.yaml"), template_yaml("dup")).unwrap();
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/dup.yaml"), template_yaml("dup")).unwrap();
        // The app builds at all only because a duplicate id no longer fails the load.
        let app = build_app_in(&dir);
        let (_, list) = get_json(&app, "/api/templates").await;
        assert_eq!(list["broken"].as_array().unwrap().len(), 1);

        std::fs::remove_file(dir.join("sub/dup.yaml")).unwrap();
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
        std::fs::create_dir_all(dir.join("Warehouse")).unwrap();
        std::fs::create_dir_all(dir.join("Shipping")).unwrap();
        // 1. Grouped template "t_wh1" in "Warehouse"
        std::fs::write(
            dir.join("Warehouse/t_wh1.yaml"),
            "name: Warehouse 1\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 10\nlayout: []\n",
        ).unwrap();
        // 2. Grouped template "t_wh2" in "Warehouse"
        std::fs::write(
            dir.join("Warehouse/t_wh2.yaml"),
            "name: Warehouse 2\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 10\nlayout: []\n",
        ).unwrap();
        // 3. Grouped template "t_ship" in "Shipping"
        std::fs::write(
            dir.join("Shipping/t_ship.yaml"),
            "name: Shipping\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 10\nlayout: []\n",
        ).unwrap();
        // 4. Ungrouped template "t_ungrouped"
        std::fs::write(
            dir.join("t_ungrouped.yaml"),
            "name: Ungrouped\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 20\n  height: 10\nlayout: []\n",
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
        let t1_yaml = "# Template 1 comment\nname: Template 1\nunit: mm\ndpi: 200\nformat:\n  type: single\n  width: 50\n  height: 18\nlayout: []\n";
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
        assert!(!t1_path.exists(), "original ungrouped file was moved");
        let moved_path = dir.join("Warehouse/t1.yaml");
        assert!(moved_path.exists(), "file now in Warehouse/t1.yaml");
        let source_after = std::fs::read_to_string(&moved_path).unwrap();
        assert_eq!(
            source_after, t1_yaml,
            "file content is unmodified (no group injected in YAML)"
        );

        // 2. Idempotent set to "Warehouse" -> 200 OK, file remains
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
        assert!(moved_path.exists());

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
        assert!(!moved_path.exists());
        let shipping_path = dir.join("Shipping/t1.yaml");
        assert!(shipping_path.exists());
        assert!(
            dir.join("Warehouse").exists(),
            "source directory is left in place"
        );

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
        assert!(t1_path.exists());
        assert!(!shipping_path.exists());
        assert!(
            dir.join("Shipping").exists(),
            "source directory is left in place"
        );

        // 5. Idempotent clear -> 200 OK
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
        assert!(t1_path.exists());

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

        // 7b. Body omitting group key -> 400 Bad Request
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

        // 8. Invalid group name (empty) -> 422 Unprocessable Entity, file unchanged
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
        assert!(t1_path.exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn template_groups_list_and_delete_endpoint() {
        let dir = temp_templates_dir();
        std::fs::create_dir_all(dir.join("Shipping/Pallets/Euro")).unwrap();
        std::fs::create_dir_all(dir.join("Warehouse")).unwrap();
        std::fs::create_dir_all(dir.join(".hidden/Sub")).unwrap();
        std::fs::create_dir_all(dir.join("invalid:group")).unwrap();
        std::fs::write(
            dir.join("Shipping/Pallets/Euro/t1.yaml"),
            template_yaml("t1"),
        )
        .unwrap();
        let app = build_app_in(&dir);

        // 1. GET /api/template-groups
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/template-groups")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let groups: Vec<String> = serde_json::from_value(json_response(resp).await).unwrap();
        assert_eq!(
            groups,
            vec![
                "Shipping".to_string(),
                "Shipping/Pallets".to_string(),
                "Shipping/Pallets/Euro".to_string(),
                "Warehouse".to_string()
            ]
        );

        // 2. DELETE non-empty group -> 409
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/template-groups/Shipping")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);

        // 3. DELETE with malformed percent sequence -> 400
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/template-groups/Shipping%ZZ")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // 4. DELETE non-existent / case-mismatched group -> 404
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/template-groups/warehouse")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // 5. DELETE empty group -> 204
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/template-groups/Warehouse")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert!(!dir.join("Warehouse").exists());

        // 6. GET /api/template-groups after deletion
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/template-groups")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let groups: Vec<String> = serde_json::from_value(json_response(resp).await).unwrap();
        assert_eq!(
            groups,
            vec![
                "Shipping".to_string(),
                "Shipping/Pallets".to_string(),
                "Shipping/Pallets/Euro".to_string()
            ]
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn template_list_nested_group_filtering() {
        let dir = temp_templates_dir();
        std::fs::create_dir_all(dir.join("Shipping/Pallets")).unwrap();
        std::fs::create_dir_all(dir.join("Shipping2")).unwrap();
        std::fs::write(dir.join("Shipping/s1.yaml"), template_yaml("s1")).unwrap();
        std::fs::write(dir.join("Shipping/Pallets/p1.yaml"), template_yaml("p1")).unwrap();
        std::fs::write(dir.join("Shipping2/s2.yaml"), template_yaml("s2")).unwrap();
        std::fs::write(dir.join("root.yaml"), template_yaml("root")).unwrap();
        let app = build_app_in(&dir);

        // Exact group query
        let (_, resp) = get_json(&app, "/api/templates?group=Shipping").await;
        let ids: Vec<&str> = resp["templates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, vec!["s1"]);

        // Nested group query
        let (_, resp) = get_json(&app, "/api/templates?group=Shipping&nested=true").await;
        let mut ids: Vec<&str> = resp["templates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap())
            .collect();
        ids.sort_unstable();
        assert_eq!(ids, vec!["p1", "s1"]);

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
            .oneshot(yaml_post(
                "/api/templates/new1",
                "PUT",
                template_yaml("new1"),
            ))
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
            .oneshot(yaml_post("/api/templates/f1", "PUT", template_yaml("f1")))
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

        std::fs::write(dir.join("bad.yaml"), "unit: nope\n").unwrap();
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
        assert_eq!(list["broken"][0]["path"], "bad.yaml");

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

    #[tokio::test]
    async fn file_endpoints_create_and_replace_and_delete_template() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("y2.yaml"), template_yaml("y2")).unwrap();
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

        let body200 = template_yaml("y2").replace("dpi: 300", "dpi: 200");
        let resp = app
            .clone()
            .oneshot(yaml_post("/api/templates/y2", "PUT", body200))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(template_count(&app).await, 1);

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/templates/y2")
                    .header("content-type", "text/yaml")
                    .header("if-none-match", "*")
                    .body(Body::from(template_yaml("y2")))
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::PRECONDITION_FAILED);

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
            !dir.join("y2.yaml").exists(),
            "the backing file is still on disk"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn template_create_duplicate_returns_412() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("dup.yaml"), template_yaml("dup")).unwrap();
        let app = build_app_in(&dir);
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/templates/dup")
                    .header("content-type", "text/yaml")
                    .header("if-none-match", "*")
                    .body(Body::from(template_yaml("dup")))
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::PRECONDITION_FAILED);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The filename half of the create guard: an unservable file occupying `{id}.yaml` blocks the
    /// create by its name alone, since its content claims no id the registry could serve.
    #[tokio::test]
    async fn template_create_is_blocked_by_an_unservable_file_at_its_destination() {
        let dir = temp_templates_dir();
        // Content the registry cannot serve, so only the filename half of the guard can catch it.
        std::fs::write(dir.join("planted.yaml"), "not: a valid template\n").unwrap();
        let app = build_app_in(&dir);

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/templates/planted")
                    .header("content-type", "text/yaml")
                    .header("if-none-match", "*")
                    .body(Body::from(template_yaml("planted")))
                    .unwrap(),
            )
            .await
            .expect("request");

        assert_eq!(resp.status(), StatusCode::PRECONDITION_FAILED);
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
        // After the app is built, so the in-memory registry does not hold `late`.
        std::fs::write(dir.join("late.yaml"), template_yaml("late")).unwrap();

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/templates/late")
                    .header("content-type", "text/yaml")
                    .header("if-none-match", "*")
                    .body(Body::from(template_yaml("late")))
                    .unwrap(),
            )
            .await
            .expect("request");

        assert_eq!(resp.status(), StatusCode::PRECONDITION_FAILED);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// With the pre-write re-read (#184), a `PUT` for an id whose winner changed on disk edits the
    /// file that currently serves it, and answers with the caller's own content. The stale-registry
    /// path that used to return the *other* template's body is gone: the handler no longer resolves
    /// the id from a set that predates the directory.
    #[tokio::test]
    async fn template_replace_writes_the_current_winner_after_a_collider_appears() {
        let dir = temp_templates_dir();
        std::fs::create_dir_all(dir.join("zzz")).unwrap();
        std::fs::write(dir.join("zzz/moved.yaml"), template_yaml("moved")).unwrap();
        let app = build_app_in(&dir);
        // Sorts before zzz/moved.yaml, so the next load hands `moved` to this file instead.
        std::fs::write(dir.join("moved.yaml"), template_yaml("moved")).unwrap();

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
            std::fs::read_to_string(dir.join("moved.yaml")).unwrap(),
            edited,
            "the write went to the file that serves the id now"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("zzz/moved.yaml")).unwrap(),
            template_yaml("moved"),
            "the file that lost the id is left alone"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A duplicate that sorts *after* the written file never displaces it, so the write succeeds
    /// normally and the duplicate is just a refused sibling. Returning 409 here would be a lie about
    /// which file serves the id (#184, round-4 review).
    #[tokio::test]
    async fn template_replace_ignores_a_later_sorting_duplicate() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("kept.yaml"), template_yaml("kept")).unwrap();
        let app = build_app_in(&dir);
        std::fs::create_dir_all(dir.join("zzz")).unwrap();
        std::fs::write(dir.join("zzz/kept.yaml"), template_yaml("kept")).unwrap();

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
            std::fs::read_to_string(dir.join("kept.yaml")).unwrap(),
            edited
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The whole of #184, end to end: a colliding file that sorts earlier lands *between* the write
    /// and the reload, so the id the caller addressed is served from another file by the time the
    /// handler answers. Before this change the handler returned `200` with that other file's body.
    #[tokio::test]
    async fn template_replace_returns_409_when_the_id_moves_between_write_and_reload() {
        let dir = temp_templates_dir();
        std::fs::create_dir_all(dir.join("zzz")).unwrap();
        std::fs::write(dir.join("zzz/moved.yaml"), template_yaml("moved")).unwrap();
        let (app, state) = build_app_in_with_state(&dir);

        let planted = dir.join("moved.yaml");
        state.set_mid_write_hook(move || {
            // Sorts before zzz/moved.yaml, so the reload that follows hands `moved` to this file.
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
        assert_eq!(files, vec!["moved.yaml", "zzz/moved.yaml"]);
        assert_eq!(
            std::fs::read_to_string(dir.join("zzz/moved.yaml")).unwrap(),
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
            .map(|b| b["path"].as_str().unwrap())
            .collect();
        assert_eq!(
            broken,
            vec!["zzz/moved.yaml"],
            "the caller's file is quarantined"
        );
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
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/templates/racer")
                    .header("content-type", "text/yaml")
                    .header("if-none-match", "*")
                    .body(Body::from(template_yaml("racer")))
                    .unwrap(),
            )
            .await
            .expect("request");

        assert_eq!(resp.status(), StatusCode::PRECONDITION_FAILED);
        assert_eq!(
            std::fs::read_to_string(dir.join("racer.yaml")).unwrap(),
            "someone else's file\n",
            "the other writer's file was not overwritten"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The same interleaving through `PUT`: the pre-write re-read finds the id free, the collider
    /// lands while the file is being published, and the create must not answer `201` describing it.
    #[tokio::test]
    async fn template_create_returns_409_when_the_id_moves_between_write_and_reload() {
        let dir = temp_templates_dir();
        let (app, state) = build_app_in_with_state(&dir);

        let planted_dir = dir.join("aaa");
        std::fs::create_dir_all(&planted_dir).unwrap();
        let planted = planted_dir.join("late.yaml");
        state.set_mid_write_hook(move || {
            std::fs::write(&planted, template_yaml_for("late", "planted")).unwrap();
        });

        let resp = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/late",
                "PUT",
                template_yaml("late"),
            ))
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
        std::fs::create_dir_all(dir.join("zzz")).unwrap();
        std::fs::write(dir.join("zzz/grouped.yaml"), template_yaml("grouped")).unwrap();
        let (app, state) = build_app_in_with_state(&dir);

        let planted_dir = dir.join("AAA");
        std::fs::create_dir_all(&planted_dir).unwrap();
        let planted = planted_dir.join("grouped.yaml");
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
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The group update shares the write -> reload -> detail shape, so it gets the same pre-write
    /// re-read and the same post-write confirmation; here that means it patches the file serving the
    /// id now, and answers for that file (#184).
    #[tokio::test]
    async fn template_group_update_writes_the_current_winner() {
        let dir = temp_templates_dir();
        std::fs::create_dir_all(dir.join("zzz")).unwrap();
        std::fs::write(dir.join("zzz/grouped.yaml"), template_yaml("grouped")).unwrap();
        let app = build_app_in(&dir);
        std::fs::write(dir.join("grouped.yaml"), template_yaml("grouped")).unwrap();

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
        assert!(dir.join("Warehouse/grouped.yaml").exists());
        assert!(!dir.join("grouped.yaml").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Deleting the winner promotes the collider, so the id survives a 204 from different content
    /// with its favorites already pruned. Refuse instead, naming the file to fix (#183).
    #[tokio::test]
    async fn template_delete_is_refused_while_the_id_collides() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("contested.yaml"), template_yaml("contested")).unwrap();
        let app = build_app_in(&dir);
        std::fs::create_dir_all(dir.join("zzz")).unwrap();
        std::fs::write(dir.join("zzz/contested.yaml"), template_yaml("contested")).unwrap();
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
            vec!["contested.yaml", "zzz/contested.yaml"],
            "exactly the files declaring the id, and no others"
        );
        assert!(
            !body["error"]["details"]
                .as_object()
                .expect("details object")
                .contains_key("reason"),
            "a 409 carries no details.reason key at all (ADR-0052)"
        );
        assert!(dir.join("contested.yaml").exists(), "nothing was unlinked");
        assert!(
            dir.join("zzz/contested.yaml").exists(),
            "nothing was unlinked"
        );

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
    #[tokio::test]
    async fn template_delete_succeeds_beside_an_unrelated_refused_file() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("target.yaml"), template_yaml("target")).unwrap();
        std::fs::write(dir.join("other.yaml"), template_yaml("other")).unwrap();
        std::fs::create_dir_all(dir.join("zzz")).unwrap();
        std::fs::write(dir.join("zzz/other.yaml"), template_yaml("other")).unwrap();
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
        std::fs::write(dir.join("fixable.yaml"), template_yaml("fixable")).unwrap();
        std::fs::create_dir_all(dir.join("zzz")).unwrap();
        std::fs::write(dir.join("zzz/fixable.yaml"), template_yaml("fixable")).unwrap();
        let app = build_app_in(&dir);

        std::fs::remove_file(dir.join("zzz/fixable.yaml")).unwrap();
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
        assert!(!dir.join("fixable.yaml").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn template_create_invalid_yaml_returns_422() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);
        let resp = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/x",
                "PUT",
                "name: x\nunit: nope\n".to_string(),
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
        let body = template_yaml("ok");
        let resp = app
            .clone()
            .oneshot(yaml_post("/api/templates/..%2fevil", "PUT", body))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        // No file escaped the templates dir.
        assert!(!dir.parent().unwrap().join("evil.yaml").exists());
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

        std::fs::write(dir.join("bad.yaml"), "unit: nope\n").unwrap();
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
        std::fs::write(dir.join("broken.yaml"), "unit: nope\n").unwrap();

        let resp = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/new1",
                "PUT",
                template_yaml("new1"),
            ))
            .await
            .expect("request");
        // Succeeds now that broken files are quarantined.
        assert_eq!(resp.status(), StatusCode::CREATED);
        assert!(dir.join("new1.yaml").exists());
        assert_eq!(template_count(&app).await, 2);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn template_create_reclassifies_to_replace_when_destination_appears_mid_request() {
        let dir = temp_templates_dir();
        let (app, state) = build_app_in_with_state(&dir);

        let planted = dir.join("reclass.yaml");
        state.set_pre_publish_hook(move || {
            std::fs::write(&planted, template_yaml_for("reclass", "initial")).unwrap();
        });

        let new_body = template_yaml_for("reclass", "updated");
        let resp = app
            .clone()
            .oneshot(yaml_post("/api/templates/reclass", "PUT", new_body.clone()))
            .await
            .expect("request");

        assert_eq!(resp.status(), StatusCode::OK);
        let detail = json_response(resp).await;
        assert_eq!(detail["name"], "updated");
        assert_eq!(
            std::fs::read_to_string(dir.join("reclass.yaml")).unwrap(),
            new_body
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn template_group_case_sibling_created_on_case_sensitive_fs() {
        let dir = temp_templates_dir();
        let case_sensitive = crate::fs_safe::probe_is_case_sensitive(&dir);
        std::fs::create_dir_all(dir.join("Warehouse")).unwrap();
        std::fs::write(dir.join("Warehouse/t1.yaml"), template_yaml("t1")).unwrap();
        let app = build_app_in(&dir);

        let resp = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/t2?group=warehouse",
                "PUT",
                template_yaml("t2"),
            ))
            .await
            .expect("request");
        if case_sensitive {
            assert_eq!(resp.status(), StatusCode::CREATED);
            let (status, groups) = get_json(&app, "/api/template-groups").await;
            assert_eq!(status, StatusCode::OK);
            let list: Vec<String> = serde_json::from_value(groups).unwrap();
            assert!(list.contains(&"Warehouse".to_string()));
            assert!(list.contains(&"warehouse".to_string()));
        } else {
            assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
            let body = json_response(resp).await;
            assert_eq!(
                body["error"]["details"]["reason"],
                "template_group_case_conflict"
            );
            assert!(
                body["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("Warehouse"),
                "case conflict must name stored spelling Warehouse"
            );
            let (status, groups) = get_json(&app, "/api/template-groups").await;
            assert_eq!(status, StatusCode::OK);
            let list: Vec<String> = serde_json::from_value(groups).unwrap();
            assert!(list.contains(&"Warehouse".to_string()));
            assert!(
                !list.contains(&"warehouse".to_string()),
                "case-folding volume must not contain distinct warehouse"
            );
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn template_group_rename_success_paths() {
        let dir = temp_templates_dir();
        std::fs::create_dir_all(dir.join("Warehosue")).unwrap();
        std::fs::create_dir_all(dir.join("Shipping/Pallets")).unwrap();

        let commented_yaml = format!("# Header comment\n{}", template_yaml("bin-tag"));
        std::fs::write(dir.join("Warehosue/bin-tag.yaml"), &commented_yaml).unwrap();
        std::fs::write(
            dir.join("Shipping/Pallets/euro.yaml"),
            template_yaml("euro"),
        )
        .unwrap();
        std::fs::write(dir.join("Warehosue/broken.yaml"), b"invalid: [yaml: broken").unwrap();

        let app = build_app_in(&dir);

        // Favorite the template
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/favorites/bin-tag")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // 1. Top-level rename: Warehosue -> Warehouse
        let resp = app
            .clone()
            .oneshot(json_req(
                "PUT",
                "/api/template-groups/Warehosue",
                json!({ "name": "Warehouse" }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let res_json = json_response(resp).await;
        assert_eq!(res_json["group"], "Warehouse");

        // File is at templates/Warehouse/bin-tag.yaml
        assert!(dir.join("Warehouse/bin-tag.yaml").exists());
        assert!(!dir.join("Warehosue").exists());

        // File bytes unchanged (comments preserved)
        let bytes = std::fs::read_to_string(dir.join("Warehouse/bin-tag.yaml")).unwrap();
        assert_eq!(bytes, commented_yaml);

        // Template id and favorites untouched
        let (status, favs) = get_json(&app, "/api/favorites").await;
        assert_eq!(status, StatusCode::OK);
        let fav_list: Vec<String> = serde_json::from_value(favs).unwrap();
        assert!(fav_list.contains(&"bin-tag".to_string()));

        // Quarantined file follows directory and reported under broken at new path
        let (status, tpls) = get_json(&app, "/api/templates").await;
        assert_eq!(status, StatusCode::OK);
        let broken = tpls["broken"].as_array().unwrap();
        assert!(broken.iter().any(|b| b["path"] == "Warehouse/broken.yaml"));

        // GET /api/template-groups lists Warehouse and not Warehosue
        let (status, groups) = get_json(&app, "/api/template-groups").await;
        assert_eq!(status, StatusCode::OK);
        let group_list: Vec<String> = serde_json::from_value(groups).unwrap();
        assert!(group_list.contains(&"Warehouse".to_string()));
        assert!(!group_list.contains(&"Warehosue".to_string()));

        // 2. Nested rename changing last segment only: Shipping/Pallets -> Shipping/Euro
        let resp = app
            .clone()
            .oneshot(json_req(
                "PUT",
                "/api/template-groups/Shipping/Pallets",
                json!({ "name": "Euro" }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let res_json = json_response(resp).await;
        assert_eq!(res_json["group"], "Shipping/Euro");
        assert!(dir.join("Shipping").is_dir());
        assert!(dir.join("Shipping/Euro/euro.yaml").exists());

        // 3. Descendants follow renamed group: Shipping -> Freight => Freight/Euro
        let resp = app
            .clone()
            .oneshot(json_req(
                "PUT",
                "/api/template-groups/Shipping",
                json!({ "name": "Freight" }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let res_json = json_response(resp).await;
        assert_eq!(res_json["group"], "Freight");
        assert!(dir.join("Freight/Euro/euro.yaml").exists());

        let (_, groups) = get_json(&app, "/api/template-groups").await;
        let group_list: Vec<String> = serde_json::from_value(groups).unwrap();
        assert!(group_list.contains(&"Freight".to_string()));
        assert!(group_list.contains(&"Freight/Euro".to_string()));
        assert!(!group_list.contains(&"Shipping".to_string()));
        assert!(!group_list.contains(&"Shipping/Euro".to_string()));

        // 4. Idempotent rename
        let resp = app
            .clone()
            .oneshot(json_req(
                "PUT",
                "/api/template-groups/Warehouse",
                json!({ "name": "Warehouse" }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let res_json = json_response(resp).await;
        assert_eq!(res_json["group"], "Warehouse");
        assert!(dir.join("Warehouse").is_dir());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn template_group_rename_refusals() {
        let dir = temp_templates_dir();
        std::fs::create_dir_all(dir.join("Shipping")).unwrap();
        std::fs::create_dir_all(dir.join("Warehouse")).unwrap();
        std::fs::write(dir.join("Shipping/t1.yaml"), template_yaml("t1")).unwrap();
        let app = build_app_in(&dir);

        // 1. Occupied destination -> 409
        let resp = app
            .clone()
            .oneshot(json_req(
                "PUT",
                "/api/template-groups/Shipping",
                json!({ "name": "Warehouse" }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
        assert!(dir.join("Shipping/t1.yaml").exists());
        assert!(dir.join("Warehouse").is_dir());

        // 2. Empty destination directory not replaced -> 409
        let empty_dest = dir.join("EmptyDest");
        std::fs::create_dir_all(&empty_dest).unwrap();
        let resp = app
            .clone()
            .oneshot(json_req(
                "PUT",
                "/api/template-groups/Shipping",
                json!({ "name": "EmptyDest" }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
        assert!(dir.join("Shipping/t1.yaml").exists());
        assert!(empty_dest.is_dir());

        // 3. Name carrying a slash -> 422 template_group_invalid
        let resp = app
            .clone()
            .oneshot(json_req(
                "PUT",
                "/api/template-groups/Shipping",
                json!({ "name": "Warehouse/Pallets" }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let err = json_response(resp).await;
        assert_eq!(err["error"]["details"]["reason"], "template_group_invalid");

        // 4. Invalid new name (CON, .., long) -> 422 template_group_invalid
        for bad_name in &["CON", "..", &"a".repeat(200), "bad\tname"] {
            let resp = app
                .clone()
                .oneshot(json_req(
                    "PUT",
                    "/api/template-groups/Shipping",
                    json!({ "name": bad_name }).to_string(),
                ))
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
            let err = json_response(resp).await;
            assert_eq!(err["error"]["details"]["reason"], "template_group_invalid");
        }

        // 5. Body omitting key -> 400
        let resp = app
            .clone()
            .oneshot(json_req(
                "PUT",
                "/api/template-groups/Shipping",
                "{}".to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // 6. Unknown group -> 404
        let resp = app
            .clone()
            .oneshot(json_req(
                "PUT",
                "/api/template-groups/UnknownGroup",
                json!({ "name": "NewName" }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // 7. Malformed percent sequence -> 400 path_param_invalid
        let resp = app
            .clone()
            .oneshot(json_req(
                "PUT",
                "/api/template-groups/%ZZ",
                json!({ "name": "ValidName" }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let err = json_response(resp).await;
        assert_eq!(err["error"]["details"]["reason"], "path_param_invalid");

        // 8. Regular file as path component -> 422 template_group_unsafe_path (says not a directory)
        std::fs::write(dir.join("file_comp"), b"content").unwrap();
        let resp = app
            .clone()
            .oneshot(json_req(
                "PUT",
                "/api/template-groups/file_comp",
                json!({ "name": "ValidName" }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let err = json_response(resp).await;
        assert_eq!(
            err["error"]["details"]["reason"],
            "template_group_unsafe_path"
        );
        assert!(err["error"]["message"]
            .as_str()
            .unwrap()
            .contains("is not a directory"));

        #[cfg(unix)]
        {
            // 9. Symlink as path component -> 422 template_group_unsafe_path (says symbolic link)
            let ext_dir = temp_templates_dir();
            std::os::unix::fs::symlink(&ext_dir, dir.join("sym_comp")).unwrap();
            let resp = app
                .clone()
                .oneshot(json_req(
                    "PUT",
                    "/api/template-groups/sym_comp",
                    json!({ "name": "ValidName" }).to_string(),
                ))
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
            let err = json_response(resp).await;
            assert_eq!(
                err["error"]["details"]["reason"],
                "template_group_unsafe_path"
            );
            assert!(err["error"]["message"]
                .as_str()
                .unwrap()
                .contains("is a symbolic link"));
            std::fs::remove_dir_all(&ext_dir).ok();
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn template_group_rename_recasing() {
        let dir = temp_templates_dir();
        let case_sensitive = crate::fs_safe::probe_is_case_sensitive(&dir);
        std::fs::create_dir_all(dir.join("shipping")).unwrap();
        std::fs::write(dir.join("shipping/t1.yaml"), template_yaml("t1")).unwrap();
        let app = build_app_in(&dir);

        if case_sensitive {
            // Recase shipping -> Shipping on free destination succeeds
            let resp = app
                .clone()
                .oneshot(json_req(
                    "PUT",
                    "/api/template-groups/shipping",
                    json!({ "name": "Shipping" }).to_string(),
                ))
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
            let res_json = json_response(resp).await;
            assert_eq!(res_json["group"], "Shipping");

            // Now create shipping again alongside Shipping
            std::fs::create_dir_all(dir.join("shipping")).unwrap();
            let (_, _) = get_json(&app, "/api/template-groups").await;

            // Renaming shipping -> Shipping when Shipping exists gives 409
            let resp = app
                .clone()
                .oneshot(json_req(
                    "PUT",
                    "/api/template-groups/shipping",
                    json!({ "name": "Shipping" }).to_string(),
                ))
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::CONFLICT);
        } else {
            // Case-folding: recasing behaviour depends on whether the
            // filesystem reports the destination as existing for a
            // no-replace rename. APFS allows the rename (200) where
            // creation of a sibling would be 409; both are permitted
            // by spec 889-893, so we assert the contract for what we
            // actually get without returning early.
            let resp = app
                .clone()
                .oneshot(json_req(
                    "PUT",
                    "/api/template-groups/shipping",
                    json!({ "name": "Shipping" }).to_string(),
                ))
                .await
                .unwrap();
            if resp.status() == StatusCode::OK {
                // Filesystem performed the recasing; confirm new spelling is served
                let res_json = json_response(resp).await;
                assert_eq!(res_json["group"], "Shipping");
                let (status, groups) = get_json(&app, "/api/template-groups").await;
                assert_eq!(status, StatusCode::OK);
                let list: Vec<String> = serde_json::from_value(groups).unwrap();
                assert_eq!(list.len(), 1);
                assert!(
                    list.contains(&"Shipping".to_string()),
                    "after recasing, listing must contain Shipping, got {list:?}"
                );
                // Second phase cannot create a distinct sibling on this filesystem,
                // so the listing still holds exactly one entry and a repeat recasing
                // to the same spelling is idempotent (200) or 409 depending on
                // whether the source alias is considered existing. We assert that
                // we do not create a second group and that the service still
                // answers consistently.
                std::fs::create_dir_all(dir.join("shipping")).unwrap();
                let (status, groups) = get_json(&app, "/api/template-groups").await;
                assert_eq!(status, StatusCode::OK);
                let list: Vec<String> = serde_json::from_value(groups).unwrap();
                assert_eq!(list.len(), 1);
                // Attempt recasing again in either direction; should not create a second group
                let resp2 = app
                    .clone()
                    .oneshot(json_req(
                        "PUT",
                        "/api/template-groups/Shipping",
                        json!({ "name": "shipping" }).to_string(),
                    ))
                    .await
                    .unwrap();
                assert_eq!(resp2.status(), StatusCode::OK);
                let (status, groups) = get_json(&app, "/api/template-groups").await;
                assert_eq!(status, StatusCode::OK);
                let list: Vec<String> = serde_json::from_value(groups).unwrap();
                assert_eq!(list.len(), 1);
            } else {
                // Filesystem reported destination as existing: 409 and nothing renamed
                assert_eq!(resp.status(), StatusCode::CONFLICT);
                let (status, groups) = get_json(&app, "/api/template-groups").await;
                assert_eq!(status, StatusCode::OK);
                let list: Vec<String> = serde_json::from_value(groups).unwrap();
                assert_eq!(list.len(), 1);
                assert!(
                    list.contains(&"shipping".to_string()),
                    "after failed recasing, listing must still contain shipping, got {list:?}"
                );
                // Second phase cannot create a second directory, so listing
                // still holds exactly one entry and a repeat request is 409 again
                let resp2 = app
                    .clone()
                    .oneshot(json_req(
                        "PUT",
                        "/api/template-groups/shipping",
                        json!({ "name": "Shipping" }).to_string(),
                    ))
                    .await
                    .unwrap();
                assert_eq!(resp2.status(), StatusCode::CONFLICT);
            }
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn template_group_rename_whole_path_limits() {
        let dir = temp_templates_dir();
        std::fs::create_dir_all(dir.join("Ancestor/Subgroup")).unwrap();
        std::fs::write(dir.join("Ancestor/Subgroup/t1.yaml"), template_yaml("t1")).unwrap();
        let app = build_app_in(&dir);

        // 1. Own path exceeding 255 chars
        let long_parent = format!(
            "{}/{}/{}/{}",
            "a".repeat(60),
            "b".repeat(60),
            "c".repeat(60),
            "d".repeat(40)
        );
        std::fs::create_dir_all(dir.join(&long_parent).join("sub")).unwrap();
        let resp = app
            .clone()
            .oneshot(json_req(
                "PUT",
                &format!("/api/template-groups/{long_parent}/sub"),
                json!({ "name": "e".repeat(60) }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let err = json_response(resp).await;
        assert_eq!(err["error"]["details"]["reason"], "template_group_invalid");

        // 2. Descendant crossing limit
        // Create an ancestor whose rename pushes descendant past 255 chars
        let deep = dir
            .join("P")
            .join("a".repeat(60))
            .join("b".repeat(60))
            .join("c".repeat(60))
            .join("d".repeat(60));
        std::fs::create_dir_all(&deep).unwrap();

        // Rename "P" to 60-character name so deep descendant path (60*5 + 4 = 304) exceeds 255 chars:
        let resp = app
            .clone()
            .oneshot(json_req(
                "PUT",
                "/api/template-groups/P",
                json!({ "name": "n".repeat(60) }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let err = json_response(resp).await;
        assert_eq!(err["error"]["details"]["reason"], "template_group_invalid");
        assert!(dir.join("P").is_dir());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn template_group_empty_destination_fails_against_replace_rename() {
        let dir = temp_templates_dir();
        let src = dir.join("SrcGroup");
        let dest = dir.join("DestGroup");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(src.join("t1.yaml"), template_yaml("t1")).unwrap();

        let root_fd = crate::fs_safe::open_dir_handle(&dir).unwrap();

        // fs_safe::rename_group_dir uses NOREPLACE and returns 409 Conflict
        let err =
            crate::fs_safe::rename_group_dir(root_fd.as_fd(), "SrcGroup", "DestGroup").unwrap_err();
        assert_eq!(err.status(), StatusCode::CONFLICT);
        assert!(src.join("t1.yaml").exists());
        assert!(dest.is_dir());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn template_symlink_refusals_on_create_move_and_delete() {
        let dir = temp_templates_dir();
        let ext_dir = temp_templates_dir();
        std::fs::write(dir.join("t1.yaml"), template_yaml("t1")).unwrap();
        let sym_dir = dir.join("outside_sym");
        std::os::unix::fs::symlink(&ext_dir, &sym_dir).unwrap();

        let app = build_app_in(&dir);

        // Create with caller-supplied symlink group -> 422 TemplateInvalid template_group_unsafe_path
        let resp = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/new_sym?group=outside_sym",
                "PUT",
                template_yaml("new_sym"),
            ))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let err = json_response(resp).await;
        assert_eq!(
            err["error"]["details"]["reason"],
            "template_group_unsafe_path"
        );

        // Move to caller-supplied symlink group -> 422 TemplateInvalid template_group_unsafe_path
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/templates/t1/group")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"group":"outside_sym"}"#))
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let err = json_response(resp).await;
        assert_eq!(
            err["error"]["details"]["reason"],
            "template_group_unsafe_path"
        );

        // Delete symlink group -> 400 InvalidRequest template_group_unsafe_path
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/template-groups/outside_sym")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let err = json_response(resp).await;
        assert_eq!(
            err["error"]["details"]["reason"],
            "template_group_unsafe_path"
        );

        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&ext_dir).ok();
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
            "data": { "message": "Hello", "code": "QR-1" },
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
            "data": { "message": "Hi", "code": "Q" }
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
            let payload = json!({"template":"brother_24mm_qr","printer":"ok-printer","data":{"message":"x","code":"y"},"copies":bad});
            let resp = app
                .clone()
                .oneshot(json_req("POST", "/api/print", payload.to_string()))
                .await
                .expect("request");
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "copies={bad}");
            let body = json_response(resp).await;
            assert_eq!(body["error"]["code"], "InvalidRequest");
            assert_eq!(body["error"]["details"]["reason"], "copies_invalid");
        }
    }

    #[tokio::test]
    async fn print_webhook_unknown_template_is_404() {
        let app = build_app();
        create_fake_printer(&app, "ok-printer", false).await;
        let payload = json!({"template":"nope","printer":"ok-printer","data":{}});
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
        let payload = json!({"template":"brother_24mm_qr","printer":"p","data":{"message":big}});
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
    async fn api_print_fields_is_rejected() {
        let app = build_app();
        create_fake_printer(&app, "ok-printer", false).await;
        let payload = json!({
            "template": "brother_24mm_qr",
            "printer": "ok-printer",
            "fields": { "message": "Hello", "code": "QR-1" }
        });
        let res = app
            .clone()
            .oneshot(json_req("POST", "/api/print", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
        assert_eq!(body["error"]["details"]["reason"], "json_malformed");
        let err = body["error"]["details"]["error"].as_str().unwrap_or("");
        assert!(
            err.contains("fields"),
            "expected error to name `fields`, got {err}"
        );
        // No dispatch: recent-templates still empty (rejection before handler).
        let recents = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/recent-templates")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(recents.status(), StatusCode::OK);
        assert_eq!(json_response(recents).await, json!([]));
    }

    #[tokio::test]
    async fn api_print_fields_alongside_data_is_rejected() {
        let app = build_app();
        create_fake_printer(&app, "ok-printer", false).await;
        let payload = json!({
            "template": "brother_24mm_qr",
            "printer": "ok-printer",
            "data": { "message": "Hello", "code": "QR-1" },
            "fields": { "message": "From fields" }
        });
        let res = app
            .clone()
            .oneshot(json_req("POST", "/api/print", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
        assert_eq!(body["error"]["details"]["reason"], "json_malformed");
        // No dispatch: recent-templates still empty (rejection before handler).
        let recents = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/recent-templates")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(recents.status(), StatusCode::OK);
        assert_eq!(json_response(recents).await, json!([]));
    }

    #[tokio::test]
    async fn api_print_neither_data_nor_fields_is_rejected() {
        let app = build_app();
        create_fake_printer(&app, "ok-printer", false).await;
        let payload = json!({"template":"brother_24mm_qr","printer":"ok-printer","copies":1});
        let res = app
            .clone()
            .oneshot(json_req("POST", "/api/print", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
        assert_eq!(body["error"]["details"]["reason"], "json_malformed");
        // No label was printed from an empty map: recent-templates still empty.
        let recents = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/recent-templates")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(recents.status(), StatusCode::OK);
        assert_eq!(json_response(recents).await, json!([]));
    }

    #[tokio::test]
    async fn api_print_unknown_key_is_rejected() {
        let app = build_app();
        create_fake_printer(&app, "ok-printer", false).await;
        let payload = json!({
            "template": "brother_24mm_qr",
            "printer": "ok-printer",
            "data": { "message": "Hello", "code": "QR-1" },
            "extra": 1
        });
        let res = app
            .clone()
            .oneshot(json_req("POST", "/api/print", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
        assert_eq!(body["error"]["details"]["reason"], "json_malformed");
        let err = body["error"]["details"]["error"].as_str().unwrap_or("");
        assert!(
            err.contains("extra"),
            "expected error to name `extra`, got {err}"
        );
    }

    #[tokio::test]
    async fn api_print_missing_data_reports_json_malformed_not_copies_invalid() {
        let app = build_app();
        create_fake_printer(&app, "ok-printer", false).await;
        let payload = json!({"template":"brother_24mm_qr","printer":"ok-printer","copies":0});
        let res = app
            .clone()
            .oneshot(json_req("POST", "/api/print", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
        assert_eq!(body["error"]["details"]["reason"], "json_malformed");
    }

    #[tokio::test]
    async fn api_print_empty_data_is_passed_to_template() {
        let app = build_app();
        create_fake_printer(&app, "ok-printer", false).await;
        let payload = json!({"template":"brother_24mm_qr","printer":"ok-printer","data":{}});
        let res = app
            .clone()
            .oneshot(json_req("POST", "/api/print", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "BatchInvalid");
        let failures = body["error"]["details"]["failures"]
            .as_array()
            .expect("failures array");
        assert!(!failures.is_empty(), "expected at least one failure");
        let first = &failures[0];
        let msg = first["message"].as_str().unwrap_or("");
        assert!(
            msg.contains("message") || msg.contains("code"),
            "expected failure to name missing param, got {msg}"
        );
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
        assert!(
            schemas.contains_key("Color"),
            "Color missing in openapi schemas"
        );
        assert!(
            schemas.contains_key("DynamicValue_Color"),
            "DynamicValue_Color missing in openapi schemas"
        );
        assert!(
            !schemas.contains_key("Ink"),
            "Ink must not be a schema component"
        );
        assert!(
            !schemas.contains_key("DynamicValue_Ink"),
            "DynamicValue_Ink must not be a schema component"
        );
        assert!(
            !schemas.contains_key("String"),
            "String must not be a schema component"
        );
        let color_schema = serde_json::to_value(&schemas["Color"]).unwrap();
        assert_eq!(
            color_schema["type"], "string",
            "Color schema must have type: string, got: {color_schema}"
        );
    }

    #[test]
    fn openapi_print_request_is_strict() {
        use utoipa::OpenApi;
        let doc = crate::openapi::ApiDoc::openapi();
        let schemas = doc.components.as_ref().unwrap().schemas.clone();
        assert!(
            schemas.contains_key("PrintRequest"),
            "PrintRequest missing in openapi schemas"
        );
        let schema = serde_json::to_value(&schemas["PrintRequest"]).unwrap();
        let required = schema["required"].as_array().expect("required array");
        let req_strs: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
        assert!(
            req_strs.contains(&"data"),
            "data must be required, got {schema}"
        );
        let props = schema["properties"].as_object().expect("properties object");
        assert!(
            !props.contains_key("fields"),
            "fields must not be a property, got {schema}"
        );
        assert!(
            props.contains_key("data"),
            "data must be a property, got {schema}"
        );
        assert_eq!(
            schema["additionalProperties"], false,
            "additionalProperties must be false, got {schema}"
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

    use std::cell::RefCell;
    use std::sync::Once;

    thread_local! {
        static TEST_LOG_BUFFER: RefCell<Option<Arc<std::sync::Mutex<Vec<u8>>>>> = const { RefCell::new(None) };
    }

    struct TestLogWriter;

    impl std::io::Write for TestLogWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            TEST_LOG_BUFFER.with(|cell| {
                if let Some(target) = cell.borrow().as_ref() {
                    target.lock().unwrap().extend_from_slice(buf);
                }
            });
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for TestLogWriter {
        type Writer = TestLogWriter;
        fn make_writer(&'a self) -> Self::Writer {
            TestLogWriter
        }
    }

    static INIT_TEST_TRACING: Once = Once::new();

    fn init_test_tracing() {
        INIT_TEST_TRACING.call_once(|| {
            let _ = tracing_subscriber::fmt()
                .with_writer(TestLogWriter)
                .with_max_level(tracing::Level::TRACE)
                .with_ansi(false)
                .try_init();
        });
    }

    #[tokio::test]
    async fn auth_login_malformed_password_not_in_logs() {
        init_test_tracing();
        let buf = Arc::new(std::sync::Mutex::new(Vec::new()));
        TEST_LOG_BUFFER.with(|cell| {
            *cell.borrow_mut() = Some(buf.clone());
        });

        let app = build_app();
        let body_str = r#"{"username":"admin","password":12345}"#;
        let req = Request::builder()
            .method("POST")
            .uri("/api/auth/login")
            .header("content-type", "application/json")
            .header("host", "localhost")
            .header("origin", "http://localhost")
            .body(Body::from(body_str))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
        assert_eq!(body["error"]["details"]["reason"], "json_malformed");
        assert!(
            body["error"]["details"]["error"].is_string(),
            "details.error must carry the parser message, got {body}"
        );

        TEST_LOG_BUFFER.with(|cell| {
            *cell.borrow_mut() = None;
        });

        let log_str = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        assert!(
            !log_str.contains("12345"),
            "log must not contain password value 12345, got: {log_str}"
        );
        assert!(
            log_str.contains("json_malformed"),
            "log must contain reason slug json_malformed, got: {log_str}"
        );
    }

    /// Asserts one endpoint answers a malformed JSON body with the documented envelope (ADR-0075).
    ///
    /// Shared by the enumerated sweep below and the OpenAPI-derived one after it, so the two cannot
    /// come to check different things about the same contract.
    async fn assert_malformed_body_returns_envelope(method: &str, uri: &str) {
        let app = build_app();
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .header("host", "localhost")
            .header("origin", "http://localhost")
            .body(Body::from("{ not valid json"))
            .unwrap();

        let resp = app.oneshot(req).await.expect(uri);
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "status for {method} {uri}"
        );
        let ct = resp
            .headers()
            .get("content-type")
            .expect("content-type")
            .to_str()
            .unwrap();
        assert!(
            ct.starts_with("application/json"),
            "content-type for {method} {uri} must be application/json, got {ct}"
        );

        let body = json_response(resp).await;
        assert_eq!(
            body["error"]["code"], "InvalidRequest",
            "code for {method} {uri}"
        );
        assert_eq!(
            body["error"]["details"]["reason"], "json_malformed",
            "reason for {method} {uri} (a path parameter that fails to deserialize surfaces here \
             as path_param_invalid instead)"
        );
        assert!(
            body["error"]["details"]["error"].is_string()
                && !body["error"]["details"]["error"]
                    .as_str()
                    .unwrap()
                    .is_empty(),
            "details.error for {method} {uri} must be a non-empty string, got {body}"
        );
    }

    #[tokio::test]
    async fn all_nineteen_json_endpoints_reject_malformed_body_identically() {
        let endpoints = [
            ("POST", "/api/printers"),
            ("POST", "/api/printers/probe"),
            ("PUT", "/api/printers/test-printer"),
            ("PUT", "/api/variables/test-var"),
            ("PUT", "/api/settings/default_printer"),
            ("POST", "/api/datetime-formats/preview"),
            ("POST", "/api/connections"),
            ("PUT", "/api/connections/conn-1"),
            ("POST", "/api/connections/conn-1/browse"),
            ("POST", "/api/connections/conn-1/materialize"),
            ("POST", "/api/auth/setup"),
            ("POST", "/api/auth/login"),
            ("POST", "/api/auth/password"),
            ("POST", "/api/users"),
            ("POST", "/api/tokens"),
            ("PUT", "/api/templates/brother_12mm/group"),
            ("POST", "/api/batch"),
            ("POST", "/api/print"),
            ("POST", "/api/render/label"),
        ];

        assert_eq!(endpoints.len(), 19);

        for (method, uri) in endpoints {
            assert_malformed_body_returns_envelope(method, uri).await;
        }
    }

    /// Every JSON-body operation in the published OpenAPI document rejects a malformed body with the
    /// documented envelope.
    ///
    /// `all_nineteen_json_endpoints_reject_malformed_body_identically` enumerates today's endpoints,
    /// so a handler added tomorrow against `axum::Json` is invisible to it. This one derives its list
    /// from `ApiDoc::openapi()` instead, so endpoint number twenty is covered on the day it is
    /// documented with a JSON request body. Dropping the `request_body` attribute does not hide an
    /// operation: utoipa's `axum_extras` infers the body from a handler argument typed `Json`, `Form`
    /// or `Bytes`, matched on the type's last path segment, so `crate::extract::Json` fires it and
    /// renaming that extractor would silently stop it. What stays invisible is a route missing from
    /// `openapi.rs` altogether, which is #229. Defence in depth, not a guarantee (#230).
    #[tokio::test]
    async fn every_documented_json_body_endpoint_returns_the_error_envelope() {
        use utoipa::OpenApi;

        fn is_json(content_type: &str) -> bool {
            let base = content_type[..content_type.find(';').unwrap_or(content_type.len())]
                .trim()
                .to_ascii_lowercase();
            base == "application/json" || base.ends_with("+json")
        }

        /// `/templates/{id}` -> `/templates/ph`. The placeholder has to route and to deserialize as
        /// the declared parameter type; every path parameter is a `String` today, and a future
        /// endpoint typing one as an integer would fail the `reason` assertion rather than the
        /// envelope it is aimed at.
        fn substitute_path_params(path: &str) -> String {
            let mut out = String::with_capacity(path.len());
            let mut rest = path;
            while let Some(open) = rest.find('{') {
                out.push_str(&rest[..open]);
                let close = rest[open..]
                    .find('}')
                    .map(|i| open + i)
                    .unwrap_or_else(|| panic!("unclosed path parameter in {path}"));
                out.push_str("ph");
                rest = &rest[close + 1..];
            }
            out.push_str(rest);
            out
        }

        let doc = crate::openapi::ApiDoc::openapi();
        let mut endpoints: Vec<(&'static str, String)> = Vec::new();
        for (path, item) in &doc.paths.paths {
            let operations = [
                ("GET", &item.get),
                ("PUT", &item.put),
                ("POST", &item.post),
                ("DELETE", &item.delete),
                ("PATCH", &item.patch),
                ("HEAD", &item.head),
                ("OPTIONS", &item.options),
                ("TRACE", &item.trace),
            ];
            for (method, operation) in operations {
                let Some(operation) = operation else { continue };
                let Some(body) = operation.request_body.as_ref() else {
                    continue;
                };
                if body.content.keys().any(|ct| is_json(ct)) {
                    endpoints.push((method, format!("/api{}", substitute_path_params(path))));
                }
            }
        }

        // Today's true count, not the enumerated test's 19. A floor set below it would let the two
        // endpoints only this test covers -- `POST /templates/{id}/inputs` and
        // `PUT /template-groups/{path}` -- drop out of discovery with every test still green, which
        // is the coverage hole this test exists to close. The floor moves up when an endpoint is
        // added and only ever moves down deliberately.
        assert!(
            endpoints.len() >= 21,
            "expected at least the 21 documented JSON-body operations, found {}: {endpoints:?}",
            endpoints.len()
        );

        for (method, uri) in &endpoints {
            assert_malformed_body_returns_envelope(method, uri).await;
        }
    }

    #[tokio::test]
    async fn put_connection_wrong_shape_and_missing_key_are_both_400_json_malformed() {
        let app = build_app();
        // Wrong-shape body: connector is integer instead of string
        let req1 = Request::builder()
            .method("PUT")
            .uri("/api/connections/conn-1")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"connector":42,"name":"home","base_url":"http://hb.lan:7745"}"#,
            ))
            .unwrap();
        let resp1 = app.oneshot(req1).await.unwrap();
        assert_eq!(resp1.status(), StatusCode::BAD_REQUEST);
        let body1 = json_response(resp1).await;
        assert_eq!(body1["error"]["code"], "InvalidRequest");
        assert_eq!(body1["error"]["details"]["reason"], "json_malformed");

        let app = build_app();
        // Missing required key 'name'
        let req2 = Request::builder()
            .method("PUT")
            .uri("/api/connections/conn-1")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"connector":"nope","base_url":"http://hb.lan:7745"}"#,
            ))
            .unwrap();
        let resp2 = app.oneshot(req2).await.unwrap();
        assert_eq!(resp2.status(), StatusCode::BAD_REQUEST);
        let body2 = json_response(resp2).await;
        assert_eq!(body2["error"]["code"], "InvalidRequest");
        assert_eq!(body2["error"]["details"]["reason"], "json_malformed");
    }

    #[tokio::test]
    async fn content_type_scenarios() {
        // 1. Content-Type absent -> 415 UnsupportedMediaType
        let app = build_app();
        let req = Request::builder()
            .method("POST")
            .uri("/api/print")
            .body(Body::from(r#"{"template":"brother_12mm","copies":1}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "UnsupportedMediaType");

        // 2. Non-JSON Content-Type text/plain -> 415 UnsupportedMediaType
        let app = build_app();
        let req = Request::builder()
            .method("POST")
            .uri("/api/print")
            .header("content-type", "text/plain")
            .body(Body::from(r#"{"template":"brother_12mm","copies":1}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "UnsupportedMediaType");

        // 3. Suffixed JSON Content-Type application/problem+json -> not 415, body deserialized
        let app = build_app();
        let req = Request::builder()
            .method("POST")
            .uri("/api/print")
            .header("content-type", "application/problem+json")
            .body(Body::from(
                r#"{"template":"non_existent_template","printer":"some-printer","data":{},"copies":1}"#,
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_ne!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        // Request reached handler and returned 404 TemplateNotFound
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn oversized_body_post_print_returns_413() {
        let app = build_app();
        // DefaultBodyLimit on print is 64 KiB (65536 bytes)
        let large_string = "a".repeat(70 * 1024);
        let payload = serde_json::json!({
            "template": "brother_12mm",
            "copies": 1,
            "data": { "key": large_string }
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/print")
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "PayloadTooLarge");
    }

    #[tokio::test]
    async fn four_already_enveloped_endpoints_have_error_envelope() {
        let endpoints = [
            ("PUT", "/api/templates/brother_12mm/group"),
            ("POST", "/api/batch"),
            ("POST", "/api/print"),
            ("POST", "/api/render/label"),
        ];
        for (method, uri) in endpoints {
            let app = build_app();
            let req = Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from("{ not valid json"))
                .unwrap();
            let resp = app.oneshot(req).await.unwrap();
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
            let body = json_response(resp).await;
            assert_eq!(body["error"]["code"], "InvalidRequest");
            assert_eq!(body["error"]["message"], "Malformed JSON body");
            assert_eq!(body["error"]["details"]["reason"], "json_malformed");
            assert!(body["error"]["details"]["error"].is_string());
        }
    }

    #[tokio::test]
    async fn path_param_invalid_utf8_on_template_source() {
        let app = build_app();
        let req = Request::builder()
            .method("GET")
            .uri("/api/templates/%FF/source")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let ct = resp
            .headers()
            .get("content-type")
            .expect("content-type")
            .to_str()
            .unwrap();
        assert!(ct.starts_with("application/json"));
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
        assert_eq!(body["error"]["details"]["reason"], "path_param_invalid");
    }

    #[tokio::test]
    async fn path_param_type_mismatch_returns_400_path_param_invalid() {
        use axum::response::IntoResponse;
        async fn dummy_numeric_handler(
            crate::extract::Path(_id): crate::extract::Path<u32>,
        ) -> axum::response::Response {
            StatusCode::OK.into_response()
        }
        let router =
            axum::Router::new().route("/items/{id}", axum::routing::get(dummy_numeric_handler));
        let req = Request::builder()
            .uri("/items/not-a-number")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
        assert_eq!(body["error"]["details"]["reason"], "path_param_invalid");
    }

    #[tokio::test]
    async fn path_param_server_classified_cases_return_500_internal() {
        use axum::response::IntoResponse;
        // Case 1: Route / handler arity disagreement (handler expects 2 params, route defines 1)
        async fn arity_mismatch_handler(
            crate::extract::Path((_a, _b)): crate::extract::Path<(u32, u32)>,
        ) -> axum::response::Response {
            StatusCode::OK.into_response()
        }
        let router =
            axum::Router::new().route("/items/{id}", axum::routing::get(arity_mismatch_handler));
        let req = Request::builder()
            .uri("/items/123")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "Internal");
        assert_ne!(body["error"]["details"]["reason"], "path_param_invalid");

        // Case 2: Path parameters absent from the request (MissingPathParams)
        use axum::extract::FromRequestParts;
        let req = Request::builder().uri("/").body(Body::empty()).unwrap();
        let (mut parts, _) = req.into_parts();
        let res = crate::extract::Path::<String>::from_request_parts(&mut parts, &()).await;
        let err = res.expect_err("should reject missing path params");
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "Internal");
        assert_ne!(body["error"]["details"]["reason"], "path_param_invalid");
    }

    #[tokio::test]
    async fn admission_precedence_with_malformed_body() {
        // 1. Unauthenticated request with malformed body -> 401 Unauthorized
        let (templates, templates_dir) = crate::templates::load_all_for_tests();
        let store = Store::open_in_memory().expect("store");
        let unauthed_app = app(Arc::new(AppState::new(templates, templates_dir, store)));
        let req = Request::builder()
            .method("POST")
            .uri("/api/printers")
            .header("content-type", "application/json")
            .body(Body::from("{ not valid json"))
            .unwrap();
        let resp = unauthed_app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "Unauthorized");
        assert_ne!(body["error"]["details"]["reason"], "json_malformed");

        // 2. Mismatched origin on state-changing request with cookie -> 403 Forbidden
        let (templates, templates_dir) = crate::templates::load_all_for_tests();
        let store = Store::open_in_memory().expect("store");
        let state = Arc::new(AppState::new(templates, templates_dir, store));
        let cookie_app = app(state.clone());
        let user = state.store().create_user("admin", "hash").await.unwrap();
        let session_secret = "test-session-secret-for-csrf";
        state
            .store()
            .create_session(
                &crate::auth::sha256_hex(session_secret),
                &user.id,
                "+30 days",
            )
            .await
            .unwrap();
        let req = Request::builder()
            .method("POST")
            .uri("/api/connections")
            .header("content-type", "application/json")
            .header("cookie", format!("labeler_session={session_secret}"))
            .header("host", "localhost")
            .header("origin", "http://evil.example.com")
            .body(Body::from("{ not valid json"))
            .unwrap();
        let resp = cookie_app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "Forbidden");
        assert_ne!(body["error"]["details"]["reason"], "json_malformed");

        // 3. Auth-managed route under LABELER_NO_AUTH=true with malformed body -> 403 Forbidden
        let (templates, templates_dir) = crate::templates::load_all_for_tests();
        let store = Store::open_in_memory().expect("store");
        let no_auth_app = app(Arc::new(
            AppState::new(templates, templates_dir, store).with_no_auth(true),
        ));
        let req = Request::builder()
            .method("POST")
            .uri("/api/auth/password")
            .header("content-type", "application/json")
            .body(Body::from("{ not valid json"))
            .unwrap();
        let resp = no_auth_app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "Forbidden");
        assert_ne!(body["error"]["details"]["reason"], "json_malformed");
    }

    #[test]
    fn src_api_binds_json_and_path_from_crate_extract() {
        let api_src = include_str!("api.rs");
        assert!(
            api_src.contains("use crate::extract::{Json, Path}")
                || (api_src.contains("crate::extract")
                    && api_src.contains("Json")
                    && api_src.contains("Path")),
            "src/api.rs must import Json and Path from crate::extract"
        );

        let start = api_src
            .find("use axum::{")
            .expect("src/api.rs must contain `use axum::{` import block");
        let rest = &api_src[start..];
        let end = rest
            .find("};")
            .expect("src/api.rs `use axum::{` block must terminate with `};`");
        let axum_tree = &rest[..end + 2];

        assert!(
            !axum_tree.contains("Json"),
            "src/api.rs should not import Json from axum: {axum_tree}"
        );
        assert!(
            !axum_tree.contains("Path"),
            "src/api.rs should not import Path from axum: {axum_tree}"
        );

        for line in api_src.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("use axum::") {
                assert!(
                    !trimmed.contains("Json"),
                    "src/api.rs should not import Json from axum: {trimmed}"
                );
                assert!(
                    !trimmed.contains("Path"),
                    "src/api.rs should not import Path from axum: {trimmed}"
                );
            }
        }
    }

    /// Proves that flow container templates serialize without `at` or `to` on packed children
    /// in API responses and round-trip successfully through template modification endpoints.
    #[tokio::test]
    async fn template_with_flow_container_http_round_trip() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);
        // Every interpolated name is read only from inside the flow container, so the derived
        // inputs are empty unless the walk descends into packed children.
        let flow_yaml = r#"name: Flow HTTP
description: Flow test
unit: mm
dpi: 200
params:
  mode:
    type: enum
    values: [short, long]
    default: short
  title:
    type: string
  subtitle:
    type: string
  code:
    type: string
format:
  type: single
  width: 60.0
  height: 30.0
layout:
  - type: container
    at: [0.0, 0.0]
    size: [60.0, 30.0]
    flow:
      direction: row
      gap: 5.0
    items:
      - type: text
        value: "{title}"
        size: [20.0, 10.0]
        font_size: 8.0
      - type: qr
        value: "{code}"
        size: [10.0, 10.0]
      - type: container
        size: [15.0, 10.0]
        when:
          mode: long
        items:
          - type: text
            value: "{subtitle}"
            at: [0.0, 0.0]
            size: [15.0, 10.0]
            font_size: 6.0
"#;
        let resp = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/flow_tpl",
                "PUT",
                flow_yaml.to_string(),
            ))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::CREATED);

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/templates/flow_tpl")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::OK);
        let detail = json_response(resp).await;

        let container = &detail["layout"][0];
        assert_eq!(container["type"], "container");
        assert_eq!(container["flow"]["direction"], "row");
        assert_eq!(container["flow"]["gap"], 5.0);

        let child_text = &container["items"][0];
        assert_eq!(child_text["type"], "text");
        assert!(
            child_text.get("at").is_none(),
            "packed text child must not serialize 'at'"
        );
        assert!(
            child_text.get("to").is_none(),
            "packed text child must not serialize 'to'"
        );

        let child_qr = &container["items"][1];
        assert_eq!(child_qr["type"], "qr");
        assert!(
            child_qr.get("at").is_none(),
            "packed qr child must not serialize 'at'"
        );
        assert!(
            child_qr.get("to").is_none(),
            "packed qr child must not serialize 'to'"
        );

        let input_names = |inputs: &Value| -> Vec<String> {
            inputs
                .as_array()
                .expect("inputs array")
                .iter()
                .map(|input| input["name"].as_str().expect("input name").to_string())
                .collect()
        };

        let all_names = input_names(&detail["inputs"]["all"]);
        for name in ["mode", "title", "subtitle", "code"] {
            assert!(
                all_names.contains(&name.to_string()),
                "{name} is read only by a packed child, so inputs.all must carry it, got {all_names:?}"
            );
        }

        let default_names = input_names(&detail["inputs"]["default"]);
        assert!(
            !default_names.contains(&"subtitle".to_string()),
            "subtitle sits under a packed child whose when: fails at mode=short, so the per-label \
             inputs must drop it, got {default_names:?}"
        );
        for name in ["mode", "title", "code"] {
            assert!(
                default_names.contains(&name.to_string()),
                "{name} is unconditional, so the per-label inputs must keep it, got {default_names:?}"
            );
        }

        let resp_src = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/templates/flow_tpl/source")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(resp_src.status(), StatusCode::OK);
        let src_bytes = bytes_response(resp_src).await;
        let src_body = String::from_utf8(src_bytes).expect("utf8");

        let resp_put = app
            .clone()
            .oneshot(yaml_post("/api/templates/flow_tpl", "PUT", src_body))
            .await
            .expect("request");
        assert_eq!(resp_put.status(), StatusCode::OK);
    }
}

#[cfg(test)]
mod auth_http_tests {
    use super::store::Store;
    use super::{app, AppState};
    use crate::TemplateRegistry;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use serde_json::Value;
    use std::sync::Arc;
    use tower::ServiceExt;

    fn test_app() -> axum::Router {
        let (templates, templates_dir) = crate::templates::load_all_for_tests();
        let store = Store::open_in_memory().expect("store");
        app(Arc::new(AppState::new(templates, templates_dir, store)))
    }

    fn test_app_with_state() -> (axum::Router, Arc<AppState>) {
        let (templates, templates_dir) = crate::templates::load_all_for_tests();
        let store = Store::open_in_memory().expect("store");
        let state = Arc::new(AppState::new(templates, templates_dir, store));
        (app(state.clone()), state)
    }

    fn test_app_no_auth() -> axum::Router {
        let (templates, templates_dir) = crate::templates::load_all_for_tests();
        let store = Store::open_in_memory().expect("store");
        app(Arc::new(
            AppState::new(templates, templates_dir, store).with_no_auth(true),
        ))
    }

    fn test_app_with_custom_templates(tpls: Vec<(&str, &str)>) -> (axum::Router, Arc<AppState>) {
        let (mut templates, templates_dir) = crate::templates::load_all_for_tests();
        for (id, yaml) in tpls {
            let def = crate::parse::parse_template(yaml).unwrap();
            templates.insert_for_tests(id.to_string(), None, def);
        }
        let store = Store::open_in_memory().expect("store");
        let state = Arc::new(AppState::new(templates, templates_dir, store).with_no_auth(true));
        (app(state.clone()), state)
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
            serde_json::json!({"template":"brother_24mm_qr","printer":"ok-printer","data":{}});
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
    async fn settings_default_connection_id_endpoints() {
        let (app, state) = test_app_with_state();
        let cookie = setup_login_cookie(&app).await;

        // 1. Initial GET reports default_connection_id: null, is_default: true
        let res = app
            .clone()
            .oneshot(req_get_cookie("/api/settings", &cookie))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_json(res).await;
        assert_eq!(
            body["default_connection_id"]["value"],
            serde_json::Value::Null
        );
        assert_eq!(body["default_connection_id"]["is_default"], true);

        // Create connection 1 (enabled)
        let res = app
            .clone()
            .oneshot(req_post_json_cookie(
                "/api/connections",
                r#"{"connector":"homebox","name":"conn1","base_url":"http://hb1.lan","credential":"sec"}"#,
                &cookie,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let conn1_id = body_json(res).await["id"].as_str().unwrap().to_string();

        // Create connection 2 (disabled)
        let res = app
            .clone()
            .oneshot(req_post_json_cookie(
                "/api/connections",
                r#"{"connector":"homebox","name":"conn2","base_url":"http://hb2.lan","credential":"sec","enabled":false}"#,
                &cookie,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let conn2_id = body_json(res).await["id"].as_str().unwrap().to_string();

        // 2. PUT with whitespace: stores and reflects trimmed id
        let put_body = format!(r#"{{"value":"  {}  "}}"#, conn1_id);
        let res = app
            .clone()
            .oneshot(req_put_json_cookie(
                "/api/settings/default_connection_id",
                &put_body,
                &cookie,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_json(res).await;
        assert_eq!(body["value"], conn1_id);
        assert_eq!(body["is_default"], false);

        // GET confirms stored
        let res = app
            .clone()
            .oneshot(req_get_cookie("/api/settings", &cookie))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_json(res).await;
        assert_eq!(body["default_connection_id"]["value"], conn1_id);
        assert_eq!(body["default_connection_id"]["is_default"], false);

        // 3. PUT with invalid values: unknown id, "", "   ", null, number, object each give 400
        let invalid_payloads = [
            r#"{"value":"unknown-connection-id"}"#,
            r#"{"value":""}"#,
            r#"{"value":"   "}"#,
            r#"{"value":null}"#,
            r#"{"value":123}"#,
            r#"{"value":{}}"#,
        ];
        for bad in invalid_payloads {
            let res = app
                .clone()
                .oneshot(req_put_json_cookie(
                    "/api/settings/default_connection_id",
                    bad,
                    &cookie,
                ))
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::BAD_REQUEST, "payload {bad}");
            let err = body_json(res).await;
            assert_eq!(err["error"]["code"], "InvalidRequest");
            assert_eq!(err["error"]["details"]["reason"], "setting_value_invalid");
        }

        // 4. PUT accepts a disabled connection's id
        let res = app
            .clone()
            .oneshot(req_put_json_cookie(
                "/api/settings/default_connection_id",
                &format!(r#"{{"value":"{}"}}"#, conn2_id),
                &cookie,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_json(res).await;
        assert_eq!(body["value"], conn2_id);
        assert_eq!(body["is_default"], false);

        // 5. DELETE resets to null / is_default: true
        let res = app
            .clone()
            .oneshot(req_delete_cookie(
                "/api/settings/default_connection_id",
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
        assert_eq!(
            body["default_connection_id"]["value"],
            serde_json::Value::Null
        );
        assert_eq!(body["default_connection_id"]["is_default"], true);

        // 6. GET reports a dangling stored id without erroring
        state
            .store()
            .set_setting(
                crate::settings::DEFAULT_CONNECTION_ID,
                "dangling-connection-id",
            )
            .await
            .unwrap();
        let res = app
            .clone()
            .oneshot(req_get_cookie("/api/settings", &cookie))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_json(res).await;
        assert_eq!(
            body["default_connection_id"]["value"],
            "dangling-connection-id"
        );
        assert_eq!(body["default_connection_id"]["is_default"], false);

        // 7. Deleting the default connection clears the setting and deleting a different one does not
        // Set default to conn1_id
        let res = app
            .clone()
            .oneshot(req_put_json_cookie(
                "/api/settings/default_connection_id",
                &format!(r#"{{"value":"{}"}}"#, conn1_id),
                &cookie,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // Delete conn2 (not default)
        let res = app
            .clone()
            .oneshot(req_delete_cookie(
                &format!("/api/connections/{}", conn2_id),
                &cookie,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);

        // Setting still names conn1
        let res = app
            .clone()
            .oneshot(req_get_cookie("/api/settings", &cookie))
            .await
            .unwrap();
        let body = body_json(res).await;
        assert_eq!(body["default_connection_id"]["value"], conn1_id);
        assert_eq!(body["default_connection_id"]["is_default"], false);

        // Delete conn1 (the default)
        let res = app
            .clone()
            .oneshot(req_delete_cookie(
                &format!("/api/connections/{}", conn1_id),
                &cookie,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);

        // Setting is now cleared
        let res = app
            .clone()
            .oneshot(req_get_cookie("/api/settings", &cookie))
            .await
            .unwrap();
        let body = body_json(res).await;
        assert_eq!(
            body["default_connection_id"]["value"],
            serde_json::Value::Null
        );
        assert_eq!(body["default_connection_id"]["is_default"], true);
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
                r#"{"template":"brother_24mm_qr","printer":"ok-printer","data":{"message":"x","code":"y"},"copies":1}"#,
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

    #[tokio::test]
    async fn omitted_boolean_and_enum_return_422_missing_field() {
        let yaml = r#"
name: Test Missing Param
unit: mm
dpi: 200
params:
  flag:
    type: boolean
  choice:
    type: enum
    values: [one, two]
format:
  type: single
  height: 20
  width: 50
layout:
  - type: text
    value: "{flag} {choice}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let (app, _state) = test_app_with_custom_templates(vec![("missing_param_tpl", yaml)]);

        // 1. Omit flag -> 422 MissingField named 'flag'
        let req = req_post_json(
            "/api/render/label",
            &serde_json::json!({
                "template": "missing_param_tpl",
                "data": { "choice": "one" }
            })
            .to_string(),
        );
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_json(res).await;
        assert_eq!(body["error"]["code"], "MissingField");
        assert_eq!(body["error"]["details"]["field"], "flag");

        // 2. Omit choice -> 422 MissingField named 'choice'
        let req = req_post_json(
            "/api/render/label",
            &serde_json::json!({
                "template": "missing_param_tpl",
                "data": { "flag": true }
            })
            .to_string(),
        );
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_json(res).await;
        assert_eq!(body["error"]["code"], "MissingField");
        assert_eq!(body["error"]["details"]["field"], "choice");
    }

    #[tokio::test]
    async fn omitted_datetime_returns_422_missing_field_and_declared_default_prints() {
        let yaml_no_default = r#"
name: Test Missing DateTime
unit: mm
dpi: 200
params:
  printed_on:
    type: datetime
format:
  type: single
  height: 20
  width: 50
layout:
  - type: text
    value: "{printed_on:iso_date}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let yaml_with_default = r#"
name: Test Default DateTime
unit: mm
dpi: 200
params:
  printed_on:
    type: datetime
    default: "{sys.now}"
format:
  type: single
  height: 20
  width: 50
layout:
  - type: text
    value: "{printed_on:iso_date}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let (app, _state) = test_app_with_custom_templates(vec![
            ("dt_no_default", yaml_no_default),
            ("dt_with_default", yaml_with_default),
        ]);

        // Omission without default -> 422 MissingField naming 'printed_on'
        let req = req_post_json(
            "/api/render/label",
            &serde_json::json!({
                "template": "dt_no_default",
                "data": {}
            })
            .to_string(),
        );
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_json(res).await;
        assert_eq!(body["error"]["code"], "MissingField");
        assert_eq!(body["error"]["details"]["field"], "printed_on");

        // Blank string without default -> also treated as omission -> 422 MissingField
        let req = req_post_json(
            "/api/render/label",
            &serde_json::json!({
                "template": "dt_no_default",
                "data": { "printed_on": "   " }
            })
            .to_string(),
        );
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_json(res).await;
        assert_eq!(body["error"]["code"], "MissingField");
        assert_eq!(body["error"]["details"]["field"], "printed_on");

        // null without default -> 422 MissingField
        let req = req_post_json(
            "/api/render/label",
            &serde_json::json!({
                "template": "dt_no_default",
                "data": { "printed_on": null }
            })
            .to_string(),
        );
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_json(res).await;
        assert_eq!(body["error"]["code"], "MissingField");
        assert_eq!(body["error"]["details"]["field"], "printed_on");

        // Omission, blank, null with default: "{sys.now}" all render 200 OK
        for val in [
            serde_json::json!({}),
            serde_json::json!({"printed_on": ""}),
            serde_json::json!({"printed_on": null}),
        ] {
            let req = req_post_json(
                "/api/render/label",
                &serde_json::json!({
                    "template": "dt_with_default",
                    "data": val
                })
                .to_string(),
            );
            let res = app.clone().oneshot(req).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn param_default_unresolvable_http_tests() {
        let yaml1 = r#"
name: T1
unit: mm
dpi: 200
params:
  foo:
    type: string
    default: "{vars.missing_key}"
format:
  type: single
  height: 20
  width: 50
layout:
  - type: text
    value: "{foo}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let yaml2 = r#"
name: T2
unit: mm
dpi: 200
params:
  choice:
    type: enum
    values: [alpha, beta]
    default: "{vars.bad_enum}"
format:
  type: single
  height: 20
  width: 50
layout:
  - type: text
    value: "{choice}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let yaml3 = r#"
name: T3
unit: mm
dpi: 200
params:
  dt:
    type: datetime
    default: "{vars.bad_date}"
format:
  type: single
  height: 20
  width: 50
layout:
  - type: text
    value: "{dt}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let yaml4 = r#"
name: T4
unit: mm
dpi: 200
params:
  flag:
    type: boolean
    default: "yes"
format:
  type: single
  height: 20
  width: 50
layout:
  - type: text
    value: "{flag}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let (app, state) = test_app_with_custom_templates(vec![
            ("t_bad_var", yaml1),
            ("t_bad_enum", yaml2),
            ("t_bad_date", yaml3),
            ("t_bad_bool", yaml4),
        ]);

        // 1. Missing variable in default
        let req = req_post_json(
            "/api/render/label",
            &serde_json::json!({ "template": "t_bad_var", "data": {} }).to_string(),
        );
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_json(res).await;
        assert_eq!(body["error"]["code"], "TemplateInvalid");
        assert_eq!(
            body["error"]["details"]["reason"],
            "param_default_unresolvable"
        );
        let msg1 = body["error"]["message"].as_str().unwrap();
        assert!(
            msg1.contains("foo"),
            "message '{msg1}' should name parameter 'foo'"
        );
        assert!(
            msg1.contains("vars.missing_key"),
            "message '{msg1}' should name failing token"
        );

        // 2. Resolved enum default not in allowed values
        state
            .store()
            .set_variable("bad_enum", "invalid_choice")
            .await
            .unwrap();
        let req = req_post_json(
            "/api/render/label",
            &serde_json::json!({ "template": "t_bad_enum", "data": {} }).to_string(),
        );
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_json(res).await;
        assert_eq!(body["error"]["code"], "TemplateInvalid");
        assert_eq!(
            body["error"]["details"]["reason"],
            "param_default_unresolvable"
        );
        let msg2 = body["error"]["message"].as_str().unwrap();
        assert!(
            msg2.contains("choice"),
            "message '{msg2}' should name parameter 'choice'"
        );
        assert!(
            msg2.contains("invalid_choice"),
            "message '{msg2}' should name resolved value"
        );

        // 3. Unparseable datetime default
        state
            .store()
            .set_variable("bad_date", "not-a-date")
            .await
            .unwrap();
        let req = req_post_json(
            "/api/render/label",
            &serde_json::json!({ "template": "t_bad_date", "data": {} }).to_string(),
        );
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_json(res).await;
        assert_eq!(body["error"]["code"], "TemplateInvalid");
        assert_eq!(
            body["error"]["details"]["reason"],
            "param_default_unresolvable"
        );
        let msg3 = body["error"]["message"].as_str().unwrap();
        assert!(
            msg3.contains("dt"),
            "message '{msg3}' should name parameter 'dt'"
        );
        assert!(
            msg3.contains("not-a-date"),
            "message '{msg3}' should name resolved value"
        );

        // 4. Boolean literal default of "yes" (invalid boolean string)
        let req = req_post_json(
            "/api/render/label",
            &serde_json::json!({ "template": "t_bad_bool", "data": {} }).to_string(),
        );
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_json(res).await;
        assert_eq!(body["error"]["code"], "TemplateInvalid");
        assert_eq!(
            body["error"]["details"]["reason"],
            "param_default_unresolvable"
        );
        let msg4 = body["error"]["message"].as_str().unwrap();
        assert!(
            msg4.contains("flag"),
            "message '{msg4}' should name parameter 'flag'"
        );
        assert!(
            msg4.contains("yes"),
            "message '{msg4}' should name invalid value 'yes'"
        );
    }

    #[tokio::test]
    async fn datetime_param_attribution_boundary_http_test() {
        let yaml = r#"
name: Attribution Boundary DT
unit: mm
dpi: 200
params:
  dt:
    type: datetime
    default: "{vars.bad_date}"
format:
  type: single
  height: 20
  width: 50
layout:
  - type: text
    value: "{dt}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let (app, state) = test_app_with_custom_templates(vec![("t_attribution_dt", yaml)]);

        let invalid_val = "2026-02-30";

        // Path 1: Caller supplies the invalid date value in request `data` -> 400 Bad Request / datetime_param_invalid
        let req_supplied = req_post_json(
            "/api/render/label",
            &serde_json::json!({
                "template": "t_attribution_dt",
                "data": { "dt": invalid_val }
            })
            .to_string(),
        );
        let res_supplied = app.clone().oneshot(req_supplied).await.unwrap();
        assert_eq!(res_supplied.status(), StatusCode::BAD_REQUEST);
        let body_supplied = body_json(res_supplied).await;
        assert_eq!(body_supplied["error"]["code"], "InvalidRequest");
        assert_eq!(
            body_supplied["error"]["details"]["reason"],
            "datetime_param_invalid"
        );

        // Path 2: Exact same value reached through resolved default (data: {}) -> 422 Unprocessable Entity / param_default_unresolvable
        state
            .store()
            .set_variable("bad_date", invalid_val)
            .await
            .unwrap();
        let req_default = req_post_json(
            "/api/render/label",
            &serde_json::json!({
                "template": "t_attribution_dt",
                "data": {}
            })
            .to_string(),
        );
        let res_default = app.clone().oneshot(req_default).await.unwrap();
        assert_eq!(res_default.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body_default = body_json(res_default).await;
        assert_eq!(body_default["error"]["code"], "TemplateInvalid");
        assert_eq!(
            body_default["error"]["details"]["reason"],
            "param_default_unresolvable"
        );
    }

    #[tokio::test]
    async fn batch_failure_reports_param_default_unresolvable() {
        let yaml = r#"
name: TBatch
unit: mm
dpi: 200
params:
  val:
    type: string
    default: "{vars.missing}"
format:
  type: single
  height: 20
  width: 50
layout:
  - type: text
    value: "{val}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let (app, _state) = test_app_with_custom_templates(vec![("t_batch_bad", yaml)]);
        let req = req_post_json(
            "/api/batch",
            &serde_json::json!({
                "template": "t_batch_bad",
                "labels": [
                    { "data": {} },
                    { "data": { "val": "overridden" } },
                    { "data": {} }
                ],
                "mode": "download"
            })
            .to_string(),
        );
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_json(res).await;
        assert_eq!(body["error"]["code"], "BatchInvalid");
        let failures = body["error"]["details"]["failures"].as_array().unwrap();
        assert_eq!(failures.len(), 2);
        assert_eq!(failures[0]["index"], 0);
        assert_eq!(failures[0]["code"], "TemplateInvalid");
        assert_eq!(failures[0]["reason"], "param_default_unresolvable");
        assert_eq!(failures[1]["index"], 2);
        assert_eq!(failures[1]["code"], "TemplateInvalid");
        assert_eq!(failures[1]["reason"], "param_default_unresolvable");
    }

    #[tokio::test]
    async fn inputs_endpoint_infallible_for_unresolvable_default() {
        let yaml = r#"
name: TInputs
unit: mm
dpi: 200
params:
  val:
    type: string
    default: "{vars.missing_var}"
format:
  type: single
  height: 20
  width: 50
layout:
  - type: text
    value: "{val}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let (app, _state) = test_app_with_custom_templates(vec![("t_infallible", yaml)]);

        // GET /api/templates/{id}
        let res = app
            .clone()
            .oneshot(req_get("/api/templates/t_infallible"))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_json(res).await;
        assert_eq!(
            body["param_defaults"]["val"]["error"]["reason"],
            "param_default_unresolvable"
        );
        assert_eq!(
            body["param_defaults"]["val"]["error"]["token"],
            "vars.missing_var"
        );

        let input_val = body["inputs"]["default"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["name"] == "val")
            .unwrap();
        assert_eq!(input_val["required"], true);
        assert!(input_val.get("default").is_none() || input_val["default"].is_null());
        assert_eq!(
            input_val["default_error"]["reason"],
            "param_default_unresolvable"
        );
        assert_eq!(input_val["default_error"]["token"], "vars.missing_var");

        // POST /api/templates/{id}/inputs
        let req = req_post_json(
            "/api/templates/t_infallible/inputs",
            &serde_json::json!({
                "labels": [{ "data": {} }]
            })
            .to_string(),
        );
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_json(res).await;
        let input_val = body["inputs"][0]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["name"] == "val")
            .unwrap();
        assert_eq!(input_val["required"], true);
        assert!(input_val.get("default").is_none() || input_val["default"].is_null());
        assert_eq!(
            input_val["default_error"]["reason"],
            "param_default_unresolvable"
        );
        assert_eq!(input_val["default_error"]["token"], "vars.missing_var");
    }

    #[tokio::test]
    async fn flow_line_gap_is_inert_without_wrap() {
        let without_line_gap = r#"
name: Flow Without Line Gap
unit: mm
dpi: 200
format: { type: single, width: 30, height: 12 }
layout:
  - type: container
    at: [0, 0]
    size: [30, 12]
    flow: { direction: row, gap: 2 }
    items:
      - type: text
        value: "A"
        size: [10, 6]
        font_size: 8
      - type: text
        value: "B"
        size: [10, 6]
        font_size: 8
"#;
        let with_line_gap = without_line_gap
            .replace("Flow Without Line Gap", "Flow With Inert Line Gap")
            .replace("gap: 2 }", "gap: 2, line_gap: 7 }");
        let (app, _state) = test_app_with_custom_templates(vec![
            ("flow_no_line_gap", without_line_gap),
            ("flow_inert_line_gap", &with_line_gap),
        ]);

        let render = |template: &str| {
            req_post_json(
                "/api/render/label?format=png",
                &serde_json::json!({ "template": template, "data": {} }).to_string(),
            )
        };
        let without = app
            .clone()
            .oneshot(render("flow_no_line_gap"))
            .await
            .unwrap();
        assert_eq!(without.status(), StatusCode::OK);
        let without = without.into_body().collect().await.unwrap().to_bytes();
        let with = app
            .clone()
            .oneshot(render("flow_inert_line_gap"))
            .await
            .unwrap();
        assert_eq!(with.status(), StatusCode::OK);
        let with = with.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(without, with, "line_gap must not alter an unwrapped layout");
    }

    #[tokio::test]
    async fn flow_wrap_and_overflow_policies_hold_at_http_boundary() {
        let wrapped = r#"
name: Wrapped Flow
unit: mm
dpi: 200
format: { type: single, width: 30, height: 20 }
layout:
  - type: container
    at: [0, 0]
    size: [30, 20]
    flow: { direction: row, gap: 2, wrap: true, line_gap: 1 }
    items:
      - { type: text, value: "A", size: [14, 6], font_size: 8 }
      - { type: text, value: "B", size: [14, 6], font_size: 8 }
      - { type: text, value: "C", size: [14, 6], font_size: 8 }
"#;
        let unwrapped = wrapped
            .replace("Wrapped Flow", "Unwrapped Flow")
            .replace(", wrap: true, line_gap: 1", "");
        let trim = r#"
name: Trim Flow
unit: mm
dpi: 200
format: { type: single, width: 20, height: 10 }
layout:
  - type: container
    at: [0, 0]
    size: [20, 10]
    flow: { direction: row, gap: 2, overflow: trim }
    items:
      - { type: container, size: [8, 6], items: [] }
      - { type: container, size: [8, 6], items: [] }
      - { type: container, size: [2, 6], items: [] }
"#;
        let fail = trim
            .replace("Trim Flow", "Fail Flow")
            .replace("overflow: trim", "overflow: fail");
        let trim_missing_text = r#"
name: Trim Still Evaluates Text
unit: mm
dpi: 200
params:
  missing:
    type: string
format: { type: single, width: 20, height: 10 }
layout:
  - type: container
    at: [0, 0]
    size: [20, 10]
    flow: { direction: row, overflow: trim }
    items:
      - { type: container, size: [20, 10], items: [] }
      - { type: text, value: "{missing}", size: [content, 4], font_size: 8 }
"#;
        let trim_missing_image = r#"
name: Trim Does Not Draw Image
unit: mm
dpi: 200
format: { type: single, width: 20, height: 10 }
layout:
  - type: container
    at: [0, 0]
    size: [20, 10]
    flow: { direction: row, overflow: trim }
    items:
      - { type: container, size: [20, 10], items: [] }
      - { type: image, name: missing_image, size: [4, 4] }
"#;
        let trim_child_too_large = r#"
name: Trim Does Not Bypass Child Bounds
unit: mm
dpi: 200
params:
  box_w:
    type: length
    default: 8
format: { type: single, width: 20, height: 10 }
layout:
  - type: container
    at: [0, 0]
    size: [20, 10]
    flow: { direction: row, overflow: trim }
    items:
      - { type: container, size: ["{box_w}", 6], items: [] }
"#;
        let (app, _state) = test_app_with_custom_templates(vec![
            ("wrapped", wrapped),
            ("unwrapped", &unwrapped),
            ("trim", trim),
            ("fail", &fail),
            ("trim_missing_text", trim_missing_text),
            ("trim_missing_image", trim_missing_image),
            ("trim_child_too_large", trim_child_too_large),
        ]);

        for template in ["wrapped", "trim", "trim_missing_image"] {
            let response = app
                .clone()
                .oneshot(req_post_json(
                    "/api/render/label?format=png",
                    &serde_json::json!({ "template": template, "data": {} }).to_string(),
                ))
                .await
                .unwrap();
            if response.status() != StatusCode::OK {
                let status = response.status();
                let body = body_json(response).await;
                panic!("{template} should render, got {status}: {body}");
            }
        }

        for template in ["unwrapped", "fail"] {
            let response = app
                .clone()
                .oneshot(req_post_json(
                    "/api/render/label?format=png",
                    &serde_json::json!({ "template": template, "data": {} }).to_string(),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
            let body = body_json(response).await;
            assert_eq!(body["error"]["code"], "UnsupportedLayoutItem");
            assert_eq!(body["error"]["details"]["reason"], "item_out_of_frame");
        }

        let response = app
            .clone()
            .oneshot(req_post_json(
                "/api/render/label?format=png",
                &serde_json::json!({ "template": "trim_missing_text", "data": {} }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_json(response).await;
        assert_eq!(body["error"]["code"], "MissingField");
        assert_eq!(body["error"]["details"]["field"], "missing");

        let response = app
            .oneshot(req_post_json(
                "/api/render/label?format=png",
                &serde_json::json!({
                    "template": "trim_child_too_large",
                    "data": { "box_w": 30 }
                })
                .to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_json(response).await;
        assert_eq!(body["error"]["code"], "UnsupportedLayoutItem");
        assert_eq!(body["error"]["details"]["reason"], "item_out_of_frame");
    }

    async fn body_bytes(res: axum::response::Response) -> Vec<u8> {
        axum::body::to_bytes(res.into_body(), 10 * 1024 * 1024)
            .await
            .expect("collect body")
            .to_vec()
    }

    #[tokio::test]
    async fn render_label_and_batch_invalid_color_parameter_refusals() {
        let text_yaml = r#"
name: DynamicColor
unit: mm
dpi: 200
params:
  brand:
    type: string
format:
  type: single
  width: 50
  height: 20
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    color: "{brand}"
"#;
        let shape_yaml = r#"
name: DynamicShapeColor
unit: mm
dpi: 200
params:
  bg_color:
    type: string
  stroke_color:
    type: string
format:
  type: single
  width: 50
  height: 20
layout:
  - type: container
    at: [0, 0]
    size: [50, 20]
    background: "{bg_color}"
    items: []
  - type: line
    at: [0, 0]
    to: [50, 20]
    stroke:
      thickness: 0.5
      color: "{stroke_color}"
"#;
        let (app, _state) = test_app_with_custom_templates(vec![
            ("dyn_color", text_yaml),
            ("dyn_shape_color", shape_yaml),
        ]);

        // 1. POST /api/render/label supplying non-colour for text returns 400 InvalidRequest / color_param_invalid naming parameter
        let req1 = req_post_json(
            "/api/render/label",
            &serde_json::json!({
                "template": "dyn_color",
                "data": { "brand": "octarine" }
            })
            .to_string(),
        );
        let res1 = app.clone().oneshot(req1).await.unwrap();
        assert_eq!(res1.status(), StatusCode::BAD_REQUEST);
        let body1 = body_json(res1).await;
        assert_eq!(body1["error"]["code"], "InvalidRequest");
        assert_eq!(body1["error"]["details"]["reason"], "color_param_invalid");
        let msg1 = body1["error"]["message"].as_str().unwrap();
        assert!(
            msg1.contains("brand"),
            "error message '{msg1}' must name the failing parameter 'brand'"
        );

        // 2. POST /api/render/label supplying non-colour for container background returns 400 InvalidRequest / color_param_invalid naming bg_color
        let req_bg = req_post_json(
            "/api/render/label",
            &serde_json::json!({
                "template": "dyn_shape_color",
                "data": { "bg_color": "octarine", "stroke_color": "black" }
            })
            .to_string(),
        );
        let res_bg = app.clone().oneshot(req_bg).await.unwrap();
        assert_eq!(res_bg.status(), StatusCode::BAD_REQUEST);
        let body_bg = body_json(res_bg).await;
        assert_eq!(body_bg["error"]["code"], "InvalidRequest");
        assert_eq!(body_bg["error"]["details"]["reason"], "color_param_invalid");
        let msg_bg = body_bg["error"]["message"].as_str().unwrap();
        assert!(
            msg_bg.contains("bg_color"),
            "error message '{msg_bg}' must name the failing parameter 'bg_color'"
        );

        // 3. POST /api/render/label supplying "{other}" chained reference for stroke returns 400 InvalidRequest / color_param_invalid naming stroke_color
        let req_stroke = req_post_json(
            "/api/render/label",
            &serde_json::json!({
                "template": "dyn_shape_color",
                "data": { "bg_color": "blue", "stroke_color": "{other}" }
            })
            .to_string(),
        );
        let res_stroke = app.clone().oneshot(req_stroke).await.unwrap();
        assert_eq!(res_stroke.status(), StatusCode::BAD_REQUEST);
        let body_stroke = body_json(res_stroke).await;
        assert_eq!(body_stroke["error"]["code"], "InvalidRequest");
        assert_eq!(
            body_stroke["error"]["details"]["reason"],
            "color_param_invalid"
        );
        let msg_stroke = body_stroke["error"]["message"].as_str().unwrap();
        assert!(
            msg_stroke.contains("stroke_color"),
            "error message '{msg_stroke}' must name the failing parameter 'stroke_color'"
        );

        // 4. POST /api/batch with 2 labels (second bad background color) returns 422 BatchInvalid with failure at index 1
        let req3 = req_post_json(
            "/api/batch",
            &serde_json::json!({
                "template": "dyn_shape_color",
                "labels": [
                    { "data": { "bg_color": "red", "stroke_color": "black" } },
                    { "data": { "bg_color": "octarine", "stroke_color": "black" } }
                ],
                "mode": "download"
            })
            .to_string(),
        );
        let res3 = app.clone().oneshot(req3).await.unwrap();
        assert_eq!(res3.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body3 = body_json(res3).await;
        assert_eq!(body3["error"]["code"], "BatchInvalid");
        let failures = body3["error"]["details"]["failures"].as_array().unwrap();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0]["index"], 1);
        assert_eq!(failures[0]["code"], "InvalidRequest");
        assert_eq!(failures[0]["reason"], "color_param_invalid");
        let msg3 = failures[0]["message"].as_str().unwrap();
        assert!(
            msg3.contains("bg_color"),
            "failure message '{msg3}' must name the failing parameter 'bg_color'"
        );
    }

    #[tokio::test]
    async fn white_color_template_loads_and_renders_successfully() {
        let yaml = r#"
name: WhiteColor
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 20
layout:
  - type: text
    value: "White on White"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    color: white
"#;
        let (app, _state) = test_app_with_custom_templates(vec![("white_color", yaml)]);

        // 1. Render PNG
        let req_png = req_post_json(
            "/api/render/label?format=png",
            &serde_json::json!({ "template": "white_color", "data": {} }).to_string(),
        );
        let res_png = app.clone().oneshot(req_png).await.unwrap();
        assert_eq!(res_png.status(), StatusCode::OK);
        let png = body_bytes(res_png).await;
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");

        // 2. Render PDF
        let req_pdf = req_post_json(
            "/api/render/label?format=pdf",
            &serde_json::json!({ "template": "white_color", "data": {} }).to_string(),
        );
        let res_pdf = app.clone().oneshot(req_pdf).await.unwrap();
        assert_eq!(res_pdf.status(), StatusCode::OK);
        let pdf = body_bytes(res_pdf).await;
        assert!(pdf.starts_with(b"%PDF"));
    }

    #[tokio::test]
    async fn colored_text_and_alpha_composite_png_rendering() {
        let red_yaml = r#"
name: RedColor
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 20
layout:
  - type: text
    value: "Red Text"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    color: red
"#;
        let alpha_yaml = r#"
name: AlphaColor
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 20
layout:
  - type: text
    value: "Alpha Text"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    color: '#00000080'
"#;
        let (app, _state) = test_app_with_custom_templates(vec![
            ("red_color", red_yaml),
            ("alpha_color", alpha_yaml),
        ]);

        // 1. Red text PNG produces CSS Level 1 red (255, 0, 0) glyph pixels
        let req_red = req_post_json(
            "/api/render/label?format=png",
            &serde_json::json!({ "template": "red_color", "data": {} }).to_string(),
        );
        let res_red = app.clone().oneshot(req_red).await.unwrap();
        assert_eq!(res_red.status(), StatusCode::OK);
        let png_red = body_bytes(res_red).await;
        let img_red = image::load_from_memory(&png_red)
            .expect("decode red png")
            .to_rgba8();
        // CSS Level 1 red (#ff0000) over white composites to (255, G, G) where G < 255
        let red_count = img_red
            .pixels()
            .filter(|p| p[0] == 255 && p[1] < 220 && p[1] == p[2])
            .count();
        assert!(
            red_count > 0,
            "rendered PNG must contain pure red (255, G, G) glyph pixels, found {red_count}"
        );
        let typst_legacy_red = img_red
            .pixels()
            .filter(|p| (p[0], p[1], p[2]) == (255, 65, 54))
            .count();
        assert_eq!(
            typst_legacy_red, 0,
            "Typst's legacy red (255, 65, 54) must not appear anywhere"
        );

        // 2. Alpha color (#00000080 over white background) composites to (128, 128, 128)
        let req_alpha = req_post_json(
            "/api/render/label?format=png",
            &serde_json::json!({ "template": "alpha_color", "data": {} }).to_string(),
        );
        let res_alpha = app.clone().oneshot(req_alpha).await.unwrap();
        assert_eq!(res_alpha.status(), StatusCode::OK);
        let png_alpha = body_bytes(res_alpha).await;
        let img_alpha = image::load_from_memory(&png_alpha)
            .expect("decode alpha png")
            .to_rgba8();
        let composite_count = img_alpha
            .pixels()
            .filter(|p| (p[0], p[1], p[2]) == (128, 128, 128))
            .count();
        assert!(
            composite_count > 0,
            "rendered PNG with #00000080 over white must composite to (128, 128, 128), found {composite_count}"
        );
    }

    #[tokio::test]
    async fn template_get_reports_declared_color_and_omits_when_absent() {
        let yaml = r#"
name: TemplateWithAndWithoutColor
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 20
layout:
  - type: text
    value: "Declared Color"
    at: [0, 0]
    size: [50, 10]
    font_size: 10
    color: red
  - type: text
    value: "Default Color"
    at: [0, 10]
    size: [50, 10]
    font_size: 10
"#;
        let (app, _state) = test_app_with_custom_templates(vec![("color_readback", yaml)]);
        let req = Request::builder()
            .uri("/api/templates/color_readback")
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let detail = body_json(res).await;
        let items = detail["layout"].as_array().expect("layout items");
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0]["color"], "red",
            "item 0 must report declared color 'red'"
        );
        assert!(
            items[1].get("color").is_none(),
            "item 1 must omit 'color' key when no color was declared, got: {:?}",
            items[1].get("color")
        );
    }

    #[tokio::test]
    async fn color_multi_slot_sheet_and_bilevel_rendering() {
        let sheet_yaml = r#"
name: SheetColor
unit: mm
dpi: 200
format:
  type: sheet
  paper_width: 50
  paper_height: 50
  label_width: 20
  label_height: 20
  positions:
    - [0, 0]
    - [25, 0]
params:
  bg:
    type: string
  txt_col:
    type: string
layout:
  - type: container
    at: [0, 0]
    size: [20, 20]
    background: "{bg}"
    items:
      - type: text
        value: "Label"
        at: [0, 0]
        size: [20, 20]
        font_size: 8
        color: "{txt_col}"
"#;
        let (app, _state) = test_app_with_custom_templates(vec![("sheet_color", sheet_yaml)]);

        // 1. Multi-slot sheet PDF rendering with painted container and text in every slot
        let req_sheet = req_post_json(
            "/api/batch",
            &serde_json::json!({
                "template": "sheet_color",
                "mode": "download",
                "labels": [
                    { "data": { "bg": "red", "txt_col": "yellow" } },
                    { "data": { "bg": "navy", "txt_col": "white" } }
                ]
            })
            .to_string(),
        );
        let res_sheet = app.clone().oneshot(req_sheet).await.unwrap();
        assert_eq!(res_sheet.status(), StatusCode::OK);
        let pdf = body_bytes(res_sheet).await;
        assert!(pdf.starts_with(b"%PDF"));

        // 2. Bilevel thresholding with light glyphs (yellow) inside dark background (navy)
        let dark_bg_light_text_yaml = r#"
name: DarkBgLightText
unit: mm
dpi: 200
format:
  type: single
  width: 30
  height: 15
layout:
  - type: container
    at: [0, 0]
    size: [30, 15]
    background: navy
    items:
      - type: text
        value: "LIGHT"
        at: [2, 2]
        size: [26, 11]
        font_size: 10
        color: yellow
"#;
        let (app2, _state2) =
            test_app_with_custom_templates(vec![("bilevel_test", dark_bg_light_text_yaml)]);
        let req_bilevel = req_post_json(
            "/api/render/label?format=png&color_mode=bilevel",
            &serde_json::json!({ "template": "bilevel_test", "data": {} }).to_string(),
        );
        let res_bilevel = app2.clone().oneshot(req_bilevel).await.unwrap();
        assert_eq!(res_bilevel.status(), StatusCode::OK);
        let png = body_bytes(res_bilevel).await;
        let img = image::load_from_memory(&png).expect("decode").to_rgba8();

        // Dark ground (navy, luminance <= 128) becomes black (0, 0, 0)
        let black_count = img
            .pixels()
            .filter(|p| (p[0], p[1], p[2]) == (0, 0, 0))
            .count();
        assert!(
            black_count > 0,
            "navy background must threshold to black in bilevel mode, found {black_count}"
        );

        // Light glyphs (yellow, luminance > 128) become white (255, 255, 255)
        let white_count = img
            .pixels()
            .filter(|p| (p[0], p[1], p[2]) == (255, 255, 255))
            .count();
        assert!(
            white_count > 0,
            "yellow glyphs must threshold to white in bilevel mode, found {white_count}"
        );

        // All pixels must be pure 1-bit thresholded black or white
        assert!(
            img.pixels().all(|p| {
                let (r, g, b) = (p[0], p[1], p[2]);
                (r, g, b) == (0, 0, 0) || (r, g, b) == (255, 255, 255)
            }),
            "bilevel output must be pure B/W thresholded"
        );
    }

    #[tokio::test]
    async fn shape_and_text_parameter_referenced_color_rendering() {
        let yaml = r#"
name: ShapeParamColors
unit: mm
dpi: 200
params:
  bg_color:
    type: string
  stroke_color:
    type: string
  text_color:
    type: string
format:
  type: single
  width: 50
  height: 30
layout:
  - type: container
    at: [0, 0]
    size: [50, 30]
    background: "{bg_color}"
    stroke:
      thickness: 1.0
      color: "{stroke_color}"
    items:
      - type: text
        value: "PARAM"
        at: [5, 5]
        size: [40, 20]
        font_size: 14
        color: "{text_color}"
"#;
        let (app, _state) = test_app_with_custom_templates(vec![("shape_param_colors", yaml)]);

        // 1. PNG render resolves container background, stroke, and text color parameters
        let req_png = req_post_json(
            "/api/render/label?format=png",
            &serde_json::json!({
                "template": "shape_param_colors",
                "data": {
                    "bg_color": "#000080",
                    "stroke_color": "#ff0000",
                    "text_color": "#ffff00"
                }
            })
            .to_string(),
        );
        let res_png = app.clone().oneshot(req_png).await.unwrap();
        assert_eq!(res_png.status(), StatusCode::OK);
        let png = body_bytes(res_png).await;
        let img = image::load_from_memory(&png)
            .expect("decode png")
            .to_rgba8();

        // Navy background pixels (0, 0, 128)
        let navy_count = img
            .pixels()
            .filter(|p| (p[0], p[1], p[2]) == (0, 0, 128))
            .count();
        assert!(
            navy_count > 0,
            "must contain navy (0, 0, 128) background pixels"
        );

        // Red stroke pixels (255, 0, 0)
        let red_count = img
            .pixels()
            .filter(|p| (p[0], p[1], p[2]) == (255, 0, 0))
            .count();
        assert!(red_count > 0, "must contain red (255, 0, 0) stroke pixels");

        // Yellow text glyph pixels over navy background
        let yellow_count = img
            .pixels()
            .filter(|p| p[0] > 180 && p[1] > 180 && p[2] < 100)
            .count();
        assert!(yellow_count > 0, "must contain yellow glyph pixels");

        // 2. PDF render resolves all three parameters
        let req_pdf = req_post_json(
            "/api/render/label?format=pdf",
            &serde_json::json!({
                "template": "shape_param_colors",
                "data": {
                    "bg_color": "#000080",
                    "stroke_color": "#ff0000",
                    "text_color": "#ffff00"
                }
            })
            .to_string(),
        );
        let res_pdf = app.clone().oneshot(req_pdf).await.unwrap();
        assert_eq!(res_pdf.status(), StatusCode::OK);
        let pdf = body_bytes(res_pdf).await;
        assert!(pdf.starts_with(b"%PDF"));
    }

    #[tokio::test]
    async fn shape_paint_filled_rounded_container_renders_png_and_pdf() {
        let shape_yaml = r#"
name: ShapePaint
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 30
layout:
  - type: container
    at: [0, 0]
    size: [50, 30]
    stroke:
      thickness: 0.5
      color: red
    background: '#f0f0f0'
    rounded: 2.0
    items:
      - type: text
        value: "Inside Shape"
        at: [5, 5]
        size: [40, 20]
        font_size: 10
"#;
        let (app, _state) = test_app_with_custom_templates(vec![("shape_paint_test", shape_yaml)]);

        // 1. HTTP POST /api/render/label?format=png
        let req_png = req_post_json(
            "/api/render/label?format=png",
            &serde_json::json!({ "template": "shape_paint_test", "data": {} }).to_string(),
        );
        let res_png = app.clone().oneshot(req_png).await.unwrap();
        assert_eq!(res_png.status(), StatusCode::OK);
        assert_eq!(res_png.headers().get("content-type").unwrap(), "image/png");
        let png_bytes = body_bytes(res_png).await;
        assert_eq!(&png_bytes[..8], b"\x89PNG\r\n\x1a\n");

        // 2. HTTP POST /api/render/label?format=pdf
        let req_pdf = req_post_json(
            "/api/render/label?format=pdf",
            &serde_json::json!({ "template": "shape_paint_test", "data": {} }).to_string(),
        );
        let res_pdf = app.clone().oneshot(req_pdf).await.unwrap();
        assert_eq!(res_pdf.status(), StatusCode::OK);
        assert_eq!(
            res_pdf.headers().get("content-type").unwrap(),
            "application/pdf"
        );
        let pdf_bytes = body_bytes(res_pdf).await;
        assert!(pdf_bytes.starts_with(b"%PDF"));
    }

    #[tokio::test]
    async fn template_get_reports_authored_shape_and_text_colors() {
        let yaml = r##"
name: AuthoredColors
unit: mm
dpi: 200
params:
  brand:
    type: string
format:
  type: single
  width: 50
  height: 30
layout:
  - type: container
    at: [0, 0]
    size: [50, 30]
    stroke:
      thickness: 0.2
      color: "#F0F"
    background: red
    rounded: 1.0
    items:
      - type: line
        at: [5, 5]
        to: [45, 5]
        stroke:
          thickness: 0.5
      - type: container
        at: [5, 10]
        size: [40, 15]
        stroke:
          thickness: 0.1
          color: "{brand}"
        background: "{brand}"
        items: []
      - type: text
        value: "Dynamic Color"
        at: [5, 20]
        size: [40, 5]
        font_size: 6
        color: "{brand}"
      - type: text
        value: "Default Color"
        at: [5, 25]
        size: [40, 5]
        font_size: 6
"##;
        let (app, _state) = test_app_with_custom_templates(vec![("authored_colors", yaml)]);
        let req = Request::builder()
            .uri("/api/templates/authored_colors")
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let detail = body_json(res).await;
        let items = detail["layout"].as_array().expect("layout items");

        // Top-level container: authored spelling preserved
        assert_eq!(items[0]["background"], "red");
        assert_eq!(items[0]["stroke"]["color"], "#F0F");
        assert_eq!(items[0]["stroke"]["thickness"], 0.2);
        assert_eq!(items[0]["rounded"], 1.0);

        // Child items inside container
        let child_items = items[0]["items"].as_array().expect("child items");
        // Line with defaulted color -> "black"
        assert_eq!(child_items[0]["stroke"]["color"], "black");
        assert_eq!(child_items[0]["stroke"]["thickness"], 0.5);

        // Nested container with stroke: { color: "{brand}" } and background: "{brand}"
        assert_eq!(child_items[1]["stroke"]["color"], "{brand}");
        assert_eq!(child_items[1]["background"], "{brand}");

        // Text item with color reference -> "{brand}"
        assert_eq!(child_items[2]["color"], "{brand}");

        // Uncoloured text item omits color key
        assert!(child_items[3].get("color").is_none());
    }

    #[tokio::test]
    async fn cross_field_paint_equality_between_text_and_container() {
        // Container with background: red and side-by-side text with color: red
        let yaml = r#"
name: CrossFieldPaint
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 20
layout:
  - type: container
    at: [0, 0]
    size: [20, 20]
    background: red
    items: []
  - type: text
    value: "RED"
    at: [25, 0]
    size: [25, 20]
    font_size: 14
    color: red
"#;
        let (app, _state) = test_app_with_custom_templates(vec![("cross_field", yaml)]);
        let req = req_post_json(
            "/api/render/label?format=png",
            &serde_json::json!({ "template": "cross_field", "data": {} }).to_string(),
        );
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let png = body_bytes(res).await;
        let img = image::load_from_memory(&png)
            .expect("decode cross field png")
            .to_rgba8();

        let width = img.width();
        // Container background pixels in left region use solid CSS Level 1 red (255, 0, 0)
        let container_red_pixels = img
            .enumerate_pixels()
            .filter(|(x, _y, p)| *x < width / 2 && (p[0], p[1], p[2]) == (255, 0, 0))
            .count();
        assert!(
            container_red_pixels > 0,
            "container background must paint standard red (255, 0, 0), found {container_red_pixels}"
        );

        // Text glyph pixels in right region use standard CSS Level 1 red (R=255, G=B < 200 on white background)
        let text_red_pixels = img
            .enumerate_pixels()
            .filter(|(x, _y, p)| *x >= width / 2 && p[0] == 255 && p[1] == p[2] && p[1] < 200)
            .count();
        assert!(
            text_red_pixels > 0,
            "text glyphs must paint standard red (R=255, G=B), found {text_red_pixels}"
        );

        // Ensure Typst's legacy red (255, 65, 54) or any non-CSS red with G != B is NOT present anywhere in text region
        let non_css_red_pixels = img
            .enumerate_pixels()
            .filter(|(x, _y, p)| *x >= width / 2 && p[0] == 255 && p[1] != p[2])
            .count();
        assert_eq!(
            non_css_red_pixels, 0,
            "Typst's legacy red with unequal green/blue channels must not appear anywhere"
        );
    }

    #[tokio::test]
    async fn issue_262_get_template_publishes_param_defaults_and_coerced_inputs() {
        let yaml = r#"
name: Issue 262 Defaults
unit: mm
dpi: 200
params:
  s:
    type: string
    default: hello
  l:
    type: length
    default: "80mm"
  i:
    type: integer
    default: 42
  b:
    type: boolean
    default: true
  e:
    type: enum
    values: [opt1, opt2]
    default: opt1
  d:
    type: datetime
    default: "{sys.now}"
  req:
    type: string
format: { type: single, width: 100, height: 50 }
layout:
  - type: text
    value: "{s} {l} {i} {b} {e} {d} {req}"
    at: [0, 0]
    size: [100, 50]
    font_size: 10
"#;
        let (app, _state) = test_app_with_custom_templates(vec![("i262_defaults", yaml)]);
        let req = Request::builder()
            .uri("/api/templates/i262_defaults")
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let detail = body_json(res).await;

        // Verify param_defaults
        let pd = &detail["param_defaults"];
        assert_eq!(pd["s"]["resolved"], "hello");
        assert_eq!(pd["l"]["resolved"], 80.0);
        assert_eq!(pd["i"]["resolved"], 42);
        assert_eq!(pd["b"]["resolved"], true);
        assert_eq!(pd["e"]["resolved"], "opt1");
        assert!(pd["d"]["resolved"].is_string());
        assert!(pd.get("req").is_none());

        // Verify inputs.default and inputs.all
        let get_input = |arr: &serde_json::Value, name: &str| {
            arr.as_array()
                .unwrap()
                .iter()
                .find(|i| i["name"] == name)
                .cloned()
                .unwrap()
        };
        for input_list_name in ["default", "all"] {
            let list = &detail["inputs"][input_list_name];
            let inp_s = get_input(list, "s");
            assert_eq!(inp_s["required"], false);
            assert_eq!(inp_s["default"], "hello");
            assert!(inp_s["default_error"].is_null());

            let inp_l = get_input(list, "l");
            assert_eq!(inp_l["required"], false);
            assert_eq!(inp_l["default"], 80.0);

            let inp_i = get_input(list, "i");
            assert_eq!(inp_i["required"], false);
            assert_eq!(inp_i["default"], 42);

            let inp_b = get_input(list, "b");
            assert_eq!(inp_b["required"], false);
            assert_eq!(inp_b["default"], true);

            let inp_e = get_input(list, "e");
            assert_eq!(inp_e["required"], false);
            assert_eq!(inp_e["default"], "opt1");

            let inp_d = get_input(list, "d");
            assert_eq!(inp_d["required"], false);
            assert!(inp_d["default"].is_string());

            let inp_req = get_input(list, "req");
            assert_eq!(inp_req["required"], true);
            assert!(inp_req["default"].is_null());
            assert!(inp_req["default_error"].is_null());
        }
    }

    #[tokio::test]
    async fn issue_262_broken_default_reports_error_and_marks_input_required() {
        let yaml = r#"
name: Broken Default Test
unit: mm
dpi: 200
params:
  broken_var:
    type: string
    default: "{vars.missing_key}"
  broken_len:
    type: length
    default: "not_a_length"
  good:
    type: string
    default: ok
format: { type: single, width: 100, height: 50 }
layout:
  - type: text
    value: "{broken_var} {broken_len} {good}"
    at: [0, 0]
    size: [100, 50]
    font_size: 10
"#;
        let (app, _state) = test_app_with_custom_templates(vec![("i262_broken", yaml)]);
        let req = Request::builder()
            .uri("/api/templates/i262_broken")
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let detail = body_json(res).await;

        let pd = &detail["param_defaults"];
        assert_eq!(
            pd["broken_var"]["error"]["reason"],
            "param_default_unresolvable"
        );
        assert_eq!(pd["broken_var"]["error"]["token"], "vars.missing_key");

        assert_eq!(
            pd["broken_len"]["error"]["reason"],
            "param_default_unresolvable"
        );
        assert_eq!(pd["broken_len"]["error"]["value"], "not_a_length");

        assert_eq!(pd["good"]["resolved"], "ok");

        let get_input = |arr: &serde_json::Value, name: &str| {
            arr.as_array()
                .unwrap()
                .iter()
                .find(|i| i["name"] == name)
                .cloned()
                .unwrap()
        };
        let inp_var = get_input(&detail["inputs"]["default"], "broken_var");
        assert_eq!(inp_var["required"], true);
        assert!(inp_var["default"].is_null());
        assert_eq!(
            inp_var["default_error"]["reason"],
            "param_default_unresolvable"
        );
        assert_eq!(inp_var["default_error"]["token"], "vars.missing_key");

        let inp_len = get_input(&detail["inputs"]["default"], "broken_len");
        assert_eq!(inp_len["required"], true);
        assert!(inp_len["default"].is_null());
        assert_eq!(
            inp_len["default_error"]["reason"],
            "param_default_unresolvable"
        );
        assert_eq!(inp_len["default_error"]["value"], "not_a_length");

        let inp_good = get_input(&detail["inputs"]["default"], "good");
        assert_eq!(inp_good["required"], false);
        assert_eq!(inp_good["default"], "ok");
        assert!(inp_good["default_error"].is_null());
    }

    #[tokio::test]
    async fn issue_262_get_templates_summaries_have_no_param_defaults() {
        let (app, _state) = test_app_with_custom_templates(vec![]);
        let res = app
            .clone()
            .oneshot(req_get("/api/templates"))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let list = body_json(res).await;
        for summary in list["templates"].as_array().unwrap() {
            assert!(
                summary.get("param_defaults").is_none(),
                "TemplateSummary must not contain param_defaults"
            );
        }
    }

    #[tokio::test]
    async fn issue_262_post_template_inputs_multi_label_batch() {
        let yaml = r#"
name: Batch Inputs Test
unit: mm
dpi: 200
params:
  site_param:
    type: string
    default: "{vars.site}"
  mode:
    type: enum
    values: [a, b]
    default: a
format: { type: single, width: 50, height: 20 }
layout:
  - type: container
    when:
      mode: a
    at: [0, 0]
    size: [50, 20]
    items:
      - type: text
        value: "Branch A: {site_param} {field_a}"
        at: [0, 0]
        size: [50, 10]
        font_size: 8
  - type: container
    when:
      mode: b
    at: [0, 0]
    size: [50, 20]
    items:
      - type: text
        value: "Branch B: {field_b}"
        at: [0, 0]
        size: [50, 10]
        font_size: 8
"#;
        let (app, state) = test_app_with_custom_templates(vec![("i262_batch", yaml)]);
        // Set site variable
        state
            .store()
            .set_variable("site", "production")
            .await
            .unwrap();

        let req = req_post_json(
            "/api/templates/i262_batch/inputs",
            &serde_json::json!({
                "labels": [
                    { "data": {} }, // mode defaults to a -> field_a active
                    { "data": { "mode": "b" } } // mode is b -> field_b active
                ]
            })
            .to_string(),
        );
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_json(res).await;
        let labels = body["inputs"].as_array().unwrap();
        assert_eq!(labels.len(), 2);

        // Label 1
        let l1_names: Vec<&str> = labels[0]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["name"].as_str().unwrap())
            .collect();
        assert!(l1_names.contains(&"site_param"));
        assert!(l1_names.contains(&"field_a"));
        assert!(!l1_names.contains(&"field_b"));
        let site_inp = labels[0]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["name"] == "site_param")
            .unwrap();
        assert_eq!(site_inp["required"], false);
        assert_eq!(site_inp["default"], "production");

        // Label 2
        let l2_names: Vec<&str> = labels[1]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["name"].as_str().unwrap())
            .collect();
        assert!(l2_names.contains(&"field_b"));
        assert!(!l2_names.contains(&"field_a"));
    }

    #[tokio::test]
    async fn issue_262_strict_render_fails_with_structured_details_for_broken_default() {
        let yaml = r#"
name: Strict Render Broken Default
unit: mm
dpi: 200
params:
  val:
    type: string
    default: "{vars.missing_key}"
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "{val}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let (app, _state) = test_app_with_custom_templates(vec![("i262_strict", yaml)]);
        let req = req_post_json(
            "/api/render/label?format=png",
            &serde_json::json!({
                "template": "i262_strict",
                "data": {}
            })
            .to_string(),
        );
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let err = body_json(res).await;
        assert_eq!(err["error"]["code"], "TemplateInvalid");
        let details = &err["error"]["details"];
        assert_eq!(details["reason"], "param_default_unresolvable");
        assert_eq!(details["param"], "val");
        assert_eq!(details["token"], "vars.missing_key");
        assert!(details.get("message").is_none());
    }

    #[tokio::test]
    async fn issue_262_thumbnail_renders_with_placeholder_when_default_broken() {
        let yaml = r#"
name: Thumbnail Broken Default
unit: mm
dpi: 200
params:
  val:
    type: string
    default: "{vars.missing_key}"
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "{val}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let (app, _state) = test_app_with_custom_templates(vec![("i262_thumb_broken", yaml)]);
        let res = app
            .clone()
            .oneshot(req_get("/api/templates/i262_thumb_broken/thumbnail"))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers().get("content-type").unwrap(), "image/png");
        let png = body_bytes(res).await;
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[tokio::test]
    async fn issue_262_when_gate_with_tokened_default() {
        let yaml = r#"
name: When Gate Token
unit: mm
dpi: 200
params:
  site_param:
    type: string
    default: "{vars.site}"
format: { type: single, width: 50, height: 20 }
layout:
  - type: container
    when:
      site_param: production
    at: [0, 0]
    size: [50, 20]
    items:
      - type: text
        value: "Production only: {prod_secret}"
        at: [0, 0]
        size: [50, 20]
        font_size: 10
"#;
        let (app, state) = test_app_with_custom_templates(vec![("i262_when_gate", yaml)]);

        // 1. Without variable, site_param is unresolvable -> container inactive
        let res = app
            .clone()
            .oneshot(req_get("/api/templates/i262_when_gate"))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let detail = body_json(res).await;
        assert!(!detail["inputs"]["default"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["name"] == "prod_secret"));

        // 2. With variable site=production -> container active
        state
            .store()
            .set_variable("site", "production")
            .await
            .unwrap();
        let res = app
            .clone()
            .oneshot(req_get("/api/templates/i262_when_gate"))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let detail = body_json(res).await;
        assert!(detail["inputs"]["default"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["name"] == "prod_secret"));
    }

    #[tokio::test]
    async fn issue_262_sys_now_format_resolves_against_settings() {
        let yaml = r#"
name: Custom Format Date
unit: mm
dpi: 200
params:
  d:
    type: string
    default: "{sys.now:custom_fmt}"
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "{d}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let (app, state) = test_app_with_custom_templates(vec![("i262_date_fmt", yaml)]);
        // Set custom format
        state
            .store()
            .set_setting("datetime_formats", r#"{"custom_fmt":"%Y/%m/%d"}"#)
            .await
            .unwrap();

        let res = app
            .clone()
            .oneshot(req_get("/api/templates/i262_date_fmt"))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let detail = body_json(res).await;
        let resolved_d = detail["param_defaults"]["d"]["resolved"].as_str().unwrap();
        assert!(
            resolved_d.contains('/'),
            "expected formatted date with slashes, got: {resolved_d}"
        );
    }

    #[tokio::test]
    async fn issue_262_unreferenced_param_with_broken_default_in_param_defaults_and_fails_render() {
        let yaml = r#"
name: Unreferenced Broken Default
unit: mm
dpi: 200
params:
  unreferenced_broken:
    type: string
    default: "{vars.missing_var}"
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "Fixed Text"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let (app, _state) = test_app_with_custom_templates(vec![("i262_unref_broken", yaml)]);
        let res = app
            .clone()
            .oneshot(req_get("/api/templates/i262_unref_broken"))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let detail = body_json(res).await;

        // Present in param_defaults with error
        assert_eq!(
            detail["param_defaults"]["unreferenced_broken"]["error"]["reason"],
            "param_default_unresolvable"
        );
        // Absent from inputs
        assert!(!detail["inputs"]["default"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["name"] == "unreferenced_broken"));
        assert!(!detail["inputs"]["all"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["name"] == "unreferenced_broken"));

        // Render still fails with 422 param_default_unresolvable
        let req = req_post_json(
            "/api/render/label?format=png",
            &serde_json::json!({
                "template": "i262_unref_broken",
                "data": {}
            })
            .to_string(),
        );
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let err = body_json(res).await;
        assert_eq!(err["error"]["code"], "TemplateInvalid");
        assert_eq!(
            err["error"]["details"]["reason"],
            "param_default_unresolvable"
        );
        assert_eq!(err["error"]["details"]["param"], "unreferenced_broken");
    }

    #[tokio::test]
    async fn issue_262_boolean_default_yes_error_and_length_coerced() {
        let yaml = r#"
name: Coercion Test
unit: mm
dpi: 200
params:
  b_bad:
    type: boolean
    default: "yes"
  l_ok:
    type: length
    default: "80mm"
format: { type: single, width: 100, height: 50 }
layout:
  - type: text
    value: "{l_ok}"
    at: [0, 0]
    size: [100, 50]
    font_size: 10
"#;
        let (app, _state) = test_app_with_custom_templates(vec![("i262_coercion", yaml)]);
        let res = app
            .clone()
            .oneshot(req_get("/api/templates/i262_coercion"))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let detail = body_json(res).await;

        assert_eq!(
            detail["param_defaults"]["b_bad"]["error"]["reason"],
            "param_default_unresolvable"
        );
        assert_eq!(detail["param_defaults"]["b_bad"]["error"]["value"], "yes");

        assert_eq!(
            detail["param_defaults"]["l_ok"]["resolved"],
            serde_json::json!(80.0)
        );
    }

    #[tokio::test]
    async fn issue_262_put_template_returns_param_defaults() {
        let yaml = r#"
name: Put Defaults
unit: mm
dpi: 200
params:
  greeting:
    type: string
    default: "{vars.hello}"
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "{greeting}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let (app, _state) = test_app_with_custom_templates(vec![]);
        let req = Request::builder()
            .method("PUT")
            .uri("/api/templates/put_def")
            .header("content-type", "text/yaml")
            .body(Body::from(yaml.to_string()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = body_json(resp).await;
        assert_eq!(
            body["param_defaults"]["greeting"]["error"]["reason"],
            "param_default_unresolvable"
        );
        assert_eq!(
            body["param_defaults"]["greeting"]["error"]["token"],
            "vars.hello"
        );
        assert!(body["inputs"]["default"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["name"] == "greeting" && i["required"] == true));
    }

    #[tokio::test]
    async fn issue_262_store_failure_returns_500_and_leaves_no_file() {
        let (app, state) = test_app_with_custom_templates(vec![]);
        // Corrupt datetime_formats so resolve_datetime_formats fails -> 500
        state
            .store()
            .set_setting("datetime_formats", "{")
            .await
            .unwrap();
        let yaml = r#"
name: Should Not Be Written
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "hi"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let req = Request::builder()
            .method("PUT")
            .uri("/api/templates/should_not_exist")
            .header("content-type", "text/yaml")
            .body(Body::from(yaml.to_string()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        // No file should have been written. GET would also 500 while setting is corrupt,
        // so clear the corrupt setting first, then verify the template was never created.
        state
            .store()
            .delete_setting("datetime_formats")
            .await
            .unwrap();
        let get = app
            .clone()
            .oneshot(req_get("/api/templates/should_not_exist"))
            .await
            .unwrap();
        assert_eq!(get.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn issue_262_inputs_every_label_carries_same_default() {
        let yaml = r#"
name: Multi Default
unit: mm
dpi: 200
params:
  msg:
    type: string
    default: hello
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "{msg}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let (app, _state) = test_app_with_custom_templates(vec![("i262_multi_same", yaml)]);
        let req = req_post_json(
            "/api/templates/i262_multi_same/inputs",
            &serde_json::json!({
                "labels": [{ "data": {} }, { "data": {} }]
            })
            .to_string(),
        );
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_json(res).await;
        let inputs = body["inputs"].as_array().unwrap();
        assert_eq!(inputs.len(), 2);
        let d0 = inputs[0]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["name"] == "msg")
            .unwrap();
        let d1 = inputs[1]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["name"] == "msg")
            .unwrap();
        assert_eq!(d0["default"], "hello");
        assert_eq!(d1["default"], "hello");
        assert_eq!(d0["default"], d1["default"]);
    }

    #[tokio::test]
    async fn issue_262_catalog_empty_vars_lists_broken_default_as_field() {
        use crate::models::{
            FontSize, Layout, ParamSpec, ParamType, ParamValue, Placement, Position, Size,
            SizeValue, TemplateFormat,
        };
        use crate::templates::TemplateContent;
        use std::collections::BTreeMap;
        let template = TemplateContent {
            name: "Catalog Test".to_string(),
            description: "".to_string(),
            unit: "mm".to_string(),
            dpi: 200,
            format: TemplateFormat::Single {
                width: crate::models::Dimension::Fixed(50.0).into(),
                height: crate::models::Dimension::Fixed(20.0).into(),
                media_width: None,
            },
            params: BTreeMap::from([
                (
                    "needs_var".to_string(),
                    ParamSpec {
                        param_type: ParamType::String { multiline: false },
                        default: Some(ParamValue::String("{vars.missing}".to_string())),
                        min: None,
                        max: None,
                        description: None,
                    },
                ),
                (
                    "needs_sys".to_string(),
                    ParamSpec {
                        param_type: ParamType::String { multiline: false },
                        default: Some(ParamValue::String("{sys.now}".to_string())),
                        min: None,
                        max: None,
                        description: None,
                    },
                ),
            ]),
            layout: Layout::Items(vec![crate::models::LayoutItem::Text {
                value: "{needs_var} {needs_sys}".to_string(),
                placement: Placement::sized(
                    Position([0.0, 0.0]),
                    Size([SizeValue::fixed(10.0), SizeValue::fixed(10.0)]),
                ),
                font_size: FontSize::Fixed(10.0),
                font_weight: None,
                color: None,
                wrap: false,
                alignment: crate::models::Alignment::default(),
                overflow: crate::models::Overflow::Ellipsis,
                when: None,
            }]),
            version: None,
        };
        let variables = BTreeMap::new();
        let dt_formats = crate::settings::resolve_datetime_formats_from(None).unwrap_or_default();
        let now = chrono::Local::now();
        let dt = crate::datetime_fmt::DateTimeResolver {
            formats: &dt_formats,
            now,
        };
        let resolved = crate::render::resolve_declared_defaults(&template, &variables, &dt);
        let fields: Vec<String> = template
            .inputs_all(&resolved)
            .into_iter()
            .filter(|i| i.required)
            .map(|i| i.name)
            .collect();
        assert!(
            fields.contains(&"needs_var".to_string()),
            "vars default with empty store should be listed as field"
        );
        assert!(
            !fields.contains(&"needs_sys".to_string()),
            "sys.now default should resolve and not be listed"
        );
    }

    #[tokio::test]
    async fn issue_262_readonly_report_matches_render_details() {
        let yaml = r#"
name: Report Match
unit: mm
dpi: 200
params:
  val:
    type: string
    default: "{vars.missing_key}"
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "{val}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let (app, _state) = test_app_with_custom_templates(vec![("i262_match", yaml)]);
        let res = app
            .clone()
            .oneshot(req_get("/api/templates/i262_match"))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let detail = body_json(res).await;
        let err = &detail["param_defaults"]["val"]["error"];
        assert_eq!(err["reason"], "param_default_unresolvable");
        assert!(
            err.get("param").is_none(),
            "read-only report must not carry param"
        );
        let reason = err["reason"].as_str().unwrap().to_string();
        let message = err["message"].as_str().unwrap().to_string();
        let token = err["token"].as_str().unwrap().to_string();
        let req = req_post_json(
            "/api/render/label?format=png",
            &serde_json::json!({ "template": "i262_match", "data": {} }).to_string(),
        );
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let err_body = body_json(res).await;
        let details = &err_body["error"]["details"];
        assert_eq!(details["reason"], reason);
        assert_eq!(details["param"], "val");
        assert_eq!(details["token"], token);
        assert_eq!(err_body["error"]["message"], message);
        assert!(
            details.get("message").is_none(),
            "message must not be duplicated in details"
        );
    }

    #[tokio::test]
    async fn container_geometry_http_render_and_batch() {
        let app = test_app_no_auth();

        // 6.2 Render endpoint: content-sized circle
        // Square resolution renders OK
        let req_square = req_post_json(
            "/api/render/label?format=png",
            &serde_json::json!({
                "template": "container_circle_content",
                "data": {}
            })
            .to_string(),
        );
        let res_square = app.clone().oneshot(req_square).await.unwrap();
        assert_eq!(res_square.status(), StatusCode::OK);
        assert_eq!(
            res_square.headers().get("content-type").unwrap(),
            "image/png"
        );

        // Non-square content circle returns 422 with UnsupportedLayoutItem and circle_box_not_square
        let bad_content_circle_yaml = r#"
name: BadContentCircle
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
layout:
  - type: container
    at: [0, 0]
    shape: circle
    size: [content, content]
    items:
      - type: text
        value: "Non Square Text"
        at: [0, 0]
        size: [30, 10]
        font_size: 8
"#;
        let (custom_app, _state) =
            test_app_with_custom_templates(vec![("bad_content_circle", bad_content_circle_yaml)]);
        let req_bad_content = req_post_json(
            "/api/render/label?format=png",
            &serde_json::json!({
                "template": "bad_content_circle",
                "data": {}
            })
            .to_string(),
        );
        let res_bad_content = custom_app.clone().oneshot(req_bad_content).await.unwrap();
        assert_eq!(res_bad_content.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body_bad_content = body_json(res_bad_content).await;
        assert_eq!(body_bad_content["error"]["code"], "UnsupportedLayoutItem");
        assert_eq!(
            body_bad_content["error"]["details"]["reason"],
            "circle_box_not_square"
        );
        assert!(body_bad_content["error"]["message"]
            .as_str()
            .unwrap()
            .contains("layout[0]"));

        // 6.3 Batch endpoint: failure returns 422 BatchInvalid with failure details
        let req_batch = req_post_json(
            "/api/batch",
            &serde_json::json!({
                "template": "bad_content_circle",
                "labels": [
                    { "data": {} }
                ],
                "mode": "download"
            })
            .to_string(),
        );
        let res_batch = custom_app.clone().oneshot(req_batch).await.unwrap();
        assert_eq!(res_batch.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body_batch = body_json(res_batch).await;
        assert_eq!(body_batch["error"]["code"], "BatchInvalid");
        let failures = body_batch["error"]["details"]["failures"]
            .as_array()
            .unwrap();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0]["index"], 0);
        assert_eq!(failures[0]["code"], "UnsupportedLayoutItem");
        assert_eq!(failures[0]["reason"], "circle_box_not_square");
        assert!(failures[0]["message"]
            .as_str()
            .unwrap()
            .contains("layout[0]"));

        // 6.4 Render endpoint: container_circle_param
        // No w supplied -> default w=20 (square) -> renders OK
        let req_param_default = req_post_json(
            "/api/render/label?format=png",
            &serde_json::json!({
                "template": "container_circle_param",
                "data": {}
            })
            .to_string(),
        );
        let res_param_default = app.clone().oneshot(req_param_default).await.unwrap();
        assert_eq!(res_param_default.status(), StatusCode::OK);

        // Supplying w=14 -> non-square (14x20) -> 422 UnsupportedLayoutItem / circle_box_not_square
        let req_param_nonsquare = req_post_json(
            "/api/render/label?format=png",
            &serde_json::json!({
                "template": "container_circle_param",
                "data": { "w": 14.0 }
            })
            .to_string(),
        );
        let res_param_nonsquare = app.clone().oneshot(req_param_nonsquare).await.unwrap();
        assert_eq!(
            res_param_nonsquare.status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let body_param_nonsquare = body_json(res_param_nonsquare).await;
        assert_eq!(
            body_param_nonsquare["error"]["code"],
            "UnsupportedLayoutItem"
        );
        assert_eq!(
            body_param_nonsquare["error"]["details"]["reason"],
            "circle_box_not_square"
        );
        assert!(body_param_nonsquare["error"]["message"]
            .as_str()
            .unwrap()
            .contains("layout[0]"));

        // 6.5 Render endpoint: container_circle_gated
        // False when: (enabled: "no") with w=14 -> succeeds
        let req_gated_off = req_post_json(
            "/api/render/label?format=png",
            &serde_json::json!({
                "template": "container_circle_gated",
                "data": { "enabled": "no", "w": 14.0 }
            })
            .to_string(),
        );
        let res_gated_off = app.clone().oneshot(req_gated_off).await.unwrap();
        assert_eq!(res_gated_off.status(), StatusCode::OK);

        // True when: (enabled: "yes") with w=14 -> refused with 422 UnsupportedLayoutItem / circle_box_not_square
        let req_gated_on = req_post_json(
            "/api/render/label?format=png",
            &serde_json::json!({
                "template": "container_circle_gated",
                "data": { "enabled": "yes", "w": 14.0 }
            })
            .to_string(),
        );
        let res_gated_on = app.clone().oneshot(req_gated_on).await.unwrap();
        assert_eq!(res_gated_on.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body_gated_on = body_json(res_gated_on).await;
        assert_eq!(body_gated_on["error"]["code"], "UnsupportedLayoutItem");
        assert_eq!(
            body_gated_on["error"]["details"]["reason"],
            "circle_box_not_square"
        );
        assert!(body_gated_on["error"]["message"]
            .as_str()
            .unwrap()
            .contains("layout[0]"));

        // 6.6 Byte-identical render for default-rect container & unknown shape quarantine
        let explicit_rect_yaml = r#"
name: Container Default Rect
unit: mm
dpi: 200
format:
  type: single
  width: 40
  height: 30
layout:
  - type: container
    at: [2, 2]
    shape: rect
    size: [36, 26]
    stroke:
      thickness: 0.5
      color: black
    background: '#f0f0f0'
    items:
      - type: text
        value: "Default Rect"
        at: [2, 2]
        size: [32, 10]
        font_size: 8
"#;
        let (rect_app, _state) =
            test_app_with_custom_templates(vec![("explicit_rect", explicit_rect_yaml)]);
        let req_default_rect = req_post_json(
            "/api/render/label?format=png",
            &serde_json::json!({
                "template": "container_default_rect",
                "data": {}
            })
            .to_string(),
        );
        let res_default_rect = app.clone().oneshot(req_default_rect).await.unwrap();
        assert_eq!(res_default_rect.status(), StatusCode::OK);
        let bytes_default_rect = body_bytes(res_default_rect).await;

        let req_explicit_rect = req_post_json(
            "/api/render/label?format=png",
            &serde_json::json!({
                "template": "explicit_rect",
                "data": {}
            })
            .to_string(),
        );
        let res_explicit_rect = rect_app.clone().oneshot(req_explicit_rect).await.unwrap();
        assert_eq!(res_explicit_rect.status(), StatusCode::OK);
        let bytes_explicit_rect = body_bytes(res_explicit_rect).await;
        assert_eq!(
            bytes_default_rect, bytes_explicit_rect,
            "omitted shape and explicit shape: rect must render byte-identically"
        );

        // Unknown shape leaves template quarantined while serving others
        let unknown_shape_yaml = r#"
name: UnknownShape
unit: mm
dpi: 200
format: { type: single, width: 40, height: 30 }
layout:
  - type: container
    at: [0, 0]
    shape: octagon
    items: []
"#;
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "labeler-quarantine-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("unknown_shape.yaml"), unknown_shape_yaml).unwrap();
        std::fs::write(dir.join("valid_tpl.yaml"), explicit_rect_yaml).unwrap();

        let registry = TemplateRegistry::load_from_dir(&dir).unwrap();
        assert!(registry.get("valid_tpl").is_some());
        assert!(registry.get("unknown_shape").is_none());
        assert_eq!(registry.broken().len(), 1);
        let broken = &registry.broken()[0];
        assert_eq!(broken.path, "unknown_shape.yaml");
        assert!(broken.error.contains("unknown shape 'octagon'"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
