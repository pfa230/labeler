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
    use crate::models::{DynamicValue, Layout, LayoutItem};
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
                    let owner = store
                        .create_user("test", "unused-hash")
                        .await
                        .expect("seed token owner");
                    store
                        .create_token(&owner.id, "test", &super::auth::sha256_hex(TEST_TOKEN))
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
                        r#"{"name":"renamed","base_url":"http://hb.lan:7745"}"#,
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
    async fn update_connection_clears_omitted_public_url() {
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

        // PUT replaces the connection, so omitting public_url clears it
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/connections/{id}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name":"renamed","base_url":"http://hb.lan:7745"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let v = json_response(res).await;
        assert_eq!(v["name"], "renamed");
        assert_eq!(v["public_url"], Value::Null);
    }

    /// `null` is not a way to clear `public_url`: a key written as `null` is a malformed body. Blank and
    /// omitted both clear it.
    #[tokio::test]
    async fn update_connection_refuses_null_public_url_and_clears_blank_or_omitted() {
        let app = build_app();
        let res = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/connections",
                r#"{"connector":"homebox","name":"home","base_url":"http://hb.lan:7745","public_url":"https://homebox.example.com","credential":"secret"}"#.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let id = json_response(res).await["id"].as_str().unwrap().to_string();
        let put = |body: &'static str| {
            app.clone().oneshot(json_req(
                "PUT",
                &format!("/api/connections/{id}"),
                body.to_string(),
            ))
        };

        let res = put(r#"{"name":"home","base_url":"http://hb.lan:7745","public_url":null}"#)
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(body["error"]["details"]["reason"], "json_malformed");
        assert!(
            body["error"]["details"]["error"]
                .as_str()
                .unwrap()
                .contains("null"),
            "{body}"
        );

        // Sent with a trailing slash, so the update path is what proves the normalization, not the
        // create path or the shared helper.
        let res = put(r#"{"name":"home","base_url":"http://hb.lan:7745","public_url":"https://hb2.example.com/"}"#)
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            json_response(res).await["public_url"],
            "https://hb2.example.com"
        );

        let res = put(r#"{"name":"home","base_url":"http://hb.lan:7745","public_url":""}"#)
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(json_response(res).await["public_url"], Value::Null);

        let res = put(r#"{"name":"home","base_url":"http://hb.lan:7745","public_url":"https://hb2.example.com"}"#)
            .await
            .unwrap();
        assert_eq!(
            json_response(res).await["public_url"],
            "https://hb2.example.com"
        );
        let res = put(r#"{"name":"home","base_url":"http://hb.lan:7745"}"#)
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(json_response(res).await["public_url"], Value::Null);
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
                        r#"{"name":"home","base_url":"https://user:pass@hb.lan"}"#,
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
                Body::from(r#"{"name":"x","base_url":"http://hb.lan:7745"}"#),
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

    /// An update returns 200 with the new fields and the connector it was created with.
    #[tokio::test]
    async fn update_connection_replaces_fields_and_keeps_the_connector() {
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
                        r#"{"name":"new-name","base_url":"http://hb-updated.lan:7745"}"#,
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

    /// Create a Homebox connection through the API and return its id.
    async fn create_homebox_connection(app: &axum::Router, base_url: &str) -> String {
        let body = json!({
            "connector": "homebox",
            "name": "home",
            "base_url": base_url,
            "credential": "hb_key",
        });
        let res = app
            .clone()
            .oneshot(json_req("POST", "/api/connections", body.to_string()))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        json_response(res).await["id"].as_str().unwrap().to_string()
    }

    /// Assert a `400 json_malformed` whose `details.error` names `cause`.
    async fn assert_json_malformed(res: axum::response::Response, cause: &str) {
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(
            body["error"]["details"]["reason"], "json_malformed",
            "{body}"
        );
        let error = body["error"]["details"]["error"]
            .as_str()
            .unwrap_or_default();
        assert!(error.contains(cause), "expected `{cause}` in {body}");
    }

    /// The connector is fixed at create, so the update body has no `connector` key: sending one is an
    /// unlisted key, refused whatever its value.
    #[tokio::test]
    async fn update_connection_refuses_a_connector_key() {
        let app = build_app();
        let id = create_homebox_connection(&app, "http://hb.lan:7745").await;
        let res = app
            .clone()
            .oneshot(json_req(
                "PUT",
                &format!("/api/connections/{id}"),
                r#"{"connector":"homebox","name":"home","base_url":"http://hb.lan:7745"}"#.into(),
            ))
            .await
            .unwrap();
        assert_json_malformed(res, "unknown field").await;
    }

    #[tokio::test]
    async fn update_connection_refuses_a_blank_credential() {
        let (app, state) = build_app_with_state();
        let id = create_homebox_connection(&app, "http://hb.lan:7745").await;
        let res = app
            .clone()
            .oneshot(json_req(
                "PUT",
                &format!("/api/connections/{id}"),
                r#"{"name":"home","base_url":"http://hb.lan:7745","credential":""}"#.into(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(body["error"]["details"]["reason"], "credential_required");
        let stored = state.store().get_connection(&id).await.unwrap().unwrap();
        assert_eq!(stored.credential, "hb_key");
    }

    /// Each request is otherwise valid, so the one `null` is the only fault it carries.
    #[tokio::test]
    async fn connection_bodies_refuse_null_keys() {
        let app = build_app();
        let id = create_homebox_connection(&app, "http://hb.lan:7745").await;
        for (method, uri, body) in [
            (
                "PUT",
                format!("/api/connections/{id}"),
                r#"{"name":"home","base_url":"http://hb.lan:7745","credential":null}"#,
            ),
            (
                "POST",
                "/api/connections".to_string(),
                r#"{"connector":"homebox","name":"home","base_url":"http://hb.lan:7745","public_url":null,"credential":"secret"}"#,
            ),
            (
                "POST",
                "/api/connections".to_string(),
                r#"{"connector":"homebox","name":"home","base_url":"http://hb.lan:7745","credential":null}"#,
            ),
        ] {
            let res = app
                .clone()
                .oneshot(json_req(method, &uri, body.into()))
                .await
                .unwrap();
            assert_json_malformed(res, "null").await;
        }
    }

    #[tokio::test]
    async fn create_connection_refuses_enabled_and_transforms() {
        let app = build_app();
        for extra in [r#""enabled":true"#, r#""transforms":[]"#] {
            let body = format!(
                r#"{{"connector":"homebox","name":"home","base_url":"http://hb.lan:7745","credential":"secret",{extra}}}"#
            );
            let res = app
                .clone()
                .oneshot(json_req("POST", "/api/connections", body))
                .await
                .unwrap();
            assert_json_malformed(res, "unknown field").await;
        }

        let id = create_homebox_connection(&app, "http://hb.lan:7745").await;
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/connections/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = json_response(res).await;
        assert!(v.get("enabled").is_none(), "{v}");
        assert!(v.get("transforms").is_none(), "{v}");
    }

    /// The URL parser drops empty userinfo, so `https://@host` parses with no username and no
    /// password; only the parser's syntax violation shows it was there.
    #[tokio::test]
    async fn connection_urls_refuse_empty_userinfo() {
        let app = build_app();
        let id = create_homebox_connection(&app, "http://hb.lan:7745").await;
        for url in ["https://@host", "https:////@host"] {
            for (field, reason) in [
                ("base_url", "base_url_invalid"),
                ("public_url", "public_url_invalid"),
            ] {
                let mut create = json!({
                    "connector": "homebox",
                    "name": "home",
                    "base_url": "http://hb.lan:7745",
                    "credential": "secret",
                });
                create[field] = json!(url);
                let mut update = json!({
                    "name": "home",
                    "base_url": "http://hb.lan:7745",
                });
                update[field] = json!(url);
                for (method, uri, body) in [
                    ("POST", "/api/connections".to_string(), create),
                    ("PUT", format!("/api/connections/{id}"), update),
                ] {
                    let res = app
                        .clone()
                        .oneshot(json_req(method, &uri, body.to_string()))
                        .await
                        .unwrap();
                    assert_eq!(
                        res.status(),
                        StatusCode::BAD_REQUEST,
                        "{method} {field} {url}"
                    );
                    let body = json_response(res).await;
                    assert_eq!(
                        body["error"]["details"]["reason"], reason,
                        "{method} {field} {url}"
                    );
                }
            }
        }
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
        assert_eq!(
            body["error"]["details"],
            json!({ "kind": "route", "id": "/api/nope" })
        );
    }

    /// errors "A request fault outranks label faults": the unknown printer is reported, not the
    /// label's undeclared key.
    #[tokio::test]
    async fn unknown_printer_outranks_a_bad_label() {
        let app = build_app();
        let payload = json!({
            "template": "brother_24mm_qr",
            "printer": "missing",
            "labels": [{ "data": { "code": "A", "message": "m", "bad_key": "x" } }]
        });
        let res = app
            .oneshot(json_req("POST", "/api/print", payload.to_string()))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "NotFound");
        assert_eq!(
            body["error"]["details"],
            json!({ "kind": "printer", "id": "missing" })
        );
    }

    #[tokio::test]
    async fn a_qr_payload_too_long_to_encode_is_qr_payload_invalid() {
        let app = build_app();
        let payload = json!({
            "template": "brother_24mm_qr",
            "data": { "code": "x".repeat(4000), "message": "m" }
        });
        let res = app
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png",
                payload.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "UnsupportedLayoutItem");
        assert_eq!(body["error"]["details"]["reason"], "qr_payload_invalid");
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
        let body = json_response(response).await;
        assert_eq!(body["error"]["code"], "NotFound");
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

    #[tokio::test]
    async fn thumbnail_enum_with_default_shows_declared_default_via_http() {
        let dir = temp_templates_dir();
        let yaml_enum = r#"
name: Enum HTTP
unit: mm
dpi: 200
params:
  - name: orientation
    type: enum
    values: [horizontal, vertical]
    default: vertical
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "{orientation}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let yaml_vertical = r#"
name: Control Vertical
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "vertical"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let yaml_horizontal = r#"
name: Control Horizontal
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "horizontal"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        std::fs::write(dir.join("enum_http.yaml"), yaml_enum).unwrap();
        std::fs::write(dir.join("control_vertical.yaml"), yaml_vertical).unwrap();
        std::fs::write(dir.join("control_horizontal.yaml"), yaml_horizontal).unwrap();
        let app = build_app_in(&dir);
        async fn fetch(app: &axum::Router, id: &str) -> Vec<u8> {
            let res = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(format!("/api/templates/{id}/thumbnail"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::OK, "thumbnail {id} status");
            assert_eq!(res.headers().get("content-type").unwrap(), "image/png");
            bytes_response(res).await
        }
        let png_enum = fetch(&app, "enum_http").await;
        let png_vertical = fetch(&app, "control_vertical").await;
        let png_horizontal = fetch(&app, "control_horizontal").await;
        assert_eq!(
            png_enum, png_vertical,
            "enum thumbnail must match vertical literal control"
        );
        assert_ne!(
            png_enum, png_horizontal,
            "enum thumbnail must differ from horizontal control"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn thumbnail_broken_enum_default_is_422_via_http() {
        let dir = temp_templates_dir();
        let yaml = r#"
name: Broken Enum HTTP
unit: mm
dpi: 200
params:
  - name: orientation
    type: enum
    values: [horizontal, vertical]
    default: "{vars.orient}"
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "{orientation}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        std::fs::write(dir.join("broken_enum.yaml"), yaml).unwrap();
        let app = build_app_in(&dir);
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/templates/broken_enum/thumbnail")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "TemplateInvalid");
        assert_eq!(body["error"]["details"]["reason"], "reference_unresolved");
        assert_eq!(body["error"]["details"]["field"], "vars.orient");
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("orientation"),
            "error must name orientation"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn thumbnail_enum_gate_without_default_takes_the_first_value_via_http() {
        let dir = temp_templates_dir();
        let yaml_gate = r#"
name: Enum Gate HTTP
unit: mm
dpi: 200
params:
  - name: outline
    type: enum
    values: [yes]
format: { type: single, width: 50, height: 20 }
layout:
  - type: container
    when:
      outline: yes
    at: [0, 0]
    size: [50, 20]
    items:
      - type: text
        value: "Gated"
        at: [0, 0]
        size: [50, 20]
        font_size: 10
"#;
        // Control with no gated items: an undefaulted enum takes its first value, so the gate
        // opens and the gated thumbnail draws the container, unlike this empty label.
        let yaml_empty = r#"
name: Empty HTTP
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout: []
"#;
        std::fs::write(dir.join("enum_gate.yaml"), yaml_gate).unwrap();
        std::fs::write(dir.join("empty_control.yaml"), yaml_empty).unwrap();
        let app = build_app_in(&dir);
        async fn fetch(app: &axum::Router, id: &str) -> Vec<u8> {
            let res = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(format!("/api/templates/{id}/thumbnail"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::OK, "thumbnail {id} status");
            assert_eq!(res.headers().get("content-type").unwrap(), "image/png");
            bytes_response(res).await
        }
        let png_gate = fetch(&app, "enum_gate").await;
        let png_empty = fetch(&app, "empty_control").await;
        assert_ne!(
            png_gate, png_empty,
            "an undefaulted gate takes its first value, so the gated container is drawn"
        );
        std::fs::remove_dir_all(&dir).ok();
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
    async fn template_list_and_detail_params_array_shape_and_empty_broken() {
        let app = build_app();
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
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_response(response).await;

        // Broken list is empty for clean catalog (omitted when empty by serde skip_serializing_if)
        assert!(
            body.get("broken").is_none() || body["broken"].as_array().is_some_and(|b| b.is_empty()),
            "broken templates list must be empty or omitted: {:?}",
            body.get("broken")
        );

        // All templates publish params as an array (never object, never null, never omitted)
        let templates = body["templates"].as_array().expect("templates array");
        assert!(!templates.is_empty());
        for tpl in templates {
            assert!(
                tpl["params"].is_array(),
                "template {} params must be an array, got: {}",
                tpl["id"],
                tpl["params"]
            );
        }

        // Detail endpoint also returns params as array in declaration order
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/templates/brother_24mm_qr")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        let detail = json_response(response).await;
        let params = detail["params"].as_array().expect("detail params array");
        let param_names: Vec<&str> = params
            .iter()
            .map(|p| p["name"].as_str().expect("param name string"))
            .collect();
        assert_eq!(param_names, vec!["code", "message"]);
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
        assert_eq!(body["error"]["code"], "NotFound");
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
        assert_eq!(body["error"]["code"], "NotFound");
    }

    #[tokio::test]
    async fn render_png() {
        let app = build_app();
        let label_payload = json!({
            "template": "brother_12mm",
            "data": {
                "message": "Hello"
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
            assert_eq!(body["error"]["details"]["reason"], "param_value_invalid");
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
            "labels": [
                { "data": { "message": "one", "printed_on": "2026-08-18" } },
                { "data": { "message": "two", "printed_on": "not a date" } },
                { "data": { "message": "three", "printed_on": "2026-08-19" } }
            ]
        });
        let response = build_app()
            .oneshot(json_req("POST", "/api/render", payload.to_string()))
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
        assert_eq!(failures[0]["details"]["reason"], "param_value_invalid");
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
        let params = body["params"].as_array().unwrap();
        let printed_on = params.iter().find(|p| p["name"] == "printed_on").unwrap();
        assert_eq!(printed_on["type"], "datetime");
        assert_eq!(
            printed_on["time"], false,
            "time is always published, so the form never has to guess: {body}"
        );
    }

    #[tokio::test]
    async fn batch_single_download_returns_zip() {
        let app = build_app();
        let payload = json!({
            "template": "brother_24mm_qr",
            "labels": [
                { "data": { "message": "Hello", "code": "QR-1" } },
                { "data": { "message": "World", "code": "QR-2" } }
            ]
        });
        let response = app
            .oneshot(json_req("POST", "/api/render", payload.to_string()))
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
            "labels": [label.clone(), label]
        });
        let response = app
            .oneshot(json_req("POST", "/api/render", payload.to_string()))
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
            "labels": [label.clone(), label]
        });
        let response = app
            .oneshot(json_req("POST", "/api/render", payload.to_string()))
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
            "labels": [
                { "data": { "message": "Hello", "code": "QR-1" } },
                // (guard) an over-capacity QR payload stands in for the label that fails
                { "data": { "message": "World", "code": "A".repeat(8000) } }
            ]
        });
        let response = app
            .oneshot(json_req("POST", "/api/render", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(response).await;
        assert_eq!(body["error"]["code"], "BatchInvalid");
        assert_eq!(body["error"]["details"]["failures"][0]["index"], 1);
        assert_eq!(
            body["error"]["details"]["failures"][0]["details"]["reason"],
            "qr_payload_invalid"
        );
    }

    #[tokio::test]
    async fn batch_print_summary_ok() {
        let app = build_app();
        create_fake_printer(&app, "ok-printer", false).await;
        let payload = json!({
            "template": "brother_24mm_qr",
            "printer": "ok-printer",
            "labels": [
                { "data": { "message": "Hello", "code": "QR-1" } },
                { "data": { "message": "World", "code": "QR-2" } }
            ]
        });
        let response = app
            .clone()
            .oneshot(json_req("POST", "/api/print", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_response(response).await;
        assert_eq!(body["total"], 2);
        assert_eq!(body["sent"], 2);
        assert_eq!(body["failed"].as_array().expect("failed array").len(), 0);
    }

    #[tokio::test]
    async fn batch_print_summary_failure() {
        let app = build_app();
        create_fake_printer(&app, "bad-printer", true).await;
        let payload = json!({
            "template": "brother_24mm_qr",
            "printer": "bad-printer",
            "labels": [
                { "data": { "message": "Hello", "code": "QR-1" } },
                { "data": { "message": "World", "code": "QR-2" } }
            ]
        });
        let response = app
            .clone()
            .oneshot(json_req("POST", "/api/print", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_response(response).await;
        assert_eq!(body["sent"], 0);
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
            "uri": "ipp://fake.test/",
            "render": { "color_mode": "bilevel", "resolution": 203 }
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
            "printer": "bl",
            "labels": [ { "data": { "message": "Hi", "code": "Q" } } ]
        });
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/print", payload.to_string()))
            .await
            .expect("req");
        assert_eq!(resp.status(), StatusCode::OK);
        let summary = json_response(resp).await;
        // sent == 1 is LOAD-BEARING: the fake driver rejects a non-PNG artifact when
        // configured bilevel, so success proves the print path rendered + sent a bilevel PNG.
        assert_eq!(summary["sent"], 1);
        assert_eq!(summary["failed"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn batch_sheet_print_failure_marks_all() {
        let app = build_app();
        create_fake_printer(&app, "bad-sheet-printer", true).await;
        let label = json!({
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
            "printer": "bad-sheet-printer",
            "labels": [label.clone(), label]
        });
        let response = app
            .clone()
            .oneshot(json_req("POST", "/api/print", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_response(response).await;
        assert_eq!(body["total"], 2);
        assert_eq!(body["sent"], 0);
        let failed = body["failed"].as_array().expect("failed array");
        assert_eq!(failed.len(), 2);
        assert_eq!(body["jobs"], 1);
    }

    #[tokio::test]
    async fn batch_sheet_print_success_one_job() {
        let app = build_app();
        create_fake_printer(&app, "ok-sheet-printer", false).await;
        let label = json!({
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
            "printer": "ok-sheet-printer",
            "labels": [label.clone(), label]
        });
        let response = app
            .clone()
            .oneshot(json_req("POST", "/api/print", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_response(response).await;
        assert_eq!(body["total"], 2);
        assert_eq!(body["sent"], 2);
        assert_eq!(body["failed"].as_array().expect("failed array").len(), 0);
        assert_eq!(body["jobs"], 1);
    }

    async fn post_batch(app: &axum::Router, uri: &str, body: Value) -> (StatusCode, Value) {
        let resp = app
            .clone()
            .oneshot(json_req("POST", uri, body.to_string()))
            .await
            .expect("request");
        let status = resp.status();
        (status, json_response(resp).await)
    }

    #[tokio::test]
    async fn inapplicable_batch_fields_are_refused_whatever_their_value() {
        let app = build_app();
        create_fake_printer(&app, "p", false).await;
        let single = json!([{ "data": { "message": "Hi", "code": "Q" } }]);
        let sheet =
            json!([{ "data": { "id": "A1", "url": "https://example.com/A1", "name": "Grinder" } }]);
        let cases = [
            (
                "/api/render",
                json!({ "template": "brother_24mm_qr", "labels": single, "start_slot": 0 }),
            ),
            (
                "/api/print",
                json!({ "template": "brother_24mm_qr", "labels": single, "start_slot": 0, "printer": "p" }),
            ),
            (
                "/api/render",
                json!({ "template": "avery5163_asset_tag", "labels": sheet, "format": "pdf" }),
            ),
            (
                "/api/render",
                json!({ "template": "avery5163_asset_tag", "labels": sheet, "format": "png" }),
            ),
        ];
        for (uri, body) in cases {
            let (status, resp) = post_batch(&app, uri, body.clone()).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri} {body}");
            assert_eq!(
                resp["error"]["details"]["reason"], "field_not_applicable",
                "{uri} {body}"
            );
        }
    }

    #[tokio::test]
    async fn batch_fields_of_the_other_endpoint_are_unlisted_keys() {
        let app = build_app();
        create_fake_printer(&app, "p", false).await;
        let labels = json!([{ "data": { "message": "Hi", "code": "Q" } }]);
        let cases = [
            (
                "/api/render",
                json!({ "template": "brother_24mm_qr", "labels": labels, "printer": "p" }),
            ),
            (
                "/api/print",
                json!({ "template": "brother_24mm_qr", "labels": labels, "printer": "p", "format": "png" }),
            ),
            (
                "/api/render",
                json!({ "template": "brother_24mm_qr", "labels": labels, "mode": "download" }),
            ),
            (
                "/api/render",
                json!({ "template": "brother_24mm_qr", "labels": labels, "start_slot": null }),
            ),
            (
                "/api/print",
                json!({ "template": "brother_24mm_qr", "labels": labels }),
            ),
        ];
        for (uri, body) in cases {
            let (status, resp) = post_batch(&app, uri, body.clone()).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri} {body}");
            assert_eq!(
                resp["error"]["details"]["reason"], "json_malformed",
                "{uri} {body}"
            );
        }
    }

    #[tokio::test]
    async fn render_format_is_png_or_pdf_exactly() {
        let app = build_app();
        let labels = json!([{ "data": { "message": "Hi", "code": "Q" } }]);
        let (status, resp) = post_batch(
            &app,
            "/api/render",
            json!({ "template": "brother_24mm_qr", "labels": labels, "format": "" }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(resp["error"]["details"]["reason"], "format_unknown");
    }

    #[tokio::test]
    async fn single_render_zip_entries_are_padded_label_indices() {
        use std::io::Read as _;
        let app = build_app();
        let label = json!({ "data": { "message": "Hi", "code": "Q" } });
        for (count, format, want_first, want_last) in [
            (10, Some("pdf"), "01.pdf", "10.pdf"),
            (1, None, "1.png", "1.png"),
        ] {
            let mut body =
                json!({ "template": "brother_24mm_qr", "labels": vec![label.clone(); count] });
            if let Some(f) = format {
                body["format"] = json!(f);
            }
            let resp = app
                .clone()
                .oneshot(json_req("POST", "/api/render", body.to_string()))
                .await
                .expect("request");
            assert_eq!(resp.status(), StatusCode::OK);
            let disposition = resp.headers()["content-disposition"]
                .to_str()
                .unwrap()
                .to_string();
            assert_eq!(disposition, "attachment; filename=\"brother_24mm_qr.zip\"");
            let bytes = resp.into_body().collect().await.unwrap().to_bytes();
            let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
            let names: Vec<String> = (0..zip.len())
                .map(|i| zip.by_index(i).unwrap().name().to_string())
                .collect();
            assert_eq!(names.len(), count);
            assert_eq!(
                (names[0].as_str(), names[count - 1].as_str()),
                (want_first, want_last)
            );
            let mut first = Vec::new();
            zip.by_index(0).unwrap().read_to_end(&mut first).unwrap();
            let magic: &[u8] = if format == Some("pdf") {
                b"%PDF"
            } else {
                b"\x89PNG"
            };
            assert!(first.starts_with(magic), "{want_first} content");
        }
    }

    #[tokio::test]
    async fn print_takes_the_shared_body_limit() {
        let app = build_app();
        let body = |len: usize| {
            json!({
                "template": "brother_24mm_qr",
                "printer": "no-such-printer",
                "labels": [{ "data": { "message": "a".repeat(len), "code": "Q" } }]
            })
        };
        // Above the old 64 KiB layer, below ~2 MiB: reaches the handler, which refuses the printer.
        let (status, resp) = post_batch(&app, "/api/print", body(100 * 1024)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(resp["error"]["details"]["kind"], "printer");
        // Above ~2 MiB: the shared limit.
        let (status, resp) = post_batch(&app, "/api/print", body(2200 * 1024)).await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(resp["error"]["code"], "PayloadTooLarge");
    }

    #[tokio::test]
    async fn removed_batch_endpoints_are_unknown_routes() {
        let app = build_app();
        for uri in ["/api/batch", "/api/import/csv"] {
            let (status, resp) = post_batch(&app, uri, json!({})).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
            assert_eq!(resp["error"]["details"]["kind"], "route", "{uri}");
        }
    }

    #[tokio::test]
    async fn render_label_pdf() {
        let app = build_app();
        let payload = json!({
            "template": "brother_12mm",
            "data": { "message": "Hello" }
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
    async fn render_label_on_a_sheet_template_is_format_unsupported() {
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
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = json_response(response).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
        assert_eq!(body["error"]["details"]["reason"], "format_unsupported");
    }

    /// `NotFound` names what is missing by `kind` and `id`, and carries no reason.
    #[tokio::test]
    async fn unknown_template_is_not_found_by_kind_and_id() {
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
        assert_eq!(body["error"]["code"], "NotFound");
        assert_eq!(
            body["error"]["details"],
            serde_json::json!({ "kind": "template", "id": "does-not-exist" })
        );
    }

    #[tokio::test]
    async fn render_enum_out_of_range_is_400_param_value_invalid() {
        let yaml = r#"
name: Orientation Label
unit: mm
dpi: 200
params:
  - name: orientation
    type: enum
    values: [horizontal, vertical]
format:
  type: single
  width: 50
  height: 30
layout:
  - type: text
    value: "{orientation}"
    at: [0, 0]
    size: [50, 10]
    font_size: 10
"#;
        let (app, _) = build_app_with_custom_templates(vec![("enum_render", yaml)]);
        let payload = serde_json::json!({
            "template": "enum_render",
            "data": { "orientation": "sideways" }
        })
        .to_string();
        let req = json_req("POST", "/api/render/label?format=png", payload);
        let res = app.oneshot(req).await.expect("request");
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
        assert_eq!(
            body["error"]["details"],
            serde_json::json!({ "reason": "param_value_invalid", "param": "orientation" })
        );
    }

    #[tokio::test]
    async fn batch_enum_out_of_range_reports_param_value_invalid_per_row() {
        let yaml = r#"
name: Orientation Label
unit: mm
dpi: 200
params:
  - name: orientation
    type: enum
    values: [horizontal, vertical]
format:
  type: single
  width: 50
  height: 30
layout:
  - type: text
    value: "{orientation}"
    at: [0, 0]
    size: [50, 10]
    font_size: 10
"#;
        let (app, _) = build_app_with_custom_templates(vec![("enum_batch", yaml)]);
        let payload = serde_json::json!({
            "template": "enum_batch",
            "labels": [
                { "data": { "orientation": "horizontal" } },
                { "data": { "orientation": "sideways" } }
            ]
        })
        .to_string();
        let req = json_req("POST", "/api/render", payload);
        let res = app.oneshot(req).await.expect("request");
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "BatchInvalid");
        let failures = body["error"]["details"]["failures"]
            .as_array()
            .expect("failures");
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0]["code"], "InvalidRequest");
        assert_eq!(
            failures[0]["details"],
            serde_json::json!({ "reason": "param_value_invalid", "param": "orientation" })
        );
        // top-level details must be BatchInvalid shape, not reshaped
        assert!(body["error"]["details"].get("failures").is_some());
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
params:
  - name: msg
    type: string
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
        assert_eq!(
            body["error"]["details"]["reason"],
            "template_validation_failed"
        );
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
  - name: val
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
  - name: dt
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
                "POST",
                yaml_dt_sys.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);

        // 3. Explicit null default is refused, like every null key
        let yaml_null = r#"
name: Null Default
unit: mm
dpi: 200
params:
  - name: str_val
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
                "POST",
                yaml_null.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(res).await;
        assert_eq!(
            body["error"]["details"]["reason"],
            "template_validation_failed"
        );
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("params.str_val.default"),
            "{body}"
        );

        // 4. Non-string datetime default is refused
        let yaml_non_str_dt = r#"
name: Non String DT
unit: mm
dpi: 200
params:
  - name: dt
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
  - name: dt
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
    async fn template_keeps_a_padded_color_literal_and_a_canonical_reference() {
        let dir = temp_templates_dir();
        let yaml = r#"
name: ColorReadback
unit: mm
dpi: 200
params:
  - name: brand
    type: string
    default: red
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

        let (status, _) = get_json(&app, "/api/templates/color_readback").await;
        assert_eq!(status, StatusCode::OK, "the template is served");

        let Layout::Items(items) = crate::parse::parse_template(yaml).unwrap().layout;
        let LayoutItem::Text {
            color: Some(DynamicValue::Literal(color)),
            ..
        } = &items[0]
        else {
            panic!("item 0 is a text with a literal color: {:?}", items[0]);
        };
        assert_eq!(color.spelling(), " red ");
        let LayoutItem::Container {
            background,
            items: children,
            ..
        } = &items[1]
        else {
            panic!("item 1 is a container: {:?}", items[1]);
        };
        assert_eq!(background, &Some(DynamicValue::Ref("brand".to_string())));
        let LayoutItem::Text {
            color: Some(DynamicValue::Literal(color)),
            ..
        } = &children[0]
        else {
            panic!("child 0 is a text with a literal color: {:?}", children[0]);
        };
        assert_eq!(color.spelling(), "blue");

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
    async fn template_line_spacing_readback_and_refusals() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);

        // 1. PUT a template with explicit line_spacing: 0.99, absent line_spacing, and ref "{pitch}"
        let yaml_valid = r#"
name: SpacingReadback
unit: mm
dpi: 200
format: { type: single, width: 50, height: 60 }
params:
  - name: pitch
    type: number
    default: 1.2
layout:
  - type: text
    value: "Explicit Spacing"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    line_spacing: 0.99
  - type: text
    value: "Default Spacing"
    at: [0, 20]
    size: [50, 20]
    font_size: 10
  - type: text
    value: "Ref Spacing"
    at: [0, 40]
    size: [50, 20]
    font_size: 10
    line_spacing: "{pitch}"
"#;
        let res = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/spacing_readback",
                "POST",
                yaml_valid.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);

        let Layout::Items(items) = crate::parse::parse_template(yaml_valid).unwrap().layout;
        let line_spacing = |item: &LayoutItem| match item {
            LayoutItem::Text { line_spacing, .. } => line_spacing.clone(),
            other => panic!("expected a text item: {other:?}"),
        };
        // Item 0 keeps authored 0.99, item 1 declares none, item 2 references "{pitch}"
        assert_eq!(line_spacing(&items[0]), Some(DynamicValue::Literal(0.99)));
        assert_eq!(line_spacing(&items[1]), None);
        assert_eq!(
            line_spacing(&items[2]),
            Some(DynamicValue::Ref("pitch".to_string()))
        );

        // 2. PUT with line_spacing on non-text items: container, qr, image, line
        for (item_type, item_yaml) in [
            (
                "container",
                r#"  - type: container
    at: [0, 0]
    size: [50, 20]
    line_spacing: 0.99
    items: []"#,
            ),
            (
                "qr",
                r#"  - type: qr
    value: "https://example.com"
    at: [0, 0]
    size: [20, 20]
    line_spacing: 0.99"#,
            ),
            (
                "image",
                r#"  - type: image
    name: logo
    at: [0, 0]
    size: [20, 20]
    line_spacing: 0.99"#,
            ),
            (
                "line",
                r#"  - type: line
    at: [0, 0]
    to: [50, 0]
    line_spacing: 0.99"#,
            ),
        ] {
            let id = format!("bad_item_{item_type}");
            let bad_yaml = format!(
                r#"name: Bad {item_type}
unit: mm
dpi: 200
format: {{ type: single, width: 50, height: 20 }}
layout:
{item_yaml}
"#
            );
            let bad_res = app
                .clone()
                .oneshot(yaml_post(&format!("/api/templates/{id}"), "PUT", bad_yaml))
                .await
                .unwrap();
            assert_eq!(bad_res.status(), StatusCode::UNPROCESSABLE_ENTITY);
            let bad_body = json_response(bad_res).await;
            assert_eq!(bad_body["error"]["code"], "TemplateInvalid");
            let msg = bad_body["error"]["message"].as_str().unwrap_or("");
            assert!(
                msg.contains("unknown field `line_spacing`") || msg.contains("line_spacing"),
                "error for {item_type} must mention line_spacing: {msg}"
            );
            assert!(
                !dir.join(format!("{id}.yaml")).exists(),
                "file {id}.yaml should not be written to disk"
            );
        }

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
                body["error"]["details"]["reason"], "template_validation_failed",
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
        assert_eq!(
            json["error"]["details"]["reason"],
            "template_validation_failed"
        );
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
  - name: orientation
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
                    .method("POST")
                    .uri("/api/templates/new_tpl")
                    .header("content-type", "text/yaml")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let json = json_response(response).await;
        assert_eq!(json["error"]["code"], "TemplateInvalid");
        assert_eq!(
            json["error"]["details"]["reason"],
            "template_validation_failed"
        );
        let msg = json["error"]["message"].as_str().unwrap_or("");
        assert!(
            msg.contains("layout[0]") && msg.contains("unknown field `option`"),
            "expected layout[0] and 'unknown field `option`' in error message, got: {msg}"
        );
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            0,
            "a refused create must leave no file"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A templates directory that cannot be read is the service's fault: `500 Internal` with no
    /// details. Deleting the directory out from under a built app reaches it without depending on
    /// the test user's uid, which a read-only directory would.
    #[tokio::test]
    async fn a_failed_templates_directory_read_is_internal() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);
        std::fs::remove_dir_all(&dir).expect("remove templates dir");

        let response = app
            .oneshot(yaml_post(
                "/api/templates/wf1",
                "POST",
                template_yaml("wf1"),
            ))
            .await
            .expect("request");
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = json_response(response).await;
        assert_eq!(body["error"]["code"], "Internal");
        assert!(body["error"].get("details").is_none(), "{body}");
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

    /// A template file reading an undeclared parameter is quarantined at startup and on reload,
    /// while the service starts and continues serving its valid siblings (#175, issue 322).
    #[tokio::test]
    async fn issue_322_template_with_undeclared_name_quarantined_at_startup_and_reload() {
        let dir = temp_templates_dir();
        // Valid sibling
        std::fs::write(dir.join("valid.yaml"), template_yaml("valid")).unwrap();

        // Undeclared name template
        let bad_yaml = r#"
name: bad_tpl
unit: mm
dpi: 200
format: { type: single, width: 20, height: 10 }
layout:
  - type: text
    value: "{undeclared_name}"
    at: [0, 0]
    size: [20, 10]
    font_size: 10
"#;
        std::fs::write(dir.join("bad.yaml"), bad_yaml).unwrap();

        // 1. Startup: Service starts, serves valid template, quarantines bad
        let app = build_app_in(&dir);
        let (_, list) = get_json(&app, "/api/templates").await;
        let templates = list["templates"].as_array().unwrap();
        assert_eq!(templates.len(), 1);
        assert_eq!(templates[0]["name"], "valid");

        let broken = list["broken"].as_array().unwrap();
        assert_eq!(broken.len(), 1);
        assert_eq!(broken[0]["path"], "bad.yaml");
        assert!(broken[0]["error"]
            .as_str()
            .unwrap()
            .contains("undeclared parameter 'undeclared_name'"));

        // 2. Reload: POST /api/templates/reload keeps valid served and bad quarantined
        let reload_body = reload(&app).await;
        assert_eq!(reload_body["count"], 1);
        assert_eq!(reload_body["broken_count"], 1);

        let (_, list_after_reload) = get_json(&app, "/api/templates").await;
        assert_eq!(list_after_reload["templates"].as_array().unwrap().len(), 1);
        assert_eq!(list_after_reload["broken"].as_array().unwrap().len(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A template write (PUT) with an undeclared parameter reference is rejected with 422 TemplateInvalid
    /// and details.reason template_validation_failed, and nothing is written to disk (issue 322).
    #[tokio::test]
    async fn issue_322_template_put_with_undeclared_name_rejected_and_not_stored() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);

        let bad_yaml = r#"
name: new_bad
unit: mm
dpi: 200
format: { type: single, width: 20, height: 10 }
layout:
  - type: text
    value: "{undeclared_name}"
    at: [0, 0]
    size: [20, 10]
    font_size: 10
"#;
        let res = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/new_bad",
                "PUT",
                bad_yaml.to_string(),
            ))
            .await
            .expect("request");
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "TemplateInvalid");
        assert_eq!(
            body["error"]["details"]["reason"],
            "template_validation_failed"
        );
        assert!(body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("undeclared parameter 'undeclared_name'"));

        // Nothing was written to disk
        assert!(!dir.join("new_bad.yaml").exists());
        assert!(!dir.join("new_bad.yml").exists());

        // Template registry has no templates and no broken files
        let (_, list) = get_json(&app, "/api/templates").await;
        assert_eq!(list["templates"].as_array().unwrap().len(), 0);
        assert!(list.get("broken").is_none() || list["broken"].as_array().unwrap().is_empty());

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
        assert_eq!(body["error"]["code"], "Internal");
        assert!(body["error"].get("details").is_none(), "{body}");

        assert_eq!(template_count(&app).await, 1);
    }

    /// A folder that can be listed but not searched (`rw-`): every entry's metadata read fails, so
    /// reload must answer `500` and keep the live set rather than quarantine every template.
    #[tokio::test]
    async fn reload_with_unsearchable_dir_fails_and_keeps_the_live_set() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_templates_dir();
        std::fs::write(dir.join("t1.yaml"), template_yaml("t1")).unwrap();
        let app = build_app_in(&dir);
        assert_eq!(template_count(&app).await, 1);

        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o600)).unwrap();
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
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
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

    const UNPARSEABLE_TEMPLATE: &str = "not: a valid template\n";

    fn delete_req(uri: &str) -> Request<Body> {
        Request::builder()
            .method("DELETE")
            .uri(uri)
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn summaries_and_detail_carry_categories() {
        let dir = temp_templates_dir();
        let pallet = template_yaml("pallet").replace(
            "description: d\n",
            "description: d\ncategories: [Shipping, Warehouse]\n",
        );
        std::fs::write(dir.join("pallet.yaml"), pallet).unwrap();
        std::fs::write(dir.join("bin.yaml"), template_yaml("bin")).unwrap();
        let app = build_app_in(&dir);

        let (status, list) = get_json(&app, "/api/templates").await;
        assert_eq!(status, StatusCode::OK);
        let categories_of = |id: &str| {
            list["templates"]
                .as_array()
                .expect("templates array")
                .iter()
                .find(|t| t["id"] == id)
                .unwrap_or_else(|| panic!("{id} is listed: {list}"))["categories"]
                .clone()
        };
        assert_eq!(categories_of("pallet"), json!(["Shipping", "Warehouse"]));
        assert_eq!(categories_of("bin"), json!([]));

        let (_, pallet) = get_json(&app, "/api/templates/pallet").await;
        assert_eq!(pallet["categories"], json!(["Shipping", "Warehouse"]));
        let (_, bin) = get_json(&app, "/api/templates/bin").await;
        assert_eq!(bin["categories"], json!([]));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn template_detail_carries_no_layout() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("pallet.yaml"), template_yaml("pallet")).unwrap();
        let app = build_app_in(&dir);

        let (status, detail) = get_json(&app, "/api/templates/pallet").await;
        assert_eq!(status, StatusCode::OK);
        assert!(detail.get("layout").is_none(), "{detail}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn a_body_declaring_version_is_refused() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("pallet.yaml"), template_yaml("pallet")).unwrap();
        let app = build_app_in(&dir);

        let body = format!("{}version: \"1\"\n", template_yaml("pallet"));
        let resp = app
            .clone()
            .oneshot(yaml_post("/api/templates/pallet", "PUT", body))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "TemplateInvalid");
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("version"),
            "{body}"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("pallet.yaml")).unwrap(),
            template_yaml("pallet"),
            "nothing was written"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn template_post_creates_the_file() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);

        let resp = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/pallet",
                "POST",
                template_yaml("pallet"),
            ))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let detail = json_response(resp).await;
        assert_eq!(detail["id"], "pallet");
        assert_eq!(
            std::fs::read_to_string(dir.join("pallet.yaml")).unwrap(),
            template_yaml("pallet")
        );
        let (status, _) = get_json(&app, "/api/templates/pallet").await;
        assert_eq!(status, StatusCode::OK, "the new template is served");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Existence is decided from the disk, so a broken `{id}.yaml` blocks a create exactly as a served
    /// one does, and the broken file is written after startup so the registry has never seen it.
    #[tokio::test]
    async fn template_post_refuses_an_existing_file_served_or_broken() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("served.yaml"), template_yaml("served")).unwrap();
        let app = build_app_in(&dir);
        std::fs::write(dir.join("pallet.yaml"), UNPARSEABLE_TEMPLATE).unwrap();

        for (id, original) in [
            ("served", template_yaml("served")),
            ("pallet", UNPARSEABLE_TEMPLATE.to_string()),
        ] {
            let resp = app
                .clone()
                .oneshot(yaml_post(
                    &format!("/api/templates/{id}"),
                    "POST",
                    template_yaml_for(id, "overwritten"),
                ))
                .await
                .expect("request");
            assert_eq!(resp.status(), StatusCode::CONFLICT, "{id}");
            let body = json_response(resp).await;
            assert_eq!(body["error"]["code"], "Conflict", "{id}");
            assert!(body["error"].get("details").is_none(), "{id}: {body}");
            assert_eq!(
                std::fs::read_to_string(dir.join(format!("{id}.yaml"))).unwrap(),
                original,
                "{id}: the existing file is unchanged"
            );
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn template_put_refuses_an_absent_template() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);

        let resp = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/pallet",
                "PUT",
                template_yaml("pallet"),
            ))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "NotFound");
        assert!(!dir.join("pallet.yaml").exists(), "nothing was written");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Regression guard: this passes before #411 too, where the upsert's create path fell back to a
    /// replace on finding the file. It pins that the replace decision is keyed off the disk, so a
    /// broken `{id}.yaml` the registry does not hold stays replaceable.
    #[tokio::test]
    async fn template_put_replaces_a_broken_file() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("pallet.yaml"), UNPARSEABLE_TEMPLATE).unwrap();
        let app = build_app_in(&dir);

        let resp = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/pallet",
                "PUT",
                template_yaml("pallet"),
            ))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            std::fs::read_to_string(dir.join("pallet.yaml")).unwrap(),
            template_yaml("pallet")
        );
        let (status, _) = get_json(&app, "/api/templates/pallet").await;
        assert_eq!(status, StatusCode::OK, "the replaced template is served");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn template_delete_removes_a_broken_file() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("pallet.yaml"), UNPARSEABLE_TEMPLATE).unwrap();
        let app = build_app_in(&dir);

        let resp = app
            .clone()
            .oneshot(delete_req("/api/templates/pallet"))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert!(!dir.join("pallet.yaml").exists(), "the file is gone");
        let (_, list) = get_json(&app, "/api/templates").await;
        assert!(list.get("broken").is_none(), "{list}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn template_source_reads_a_broken_file() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("pallet.yaml"), UNPARSEABLE_TEMPLATE).unwrap();
        let app = build_app_in(&dir);

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/templates/pallet/source")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "text/yaml; charset=utf-8"
        );
        assert_eq!(bytes_response(resp).await, UNPARSEABLE_TEMPLATE.as_bytes());

        let (status, _) = get_json(&app, "/api/templates/pallet").await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "a broken template has no detail"
        );

        std::fs::remove_dir_all(&dir).ok();
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
                "POST",
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
        assert_eq!(body["error"]["code"], "NotFound");

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
            .oneshot(yaml_post("/api/templates/f1", "POST", template_yaml("f1")))
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

    /// The create must publish with the no-replace primitive, not a rename. Staged with the
    /// pre-publish hook: the destination appears after validation and before the publish, which is
    /// the one state a stat-then-rename create cannot catch and `rename` would silently overwrite.
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
            .oneshot(yaml_post(
                "/api/templates/racer",
                "POST",
                template_yaml("racer"),
            ))
            .await
            .expect("request");

        assert_eq!(resp.status(), StatusCode::CONFLICT);
        assert_eq!(
            std::fs::read_to_string(dir.join("racer.yaml")).unwrap(),
            "someone else's file\n",
            "the other writer's file was not overwritten"
        );
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
                "POST",
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
            .oneshot(yaml_post("/api/templates/..%2fevil", "POST", body))
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

    #[tokio::test]
    async fn template_put_params_shape_and_duplicate_refusals() {
        let dir = temp_templates_dir();
        std::fs::write(dir.join("test_tpl.yaml"), template_yaml("test_tpl")).unwrap();
        let app = build_app_in(&dir);

        // 1. params: null is rejected with 422 template_validation_failed
        let yaml_null = "name: T\nunit: mm\ndpi: 200\nformat: { type: single, width: 20, height: 10 }\nparams: null\nlayout: []\n";
        let resp = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/test_tpl",
                "PUT",
                yaml_null.to_string(),
            ))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "TemplateInvalid");
        assert_eq!(
            body["error"]["details"]["reason"],
            "template_validation_failed"
        );

        // 2. legacy mapping params: is rejected with 422 template_validation_failed
        let yaml_map = "name: T\nunit: mm\ndpi: 200\nformat: { type: single, width: 20, height: 10 }\nparams:\n  a:\n    type: string\nlayout: []\n";
        let resp = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/test_tpl",
                "PUT",
                yaml_map.to_string(),
            ))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "TemplateInvalid");
        assert_eq!(
            body["error"]["details"]["reason"],
            "template_validation_failed"
        );

        // 3. duplicate parameter name is rejected with 422 template_validation_failed naming duplicate
        let yaml_dup = "name: T\nunit: mm\ndpi: 200\nformat: { type: single, width: 20, height: 10 }\nparams:\n  - name: title\n    type: string\n  - name: title\n    type: string\nlayout: []\n";
        let resp = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/test_tpl",
                "PUT",
                yaml_dup.to_string(),
            ))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "TemplateInvalid");
        assert_eq!(
            body["error"]["details"]["reason"],
            "template_validation_failed"
        );
        assert!(body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("duplicate parameter name 'title'"));

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
                "POST",
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

    fn json_req(method: &str, uri: &str, body: String) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap()
    }

    fn printer_json(id: &str) -> String {
        json!({ "id": id, "name": id, "uri": format!("ipp://host/printers/{id}") }).to_string()
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
        assert_eq!(detail["uri"], "ipp://host/printers/office");

        let replace = json!({ "name": "Front Desk", "uri": "ipp://h/p" }).to_string();
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
        assert_eq!(json_response(resp).await["error"]["code"], "Conflict");
    }

    #[tokio::test]
    async fn printer_create_unsafe_id_returns_400() {
        let app = build_app();
        let body = json!({ "id": "../evil", "name": "P", "uri": "ipp://h/q" }).to_string();
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/printers", body))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            json_response(resp).await["error"]["details"]["reason"],
            "printer_id_invalid"
        );
    }

    #[tokio::test]
    async fn printer_get_unknown_returns_404() {
        let app = build_app();
        let (status, body) = get_json(&app, "/api/printers/nope").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "NotFound");
    }

    async fn create_fake_printer(app: &axum::Router, id: &str, fail: bool) {
        let uri = if fail {
            "ipp://fake.test/?fail=1"
        } else {
            "ipp://fake.test/"
        };
        let body = json!({ "id": id, "name": id, "uri": uri }).to_string();
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
            "uri": "ipp://fake.test/?bilevel=1&png=1&dpi=180&model=Brother%20PT-2730"
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
        let body = json!({ "uri": "ipp://fake.test/?bilevel=0" }).to_string();
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
        let body = json!({ "uri": "ipp://fake.test/?probe=unreachable" }).to_string();
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
    async fn probe_missing_uri_is_400() {
        let app = build_app();
        let body = json!({}).to_string();
        let resp = app
            .oneshot(json_req("POST", "/api/printers/probe", body))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn probe_malformed_uri_is_400() {
        let app = build_app();
        let body = json!({ "uri": "ipp://" }).to_string();
        let resp = app
            .oneshot(json_req("POST", "/api/printers/probe", body))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn printer_replace_missing_returns_404() {
        let app = build_app();
        let resp = app
            .clone()
            .oneshot(json_req(
                "PUT",
                "/api/printers/ghost",
                json!({ "name": "ghost", "uri": "ipp://h/q" }).to_string(),
            ))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(json_response(resp).await["error"]["code"], "NotFound");
    }

    async fn print_sent(app: &axum::Router, printer: &str) -> u64 {
        let payload = json!({
            "template": "brother_24mm_qr",
            "printer": printer,
            "labels": [{ "data": { "message": "x", "code": "y" } }]
        });
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/print", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::OK, "print to {printer}");
        json_response(resp).await["sent"]
            .as_u64()
            .expect("sent count")
    }

    #[tokio::test]
    async fn printer_record_is_flat_and_never_returns_password() {
        let app = build_app();
        let create = json!({
            "id": "flat", "name": "Flat", "uri": "ipps://h/q",
            "username": "u", "password": "s3cret", "insecure": true
        });
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/printers", create.to_string()))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let created = json_response(resp).await;
        let (_, detail) = get_json(&app, "/api/printers/flat").await;
        let (_, list) = get_json(&app, "/api/printers").await;
        for printer in [&created, &detail, &list[0]] {
            for gone in ["kind", "config", "is_default", "password"] {
                assert!(printer.get(gone).is_none(), "{gone} present in {printer}");
            }
            assert_eq!(printer["uri"], "ipps://h/q");
            assert_eq!(printer["username"], "u");
            assert_eq!(printer["insecure"], true);
        }
        let replace = json!({ "name": "Flat", "uri": "ipps://h/q", "username": "u" });
        let resp = app
            .clone()
            .oneshot(json_req("PUT", "/api/printers/flat", replace.to_string()))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(json_response(resp).await.get("password").is_none());
    }

    /// The response omits unset optional fields rather than writing `null`: the UI echoes a loaded
    /// printer back into `PUT`, and the bodies refuse `null`.
    #[tokio::test]
    async fn printer_record_omits_unset_optional_fields() {
        let app = build_app();
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/printers", printer_json("bare")))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let expected = json!({
            "id": "bare", "name": "bare", "uri": "ipp://host/printers/bare", "insecure": false
        });
        assert_eq!(json_response(resp).await, expected);
        let (_, detail) = get_json(&app, "/api/printers/bare").await;
        assert_eq!(detail, expected);

        let with_resolution = json!({
            "name": "bare", "uri": "ipp://host/printers/bare", "render": { "resolution": 300 }
        });
        let resp = app
            .clone()
            .oneshot(json_req(
                "PUT",
                "/api/printers/bare",
                with_resolution.to_string(),
            ))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::OK);
        let (_, detail) = get_json(&app, "/api/printers/bare").await;
        assert_eq!(detail["render"], json!({ "resolution": 300 }));
    }

    #[tokio::test]
    async fn printer_nested_body_is_refused_and_flat_body_accepted() {
        let app = build_app();
        let nested =
            json!({ "id": "n", "name": "N", "kind": "cups", "config": { "uri": "ipp://h/q" } });
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/printers", nested.to_string()))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            json_response(resp).await["error"]["details"]["reason"],
            "json_malformed"
        );
        let flat = json!({ "id": "n", "name": "N", "uri": "ipp://h/q" });
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/printers", flat.to_string()))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    /// printing spec, "Replacing a printer".
    #[tokio::test]
    async fn printer_replace_without_insecure_stores_false() {
        let app = build_app();
        let create = json!({ "id": "i", "name": "I", "uri": "ipps://h/q", "insecure": true });
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/printers", create.to_string()))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let replace = json!({ "name": "I", "uri": "ipps://h/q" });
        let resp = app
            .clone()
            .oneshot(json_req("PUT", "/api/printers/i", replace.to_string()))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::OK);
        let (_, detail) = get_json(&app, "/api/printers/i").await;
        assert_eq!(detail["insecure"], false);
    }

    #[tokio::test]
    async fn printer_replace_refuses_an_id_in_the_body() {
        let app = build_app();
        let create = json!({ "id": "r", "name": "R", "uri": "ipp://h/q" });
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/printers", create.to_string()))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let with_id = json!({ "id": "r", "name": "R2", "uri": "ipp://h/q" });
        let resp = app
            .clone()
            .oneshot(json_req("PUT", "/api/printers/r", with_id.to_string()))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            json_response(resp).await["error"]["details"]["reason"],
            "json_malformed"
        );
        let without_id = json!({ "name": "R2", "uri": "ipp://h/q" });
        let resp = app
            .clone()
            .oneshot(json_req("PUT", "/api/printers/r", without_id.to_string()))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(json_response(resp).await["name"], "R2");
    }

    /// printing spec, "Password survives an edit", plus the `""` clear and replace rules. The fake
    /// driver's `send` fails unless the stored password equals the URI's `password` knob, so each
    /// print observes the secret that reached dispatch.
    #[tokio::test]
    async fn printer_password_keep_replace_and_clear_reach_dispatch() {
        let app = build_app();
        let create = json!({ "id": "pw", "name": "pw", "uri": "ipp://fake.test/?password=s1", "password": "s1" });
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/printers", create.to_string()))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::CREATED);
        assert_eq!(print_sent(&app, "pw").await, 1, "created");

        async fn replace(app: &axum::Router, body: Value) {
            let resp = app
                .clone()
                .oneshot(json_req("PUT", "/api/printers/pw", body.to_string()))
                .await
                .expect("request");
            assert_eq!(resp.status(), StatusCode::OK, "replace with {body}");
        }

        replace(
            &app,
            json!({ "name": "pw", "uri": "ipp://fake.test/?password=other" }),
        )
        .await;
        assert_eq!(print_sent(&app, "pw").await, 0, "a wrong password fails");
        replace(
            &app,
            json!({ "name": "pw", "uri": "ipp://fake.test/?password=s1" }),
        )
        .await;
        assert_eq!(print_sent(&app, "pw").await, 1, "omitted password kept");
        replace(
            &app,
            json!({ "name": "pw", "uri": "ipp://fake.test/?password=s2", "password": "s2" }),
        )
        .await;
        assert_eq!(print_sent(&app, "pw").await, 1, "password replaced");
        replace(
            &app,
            json!({ "name": "pw", "uri": "ipp://fake.test/", "password": "" }),
        )
        .await;
        assert_eq!(print_sent(&app, "pw").await, 1, "empty string clears");
    }

    /// errors spec: a key written as `null` is `json_malformed`; an absent key is accepted.
    #[tokio::test]
    async fn printer_body_refuses_null_keys_and_accepts_absent_ones() {
        let app = build_app();
        for key in ["password", "ca_cert"] {
            let mut body = json!({ "id": "nn", "name": "NN", "uri": "ipp://h/q" });
            body[key] = Value::Null;
            let resp = app
                .clone()
                .oneshot(json_req("POST", "/api/printers", body.to_string()))
                .await
                .expect("request");
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{key}: null");
            assert_eq!(
                json_response(resp).await["error"]["details"]["reason"],
                "json_malformed",
                "{key}: null"
            );
        }
        let absent = json!({ "id": "nn", "name": "NN", "uri": "ipp://h/q" });
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/printers", absent.to_string()))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let null_on_put = json!({ "name": "NN", "uri": "ipp://h/q", "password": null });
        let resp = app
            .clone()
            .oneshot(json_req("PUT", "/api/printers/nn", null_on_put.to_string()))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            json_response(resp).await["error"]["details"]["reason"],
            "json_malformed"
        );
    }

    async fn put_default_printer(app: &axum::Router, value: Value) -> axum::response::Response {
        app.clone()
            .oneshot(json_req(
                "PUT",
                "/api/settings/default_printer_id",
                json!({ "value": value }).to_string(),
            ))
            .await
            .expect("request")
    }

    /// printing spec, "Deleting the default printer".
    #[tokio::test]
    async fn deleting_the_default_printer_clears_the_setting() {
        let app = build_app();
        for id in ["p1", "p2"] {
            let resp = app
                .clone()
                .oneshot(json_req("POST", "/api/printers", printer_json(id)))
                .await
                .expect("request");
            assert_eq!(resp.status(), StatusCode::CREATED);
        }
        assert_eq!(
            put_default_printer(&app, json!("p1")).await.status(),
            StatusCode::OK
        );
        for (id, expected) in [("p2", json!("p1")), ("p1", Value::Null)] {
            let resp = app
                .clone()
                .oneshot(json_req(
                    "DELETE",
                    &format!("/api/printers/{id}"),
                    String::new(),
                ))
                .await
                .expect("request");
            assert_eq!(resp.status(), StatusCode::NO_CONTENT);
            let (_, settings) = get_json(&app, "/api/settings").await;
            assert_eq!(
                settings["default_printer_id"]["value"], expected,
                "after deleting {id}"
            );
            assert_eq!(
                settings["default_printer_id"]["is_default"],
                expected.is_null(),
                "after deleting {id}"
            );
        }
    }

    #[tokio::test]
    async fn default_printer_id_setting_trims_validates_and_resets() {
        let app = build_app();
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/printers", printer_json("p1")))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::CREATED);

        let resp = put_default_printer(&app, json!("  p1 ")).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            json_response(resp).await,
            json!({ "value": "p1", "is_default": false })
        );
        let (_, settings) = get_json(&app, "/api/settings").await;
        assert_eq!(
            settings["default_printer_id"],
            json!({ "value": "p1", "is_default": false })
        );

        for bad in [
            json!(""),
            json!("   "),
            json!(123),
            json!(null),
            json!({}),
            json!("ghost"),
        ] {
            let resp = put_default_printer(&app, bad.clone()).await;
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "value {bad}");
            assert_eq!(
                json_response(resp).await["error"]["details"]["reason"],
                "setting_value_invalid",
                "value {bad}"
            );
        }

        let resp = app
            .clone()
            .oneshot(json_req(
                "DELETE",
                "/api/settings/default_printer_id",
                String::new(),
            ))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        let (_, settings) = get_json(&app, "/api/settings").await;
        assert_eq!(
            settings["default_printer_id"],
            json!({ "value": null, "is_default": true })
        );
    }

    #[tokio::test]
    async fn printer_default_endpoints_are_gone() {
        let app = build_app();
        for method in ["POST", "DELETE"] {
            let resp = app
                .clone()
                .oneshot(json_req(method, "/api/printers/p1/default", String::new()))
                .await
                .expect("request");
            assert_eq!(resp.status(), StatusCode::NOT_FOUND, "{method}");
            assert_eq!(
                json_response(resp).await["error"]["details"]["kind"],
                "route",
                "{method}"
            );
        }
    }

    #[tokio::test]
    async fn probe_takes_flat_connection_fields_only() {
        let app = build_app();
        let resp = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/printers/probe",
                json!({ "uri": "ipp://fake.test/?probe=unreachable" }).to_string(),
            ))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(json_response(resp).await["status"], "unreachable");
        let resp = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/printers/probe",
                json!({ "name": "x", "uri": "ipp://fake.test/?probe=unreachable" }).to_string(),
            ))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            json_response(resp).await["error"]["details"]["reason"],
            "json_malformed"
        );
    }

    /// settings spec, "Override and reset". Regression guard: passes before #417 too.
    #[tokio::test]
    async fn datetime_formats_override_and_reset_scenario() {
        let app = build_app();
        let resp = app
            .clone()
            .oneshot(json_req(
                "PUT",
                "/api/settings/datetime_formats",
                json!({ "value": { "day": "%d" } }).to_string(),
            ))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            json_response(resp).await,
            json!({ "value": { "day": "%d" }, "is_default": false })
        );
        let resp = app
            .clone()
            .oneshot(json_req(
                "DELETE",
                "/api/settings/datetime_formats",
                String::new(),
            ))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        let (_, settings) = get_json(&app, "/api/settings").await;
        // A second DELETE of a setting already at its default is still 204.
        let resp = app
            .clone()
            .oneshot(json_req(
                "DELETE",
                "/api/settings/datetime_formats",
                String::new(),
            ))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            settings["datetime_formats"],
            json!({
                "value": {
                    "iso_date": "%Y-%m-%d",
                    "iso_date_time": "%Y-%m-%d %H:%M",
                    "short_date": "%m/%d/%Y",
                    "long_date": "%B %-d, %Y",
                    "time": "%H:%M"
                },
                "is_default": true
            })
        );
    }

    /// settings spec, "Wrong type". Regression guard: passes before #417 too.
    #[tokio::test]
    async fn datetime_formats_wrong_type_scenario() {
        let app = build_app();
        let resp = app
            .clone()
            .oneshot(json_req(
                "PUT",
                "/api/settings/datetime_formats",
                json!({ "value": "%d" }).to_string(),
            ))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            json_response(resp).await["error"]["details"]["reason"],
            "setting_value_invalid"
        );
    }

    #[tokio::test]
    async fn settings_lists_exactly_the_known_keys() {
        let app = build_app();
        let (status, settings) = get_json(&app, "/api/settings").await;
        assert_eq!(status, StatusCode::OK);
        let keys: Vec<&str> = settings
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "datetime_formats",
                "default_connection_id",
                "default_printer_id"
            ]
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
        let state = build_app_with_state().1;
        let c = state
            .store()
            .create_connection(crate::store::NewConnection {
                connector: "homebox",
                name: "h",
                base_url: &hb.uri(),
                public_url: None,
                credential: "hb_key",
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
        let state = build_app_with_state().1;
        let c = state
            .store()
            .create_connection(crate::store::NewConnection {
                connector: "homebox",
                name: "h",
                base_url: &hb.uri(),
                public_url: Some("https://public.homebox.domain"),
                credential: "hb_key",
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

        // Clear public_url by omitting it from the PUT
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/connections/{}", c.id))
                    .header("content-type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"name":"h","base_url":"{}"}}"#,
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
        let state = build_app_with_state().1;
        let c = state
            .store()
            .create_connection(crate::store::NewConnection {
                connector: "homebox",
                name: "h",
                base_url: &hb.uri(),
                public_url: None,
                credential: "hb_key",
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
        let state = build_app_with_state().1;
        let c = state
            .store()
            .create_connection(crate::store::NewConnection {
                connector: "homebox",
                name: "h",
                base_url: &hb.uri(),
                public_url: None,
                credential: "hb_key",
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
        let state = build_app_with_state().1;
        let c = state
            .store()
            .create_connection(crate::store::NewConnection {
                connector: "homebox",
                name: "h",
                base_url: "http://hb.lan:7745",
                public_url: None,
                credential: "hb_key",
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
        let state = build_app_with_state().1;
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
    async fn connection_schema_reports_tags_multi_valued() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let hb = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities/fields"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!(["sku"])))
            .mount(&hb)
            .await;
        let state = build_app_with_state().1;
        let c = state
            .store()
            .create_connection(crate::store::NewConnection {
                connector: "homebox",
                name: "h",
                base_url: &hb.uri(),
                public_url: None,
                credential: "hb_key",
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
        let ent_cols = entities["columns"].as_array().unwrap();

        // 4.1: tags column on entities with ty: text, tier: cheap, multi_valued: true
        let tags_col = ent_cols
            .iter()
            .find(|c| c["key"] == "tags")
            .expect("tags column present");
        assert_eq!(tags_col["ty"], "text");
        assert_eq!(tags_col["tier"], "cheap");
        assert_eq!(tags_col["multi_valued"], true);

        // 4.2: every other FieldSpec in entities has multi_valued: false as a present key
        for col in ent_cols {
            if col["key"] != "tags" {
                assert_eq!(
                    col.get("multi_valued"),
                    Some(&serde_json::json!(false)),
                    "column {} must carry multi_valued: false",
                    col["key"]
                );
            }
        }
        // 4.1: no tags column on locations
        let locations = v["resources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == "locations")
            .unwrap();
        let loc_cols = locations["columns"].as_array().unwrap();
        assert!(!loc_cols.iter().any(|c| c["key"] == "tags"));
        for col in loc_cols {
            assert_eq!(
                col.get("multi_valued"),
                Some(&serde_json::json!(false)),
                "location column {} must carry multi_valued: false",
                col["key"]
            );
        }
    }

    #[tokio::test]
    async fn browse_with_tags_multi_valued_cell_and_no_extra_requests() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let hb = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {
                        "id": "e1",
                        "name": "LEGO Set",
                        "quantity": 2,
                        "purchasePrice": 49.99,
                        "tags": [
                            {"name": "KIDS", "id": "t1", "color": "red"},
                            {"name": "CONSUMABLE", "id": "t2"}
                        ]
                    }
                ],
                "total": 1
            })))
            .expect(1)
            .mount(&hb)
            .await;
        let state = build_app_with_state().1;
        let c = state
            .store()
            .create_connection(crate::store::NewConnection {
                connector: "homebox",
                name: "h",
                base_url: &hb.uri(),
                public_url: None,
                credential: "hb_key",
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

        // 4.3: tags cell as ["KIDS","CONSUMABLE"] in order, others as string / number
        assert_eq!(
            row["cells"]["tags"],
            serde_json::json!(["KIDS", "CONSUMABLE"])
        );
        assert_eq!(row["cells"]["name"], "LEGO Set");
        assert_eq!(row["cells"]["quantity"], 2.0);
        assert_eq!(row["cells"]["purchasePrice"], 49.99);

        // 4.4: exactly 1 request made to upstream mock server
        let received = hb.received_requests().await.unwrap();
        assert_eq!(received.len(), 1);
    }

    #[tokio::test]
    async fn materialize_and_browse_tags_and_undeclared_arrays() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let hb = MockServer::start().await;

        // Browse mock
        Mock::given(method("GET"))
            .and(path("/api/v1/entities"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {
                        "id": "e1",
                        "name": "Tagged Item",
                        "quantity": 5,
                        "tags": [
                            {"name": "KIDS"},
                            {"name": "CONSUMABLE"}
                        ],
                        "attachments": [{"id": "a1"}]
                    },
                    {
                        "id": "e2",
                        "name": "Untagged Item",
                        "quantity": 1,
                        "tags": [],
                        "children": [{"id": "c1"}]
                    }
                ],
                "total": 2
            })))
            .mount(&hb)
            .await;

        // Materialize details mocks
        Mock::given(method("GET"))
            .and(path("/api/v1/entities/e1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "e1",
                "name": "Tagged Item",
                "quantity": 5,
                "tags": [
                    {"name": "KIDS"},
                    {"name": "CONSUMABLE"}
                ],
                "attachments": [{"id": "a1"}]
            })))
            .mount(&hb)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/v1/entities/e2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "e2",
                "name": "Untagged Item",
                "quantity": 1,
                "tags": null,
                "children": [{"id": "c1"}]
            })))
            .mount(&hb)
            .await;

        let state = build_app_with_state().1;
        let c = state
            .store()
            .create_connection(crate::store::NewConnection {
                connector: "homebox",
                name: "h",
                base_url: &hb.uri(),
                public_url: None,
                credential: "hb_key",
            })
            .await
            .unwrap();
        let router = with_auth(app(state.clone()));

        // Browse verification:
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
        let browse_v: Value = serde_json::from_slice(&body).unwrap();
        let rows = browse_v["rows"].as_array().unwrap();

        // e1: tags cell is ["KIDS", "CONSUMABLE"], undeclared attachments has no cell
        assert_eq!(
            rows[0]["cells"]["tags"],
            serde_json::json!(["KIDS", "CONSUMABLE"])
        );
        assert!(rows[0]["cells"].get("attachments").is_none());

        // e2 (untagged): 4.6 tags cell is [] (array, not "", not null, not absent), undeclared children has no cell
        assert_eq!(rows[1]["cells"]["tags"], serde_json::json!([]));
        assert!(rows[1]["cells"].get("children").is_none());

        // Materialize e1: 4.5
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/connections/{}/materialize", c.id))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"rows":[{"resource":"entities","key":"e1"}],"fields":["name","quantity","tags","attachments"],"expansion":"as_listed"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let mat_v: Value = serde_json::from_slice(&body).unwrap();
        let data1 = &mat_v[0]["data"];

        assert_eq!(data1["tags"], serde_json::json!(["KIDS", "CONSUMABLE"]));
        assert_eq!(data1["name"], serde_json::json!("Tagged Item"));
        // Assert quantity is JSON string and not JSON number
        assert!(
            data1["quantity"].is_string(),
            "quantity must be string on materialize"
        );
        assert_eq!(data1["quantity"], serde_json::json!("5"));
        // 4.7: Undeclared array key attachments yields empty string on materialize
        assert_eq!(data1["attachments"], serde_json::json!(""));

        // Materialize e2 (untagged): 4.6 & 4.7
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/connections/{}/materialize", c.id))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"rows":[{"resource":"entities","key":"e2"}],"fields":["name","tags","children"],"expansion":"as_listed"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let mat_v2: Value = serde_json::from_slice(&body).unwrap();
        let data2 = &mat_v2[0]["data"];

        // tags is [] (not "", not null, not absent)
        assert!(data2.get("tags").is_some(), "tags must be present");
        assert!(data2["tags"].is_array(), "tags must be array");
        assert_eq!(data2["tags"], serde_json::json!([]));
        // children undeclared array yields empty string
        assert_eq!(data2["children"], serde_json::json!(""));
    }

    /// A Homebox stand-in answering `GET /api/v1/entities` with `list`.
    async fn homebox_listing(list: Value) -> wiremock::MockServer {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let hb = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities"))
            .respond_with(ResponseTemplate::new(200).set_body_json(list))
            .mount(&hb)
            .await;
        hb
    }

    async fn browse(app: &axum::Router, id: &str, body: &str) -> axum::response::Response {
        app.clone()
            .oneshot(json_req(
                "POST",
                &format!("/api/connections/{id}/browse"),
                body.into(),
            ))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn transform_preview_route_is_gone() {
        let hb =
            homebox_listing(json!({"items": [{"id": "e1", "name": "Drill"}], "total": 1})).await;
        let app = build_app();
        let id = create_homebox_connection(&app, &hb.uri()).await;
        let res = app
            .clone()
            .oneshot(json_req(
                "POST",
                &format!("/api/connections/{id}/transforms/preview"),
                r#"{"transforms":[{"resource":"entities","source":"name","pattern":"^(?<n>.*)$"}],"rule":0}"#.into(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert_eq!(json_response(res).await["error"]["code"], "NotFound");
    }

    /// Browse sends `page` and `page_size` to Homebox as given, with no upper bound, and answers with
    /// exactly `rows`, `has_more` and `count`.
    #[tokio::test]
    async fn browse_passes_page_and_page_size_upstream_unchanged() {
        use wiremock::matchers::{method, path, query_param};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let hb = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities"))
            .and(query_param("page", "3"))
            .and(query_param("pageSize", "500"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [{"id": "e1", "name": "Drill"}], "total": 2000
            })))
            .mount(&hb)
            .await;
        let app = build_app();
        let id = create_homebox_connection(&app, &hb.uri()).await;
        let res = browse(
            &app,
            &id,
            r#"{"resource":"entities","page":3,"page_size":500}"#,
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let v = json_response(res).await;
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["count", "has_more", "rows"]);
        assert_eq!(v["has_more"], true);
        assert_eq!(v["count"], 2000);
        assert_eq!(v["rows"][0]["id"]["key"], "e1");
    }

    #[tokio::test]
    async fn browse_refuses_malformed_paging_keys() {
        let hb =
            homebox_listing(json!({"items": [{"id": "e1", "name": "Drill"}], "total": 1})).await;
        let app = build_app();
        let id = create_homebox_connection(&app, &hb.uri()).await;
        for (body, cause) in [
            (r#"{"resource":"entities","page":0}"#, "nonzero"),
            (r#"{"resource":"entities","page_size":0}"#, "nonzero"),
            (r#"{"resource":"entities","page":null}"#, "null"),
            (r#"{"resource":"entities","parent":null}"#, "null"),
            (r#"{"resource":"entities","cursor":"x"}"#, "unknown field"),
        ] {
            assert_json_malformed(browse(&app, &id, body).await, cause).await;
        }
    }

    /// `count` is the upstream total: a list without one is not the expected JSON, not an empty
    /// resource.
    #[tokio::test]
    async fn browse_without_an_upstream_total_is_a_bad_response() {
        let hb = homebox_listing(json!({"items": [{"id": "e1", "name": "Drill"}]})).await;
        let app = build_app();
        let id = create_homebox_connection(&app, &hb.uri()).await;
        let res = browse(&app, &id, r#"{"resource":"entities"}"#).await;
        assert_eq!(res.status(), StatusCode::BAD_GATEWAY);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "Upstream");
        assert_eq!(body["error"]["details"]["reason"], "bad_response");
    }

    /// No address screening: the state the service runs with reaches an upstream on 127.0.0.1.
    #[tokio::test]
    async fn browse_reaches_a_loopback_upstream_through_the_production_state() {
        let hb =
            homebox_listing(json!({"items": [{"id": "e1", "name": "Drill"}], "total": 1})).await;
        let app = build_app();
        let id = create_homebox_connection(&app, &hb.uri()).await;
        let res = browse(&app, &id, r#"{"resource":"entities"}"#).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(json_response(res).await["rows"][0]["id"]["key"], "e1");
    }

    /// A parse error quotes the offending upstream value, which can be anything the upstream holds,
    /// the credential included; the reported message must not carry it.
    #[tokio::test]
    async fn an_unparseable_upstream_body_does_not_echo_the_credential() {
        let hb = homebox_listing(json!({
            "items": [{"id": "e1", "name": "Drill", "quantity": "hb_key"}], "total": 1
        }))
        .await;
        let app = build_app();
        let id = create_homebox_connection(&app, &hb.uri()).await;
        let res = browse(&app, &id, r#"{"resource":"entities"}"#).await;
        assert_eq!(res.status(), StatusCode::BAD_GATEWAY);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "Upstream");
        assert_eq!(body["error"]["details"]["reason"], "bad_response");
        assert!(!body.to_string().contains("hb_key"), "{body}");
    }

    #[tokio::test]
    async fn materialize_stringifies_a_numeric_custom_value_and_blanks_an_object() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let hb = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities/e1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "e1",
                "name": "Drill",
                "entityType": {"name": "item"},
                "fields": [{"name": "N", "value": 7}]
            })))
            .mount(&hb)
            .await;
        let app = build_app();
        let id = create_homebox_connection(&app, &hb.uri()).await;
        let res = app
            .clone()
            .oneshot(json_req(
                "POST",
                &format!("/api/connections/{id}/materialize"),
                r#"{"rows":[{"resource":"entities","key":"e1"}],"fields":["entityType","custom:N"],"expansion":"as_listed"}"#.into(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let v = json_response(res).await;
        assert_eq!(v[0]["data"], json!({"entityType": "", "custom:N": "7"}));
    }

    /// Pin (passes before #416): a location's custom field materializes like an item's, per the
    /// spec scenario "A location custom field".
    #[tokio::test]
    async fn materialize_reads_a_location_custom_field() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let hb = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/entities/l1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "l1",
                "name": "Shelf",
                "fields": [{"name": "Code", "textValue": "BOX.123"}]
            })))
            .mount(&hb)
            .await;
        let app = build_app();
        let id = create_homebox_connection(&app, &hb.uri()).await;
        let res = app
            .clone()
            .oneshot(json_req(
                "POST",
                &format!("/api/connections/{id}/materialize"),
                r#"{"rows":[{"resource":"locations","key":"l1"}],"fields":["custom:Code"],"expansion":"as_listed"}"#.into(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let v = json_response(res).await;
        assert_eq!(v[0]["data"], json!({"custom:Code": "BOX.123"}));
    }

    /// No address screening on printers either: a probe of a closed loopback port fails in the IPP
    /// transport, and that failure is what the detail reports.
    #[tokio::test]
    async fn probe_of_a_closed_loopback_port_reports_the_transport_error() {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let body = json!({ "uri": format!("ipp://127.0.0.1:{port}/ipp/print") });
        let res = build_app()
            .oneshot(json_req("POST", "/api/printers/probe", body.to_string()))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let v = json_response(res).await;
        assert_eq!(v["status"], "unreachable");
        let detail = v["detail"].as_str().unwrap();
        assert!(!detail.contains("blocked"), "{detail}");
        assert!(detail.contains("error sending request"), "{detail}");
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
    async fn api_print_empty_data_is_passed_to_template() {
        let app = build_app();
        create_fake_printer(&app, "ok-printer", false).await;
        let payload =
            json!({"template":"brother_24mm_qr","printer":"ok-printer","labels":[{"data":{}}]});
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
    }

    #[test]
    fn openapi_connection_credential_required_on_create_only() {
        use utoipa::OpenApi;
        let doc = crate::openapi::ApiDoc::openapi();
        let schemas = doc.components.as_ref().unwrap().schemas.clone();
        let required = |name: &str| -> Vec<String> {
            let schema = serde_json::to_value(&schemas[name]).unwrap();
            schema["required"]
                .as_array()
                .expect("required array")
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        };
        assert!(required("ConnectionCreate").contains(&"credential".to_string()));
        assert!(!required("ConnectionUpdate").contains(&"credential".to_string()));
    }

    #[test]
    fn openapi_batch_requests_are_strict() {
        use utoipa::OpenApi;
        let doc = crate::openapi::ApiDoc::openapi();
        let schemas = doc.components.as_ref().unwrap().schemas.clone();
        let names = |v: &Value| -> std::collections::BTreeSet<String> {
            match v {
                Value::Array(a) => a.iter().map(|x| x.as_str().unwrap().to_string()).collect(),
                Value::Object(o) => o.keys().cloned().collect(),
                _ => panic!("expected array or object, got {v}"),
            }
        };
        let set = |xs: &[&str]| {
            xs.iter()
                .map(|x| x.to_string())
                .collect::<std::collections::BTreeSet<_>>()
        };
        for (name, properties, required) in [
            (
                "RenderRequest",
                &["template", "labels", "start_slot", "format"][..],
                &["template", "labels"][..],
            ),
            (
                "PrintRequest",
                &["template", "labels", "start_slot", "printer"][..],
                &["template", "labels", "printer"][..],
            ),
        ] {
            let schema = serde_json::to_value(&schemas[name]).unwrap();
            assert_eq!(
                names(&schema["properties"]),
                set(properties),
                "{name}: {schema}"
            );
            assert_eq!(
                names(&schema["required"]),
                set(required),
                "{name}: {schema}"
            );
            assert_eq!(schema["additionalProperties"], false, "{name}: {schema}");
            assert!(
                !schema.to_string().contains("\"null\""),
                "{name} offers null: {schema}"
            );
        }
    }

    /// The printer bodies refuse `null` (errors spec, request body rejection), so the published
    /// schemas must not offer it, and the response omits unset fields rather than writing `null`.
    #[test]
    fn openapi_printer_schemas_are_not_nullable() {
        use utoipa::OpenApi;
        let doc = crate::openapi::ApiDoc::openapi();
        let schemas = doc.components.as_ref().unwrap().schemas.clone();
        for name in [
            "Printer",
            "NewPrinter",
            "PrinterUpdate",
            "PrinterConnection",
            "RenderProfile",
        ] {
            let schema = serde_json::to_string(&schemas[name]).unwrap();
            assert!(!schema.contains("\"null\""), "{name} offers null: {schema}");
        }
    }

    #[test]
    fn openapi_render_label_request_is_strict() {
        use utoipa::OpenApi;
        let doc = crate::openapi::ApiDoc::openapi();
        let schemas = doc.components.as_ref().unwrap().schemas.clone();
        assert!(
            schemas.contains_key("RenderLabelRequest"),
            "RenderLabelRequest missing in openapi schemas"
        );
        let schema = serde_json::to_value(&schemas["RenderLabelRequest"]).unwrap();
        assert!(
            !schema.as_object().unwrap().contains_key("allOf"),
            "RenderLabelRequest must not use allOf, got {schema}"
        );
        let required = schema["required"].as_array().expect("required array");
        let req_strs: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
        assert!(
            req_strs.contains(&"template"),
            "template must be required, got {schema}"
        );
        assert!(
            req_strs.contains(&"data"),
            "data must be required, got {schema}"
        );
        let props = schema["properties"].as_object().expect("properties object");
        assert!(
            props.contains_key("template"),
            "template must be a property, got {schema}"
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

    // axum's global DefaultBodyLimit (~2 MiB) triggers the JsonRejection->PayloadTooLarge path.
    #[tokio::test]
    async fn batch_oversized_body_is_413() {
        let app = build_app();
        // ~2.1 MiB body; exceeds the global ~2 MiB DefaultBodyLimit.
        let big = "x".repeat(2 * 1024 * 1024 + 100 * 1024);
        let payload = json!({"template":"brother_24mm_qr","labels":[{"data":{"message":big}}]});
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/render", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(
            json_response(resp).await["error"]["code"],
            "PayloadTooLarge"
        );
    }

    // End-to-end guard on the security-critical merge->upsert wiring: the API never echoes the
    // password, so a regression (redact-before-upsert, or merging the wrong object) would silently
    // wipe the STORED secret without any response-body test noticing. Read the store directly.
    #[tokio::test]
    async fn printer_password_persists_across_update_and_clears_on_empty() {
        let (app, state) = build_app_with_state();

        let create = json!({
            "id": "persist", "name": "Persist", "uri": "ipps://h/q", "username": "u", "password": "s3cret"
        });
        let resp = app
            .clone()
            .oneshot(json_req("POST", "/api/printers", create.to_string()))
            .await
            .expect("req");
        assert_eq!(resp.status(), StatusCode::CREATED);

        // PUT omitting password (change only the name): the stored secret must be KEPT.
        let upd = json!({ "name": "Renamed", "uri": "ipps://h/q", "username": "u" });
        let p = app
            .clone()
            .oneshot(json_req("PUT", "/api/printers/persist", upd.to_string()))
            .await
            .expect("req");
        assert_eq!(p.status(), StatusCode::OK);

        let stored = state
            .store()
            .get_printer_connection("persist")
            .await
            .expect("store read")
            .expect("printer exists");
        assert_eq!(
            stored.password.as_deref(),
            Some("s3cret"),
            "password must persist across a password-omitting update"
        );

        // PUT with password "": the stored secret must be CLEARED.
        let clr =
            json!({ "name": "Renamed", "uri": "ipps://h/q", "username": "u", "password": "" });
        let c = app
            .clone()
            .oneshot(json_req("PUT", "/api/printers/persist", clr.to_string()))
            .await
            .expect("req");
        assert_eq!(c.status(), StatusCode::OK);

        let stored = state
            .store()
            .get_printer_connection("persist")
            .await
            .expect("store read")
            .expect("printer exists");
        assert_eq!(
            stored.password, None,
            "an empty password must clear the stored one"
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
        async fn print_ok(app: &axum::Router, id: &str, fields: serde_json::Value) {
            let mut body = fields;
            body["id"] = json!(id);
            body["name"] = json!(id);
            let body = body.to_string();
            let c = app
                .clone()
                .oneshot(json_req("POST", "/api/printers", body))
                .await
                .expect("req");
            assert_eq!(c.status(), StatusCode::CREATED, "create {id}");
            let payload = json!({
                "template": "brother_24mm_qr",
                "printer": id,
                "labels": [ { "data": { "message": "Hi", "code": "Q" } } ]
            });
            let resp = app
                .clone()
                .oneshot(json_req("POST", "/api/print", payload.to_string()))
                .await
                .expect("req");
            assert_eq!(resp.status(), StatusCode::OK, "print {id}");
            assert_eq!(json_response(resp).await["sent"], 1, "sent {id}");
        }
        // 1. no render + caps bilevel+png -> negotiated bilevel -> PNG
        print_ok(
            &app,
            "neg",
            json!({ "uri": "ipp://fake.test/?bilevel=1&png=1&dpi=203" }),
        )
        .await;
        // 2. render color + caps bilevel -> configured wins -> PDF (color/pdf)
        print_ok(
            &app,
            "sup",
            json!({ "uri": "ipp://fake.test/?bilevel=1&png=1", "render": { "color_mode": "color" } }),
        )
        .await;
        // 3. no render + no caps -> default Color -> PDF
        print_ok(&app, "def", json!({ "uri": "ipp://fake.test/" })).await;
        // 4. render bilevel + no caps -> configured bilevel -> PNG
        print_ok(
            &app,
            "cfg",
            json!({ "uri": "ipp://fake.test/", "render": { "color_mode": "bilevel" } }),
        )
        .await;
    }

    #[tokio::test]
    async fn every_job_carries_the_template_media_size() {
        let app = build_app();
        let short = json!({ "message": "a", "code": "Q" });
        let long = json!({ "message": "a much longer message on the tape", "code": "Q" });
        // The same fixtures `build_app` serves.
        let registry = crate::templates::load_all_for_tests().0;
        let template = registry.get("brother_24mm_qr").expect("fixture");
        let (settings, formats) = (
            std::collections::BTreeMap::new(),
            std::collections::BTreeMap::new(),
        );
        let datetime = crate::datetime_fmt::DateTimeResolver {
            formats: &formats,
            now: chrono::Local::now(),
        };
        let env = crate::render::resolve_environment(template, &settings, &datetime).unwrap();
        let length = |data: &Value| {
            let data: std::collections::HashMap<String, Value> =
                serde_json::from_value(data.clone()).unwrap();
            let label = crate::render::render_single_label_as(
                template,
                &data,
                &env,
                crate::render::SingleKind::Png,
                Default::default(),
            )
            .unwrap();
            crate::driver::hundredths_mm(label.width_mm)
        };
        let short_y = length(&short);
        assert_ne!(
            short_y,
            length(&long),
            "the two labels must differ in length"
        );

        // tape24 reports 12 mm loaded (`media=12`) but accepts x=2400 with the short label's length:
        // a surviving 409 pre-check (it reads the advertised width) fails this test.
        // tape12 refuses x=2400 at send time.
        for (id, knobs) in [
            ("tape24", format!("media=12&media_x=2400&media_y={short_y}")),
            ("tape12", "media_x=1200".to_string()),
        ] {
            let body = json!({ "id": id, "name": id, "uri": format!("ipp://fake.test/?{knobs}") });
            let resp = app
                .clone()
                .oneshot(json_req("POST", "/api/printers", body.to_string()))
                .await
                .expect("request");
            assert_eq!(resp.status(), StatusCode::CREATED);
        }
        let labels = json!([{ "data": short }, { "data": long }]);
        let print = |printer: &'static str| {
            let app = app.clone();
            let body =
                json!({ "template": "brother_24mm_qr", "printer": printer, "labels": labels });
            async move {
                let resp = app
                    .oneshot(json_req("POST", "/api/print", body.to_string()))
                    .await
                    .expect("request");
                assert_eq!(resp.status(), StatusCode::OK, "print to {printer}");
                json_response(resp).await
            }
        };
        let indices = |summary: &Value| -> Vec<Value> {
            summary["failed"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| f["index"].clone())
                .collect()
        };

        let tape24 = print("tape24").await;
        assert_eq!(
            (
                tape24["total"].clone(),
                tape24["sent"].clone(),
                tape24["jobs"].clone()
            ),
            (json!(2), json!(1), json!(2))
        );
        assert_eq!(
            indices(&tape24),
            vec![json!(1)],
            "only the long label's length differs"
        );

        let tape12 = print("tape12").await;
        assert_eq!(
            tape12["sent"], 0,
            "the printer refuses both jobs; no 409 before sending"
        );
        assert_eq!(indices(&tape12), vec![json!(0), json!(1)]);
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

    #[tokio::test]
    async fn panic_envelope_and_logging_for_payload_shapes() {
        init_test_tracing();
        let buf = Arc::new(std::sync::Mutex::new(Vec::new()));
        TEST_LOG_BUFFER.with(|cell| {
            *cell.borrow_mut() = Some(buf.clone());
        });

        let app = build_app();

        // 1. Formatted panic: payload is String
        let req1 = Request::builder()
            .method("GET")
            .uri("/api/test/panic/formatted")
            .body(Body::empty())
            .unwrap();
        let resp1 = app.clone().oneshot(req1).await.unwrap();
        assert_eq!(resp1.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            resp1
                .headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap(),
            "application/json"
        );
        let body1 = json_response(resp1).await;
        assert_eq!(body1["error"]["code"], "Internal");
        assert!(
            body1["error"].get("details").is_none()
                || body1["error"]["details"].get("reason").is_none(),
            "error.details must be absent or contain no reason key, got {body1}"
        );
        let msg1 = body1["error"]["message"].as_str().expect("message string");
        assert!(
            !serde_json::to_string(&body1)
                .unwrap()
                .contains("distinctive interpolated panic"),
            "response body must not contain panic payload"
        );

        // 2. Literal panic: payload is &'static str
        let req2 = Request::builder()
            .method("GET")
            .uri("/api/test/panic/literal")
            .body(Body::empty())
            .unwrap();
        let resp2 = app.clone().oneshot(req2).await.unwrap();
        assert_eq!(resp2.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            resp2
                .headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap(),
            "application/json"
        );
        let body2 = json_response(resp2).await;
        assert_eq!(body2["error"]["code"], "Internal");
        assert!(
            body2["error"].get("details").is_none()
                || body2["error"]["details"].get("reason").is_none(),
            "error.details must be absent or contain no reason key, got {body2}"
        );
        let msg2 = body2["error"]["message"].as_str().expect("message string");
        assert!(
            !serde_json::to_string(&body2)
                .unwrap()
                .contains("distinctive literal panic"),
            "response body must not contain panic payload"
        );

        // 3. Any panic: payload is neither String nor &'static str (123_u32)
        let req3 = Request::builder()
            .method("GET")
            .uri("/api/test/panic/any")
            .body(Body::empty())
            .unwrap();
        let resp3 = app.clone().oneshot(req3).await.unwrap();
        assert_eq!(resp3.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            resp3
                .headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap(),
            "application/json"
        );
        let body3 = json_response(resp3).await;
        assert_eq!(body3["error"]["code"], "Internal");
        assert!(
            body3["error"].get("details").is_none()
                || body3["error"]["details"].get("reason").is_none(),
            "error.details must be absent or contain no reason key, got {body3}"
        );
        let msg3 = body3["error"]["message"].as_str().expect("message string");

        // 2.4: Assert error.message is byte-equal across all three shapes, in one place
        assert_eq!(
            msg1, msg2,
            "error.message must be identical between formatted and literal"
        );
        assert_eq!(
            msg2, msg3,
            "error.message must be identical between literal and any"
        );

        TEST_LOG_BUFFER.with(|cell| {
            *cell.borrow_mut() = None;
        });

        let log_str = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        // 2.2: Assert logs for all three shapes
        assert!(
            log_str.contains("distinctive interpolated panic: payload-value-42"),
            "log must contain full formatted panic payload, got: {log_str}"
        );
        assert!(
            log_str.contains("distinctive literal panic payload"),
            "log must contain full literal panic payload, got: {log_str}"
        );
        assert!(
            log_str.contains(crate::api::UNREADABLE_PANIC_MARKER),
            "log must contain unreadable payload marker, got: {log_str}"
        );
    }

    #[tokio::test]
    async fn panic_outside_api_returns_internal_envelope() {
        let app = build_app();
        let req = Request::builder()
            .method("GET")
            .uri("/test/panic")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            resp.headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap(),
            "application/json"
        );
        let body = json_response(resp).await;
        assert_eq!(body["error"]["code"], "Internal");
    }

    #[tokio::test]
    async fn service_survives_covered_panic() {
        let app = build_app();

        // Trigger a covered panic
        let req = Request::builder()
            .method("GET")
            .uri("/api/test/panic/literal")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

        // Subsequent unrelated request is served normally on the same app
        let req2 = Request::builder()
            .method("GET")
            .uri("/api/health")
            .body(Body::empty())
            .unwrap();
        let resp2 = app.oneshot(req2).await.unwrap();
        assert_eq!(resp2.status(), StatusCode::OK);
        let body2 = json_response(resp2).await;
        assert_eq!(body2["status"], "ok");
    }

    /// Asserts one endpoint answers a malformed JSON body with the documented envelope (#225).
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
    async fn all_eighteen_json_endpoints_reject_malformed_body_identically() {
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
            ("POST", "/api/render"),
            ("POST", "/api/print"),
            ("POST", "/api/render/label"),
        ];

        assert_eq!(endpoints.len(), 18);

        for (method, uri) in endpoints {
            assert_malformed_body_returns_envelope(method, uri).await;
        }
    }

    /// Every JSON-body operation in the published OpenAPI document rejects a malformed body with the
    /// documented envelope.
    ///
    /// `all_eighteen_json_endpoints_reject_malformed_body_identically` enumerates today's endpoints,
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

        // Today's true count. A floor set below it would let an endpoint drop out of discovery with
        // every test still green, which is the coverage hole this test exists to close. The floor
        // moves up when an endpoint is added and only ever moves down deliberately.
        assert!(
            endpoints.len() >= 18,
            "expected at least the 18 documented JSON-body operations, found {}: {endpoints:?}",
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
            .body(Body::from(r#"{"template":"brother_12mm","labels":[]}"#))
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
            .body(Body::from(r#"{"template":"brother_12mm","labels":[]}"#))
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
                r#"{"template":"non_existent_template","printer":"some-printer","labels":[{"data":{}}]}"#,
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_ne!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        // Request reached handler and returned 404 NotFound
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn three_already_enveloped_endpoints_have_error_envelope() {
        let endpoints = [
            ("POST", "/api/render"),
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
        let flow_yaml = r#"name: Flow HTTP
description: Flow test
unit: mm
dpi: 200
params:
  - name: mode
    type: enum
    values: [short, long]
    default: short
  - name: title
    type: string
  - name: subtitle
    type: string
  - name: code
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
                "POST",
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

        let Layout::Items(items) = crate::parse::parse_template(flow_yaml).unwrap().layout;
        let LayoutItem::Container {
            flow: Some(flow),
            items: children,
            ..
        } = &items[0]
        else {
            panic!("item 0 is a flow container: {:?}", items[0]);
        };
        assert_eq!(flow.direction, crate::models::FlowDirection::Row);
        assert_eq!(flow.gap, 5.0);
        for (index, child) in children[..2].iter().enumerate() {
            let placement = child.placement().expect("a boxed child");
            assert!(placement.at.is_none(), "packed child {index} has no 'at'");
            assert!(
                matches!(placement.extent, crate::models::Extent::Size(_)),
                "packed child {index} is sized, not cornered with 'to'"
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

    #[tokio::test]
    async fn template_put_rejects_invalid_list_templates() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);

        let cases = [
            // List in when condition
            (
                "put_when_list",
                r#"
name: WhenList
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  - name: tags
    type: list
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    when: { tags: KIDS }
"#,
            ),
            // Image binding list
            (
                "put_img_list",
                r#"
name: ImageList
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  - name: tags
    type: list
layout:
  - type: image
    name: tags
    at: [0, 0]
    size: [50, 20]
"#,
            ),
            // Bare token on list
            (
                "put_bare_list",
                r#"
name: BareList
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  - name: tags
    type: list
layout:
  - type: text
    value: "{tags}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#,
            ),
            // Format on list
            (
                "put_format_list",
                r#"
name: FormatList
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  - name: tags
    type: list
layout:
  - type: text
    value: "{tags:short_date}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#,
            ),
            // Join on non-list
            (
                "put_join_non_list",
                r#"
name: JoinNonList
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  - name: title
    type: string
layout:
  - type: text
    value: "{title:join(', ')}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#,
            ),
        ];

        for (id, yaml) in cases {
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
                "template {id} must be rejected with 422"
            );
            let body = json_response(response).await;
            assert_eq!(body["error"]["code"], "TemplateInvalid");
            assert_eq!(
                body["error"]["details"]["reason"],
                "template_validation_failed"
            );
            assert!(
                !dir.join(format!("{id}.yaml")).exists(),
                "nothing should be stored for {id}"
            );
        }
    }

    #[tokio::test]
    async fn render_label_list_param_joining_and_refusals() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);

        let list_tpl = r#"
name: ListTemplate
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  - name: tags
    type: list
    default: [KIDS, CONSUMABLE]
layout:
  - type: text
    value: "Tags: {tags:join(', ')}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let put_res = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/list_tpl",
                "POST",
                list_tpl.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(put_res.status(), StatusCode::CREATED);

        // 1. Valid list overrides default
        let res = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png",
                json!({ "template": "list_tpl", "data": { "tags": ["A", "B"] } }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 2. Empty list [] joins to empty string rather than the declared default
        let res = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png",
                json!({ "template": "list_tpl", "data": { "tags": [] } }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        // Directly verify resolve does not replace [] with the default
        {
            use crate::models::{ParamSpec, ParamType};
            let mut params = indexmap::IndexMap::new();
            params.insert(
                "tags".to_string(),
                ParamSpec {
                    param_type: ParamType::List,
                    default: Some(crate::models::ParamValue::List(vec![
                        "KIDS".to_string(),
                        "CONSUMABLE".to_string(),
                    ])),
                    description: None,
                    min: None,
                    max: None,
                    default_instant: None,
                },
            );
            let tpl = crate::templates::TemplateContent {
                name: "x".to_string(),
                description: String::new(),
                categories: Vec::new(),
                unit: "mm".to_string(),
                dpi: 200,
                format: crate::models::TemplateFormat::Single {
                    width: crate::models::Dimension::Fixed(50.0).into(),
                    height: crate::models::Dimension::Fixed(20.0).into(),
                    media_width: None,
                },
                params,
                layout: crate::models::Layout::Items(vec![]),
            };
            let mut data = std::collections::HashMap::new();
            data.insert("tags".to_string(), serde_json::json!([]));
            let resolved =
                crate::render::resolve_parameters(&tpl, &data, &Default::default()).unwrap();
            assert_eq!(resolved.data.get("tags"), Some(&serde_json::json!([])));
        }

        // 3. Null list uses default
        let res = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png",
                json!({ "template": "list_tpl", "data": { "tags": null } }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 4. Omitted tags uses default
        let res = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png",
                json!({ "template": "list_tpl", "data": {} }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 5. Non-array string is refused with 400 InvalidRequest, param_value_invalid
        let res = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png",
                json!({ "template": "list_tpl", "data": { "tags": "A, B" } }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
        assert_eq!(body["error"]["details"]["reason"], "param_value_invalid");
        assert!(body["error"]["message"].as_str().unwrap().contains("tags"));

        // 6. Non-string element is refused with 400 InvalidRequest, param_value_invalid naming its position
        let res = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png",
                json!({ "template": "list_tpl", "data": { "tags": ["A", 123] } }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
        assert_eq!(body["error"]["details"]["reason"], "param_value_invalid");
        assert_eq!(body["error"]["details"]["param"], "tags");
        assert_eq!(body["error"]["details"]["element"], 1);
    }

    #[tokio::test]
    async fn render_label_list_param_without_default_reads_as_empty() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);

        let list_tpl = r#"
name: ListRequired
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  - name: tags
    type: list
layout:
  - type: text
    value: "Tags: {tags:join(', ')}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let put_res = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/list_req",
                "POST",
                list_tpl.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(put_res.status(), StatusCode::CREATED);

        // Omitted and null each read as the empty list
        let empty = render_png_bytes(&app, "list_req", json!({ "tags": [] })).await;
        assert_eq!(render_png_bytes(&app, "list_req", json!({})).await, empty);
        assert_eq!(
            render_png_bytes(&app, "list_req", json!({ "tags": null })).await,
            empty
        );

        // Provided -> 200 OK
        let res = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png",
                json!({ "template": "list_req", "data": { "tags": ["Item1"] } }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn render_label_undeclared_array_scalar_slot_refusal() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);

        let scalar_tpl = r#"
name: ScalarTemplate
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: text
    value: "Title: {title}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let put_res = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/scalar_tpl",
                "PUT",
                scalar_tpl.to_string(),
            ))
            .await
            .unwrap();
        // Under Issue 322, undeclared template parameters are rejected at template write (PUT)
        assert_eq!(put_res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(put_res).await;
        assert_eq!(body["error"]["code"], "TemplateInvalid");
        assert_eq!(
            body["error"]["details"]["reason"],
            "template_validation_failed"
        );

        // Image template binding undeclared name
        let img_tpl = r#"
name: ImageTemplate
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: image
    name: logo
    at: [0, 0]
    size: [50, 20]
"#;
        let put_img = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/img_tpl",
                "PUT",
                img_tpl.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(put_img.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(put_img).await;
        assert_eq!(body["error"]["code"], "TemplateInvalid");
        assert_eq!(
            body["error"]["details"]["reason"],
            "template_validation_failed"
        );

        // Image with content sizing binding undeclared name
        let img_content_tpl = r#"
name: ImageContentTemplate
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
layout:
  - type: image
    name: logo
    at: [0, 0]
    size: [content, content]
"#;
        let put_img_content = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/img_content_tpl",
                "PUT",
                img_content_tpl.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(put_img_content.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(put_img_content).await;
        assert_eq!(body["error"]["code"], "TemplateInvalid");
        assert_eq!(
            body["error"]["details"]["reason"],
            "template_validation_failed"
        );

        // Declared string parameter refusing an array value -> 400 InvalidRequest, param_value_invalid
        let string_param_tpl = r#"
name: StringParamTemplate
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  - name: title
    type: string
layout:
  - type: text
    value: "{title}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let put_str = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/string_param_tpl",
                "POST",
                string_param_tpl.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(put_str.status(), StatusCode::CREATED);

        let res = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png",
                json!({ "template": "string_param_tpl", "data": { "title": ["A", "B"] } })
                    .to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
        assert_eq!(body["error"]["details"]["reason"], "param_value_invalid");
        assert_eq!(
            body["error"]["message"],
            r#"invalid value for parameter 'title': ["A","B"] is not a string"#
        );
    }

    #[tokio::test]
    async fn render_batch_list_param_refusal() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);

        let list_tpl = r#"
name: BatchList
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  - name: tags
    type: list
layout:
  - type: text
    value: "Tags: {tags:join(', ')}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let put_res = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/batch_list_tpl",
                "POST",
                list_tpl.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(put_res.status(), StatusCode::CREATED);

        // Batch with one invalid item fails the batch with 422 BatchInvalid
        let batch_payload = json!({
            "template": "batch_list_tpl",
            "labels": [
                { "data": { "tags": ["Valid1", "Valid2"] } },
                { "data": { "tags": "invalid-string" } }
            ]
        });
        let res = app
            .clone()
            .oneshot(json_req("POST", "/api/render", batch_payload.to_string()))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "BatchInvalid");
        let failures = body["error"]["details"]["failures"].as_array().unwrap();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0]["index"], 1);
        assert_eq!(failures[0]["code"], "InvalidRequest");
        assert_eq!(failures[0]["details"]["reason"], "param_value_invalid");
    }

    #[tokio::test]
    async fn template_detail_and_thumbnail_list_param() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);

        let list_tpl = r#"
name: DetailList
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  - name: tags
    type: list
    default: [KIDS, CONSUMABLE]
layout:
  - type: text
    value: "Tags: {tags:join(', ')}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let put_res = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/detail_list",
                "POST",
                list_tpl.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(put_res.status(), StatusCode::CREATED);

        // GET /api/templates/detail_list
        let res = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/api/templates/detail_list")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = json_response(res).await;

        // Check params
        let params = body["params"].as_array().unwrap();
        let tags_param = params.iter().find(|p| p["name"] == "tags").unwrap();
        assert_eq!(tags_param["type"], "list");
        assert_eq!(tags_param["control"], "list");
        assert_eq!(tags_param["default"], json!(["KIDS", "CONSUMABLE"]));

        // Thumbnail renders successfully
        let thumb_res = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/api/templates/detail_list/thumbnail")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(thumb_res.status(), StatusCode::OK);
        assert_eq!(thumb_res.headers()["content-type"], "image/png");

        // Thumbnail for template without default renders parameter's own name
        let no_def_tpl = r#"
name: NoDefList
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  - name: tags
    type: list
layout:
  - type: text
    value: "Tags: {tags:join(', ')}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let put_no_def = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/no_def_list",
                "POST",
                no_def_tpl.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(put_no_def.status(), StatusCode::CREATED);

        let thumb_no_def = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/api/templates/no_def_list/thumbnail")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(thumb_no_def.status(), StatusCode::OK);

        // Thumbnail for template with default: [] renders empty list
        let empty_def_tpl = r#"
name: EmptyDefList
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  - name: tags
    type: list
    default: []
layout:
  - type: text
    value: "Tags: {tags:join(', ')}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let put_empty_def = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/empty_def_list",
                "POST",
                empty_def_tpl.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(put_empty_def.status(), StatusCode::CREATED);

        let thumb_empty_def = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/api/templates/empty_def_list/thumbnail")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(thumb_empty_def.status(), StatusCode::OK);

        // Published list claims for task 5.5
        // 1. No-default list publishes the list control and no default
        let res_no_def = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/api/templates/no_def_list")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_no_def.status(), StatusCode::OK);
        let body_no_def = json_response(res_no_def).await;
        let params_no_def = body_no_def["params"].as_array().unwrap();
        let tags_no_def = params_no_def.iter().find(|p| p["name"] == "tags").unwrap();
        assert_eq!(tags_no_def["control"], "list");
        assert!(tags_no_def.get("default").is_none());

        // 2. Declared string parameter is not list control
        let str_tpl = r#"
name: StrNotList
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  - name: body
    type: string
layout:
  - type: text
    value: "Title: {body}"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
"#;
        let put_str = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/str_not_list",
                "POST",
                str_tpl.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(put_str.status(), StatusCode::CREATED);
        let res_str = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/api/templates/str_not_list")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_str.status(), StatusCode::OK);
        let body_str = json_response(res_str).await;
        let params_str = body_str["params"].as_array().unwrap();
        let body_input = params_str.iter().find(|p| p["name"] == "body").unwrap();
        assert_ne!(body_input["control"], "list");
        assert_eq!(body_input["control"], "text");

        // 3. Empty list default renders without 422 when omitted (not an omission)
        let res_empty_render = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png",
                json!({ "template": "empty_def_list", "data": {} }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(
            res_empty_render.status(),
            StatusCode::OK,
            "default: [] must render"
        );
    }

    #[tokio::test]
    async fn render_label_repetition_expansion_empty_and_multiple() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);

        // 4.7: Template with repetition container and before/after siblings
        let rep_tpl = r#"
name: RepExpansion
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
params:
  - name: tags
    type: list
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow: { direction: column }
    items:
      - type: text
        value: "PRE"
        size: [content, content]
        font_size: 8
      - type: container
        repeat: tags
        size: [content, content]
        flow: { direction: column }
        items:
          - type: text
            value: "Tag: {tags}"
            size: [content, content]
            font_size: 8
      - type: text
        value: "POST"
        size: [content, content]
        font_size: 8
"#;
        let put_res = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/rep_expansion",
                "POST",
                rep_tpl.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(put_res.status(), StatusCode::CREATED);

        // 4.7: Over ["A", "B", "C"] produces 3 instances in authored request order with siblings in place
        let res_multi = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png",
                json!({ "template": "rep_expansion", "data": { "tags": ["A", "B", "C"] } })
                    .to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res_multi.status(), StatusCode::OK);
        assert_eq!(res_multi.headers()["content-type"], "image/png");

        // 4.7: Over [] produces 0 instances, siblings keep places, no error
        let res_empty = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png",
                json!({ "template": "rep_expansion", "data": { "tags": [] } }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res_empty.status(), StatusCode::OK);
        assert_eq!(res_empty.headers()["content-type"], "image/png");

        // 4.7: Declared default: supplying elements renders when data is omitted
        let def_tpl = r#"
name: RepDefault
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
params:
  - name: tags
    type: list
    default: ["DefA", "DefB"]
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow: { direction: column }
    items:
      - type: container
        repeat: tags
        size: [content, content]
        flow: { direction: column }
        items:
          - type: text
            value: "Tag: {tags}"
            size: [content, content]
            font_size: 8
"#;
        let put_def = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/rep_default",
                "POST",
                def_tpl.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(put_def.status(), StatusCode::CREATED);

        let res_def = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png",
                json!({ "template": "rep_default", "data": {} }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res_def.status(), StatusCode::OK);
        assert_eq!(res_def.headers()["content-type"], "image/png");

        // 4.7: default: [] draws strip with no instances and no error
        let def_empty_tpl = r#"
name: RepDefaultEmpty
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
params:
  - name: tags
    type: list
    default: []
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow: { direction: column }
    items:
      - type: container
        repeat: tags
        size: [content, content]
        flow: { direction: column }
        items:
          - type: text
            value: "Tag: {tags}"
            size: [content, content]
            font_size: 8
"#;
        let put_def_empty = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/rep_default_empty",
                "POST",
                def_empty_tpl.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(put_def_empty.status(), StatusCode::CREATED);

        let res_def_empty = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png",
                json!({ "template": "rep_default_empty", "data": {} }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res_def_empty.status(), StatusCode::OK);
        assert_eq!(res_def_empty.headers()["content-type"], "image/png");

        // 4.7: An absent undefaulted list draws what the empty list draws
        assert_eq!(
            render_png_bytes(&app, "rep_expansion", json!({})).await,
            render_png_bytes(&app, "rep_expansion", json!({ "tags": [] })).await
        );
    }

    #[tokio::test]
    async fn render_label_repetition_auto_sizing() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);

        // 4.8: HTTP smoke test for repetition auto-sizing (exact drawn geometry assertions are in render::tests::repeating_container_drawn_geometry_sizes_each_instance_to_its_own_element)
        let rep_tpl = r##"
name: RepAutoSizing
unit: mm
dpi: 200
format: { type: single, width: 100, height: 100 }
params:
  - name: tags
    type: list
layout:
  - type: container
    at: [0, 0]
    size: [100, 100]
    flow: { direction: column }
    items:
      - type: container
        repeat: tags
        size: [content, content]
        background: "#eeeeee"
        flow: { direction: column }
        items:
          - type: text
            value: "{tags}"
            size: [content, content]
            font_size: 8
"##;
        let put_res = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/rep_autosize",
                "POST",
                rep_tpl.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(put_res.status(), StatusCode::CREATED);

        let res = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png",
                json!({ "template": "rep_autosize", "data": { "tags": ["Short", "Medium tag", "A substantially longer tag text"] } }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers()["content-type"], "image/png");
    }

    #[tokio::test]
    async fn render_label_repetition_nested_and_scope() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);

        // 4.11: Nested repeating containers compose, reading both {cats} and {items} in order;
        // bare {p} prints one element; when: inside compares bound element; joined outside prints joined list
        let rep_tpl = r#"
name: RepNestedScope
unit: mm
dpi: 200
format: { type: single, width: 100, height: 100 }
params:
  - name: cats
    type: list
  - name: items
    type: list
layout:
  - type: container
    at: [0, 0]
    size: [100, 100]
    flow: { direction: column }
    items:
      - type: text
        value: "All: {cats:join('+')}"
        size: [content, content]
        font_size: 8
      - type: container
        repeat: cats
        size: [content, content]
        flow: { direction: column }
        items:
          - type: container
            repeat: items
            size: [content, content]
            flow: { direction: column }
            items:
              - type: text
                value: "{cats}: {items}"
                size: [content, content]
                font_size: 8
              - type: text
                when:
                  items: Apple
                value: "(FAVORITE)"
                size: [content, content]
                font_size: 8
"#;
        let put_res = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/rep_nested_scope",
                "POST",
                rep_tpl.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(put_res.status(), StatusCode::CREATED);

        let res = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png",
                json!({
                    "template": "rep_nested_scope",
                    "data": {
                        "cats": ["Fruit", "Veg"],
                        "items": ["Apple", "Broccoli"]
                    }
                })
                .to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers()["content-type"], "image/png");
    }

    #[tokio::test]
    async fn render_label_repetition_when_gating() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);

        // 4.7: when: on container gates whole repetition once; child when: evaluated per element
        let rep_tpl = r#"
name: RepWhen
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
params:
  - name: show_tags
    type: enum
    values: [yes, no]
    default: yes
  - name: tags
    type: list
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow: { direction: column }
    items:
      - type: container
        when:
          show_tags: yes
        repeat: tags
        size: [content, content]
        flow: { direction: column }
        items:
          - type: text
            value: "Tag: {tags}"
            size: [content, content]
            font_size: 8
          - type: text
            when:
              tags: special
            value: "(SPECIAL)"
            size: [content, content]
            font_size: 8
"#;
        let put_res = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/rep_when",
                "POST",
                rep_tpl.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(put_res.status(), StatusCode::CREATED);

        // When gated off: does not require tags parameter -> 200 OK
        let res_off = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png",
                json!({ "template": "rep_when", "data": { "show_tags": "no" } }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res_off.status(), StatusCode::OK);
        assert_eq!(res_off.headers()["content-type"], "image/png");

        // When active: evaluates child when per element -> 200 OK
        let res_active = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png",
                json!({ "template": "rep_when", "data": { "show_tags": "yes", "tags": ["normal", "special"] } }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res_active.status(), StatusCode::OK);
        assert_eq!(res_active.headers()["content-type"], "image/png");
    }

    #[tokio::test]
    async fn render_label_repetition_arrangement() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);

        // 4.9: Overrun: instances that overrun fail with UnsupportedLayoutItem and details.reason item_out_of_frame
        // naming layout[0].items[0]#2 for the third
        let rep_overflow = r#"
name: RepOverflow
unit: mm
dpi: 200
format: { type: single, width: 50, height: 25 }
params:
  - name: tags
    type: list
layout:
  - type: container
    at: [0, 0]
    size: [50, 25]
    flow: { direction: column }
    items:
      - type: container
        repeat: tags
        size: [50, 10]
        items:
          - type: text
            at: [0, 0]
            value: "{tags}"
            size: [10, 5]
            font_size: 8
"#;
        let put_overflow = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/rep_overflow",
                "POST",
                rep_overflow.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(put_overflow.status(), StatusCode::CREATED);

        let res_overflow = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png",
                json!({ "template": "rep_overflow", "data": { "tags": ["A", "B", "C"] } })
                    .to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res_overflow.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body_overflow = json_response(res_overflow).await;
        assert_eq!(body_overflow["error"]["code"], "UnsupportedLayoutItem");
        assert_eq!(
            body_overflow["error"]["details"]["reason"],
            "item_out_of_frame"
        );
        let msg = body_overflow["error"]["message"].as_str().unwrap();
        assert!(
            msg.contains("layout[0].items[0]#2"),
            "expected layout[0].items[0]#2 in error message: {msg}"
        );

        // 4.9: The same container under overflow: trim draws the first two and succeeds
        let rep_trim = r#"
name: RepTrim
unit: mm
dpi: 200
format: { type: single, width: 50, height: 25 }
params:
  - name: tags
    type: list
layout:
  - type: container
    at: [0, 0]
    size: [50, 25]
    flow: { direction: column, overflow: trim }
    items:
      - type: container
        repeat: tags
        size: [50, 10]
        items:
          - type: text
            at: [0, 0]
            value: "Tag: {tags}"
            size: [10, 5]
            font_size: 8
"#;
        let put_trim = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/rep_trim",
                "POST",
                rep_trim.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(put_trim.status(), StatusCode::CREATED);

        let res_trim = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png",
                json!({ "template": "rep_trim", "data": { "tags": ["A", "B", "C"] } }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res_trim.status(), StatusCode::OK);
        assert_eq!(res_trim.headers()["content-type"], "image/png");

        // 4.9: Under wrap: true the third begins a second line
        let rep_wrap = r#"
name: RepWrap
unit: mm
dpi: 200
format: { type: single, width: 25, height: 50 }
params:
  - name: tags
    type: list
layout:
  - type: container
    at: [0, 0]
    size: [25, 50]
    flow: { direction: row, wrap: true }
    items:
      - type: container
        repeat: tags
        size: [10, 10]
        items:
          - type: text
            at: [0, 0]
            value: "{tags}"
            size: [10, 10]
            font_size: 8
"#;
        let put_wrap = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/rep_wrap",
                "POST",
                rep_wrap.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(put_wrap.status(), StatusCode::CREATED);

        let res_wrap = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png",
                json!({ "template": "rep_wrap", "data": { "tags": ["A", "B", "C"] } }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res_wrap.status(), StatusCode::OK);
        assert_eq!(res_wrap.headers()["content-type"], "image/png");
    }

    #[tokio::test]
    async fn render_label_repetition_extentless_container() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);

        // 4.10: Repeating container written with neither size nor to renders one instance for
        // a one-element list and fails with item_out_of_frame naming the second instance for a
        // two-element one
        let rep_extentless = r#"
name: RepExtentless
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
params:
  - name: tags
    type: list
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow: { direction: column }
    items:
      - type: container
        repeat: tags
        items:
          - type: text
            at: [0, 0]
            value: "{tags}"
            size: [10, 10]
            font_size: 8
"#;
        let put_res = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/rep_extentless",
                "POST",
                rep_extentless.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(put_res.status(), StatusCode::CREATED);

        // 1 element renders successfully
        let res_one = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png",
                json!({ "template": "rep_extentless", "data": { "tags": ["A"] } }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res_one.status(), StatusCode::OK);
        assert_eq!(res_one.headers()["content-type"], "image/png");

        // 2 elements fail with item_out_of_frame naming layout[0].items[0]#1
        let res_two = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png",
                json!({ "template": "rep_extentless", "data": { "tags": ["A", "B"] } }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res_two.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body_two = json_response(res_two).await;
        assert_eq!(body_two["error"]["code"], "UnsupportedLayoutItem");
        assert_eq!(body_two["error"]["details"]["reason"], "item_out_of_frame");
        let msg = body_two["error"]["message"].as_str().unwrap();
        assert!(
            msg.contains("layout[0].items[0]#1"),
            "expected layout[0].items[0]#1 in error message: {msg}"
        );
    }

    #[tokio::test]
    async fn repetition_roundtrip_and_http_refusals() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);

        // 1.6: Round-trip through GET /api/templates/{id} and resubmitting unchanged
        let rep_tpl = r#"name: RoundTrip
unit: mm
dpi: 200
format:
  type: single
  width: 50
  height: 50
params:
  - name: tags
    type: list
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow:
      direction: column
    items:
      - type: container
        repeat: tags
        size: [content, content]
        flow:
          direction: column
        items:
          - type: text
            value: "{tags}"
            size: [content, content]
            font_size: 8
"#;
        let put_res = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/round_trip",
                "POST",
                rep_tpl.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(put_res.status(), StatusCode::CREATED);

        let get_res = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/api/templates/round_trip")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_res.status(), StatusCode::OK);
        let Layout::Items(items) = crate::parse::parse_template(rep_tpl).unwrap().layout;
        assert_eq!(items.len(), 1);
        let LayoutItem::Container {
            items: children, ..
        } = &items[0]
        else {
            panic!("item 0 is a container: {:?}", items[0]);
        };
        assert_eq!(children.len(), 1);
        let LayoutItem::Container {
            repeat, placement, ..
        } = &children[0]
        else {
            panic!("child 0 is a container: {:?}", children[0]);
        };
        assert_eq!(repeat.as_deref(), Some("tags"));
        assert!(placement.at.is_none());
        assert!(matches!(placement.extent, crate::models::Extent::Size(_)));

        // Resubmitting the returned source document unchanged is accepted
        let source_res = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/api/templates/round_trip/source")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(source_res.status(), StatusCode::OK);
        let source_bytes = axum::body::to_bytes(source_res.into_body(), 64 * 1024)
            .await
            .unwrap();
        let source_str = String::from_utf8(source_bytes.to_vec()).unwrap();
        let put_again = app
            .clone()
            .oneshot(yaml_post("/api/templates/round_trip", "PUT", source_str))
            .await
            .unwrap();
        assert_eq!(put_again.status(), StatusCode::OK);

        // 2.8: HTTP PUT refusals for all 8 repetition refusals
        // Establish an existing template at "keeper" to verify it remains byte-for-byte unchanged
        let keeper_tpl = r#"name: Keeper
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
layout:
  - type: text
    value: "Original keeper template"
    at: [0, 0]
    size: [50, 10]
    font_size: 8
"#;
        let put_keeper = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/keeper",
                "POST",
                keeper_tpl.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(put_keeper.status(), StatusCode::CREATED);

        let keeper_orig_src = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/api/templates/keeper/source")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let keeper_orig_bytes = axum::body::to_bytes(keeper_orig_src.into_body(), 64 * 1024)
            .await
            .unwrap();

        let eight_refusals = [
            // 1. (1.3) Null repeat
            (
                "null_repeat",
                r#"name: NullRep
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
params: [{ name: tags, type: list }]
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow: { direction: column }
    items:
      - type: container
        repeat: null
        size: [10, 10]
        items: []
"#,
                "layout[0].items[0].repeat",
            ),
            // 2. (1.4) Unpacked repeat on root container
            (
                "unpacked_repeat",
                r#"name: UnpackedRep
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
params: [{ name: tags, type: list }]
layout:
  - type: container
    repeat: tags
    at: [0, 0]
    size: [50, 50]
    items: []
"#,
                "layout[0].repeat",
            ),
            // 3. (1.5) Repeat on text item
            (
                "text_repeat",
                r#"name: TextRep
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
params: [{ name: tags, type: list }]
layout:
  - type: text
    repeat: tags
    value: "hi"
    at: [0, 0]
    size: [50, 10]
    font_size: 8
"#,
                "layout[0]",
            ),
            // 4. (2.2) Repeat on undeclared param
            (
                "undeclared_repeat",
                r#"name: UndeclaredRep
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow: { direction: column }
    items:
      - type: container
        repeat: missing
        size: [10, 10]
        items: []
"#,
                "layout[0].items[0]",
            ),
            // 5. (2.3) Repeat on non-list param
            (
                "wrong_type_repeat",
                r#"name: WrongTypeRep
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
params: [{ name: title, type: string }]
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow: { direction: column }
    items:
      - type: container
        repeat: title
        size: [10, 10]
        items: []
"#,
                "layout[0].items[0]",
            ),
            // 6. (2.4) Nested repeat on same param
            (
                "nested_same_repeat",
                r#"name: NestedSameRep
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
params: [{ name: tags, type: list }]
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow: { direction: column }
    items:
      - type: container
        repeat: tags
        size: [10, 10]
        flow: { direction: column }
        items:
          - type: container
            repeat: tags
            size: [10, 10]
            items: []
"#,
                "layout[0].items[0].items[0]",
            ),
            // 7. (2.5) Join in repeat scope
            (
                "join_in_repeat",
                r#"name: JoinInRep
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
params: [{ name: tags, type: list }]
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow: { direction: column }
    items:
      - type: container
        repeat: tags
        size: [10, 10]
        flow: { direction: column }
        items:
          - type: text
            value: "{tags:join(',')}"
            size: [10, 5]
            font_size: 8
"#,
                "layout[0].items[0].items[0]",
            ),
            // 8. (2.6) Bare format in repeat scope
            (
                "format_in_repeat",
                r#"name: FormatInRep
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
params: [{ name: tags, type: list }]
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow: { direction: column }
    items:
      - type: container
        repeat: tags
        size: [10, 10]
        flow: { direction: column }
        items:
          - type: text
            value: "{tags:short_date}"
            size: [10, 5]
            font_size: 8
"#,
                "layout[0].items[0].items[0]",
            ),
        ];

        for (id, yaml_content, expected_path) in eight_refusals {
            // 2.8: Create-only write returns 422 and creates no file
            let res = app
                .clone()
                .oneshot(yaml_post(
                    &format!("/api/templates/{id}"),
                    "PUT",
                    yaml_content.to_string(),
                ))
                .await
                .unwrap();
            assert_eq!(
                res.status(),
                StatusCode::UNPROCESSABLE_ENTITY,
                "refusal for {id} must return 422"
            );
            let body = json_response(res).await;
            assert_eq!(body["error"]["code"], "TemplateInvalid");
            assert_eq!(
                body["error"]["details"]["reason"],
                "template_validation_failed"
            );
            assert!(
                body["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains(expected_path),
                "error message for {id} must contain path {expected_path}: {}",
                body["error"]["message"]
            );

            // Verify no file was created
            assert!(
                !dir.join(format!("{id}.yaml")).exists(),
                "create-only write for {id} must create no file"
            );
            let get_404 = app
                .clone()
                .oneshot(
                    axum::http::Request::builder()
                        .method("GET")
                        .uri(format!("/api/templates/{id}"))
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(get_404.status(), StatusCode::NOT_FOUND);

            // 2.8: Overwriting an existing template at "keeper" returns 422 and leaves it byte-for-byte unchanged
            let res_keeper = app
                .clone()
                .oneshot(yaml_post(
                    "/api/templates/keeper",
                    "PUT",
                    yaml_content.to_string(),
                ))
                .await
                .unwrap();
            assert_eq!(res_keeper.status(), StatusCode::UNPROCESSABLE_ENTITY);

            let keeper_src = app
                .clone()
                .oneshot(
                    axum::http::Request::builder()
                        .method("GET")
                        .uri("/api/templates/keeper/source")
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(keeper_src.status(), StatusCode::OK);
            let keeper_bytes = axum::body::to_bytes(keeper_src.into_body(), 64 * 1024)
                .await
                .unwrap();
            assert_eq!(
                keeper_bytes, keeper_orig_bytes,
                "keeper template must be byte-for-byte unchanged after refused write ({id})"
            );
        }
    }

    #[tokio::test]
    async fn template_detail_and_thumbnail_repetition() {
        let dir = temp_templates_dir();
        let app = build_app_in(&dir);

        // 5.4 & 5.5: Repeat-only template publishes tags with control list
        let rep_tpl = r#"name: RepDetail
unit: mm
dpi: 200
format: { type: single, width: 50, height: 50 }
params:
  - name: tags
    type: list
  - name: extra
    type: string
layout:
  - type: container
    at: [0, 0]
    size: [50, 50]
    flow: { direction: column }
    items:
      - type: container
        repeat: tags
        size: [content, content]
        flow: { direction: column }
        items:
          - type: text
            value: "Tag: {tags}"
            size: [content, content]
            font_size: 8
          - type: text
            when:
              tags: A
              extra: yes
            value: "Extra A"
            size: [content, content]
            font_size: 8
"#;
        let put_res = app
            .clone()
            .oneshot(yaml_post(
                "/api/templates/rep_detail",
                "POST",
                rep_tpl.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(put_res.status(), StatusCode::CREATED);

        let get_res = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/api/templates/rep_detail")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_res.status(), StatusCode::OK);
        let detail = json_response(get_res).await;

        let params = detail["params"].as_array().unwrap();
        let tags = params.iter().find(|p| p["name"] == "tags").unwrap();
        assert_eq!(tags["control"], "list");

        // 5.5: Thumbnail of repeat-only template draws 1 instance
        let thumb_res = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/api/templates/rep_detail/thumbnail")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(thumb_res.status(), StatusCode::OK);
        assert_eq!(thumb_res.headers()["content-type"], "image/png");
    }

    // -----------------------------------------------------------------------
    // Issue #324 tests: refusing unrecognized data keys on render/batch/print/csv
    // -----------------------------------------------------------------------

    fn build_app_with_custom_templates(tpls: Vec<(&str, &str)>) -> (axum::Router, Arc<AppState>) {
        let (mut templates, templates_dir) = crate::templates::load_all_for_tests();
        for (id, yaml) in tpls {
            let def = crate::parse::parse_template(yaml).unwrap();
            templates.insert_for_tests(id.to_string(), def);
        }
        let store = Store::open_in_memory().expect("store");
        seed_token(&store);
        let state = Arc::new(AppState::new(templates, templates_dir, store));
        (with_auth(app(state.clone())), state)
    }

    #[tokio::test]
    async fn issue_324_4_1_single_render_refuses_unrecognized_key() {
        let app = build_app();
        // homebox-qr reads {vars.qr_base_url}; unset, every request is a template fault.
        set_variable(&app, "qr_base_url", "https://example.com/").await;
        let payload = json!({
            "template": "homebox-qr",
            "data": {
                "id": "ITEM-1",
                "message": "Asset",
                "sku_legacy": "X-1"
            }
        });
        let res = app
            .oneshot(json_req("POST", "/api/render/label", payload.to_string()))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
        assert_eq!(body["error"]["details"]["reason"], "data_key_unknown");
        let msg = body["error"]["message"].as_str().unwrap();
        assert!(msg.contains("'sku_legacy'"));
        assert!(msg.contains("'homebox-qr'"));
    }

    #[tokio::test]
    async fn issue_324_4_2_single_render_multiple_unrecognized_keys_sorted() {
        let app = build_app();
        // homebox-qr reads {vars.qr_base_url}; unset, every request is a template fault.
        set_variable(&app, "qr_base_url", "https://example.com/").await;
        let payload = json!({
            "template": "homebox-qr",
            "data": {
                "zeta": "z",
                "alpha": "a",
                "mid": "m",
                "id": "ITEM-1",
                "message": "Asset"
            }
        });
        let res = app
            .oneshot(json_req("POST", "/api/render/label", payload.to_string()))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
        assert_eq!(body["error"]["details"]["reason"], "data_key_unknown");
        let msg = body["error"]["message"].as_str().unwrap();
        assert!(msg.contains("'alpha', 'mid', 'zeta'"));
        assert!(msg.contains("'homebox-qr'"));
    }

    #[tokio::test]
    async fn issue_324_4_3_single_render_declared_parameter_not_read_by_active_items_succeeds() {
        let yaml = r#"
name: Gated Declared
unit: mm
dpi: 200
format: { type: single, width: 50, height: 30 }
params:
  - name: orientation
    type: enum
    values: [horizontal, vertical]
    default: horizontal
  - name: title
    type: string
  - name: subtitle
    type: string
layout:
  - type: text
    value: "{title}"
    font_size: 10
    at: [0, 0]
    size: [50, 10]
  - type: container
    when: { orientation: vertical }
    at: [0, 10]
    size: [50, 10]
    items:
      - type: text
        value: "{subtitle}"
        font_size: 8
        at: [0, 0]
        size: [50, 10]
"#;
        let (app, _) = build_app_with_custom_templates(vec![("gated_declared", yaml)]);
        let payload = json!({
            "template": "gated_declared",
            "data": {
                "orientation": "horizontal",
                "title": "Hello",
                "subtitle": "Unread"
            }
        });
        let res = app
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=png",
                payload.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = bytes_response(res).await;
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[tokio::test]
    async fn issue_324_4_4_batch_reports_every_offending_label() {
        let app = build_app();
        let payload = json!({
            "template": "brother_24mm_qr",
            "labels": [
                { "data": { "code": "QR1", "message": "one", "bad0": "x" } },
                { "data": { "code": "QR2", "message": "two" } },
                { "data": { "code": "QR3", "message": "three", "bad2": "y" } }
            ]
        });
        let res = app
            .oneshot(json_req("POST", "/api/render", payload.to_string()))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "BatchInvalid");
        let failures = body["error"]["details"]["failures"].as_array().unwrap();
        assert_eq!(failures.len(), 2);
        assert_eq!(failures[0]["index"], 0);
        assert_eq!(failures[0]["code"], "InvalidRequest");
        assert_eq!(failures[0]["details"]["reason"], "data_key_unknown");
        assert_eq!(failures[1]["index"], 2);
        assert_eq!(failures[1]["code"], "InvalidRequest");
        assert_eq!(failures[1]["details"]["reason"], "data_key_unknown");
    }

    #[tokio::test]
    async fn issue_324_4_5_sheet_batch_reports_failures_and_is_atomic_across_pages() {
        let app = build_app();
        // 1. Sheet batch with 3 labels (indices 0 and 2 failing)
        let valid_sheet_data = json!({
            "id": "1",
            "url": "http://example.com",
            "name": "Item",
            "tags": "t",
            "description": "desc"
        });
        let mut bad_sheet_data0 = valid_sheet_data.as_object().unwrap().clone();
        bad_sheet_data0.insert("bad0".to_string(), json!("x"));
        let mut bad_sheet_data2 = valid_sheet_data.as_object().unwrap().clone();
        bad_sheet_data2.insert("bad2".to_string(), json!("y"));

        let payload3 = json!({
            "template": "avery5163_asset_tag",
            "labels": [
                { "data": bad_sheet_data0 },
                { "data": valid_sheet_data },
                { "data": bad_sheet_data2 }
            ]
        });
        let res3 = app
            .clone()
            .oneshot(json_req("POST", "/api/render", payload3.to_string()))
            .await
            .unwrap();
        assert_eq!(res3.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body3 = json_response(res3).await;
        assert_eq!(body3["error"]["code"], "BatchInvalid");
        let failures3 = body3["error"]["details"]["failures"].as_array().unwrap();
        assert_eq!(failures3.len(), 2);
        assert_eq!(failures3[0]["index"], 0);
        assert_eq!(failures3[0]["details"]["reason"], "data_key_unknown");
        assert_eq!(failures3[1]["index"], 2);
        assert_eq!(failures3[1]["details"]["reason"], "data_key_unknown");

        // 2. Multi-page sheet: 11 labels (10 per page), only label index 10 (page 2) fails
        let mut labels11 = Vec::new();
        for _ in 0..10 {
            labels11.push(json!({ "data": valid_sheet_data }));
        }
        labels11.push(json!({ "data": bad_sheet_data0 }));
        let payload11 = json!({
            "template": "avery5163_asset_tag",
            "labels": labels11
        });
        let res11 = app
            .clone()
            .oneshot(json_req("POST", "/api/render", payload11.to_string()))
            .await
            .unwrap();
        assert_eq!(res11.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body11 = json_response(res11).await;
        assert_eq!(body11["error"]["code"], "BatchInvalid");
        let failures11 = body11["error"]["details"]["failures"].as_array().unwrap();
        assert_eq!(failures11.len(), 1);
        assert_eq!(failures11[0]["index"], 10);
        assert_eq!(failures11[0]["details"]["reason"], "data_key_unknown");
    }

    #[tokio::test]
    async fn print_reports_failures_per_label() {
        let app = build_app();
        // homebox-qr reads {vars.qr_base_url}; unset, every request is a template fault.
        set_variable(&app, "qr_base_url", "https://example.com/").await;
        create_fake_printer(&app, "test-prn", false).await;

        let label = json!({ "data": { "id": "1", "message": "msg", "undeclared_key": "val" } });
        let payload = json!({
            "template": "homebox-qr",
            "printer": "test-prn",
            "labels": [label.clone(), label.clone(), label]
        });
        let res = app
            .clone()
            .oneshot(json_req("POST", "/api/print", payload.to_string()))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "BatchInvalid");
        let failures = body["error"]["details"]["failures"].as_array().unwrap();
        assert_eq!(failures.len(), 3);
        for (i, failure) in failures.iter().enumerate() {
            assert_eq!(failure["index"], i);
            assert_eq!(failure["code"], "InvalidRequest");
            assert_eq!(failure["details"]["reason"], "data_key_unknown");
        }

        // No print job dispatched: recent-templates remains empty
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
    async fn issue_324_4_7_uncoercible_integer_is_param_value_invalid() {
        let yaml = r#"
name: Integer Param Test
unit: mm
dpi: 200
format: { type: single, width: 50, height: 30 }
params:
  - name: title
    type: string
  - name: count
    type: integer
    default: 1
layout:
  - type: text
    value: "{title}"
    font_size: 10
    at: [0, 0]
    size: [50, 10]
"#;
        let (app, _) = build_app_with_custom_templates(vec![("int_param_tpl", yaml)]);

        // An uncoercible integer is refused naming the parameter.
        let payload_bad_val = json!({
            "template": "int_param_tpl",
            "data": {
                "title": "Item",
                "count": "abc"
            }
        });
        let res = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label",
                payload_bad_val.to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
        assert_eq!(body["error"]["details"]["reason"], "param_value_invalid");
    }

    #[tokio::test]
    async fn issue_324_4_9_batch_admission_cap_precedes_data_key_validation() {
        let app = build_app();
        // homebox-qr reads {vars.qr_base_url}; unset, every request is a template fault.
        set_variable(&app, "qr_base_url", "https://example.com/").await;
        let mut labels = Vec::with_capacity(501);
        for i in 0..501 {
            labels.push(json!({
                "data": {
                    "id": format!("ID-{i}"),
                    "message": "m",
                    "bad_key": "val"
                }
            }));
        }
        let payload = json!({
            "template": "homebox-qr",
            "labels": labels
        });
        let res = app
            .oneshot(json_req("POST", "/api/render", payload.to_string()))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "PayloadTooLarge");
    }

    #[tokio::test]
    async fn issue_324_6_1_batch_mixed_failures_reported() {
        let app = build_app();
        // homebox-qr reads {vars.qr_base_url}; unset, every request is a template fault.
        set_variable(&app, "qr_base_url", "https://example.com/").await;
        let payload = json!({
            "template": "homebox-qr",
            "labels": [
                { "data": { "id": "1", "message": "one", "bad0": "x" } },
                // (guard) an over-capacity QR payload stands in for the label that fails
                { "data": { "id": "A".repeat(8000), "message": "two" } }
            ]
        });
        let res = app
            .oneshot(json_req("POST", "/api/render", payload.to_string()))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "BatchInvalid");
        let failures = body["error"]["details"]["failures"].as_array().unwrap();
        assert_eq!(failures.len(), 2);
        assert_eq!(failures[0]["index"], 0);
        assert_eq!(failures[0]["code"], "InvalidRequest");
        assert_eq!(failures[0]["details"]["reason"], "data_key_unknown");
        assert_eq!(failures[1]["index"], 1);
        assert_eq!(failures[1]["code"], "UnsupportedLayoutItem");
        assert_eq!(failures[1]["details"]["reason"], "qr_payload_invalid");
    }

    /// errors "Two labels fail differently": each failing label is its own error object plus
    /// `index`, its reason and details under `details`.
    #[tokio::test]
    async fn batch_failures_are_per_label_error_objects() {
        let app = build_app();
        // (guard) label 2's `code` is beyond any QR's capacity.
        let payload = json!({
            "template": "brother_24mm_qr",
            "labels": [
                { "data": { "code": "A", "message": "hello", "bad_key": "x" } },
                { "data": { "code": "B", "message": "fine" } },
                { "data": { "code": "A".repeat(8000), "message": "long" } }
            ]
        });
        let res = app
            .oneshot(json_req("POST", "/api/render", payload.to_string()))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let mut body = json_response(res).await;
        assert_eq!(body["error"]["code"], "BatchInvalid");
        let failures = body["error"]["details"]["failures"]
            .as_array_mut()
            .expect("failures");
        for failure in failures.iter_mut() {
            assert!(failure["message"].as_str().is_some_and(|m| !m.is_empty()));
            failure.as_object_mut().unwrap().remove("message");
        }
        assert_eq!(
            json!(failures),
            json!([
                { "index": 0, "code": "InvalidRequest", "details": { "reason": "data_key_unknown" } },
                {
                    "index": 2,
                    "code": "UnsupportedLayoutItem",
                    "details": { "reason": "qr_payload_invalid" }
                }
            ])
        );
    }

    #[tokio::test]
    async fn issue_337_render_label_option_envelope_key_rejected() {
        let app = build_app();
        let payload = json!({
            "template": "shelf",
            "data": { "title": "Bolts" },
            "option": { "x": "1" }
        });
        let res = app
            .clone()
            .oneshot(json_req("POST", "/api/render/label", payload.to_string()))
            .await
            .expect("request");
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
        assert_eq!(body["error"]["details"]["reason"], "json_malformed");
        let err = body["error"]["details"]["error"].as_str().unwrap_or("");
        assert!(
            err.contains("unknown field `option`, expected `template` or `data`"),
            "expected error to contain backticked option message, got: {err}"
        );
    }

    #[tokio::test]
    async fn issue_337_batch_option_envelope_key_rejected() {
        let app = build_app();
        // POST /api/render
        let batch_payload = json!({
            "template": "shelf",
            "labels": [
                { "data": { "title": "Bolts" }, "option": { "x": "1" } }
            ]
        });
        let res_batch = app
            .clone()
            .oneshot(json_req("POST", "/api/render", batch_payload.to_string()))
            .await
            .expect("request");
        assert_eq!(res_batch.status(), StatusCode::BAD_REQUEST);
        let body_batch = json_response(res_batch).await;
        assert_eq!(body_batch["error"]["code"], "InvalidRequest");
        assert_eq!(body_batch["error"]["details"]["reason"], "json_malformed");
        let err_batch = body_batch["error"]["details"]["error"]
            .as_str()
            .unwrap_or("");
        assert!(
            err_batch.contains("unknown field `option`, expected `data`"),
            "expected batch error to contain backticked option message, got: {err_batch}"
        );
    }

    #[tokio::test]
    async fn issue_337_misspelled_dataa_envelope_key_rejected_on_both_endpoints() {
        let app = build_app();

        // 1. POST /api/render/label
        let render_payload = json!({
            "template": "shelf",
            "dataa": { "title": "Bolts" }
        });
        let res_render = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label",
                render_payload.to_string(),
            ))
            .await
            .expect("request");
        assert_eq!(res_render.status(), StatusCode::BAD_REQUEST);
        let body_render = json_response(res_render).await;
        assert_eq!(body_render["error"]["code"], "InvalidRequest");
        assert_eq!(body_render["error"]["details"]["reason"], "json_malformed");
        let err_render = body_render["error"]["details"]["error"]
            .as_str()
            .unwrap_or("");
        assert!(
            err_render.contains("unknown field `dataa`, expected `template` or `data`"),
            "expected render error to name dataa, got: {err_render}"
        );

        // 2. POST /api/render
        let batch_payload = json!({
            "template": "shelf",
            "labels": [
                { "dataa": { "title": "Bolts" } }
            ]
        });
        let res_batch = app
            .clone()
            .oneshot(json_req("POST", "/api/render", batch_payload.to_string()))
            .await
            .expect("request");
        assert_eq!(res_batch.status(), StatusCode::BAD_REQUEST);
        let body_batch = json_response(res_batch).await;
        assert_eq!(body_batch["error"]["code"], "InvalidRequest");
        assert_eq!(body_batch["error"]["details"]["reason"], "json_malformed");
        let err_batch = body_batch["error"]["details"]["error"]
            .as_str()
            .unwrap_or("");
        assert!(
            err_batch.contains("unknown field `dataa`, expected `data`"),
            "expected batch error to name dataa, got: {err_batch}"
        );
    }

    #[tokio::test]
    async fn issue_337_render_label_unknown_envelope_key_with_invalid_format_reports_json_malformed(
    ) {
        let app = build_app();
        let payload = json!({
            "template": "shelf",
            "data": { "title": "Bolts" },
            "option": { "x": "1" }
        });
        let res = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/render/label?format=invalid_fmt",
                payload.to_string(),
            ))
            .await
            .expect("request");
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = json_response(res).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
        assert_eq!(body["error"]["details"]["reason"], "json_malformed");
    }
}

#[cfg(test)]
mod auth_http_tests {
    use super::store::Store;
    use super::{app, AppState};
    use crate::models::{Color, DynamicValue, Layout, LayoutItem};
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
            templates.insert_for_tests(id.to_string(), def);
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
    async fn print_requires_auth() {
        let app = test_app();
        let payload = serde_json::json!({"template":"brother_24mm_qr","printer":"ok-printer","labels":[{"data":{}}]});
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
    async fn settings_reject_bad_value_and_unknown_key() {
        let app = test_app();
        let cookie = setup_login_cookie(&app).await;

        // not an object, a bad name, a bad pattern, a non-string pattern: all setting_value_invalid
        for bad in [
            r#"{"value":["%d"]}"#,
            r#"{"value":{"bad name":"%d"}}"#,
            r#"{"value":{"x":"%!"}}"#,
            r#"{"value":{"x":5}}"#,
        ] {
            let res = app
                .clone()
                .oneshot(req_put_json_cookie(
                    "/api/settings/datetime_formats",
                    bad,
                    &cookie,
                ))
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::BAD_REQUEST, "bad value {bad}");
            assert_eq!(
                body_json(res).await["error"]["details"]["reason"],
                "setting_value_invalid",
                "bad value {bad}"
            );
        }

        // unknown key on PUT and DELETE is 404 NotFound, including the removed settings
        for key in ["nope", "job_log_retention_days", "max_label_dimension_mm"] {
            let res = app
                .clone()
                .oneshot(req_put_json_cookie(
                    &format!("/api/settings/{key}"),
                    r#"{"value":1}"#,
                    &cookie,
                ))
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::NOT_FOUND, "PUT {key}");
            assert_eq!(body_json(res).await["error"]["code"], "NotFound");

            let res = app
                .clone()
                .oneshot(req_delete_cookie(&format!("/api/settings/{key}"), &cookie))
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::NOT_FOUND, "DELETE {key}");
            assert_eq!(body_json(res).await["error"]["code"], "NotFound");
        }
    }

    #[tokio::test]
    async fn settings_default_connection_id_endpoints() {
        let app = test_app();
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

        // Create connection 1
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

        // Create connection 2
        let res = app
            .clone()
            .oneshot(req_post_json_cookie(
                "/api/connections",
                r#"{"connector":"homebox","name":"conn2","base_url":"http://hb2.lan","credential":"sec"}"#,
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

        // 4. PUT accepts another connection's id
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

        // 6. Deleting the default connection clears the setting and deleting a different one does not
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
                r#"{"id":"ok-printer","name":"ok-printer","uri":"ipp://fake.test/"}"#,
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
                r#"{"template":"brother_24mm_qr","printer":"ok-printer","labels":[{"data":{"message":"x","code":"y"}}]}"#,
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
    }

    /// settings spec, "Six most recent".
    #[tokio::test]
    async fn recent_templates_returns_the_six_most_recent() {
        let (app, state) = test_app_with_custom_templates(vec![]);
        let res = app
            .clone()
            .oneshot(req_get("/api/templates"))
            .await
            .unwrap();
        let list = body_json(res).await;
        let ids: Vec<String> = list["templates"]
            .as_array()
            .unwrap()
            .iter()
            .take(8)
            .map(|t| t["id"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(ids.len(), 8);
        for id in &ids {
            state
                .store()
                .record_job(id, None, "ok", None, "local")
                .await
                .unwrap();
        }
        let newest_first: Vec<String> = ids.iter().rev().take(6).cloned().collect();
        for uri in ["/api/recent-templates", "/api/recent-templates?limit=20"] {
            let res = app.clone().oneshot(req_get(uri)).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK, "{uri}");
            assert_eq!(
                body_json(res).await,
                serde_json::json!(newest_first),
                "{uri}"
            );
        }
    }

    fn req_bearer(method: &str, uri: &str, body: &str, secret: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {secret}"))
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    /// Create an API token through `cookie`'s session, returning `(id, secret)`.
    async fn create_token(app: &axum::Router, cookie: &str) -> (String, String) {
        let res = app
            .clone()
            .oneshot(req_post_json_cookie(
                "/api/tokens",
                r#"{"name":"t"}"#,
                cookie,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let body = body_json(res).await;
        (
            body["id"].as_str().unwrap().to_string(),
            body["secret"].as_str().unwrap().to_string(),
        )
    }

    async fn my_id(app: &axum::Router, cookie: &str) -> String {
        let res = app
            .clone()
            .oneshot(req_get_cookie("/api/auth/me", cookie))
            .await
            .unwrap();
        body_json(res).await["me"]["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    /// auth spec, "Tokens are personal".
    #[tokio::test]
    async fn tokens_are_personal() {
        let app = test_app();
        let cookie_a = setup_login_cookie(&app).await;
        let cookie_b = login_second_user(&app, &cookie_a, "bob").await;
        let (id, secret) = create_token(&app, &cookie_a).await;

        let res = app
            .clone()
            .oneshot(req_get_cookie("/api/tokens", &cookie_b))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(body_json(res).await, serde_json::json!([]));
        let res = app
            .clone()
            .oneshot(req_delete_cookie(&format!("/api/tokens/{id}"), &cookie_b))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);

        let res = app
            .clone()
            .oneshot(req_get_cookie("/api/tokens", &cookie_a))
            .await
            .unwrap();
        assert_eq!(body_json(res).await[0]["id"], id);
        let res = app
            .clone()
            .oneshot(req_bearer("GET", "/api/templates", "", &secret))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    /// auth spec, "Deleting a user deletes their tokens".
    #[tokio::test]
    async fn deleting_a_user_deletes_their_tokens() {
        let app = test_app();
        let cookie_a = setup_login_cookie(&app).await;
        let cookie_b = login_second_user(&app, &cookie_a, "bob").await;
        let (_, secret) = create_token(&app, &cookie_a).await;
        let a_id = my_id(&app, &cookie_a).await;

        let res = app
            .clone()
            .oneshot(req_delete_cookie(&format!("/api/users/{a_id}"), &cookie_b))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        let res = app
            .clone()
            .oneshot(req_bearer("GET", "/api/templates", "", &secret))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    /// auth spec, "A token reports its owner".
    #[tokio::test]
    async fn token_reports_its_owner_on_me() {
        let app = test_app();
        let cookie = setup_login_cookie(&app).await;
        let (_, secret) = create_token(&app, &cookie).await;
        let res = app
            .clone()
            .oneshot(req_bearer("GET", "/api/auth/me", "", &secret))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_json(res).await;
        assert_eq!(body["authed"], true);
        assert_eq!(
            body["me"],
            serde_json::json!({ "id": my_id(&app, &cookie).await, "username": "a" })
        );
    }

    /// printing spec, "A token prints as its owner".
    #[tokio::test]
    async fn token_prints_as_its_owner() {
        let app = test_app();
        let cookie = setup_login_cookie(&app).await;
        let (_, secret) = create_token(&app, &cookie).await;
        let res = app
            .clone()
            .oneshot(req_post_json_cookie(
                "/api/printers",
                r#"{"id":"tp","name":"tp","uri":"ipp://fake.test/"}"#,
                &cookie,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let res = app
            .clone()
            .oneshot(req_bearer(
                "POST",
                "/api/print",
                r#"{"template":"brother_24mm_qr","printer":"tp","labels":[{"data":{"message":"x","code":"y"}}]}"#,
                &secret,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(body_json(res).await["sent"], 1);
        let res = app
            .clone()
            .oneshot(req_get_cookie("/api/recent-templates", &cookie))
            .await
            .unwrap();
        assert_eq!(body_json(res).await, serde_json::json!(["brother_24mm_qr"]));
    }

    #[tokio::test]
    async fn token_cannot_delete_its_own_user() {
        let app = test_app();
        let cookie_a = setup_login_cookie(&app).await;
        login_second_user(&app, &cookie_a, "bob").await;
        let (_, secret) = create_token(&app, &cookie_a).await;
        let a_id = my_id(&app, &cookie_a).await;
        let res = app
            .clone()
            .oneshot(req_bearer(
                "DELETE",
                &format!("/api/users/{a_id}"),
                "",
                &secret,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CONFLICT);
    }

    /// auth spec, `/auth/password` refuses token callers. Regression guard: passes before #417 too.
    #[tokio::test]
    async fn token_cannot_change_password() {
        let app = test_app();
        let cookie = setup_login_cookie(&app).await;
        let (_, secret) = create_token(&app, &cookie).await;
        let res = app
            .clone()
            .oneshot(req_bearer(
                "POST",
                "/api/auth/password",
                r#"{"current_password":"pw123456","new_password":"other"}"#,
                &secret,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    /// auth spec, "An unknown bearer token beats a valid cookie". Regression guard: passes before
    /// #417 too.
    #[tokio::test]
    async fn unknown_bearer_beats_a_valid_cookie() {
        let app = test_app();
        let cookie = setup_login_cookie(&app).await;
        let req = Request::builder()
            .uri("/api/templates")
            .header("cookie", &cookie)
            .header("authorization", "Bearer nope")
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    /// auth spec, "No-auth mode": a present but unparseable Origin is refused.
    #[tokio::test]
    async fn no_auth_refuses_an_unreadable_origin() {
        let app = test_app_no_auth();
        let req = Request::builder()
            .method("POST")
            .uri("/api/render/label")
            .header("content-type", "application/json")
            .header("host", "localhost")
            .header(
                "origin",
                axum::http::HeaderValue::from_bytes(b"http://local\xffhost").unwrap(),
            )
            .body(Body::from(
                r#"{"template":"brother_24mm_qr","data":{"message":"x","code":"y"}}"#,
            ))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    /// auth spec, "No-auth mode": with no Host to compare against, a present Origin cannot match.
    #[tokio::test]
    async fn no_auth_refuses_an_origin_without_host() {
        let app = test_app_no_auth();
        let req = Request::builder()
            .method("POST")
            .uri("/api/templates/reload")
            .header("origin", "http://evil.example")
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
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
    async fn omitted_boolean_renders_false_and_omitted_enum_prints_nothing() {
        let yaml = r#"
name: Test Missing Param
unit: mm
dpi: 200
params:
  - name: flag
    type: boolean
  - name: choice
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

        // 1. Omit flag -> a boolean is never absent: it renders as false
        let req = req_post_json(
            "/api/render/label",
            &serde_json::json!({
                "template": "missing_param_tpl",
                "data": { "choice": "one" }
            })
            .to_string(),
        );
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 2. Omit choice -> it prints nothing
        let req = req_post_json(
            "/api/render/label",
            &serde_json::json!({
                "template": "missing_param_tpl",
                "data": { "flag": true }
            })
            .to_string(),
        );
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn omitted_datetime_prints_nothing_and_declared_default_prints() {
        let yaml_no_default = r#"
name: Test Missing DateTime
unit: mm
dpi: 200
params:
  - name: printed_on
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
  - name: printed_on
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

        // Omission, blank and null without default each print nothing
        for val in [
            serde_json::json!({}),
            serde_json::json!({"printed_on": "   "}),
            serde_json::json!({"printed_on": null}),
        ] {
            let req = req_post_json(
                "/api/render/label",
                &serde_json::json!({ "template": "dt_no_default", "data": val }).to_string(),
            );
            let res = app.clone().oneshot(req).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK, "{val}");
        }

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
    async fn datetime_param_attribution_boundary_http_test() {
        let yaml = r#"
name: Attribution Boundary DT
unit: mm
dpi: 200
params:
  - name: dt
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

        // Path 1: Caller supplies the invalid date value in request `data`, while the default
        // resolves -> 400 Bad Request / param_value_invalid
        state
            .store()
            .set_variable("bad_date", "2026-02-28")
            .await
            .unwrap();
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
            "param_value_invalid"
        );

        // Path 2: Exact same value reached through the tokened default -> 422 TemplateInvalid /
        // reference_unresolved naming the parameter, whatever the label carries
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
            "reference_unresolved"
        );
        assert_eq!(body_default["error"]["details"]["field"], "dt");
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
  - name: missing
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
params:
  - name: missing_image
    type: string
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
  - name: box_w
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

        // A trimmed item's text is still evaluated: absent, it reads as the empty string.
        let mut pngs = Vec::new();
        for data in [serde_json::json!({}), serde_json::json!({ "missing": "" })] {
            let response = app
                .clone()
                .oneshot(req_post_json(
                    "/api/render/label?format=png",
                    &serde_json::json!({ "template": "trim_missing_text", "data": data })
                        .to_string(),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            pngs.push(body_bytes(response).await);
        }
        assert_eq!(pngs[0], pngs[1]);

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
  - name: brand
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
  - name: bg_color
    type: string
  - name: stroke_color
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

        // 4. POST /api/render with 2 labels (second bad background color) returns 422 BatchInvalid with failure at index 1
        let req3 = req_post_json(
            "/api/render",
            &serde_json::json!({
                "template": "dyn_shape_color",
                "labels": [
                    { "data": { "bg_color": "red", "stroke_color": "black" } },
                    { "data": { "bg_color": "octarine", "stroke_color": "black" } }
                ]
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
        assert_eq!(failures[0]["details"]["reason"], "color_param_invalid");
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

    #[test]
    fn template_parses_a_declared_color_and_none_when_absent() {
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
        let Layout::Items(items) = crate::parse::parse_template(yaml).unwrap().layout;
        assert_eq!(items.len(), 2);
        let color = |item: &LayoutItem| match item {
            LayoutItem::Text { color, .. } => color.clone(),
            other => panic!("expected a text item: {other:?}"),
        };
        assert!(
            matches!(color(&items[0]), Some(DynamicValue::Literal(c)) if c.spelling() == "red"),
            "item 0 keeps declared color 'red'"
        );
        assert_eq!(color(&items[1]), None, "item 1 declares no color");
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
  - name: bg
    type: string
  - name: txt_col
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
            "/api/render",
            &serde_json::json!({
                "template": "sheet_color",
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
  - name: bg_color
    type: string
  - name: stroke_color
    type: string
  - name: text_color
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

    #[test]
    fn template_parses_authored_shape_and_text_colors() {
        let yaml = r##"
name: AuthoredColors
unit: mm
dpi: 200
params:
  - name: brand
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
        let Layout::Items(items) = crate::parse::parse_template(yaml).unwrap().layout;
        let spelling = |paint: &DynamicValue<Color>| match paint {
            DynamicValue::Literal(color) => color.spelling().to_string(),
            DynamicValue::Ref(name) => format!("{{{name}}}"),
        };
        let LayoutItem::Container {
            stroke: Some(stroke),
            background: Some(background),
            rounded,
            items: children,
            ..
        } = &items[0]
        else {
            panic!("item 0 is a stroked, filled container: {:?}", items[0]);
        };

        // Top-level container: authored spelling preserved
        assert_eq!(spelling(background), "red");
        assert_eq!(spelling(&stroke.color), "#F0F");
        assert_eq!(stroke.thickness, 0.2);
        assert_eq!(*rounded, Some(1.0));

        // Line with defaulted color -> "black"
        let LayoutItem::Line {
            stroke: Some(stroke),
            ..
        } = &children[0]
        else {
            panic!("child 0 is a stroked line: {:?}", children[0]);
        };
        assert_eq!(spelling(&stroke.color), "black");
        assert_eq!(stroke.thickness, 0.5);

        // Nested container with stroke: { color: "{brand}" } and background: "{brand}"
        let LayoutItem::Container {
            stroke: Some(stroke),
            background: Some(background),
            ..
        } = &children[1]
        else {
            panic!("child 1 is a stroked, filled container: {:?}", children[1]);
        };
        assert_eq!(spelling(&stroke.color), "{brand}");
        assert_eq!(spelling(background), "{brand}");

        // Text item with color reference, and an uncoloured one
        let text_color = |item: &LayoutItem| match item {
            LayoutItem::Text { color, .. } => color.as_ref().map(spelling),
            other => panic!("expected a text item: {other:?}"),
        };
        assert_eq!(text_color(&children[2]).as_deref(), Some("{brand}"));
        assert_eq!(text_color(&children[3]), None);
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
    async fn issue_262_thumbnail_fails_when_a_default_is_broken() {
        let yaml = r#"
name: Thumbnail Broken Default
unit: mm
dpi: 200
params:
  - name: val
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
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_json(res).await;
        assert_eq!(body["error"]["code"], "TemplateInvalid");
        assert_eq!(body["error"]["details"]["reason"], "reference_unresolved");
        assert_eq!(body["error"]["details"]["field"], "vars.missing_key");
    }

    #[tokio::test]
    async fn issue_262_when_gate_with_tokened_default() {
        let yaml = r#"
name: When Gate Token
unit: mm
dpi: 200
params:
  - name: site_param
    type: string
    default: "{vars.site}"
  - name: prod_secret
    type: string
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
        let render = || {
            req_post_json(
                "/api/render/label",
                &serde_json::json!({ "template": "i262_when_gate", "data": { "prod_secret": "S" } })
                    .to_string(),
            )
        };

        // 1. Without the variable, every render is a template fault.
        let res = app.clone().oneshot(render()).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_json(res).await;
        assert_eq!(body["error"]["details"]["reason"], "reference_unresolved");
        assert_eq!(body["error"]["details"]["field"], "vars.site");

        // (guard) 2. site=staging -> container inactive.
        state.store().set_variable("site", "staging").await.unwrap();
        let res = app.clone().oneshot(render()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let staging = body_bytes(res).await;

        // 3. site=production -> container active: the gate opened, so the label differs.
        state
            .store()
            .set_variable("site", "production")
            .await
            .unwrap();
        let res = app.clone().oneshot(render()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_ne!(body_bytes(res).await, staging);
    }

    #[tokio::test]
    async fn issue_262_sys_now_format_resolves_against_settings() {
        let yaml = r#"
name: Custom Format Date
unit: mm
dpi: 200
params:
  - name: d
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
        let resolved_d = detail["params"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == "d")
            .unwrap()["default"]
            .as_str()
            .unwrap();
        assert!(
            resolved_d.contains('/'),
            "expected formatted date with slashes, got: {resolved_d}"
        );
    }

    #[test]
    fn issue_262_boolean_yes_and_suffixed_length_defaults_are_load_refusals() {
        let template = |param: &str| {
            format!(
                "name: Coercion Test\nunit: mm\ndpi: 200\nparams:\n{param}\nformat: {{ type: single, width: 100, height: 50 }}\nlayout: []\n"
            )
        };
        for (param, path) in [
            (
                "  - name: b_bad\n    type: boolean\n    default: \"yes\"",
                "params.b_bad.default",
            ),
            (
                "  - name: l_bad\n    type: length\n    default: \"80mm\"",
                "params.l_bad.default",
            ),
        ] {
            let err = crate::parse::parse_template(&template(param))
                .expect_err("the default must be refused at load")
                .to_string();
            assert!(err.contains(path), "{path}: {err}");
        }
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
            .method("POST")
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
            "/api/render",
            &serde_json::json!({
                "template": "bad_content_circle",
                "labels": [
                    { "data": {} }
                ]
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
        assert_eq!(failures[0]["details"]["reason"], "circle_box_not_square");
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

    /// Task 1.5: HTTP tests asserting emitted lines and status for wrap: true over-wide word.
    /// 1. Shrink-to-fit case: 200 OK, renders whole on one line at the reduced font size.
    /// 2. Floor fail case: 422 Unprocessable Entity with reason text_does_not_fit.
    #[tokio::test]
    async fn http_render_wrap_true_over_wide_word_shrink_and_floor_fail() {
        let shrink_yaml = r#"
name: wrap_shrink_test
unit: mm
dpi: 200
format: { type: single, width: 30, height: 20 }
layout:
  - type: text
    value: "Refrigeration"
    at: [0, 0]
    size: [30, 20]
    wrap: true
    font_size: { min: 6, max: 20 }
    overflow: fail
"#;

        let fail_yaml = r#"
name: wrap_floor_fail_test
unit: mm
dpi: 200
format: { type: single, width: 15, height: 20 }
layout:
  - type: text
    value: "Refrigeration"
    at: [0, 0]
    size: [15, 20]
    wrap: true
    font_size: { min: 14, max: 20 }
    overflow: fail
"#;

        let (app, _state) = test_app_with_custom_templates(vec![
            ("wrap_shrink_test", shrink_yaml),
            ("wrap_floor_fail_test", fail_yaml),
        ]);

        // 1. Shrink-to-fit: status 200 OK, rendered PNG has exactly 1 ink band (word kept whole on 1 line)
        let req_shrink = req_post_json(
            "/api/render/label?format=png",
            &serde_json::json!({
                "template": "wrap_shrink_test",
                "data": {}
            })
            .to_string(),
        );
        let res_shrink = app.clone().oneshot(req_shrink).await.unwrap();
        assert_eq!(res_shrink.status(), StatusCode::OK);
        let png_bytes = body_bytes(res_shrink).await;

        let count_ink_bands = |png: &[u8]| -> usize {
            let img = image::load_from_memory(png).expect("decode").to_luma8();
            let (w, h) = (img.width(), img.height());
            let mut bands = 0;
            let mut inside = false;
            for y in 0..h {
                let inked = (0..w).any(|x| img.get_pixel(x, y).0[0] < 128);
                if inked && !inside {
                    bands += 1;
                }
                inside = inked;
            }
            bands
        };

        let bands = count_ink_bands(&png_bytes);
        assert_eq!(
            bands, 1,
            "shrink-to-fit must render word whole on 1 line (1 ink band), got {bands}"
        );

        // 2. Floor fail: status 422 Unprocessable Entity, reason text_does_not_fit
        let req_fail = req_post_json(
            "/api/render/label?format=png",
            &serde_json::json!({
                "template": "wrap_floor_fail_test",
                "data": {}
            })
            .to_string(),
        );
        let res_fail = app.clone().oneshot(req_fail).await.unwrap();
        assert_eq!(res_fail.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_json(res_fail).await;
        assert_eq!(body["error"]["code"], "UnsupportedLayoutItem");
        assert_eq!(body["error"]["details"]["reason"], "text_does_not_fit");
    }

    #[tokio::test]
    async fn issue_364_line_spacing_endpoint_render_measured_tests() {
        let ink_rows_helper = |png: &[u8]| -> (u32, u32) {
            let img = image::load_from_memory(png).expect("decode").to_luma8();
            let (w, h) = (img.width(), img.height());
            let inked: Vec<u32> = (0..h)
                .filter(|&y| (0..w).any(|x| img.get_pixel(x, y).0[0] < 128))
                .collect();
            assert!(!inked.is_empty(), "rendered label has no ink");
            (inked[0], inked[inked.len() - 1])
        };

        let line1_ink_height = |png: &[u8]| -> u32 {
            let img = image::load_from_memory(png).expect("decode").to_luma8();
            let (w, h) = (img.width(), img.height());
            let mut line1_top = None;
            let mut line1_bottom = None;
            let mut inside_band = false;
            for y in 0..h {
                let has_ink = (0..w).any(|x| img.get_pixel(x, y).0[0] < 128);
                if has_ink {
                    if !inside_band {
                        inside_band = true;
                        if line1_top.is_none() {
                            line1_top = Some(y);
                        }
                    }
                } else if inside_band {
                    line1_bottom = Some(y - 1);
                    break;
                }
            }
            line1_bottom.expect("line1 bottom") - line1_top.expect("line1 top") + 1
        };

        let tpl_pitch_num = r#"
name: tpl_pitch_num
unit: mm
dpi: 180
format: { type: single, width: 100, height: 60 }
params:
  - name: pitch
    type: number
  - name: text
    type: string
    default: "Hxy\nHxy"
layout:
  - type: text
    value: "{text}"
    at: [0, 0]
    size: [100, 60]
    font_size: 20
    line_spacing: "{pitch}"
"#;

        let tpl_pitch_int = r#"
name: tpl_pitch_int
unit: mm
dpi: 180
format: { type: single, width: 100, height: 60 }
params:
  - name: pitch
    type: integer
  - name: text
    type: string
    default: "Hxy\nHxy"
layout:
  - type: text
    value: "{text}"
    at: [0, 0]
    size: [100, 60]
    font_size: 20
    line_spacing: "{pitch}"
"#;

        let tpl_pitch_default = r#"
name: tpl_pitch_default
unit: mm
dpi: 180
format: { type: single, width: 100, height: 60 }
params:
  - name: pitch
    type: number
    default: 0.99
  - name: text
    type: string
    default: "Hxy\nHxy"
layout:
  - type: text
    value: "{text}"
    at: [0, 0]
    size: [100, 60]
    font_size: 20
    line_spacing: "{pitch}"
"#;

        let tpl_pitch_shrink = r#"
name: tpl_pitch_shrink
unit: mm
dpi: 180
format: { type: single, width: 100, height: 10 }
params:
  - name: pitch
    type: number
layout:
  - type: text
    value: "Hxy\nHxy"
    at: [0, 0]
    size: [100, 10]
    font_size: { min: 8, max: 24 }
    line_spacing: "{pitch}"
"#;

        let (app, _state) = test_app_with_custom_templates(vec![
            ("tpl_pitch_num", tpl_pitch_num),
            ("tpl_pitch_int", tpl_pitch_int),
            ("tpl_pitch_default", tpl_pitch_default),
            ("tpl_pitch_shrink", tpl_pitch_shrink),
        ]);

        let render_pitch_png = |template: &str, data: serde_json::Value| {
            let app = app.clone();
            let template = template.to_string();
            async move {
                let req = req_post_json(
                    "/api/render/label?format=png",
                    &serde_json::json!({
                        "template": template,
                        "data": data
                    })
                    .to_string(),
                );
                let res = app.oneshot(req).await.unwrap();
                assert_eq!(res.status(), StatusCode::OK);
                body_bytes(res).await
            }
        };

        // Task 6.1: Render one template declaring line_spacing: "{pitch}" twice (0.99 and 1.5)
        let png_1line_099 = render_pitch_png(
            "tpl_pitch_num",
            serde_json::json!({ "pitch": 0.99, "text": "Hxy" }),
        )
        .await;
        let png_2line_099 = render_pitch_png(
            "tpl_pitch_num",
            serde_json::json!({ "pitch": 0.99, "text": "Hxy\nHxy" }),
        )
        .await;
        let (_, bottom1_099) = ink_rows_helper(&png_1line_099);
        let (_, bottom2_099) = ink_rows_helper(&png_2line_099);
        let pitch_px_099 = (bottom2_099 - bottom1_099) as f32;
        let drift_099 = (pitch_px_099 - 0.99 * 50.0).abs();
        assert!(
            drift_099 <= 1.0,
            "pitch 0.99: measured {pitch_px_099}px, expected 49.5px (drift {drift_099}px)"
        );

        let png_1line_15 = render_pitch_png(
            "tpl_pitch_num",
            serde_json::json!({ "pitch": 1.5, "text": "Hxy" }),
        )
        .await;
        let png_2line_15 = render_pitch_png(
            "tpl_pitch_num",
            serde_json::json!({ "pitch": 1.5, "text": "Hxy\nHxy" }),
        )
        .await;
        let (_, bottom1_15) = ink_rows_helper(&png_1line_15);
        let (_, bottom2_15) = ink_rows_helper(&png_2line_15);
        let pitch_px_15 = (bottom2_15 - bottom1_15) as f32;
        let drift_15 = (pitch_px_15 - 1.5 * 50.0).abs();
        assert!(
            drift_15 <= 1.0,
            "pitch 1.5: measured {pitch_px_15}px, expected 75.0px (drift {drift_15}px)"
        );

        // Task 6.7: integer-typed pitch 2 renders with ink bands 2 font sizes apart
        let png_1line_2 = render_pitch_png(
            "tpl_pitch_int",
            serde_json::json!({ "pitch": 2, "text": "Hxy" }),
        )
        .await;
        let png_2line_2 = render_pitch_png(
            "tpl_pitch_int",
            serde_json::json!({ "pitch": 2, "text": "Hxy\nHxy" }),
        )
        .await;
        let (_, bottom1_2) = ink_rows_helper(&png_1line_2);
        let (_, bottom2_2) = ink_rows_helper(&png_2line_2);
        let pitch_px_2 = (bottom2_2 - bottom1_2) as f32;
        let drift_2 = (pitch_px_2 - 2.0 * 50.0).abs();
        assert!(
            drift_2 <= 1.0,
            "integer pitch 2: measured {pitch_px_2}px, expected 100.0px (drift {drift_2}px)"
        );

        // Task 6.12: pitch parameter declaring default: 0.99, omitted at request, renders at 0.99 font sizes
        let png_1line_def =
            render_pitch_png("tpl_pitch_default", serde_json::json!({ "text": "Hxy" })).await;
        let png_2line_def = render_pitch_png(
            "tpl_pitch_default",
            serde_json::json!({ "text": "Hxy\nHxy" }),
        )
        .await;
        let (_, bottom1_def) = ink_rows_helper(&png_1line_def);
        let (_, bottom2_def) = ink_rows_helper(&png_2line_def);
        let pitch_px_def = (bottom2_def - bottom1_def) as f32;
        let drift_def = (pitch_px_def - 0.99 * 50.0).abs();
        assert!(
            drift_def <= 1.0,
            "default pitch 0.99: measured {pitch_px_def}px, expected 49.5px (drift {drift_def}px)"
        );

        // Task 6.3: height-bound two-line item with range font_size: tighter pitch (0.99) settles at larger size than looser (1.5)
        let png_shrink_099 =
            render_pitch_png("tpl_pitch_shrink", serde_json::json!({ "pitch": 0.99 })).await;
        let png_shrink_15 =
            render_pitch_png("tpl_pitch_shrink", serde_json::json!({ "pitch": 1.5 })).await;
        let h_099 = line1_ink_height(&png_shrink_099);
        let h_15 = line1_ink_height(&png_shrink_15);
        assert!(
            h_099 > h_15,
            "tighter pitch (0.99) line 1 height ({h_099}px) must be greater than looser pitch (1.5) line 1 height ({h_15}px)"
        );
    }

    #[tokio::test]
    async fn issue_364_line_spacing_endpoint_refusal_tests() {
        let tpl_pitch_num = r#"
name: tpl_pitch_num_refusal
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  - name: pitch
    type: number
    default: 1.2
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    line_spacing: "{pitch}"
"#;

        let tpl_pitch_int = r#"
name: tpl_pitch_int_refusal
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  - name: pitch
    type: integer
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    line_spacing: "{pitch}"
"#;

        let tpl_pitch_int_overflow_fail = r#"
name: tpl_pitch_int_overflow_fail
unit: mm
dpi: 200
format: { type: single, width: 50, height: 10 }
params:
  - name: pitch
    type: integer
layout:
  - type: text
    value: "Hxy\nHxy"
    at: [0, 0]
    size: [50, 10]
    font_size: 10
    line_spacing: "{pitch}"
    overflow: fail
"#;

        let tpl_pitch_def_zero = r#"
name: tpl_pitch_def_zero
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  - name: pitch
    type: number
    default: 0
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    line_spacing: "{pitch}"
"#;

        let tpl_pitch_def_nan = r#"
name: tpl_pitch_def_nan
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  - name: pitch
    type: number
    default: .nan
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    line_spacing: "{pitch}"
"#;

        let (app, _state) = test_app_with_custom_templates(vec![
            ("tpl_pitch_num_refusal", tpl_pitch_num),
            ("tpl_pitch_int_refusal", tpl_pitch_int),
            ("tpl_pitch_int_overflow_fail", tpl_pitch_int_overflow_fail),
            ("tpl_pitch_def_zero", tpl_pitch_def_zero),
        ]);

        let post_render = |template: &str, data: serde_json::Value| {
            let app = app.clone();
            let template = template.to_string();
            async move {
                let req = req_post_json(
                    "/api/render/label?format=png",
                    &serde_json::json!({
                        "template": template,
                        "data": data
                    })
                    .to_string(),
                );
                app.oneshot(req).await.unwrap()
            }
        };

        // Task 6.4: supplied 0 and -0.5 are refused with 400 InvalidRequest, line_spacing_param_invalid, message naming layout path and pitch
        for bad_pitch in [serde_json::json!(0), serde_json::json!(-0.5)] {
            let res = post_render(
                "tpl_pitch_num_refusal",
                serde_json::json!({ "pitch": bad_pitch }),
            )
            .await;
            assert_eq!(res.status(), StatusCode::BAD_REQUEST);
            let body = body_json(res).await;
            assert_eq!(body["error"]["code"], "InvalidRequest");
            assert_eq!(
                body["error"]["details"]["reason"],
                "line_spacing_param_invalid"
            );
            let msg = body["error"]["message"].as_str().unwrap();
            assert!(
                msg.contains("layout[0]") && msg.contains("pitch"),
                "message must name layout[0] and pitch: {msg}"
            );
        }

        // Task 6.5: supplied value numeric resolution cannot read as a number is refused with 400, param_value_invalid, naming pitch, not line_spacing_param_invalid
        let res = post_render(
            "tpl_pitch_num_refusal",
            serde_json::json!({ "pitch": "invalid_num" }),
        )
        .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = body_json(res).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
        assert_eq!(body["error"]["details"]["reason"], "param_value_invalid");
        let msg = body["error"]["message"].as_str().unwrap();
        assert!(msg.contains("pitch"), "message must name pitch: {msg}");

        // Task 6.6: a supplied non-finite number is refused with 400, param_value_invalid, naming pitch
        for bad_num in [serde_json::json!("NaN"), serde_json::json!("inf")] {
            let res = post_render(
                "tpl_pitch_num_refusal",
                serde_json::json!({ "pitch": bad_num }),
            )
            .await;
            assert_eq!(res.status(), StatusCode::BAD_REQUEST);
            let body = body_json(res).await;
            assert_eq!(body["error"]["code"], "InvalidRequest");
            assert_eq!(body["error"]["details"]["reason"], "param_value_invalid");
            assert_eq!(body["error"]["details"]["param"], "pitch");
        }

        // Tasks 6.8 and 6.9: an integer-typed pitch supplied as a whole JSON number outside the
        // i64 range is not an integer: 400 param_value_invalid, not a saturated pitch
        for out_of_range in [serde_json::json!(1e300), serde_json::json!(-1e300)] {
            let res = post_render(
                "tpl_pitch_int_overflow_fail",
                serde_json::json!({ "pitch": out_of_range }),
            )
            .await;
            assert_eq!(res.status(), StatusCode::BAD_REQUEST);
            let body = body_json(res).await;
            assert_eq!(body["error"]["details"]["reason"], "param_value_invalid");
            assert_eq!(body["error"]["details"]["param"], "pitch");
        }

        // Task 6.10: integer-typed pitch supplied as string "9223372036854775808" fails integer parsing -> 400 param_value_invalid
        let res = post_render(
            "tpl_pitch_int_refusal",
            serde_json::json!({ "pitch": "9223372036854775808" }),
        )
        .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = body_json(res).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
        assert_eq!(body["error"]["details"]["reason"], "param_value_invalid");
        let msg = body["error"]["message"].as_str().unwrap();
        assert!(msg.contains("pitch"), "message must name pitch: {msg}");

        // Task 6.13: pitch parameter declaring default: 0 loads and is served, and omitting pitch is refused with 400 line_spacing_param_invalid
        let res = post_render("tpl_pitch_def_zero", serde_json::json!({})).await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = body_json(res).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
        assert_eq!(
            body["error"]["details"]["reason"],
            "line_spacing_param_invalid"
        );
        let msg = body["error"]["message"].as_str().unwrap();
        assert!(
            msg.contains("layout[0]") && msg.contains("pitch"),
            "message must name layout[0] and pitch: {msg}"
        );

        // Task 6.14: a pitch parameter declaring default: .nan is refused at load
        let err = crate::parse::parse_template(tpl_pitch_def_nan)
            .unwrap_err()
            .to_string();
        assert!(err.contains("params.pitch.default"), "{err}");
    }

    #[tokio::test]
    async fn issue_364_line_spacing_batch_tests() {
        let tpl_pitch_num = r#"
name: tpl_pitch_batch
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  - name: pitch
    type: number
    default: 1.2
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    line_spacing: "{pitch}"
"#;

        let tpl_pitch_def_nan = r#"
name: tpl_pitch_batch_nan
unit: mm
dpi: 200
format: { type: single, width: 50, height: 20 }
params:
  - name: pitch
    type: number
    default: .nan
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [50, 20]
    font_size: 10
    line_spacing: "{pitch}"
"#;

        let (app, _state) =
            test_app_with_custom_templates(vec![("tpl_pitch_batch", tpl_pitch_num)]);

        // Task 7.1: 3 labels from template declaring line_spacing: "{pitch}",
        // 1st and 3rd supplying usable pitch (1.2, 1.5) and 2nd supplying 0 ->
        // refuse whole request with 422 BatchInvalid, exactly 1 failures entry at index 1
        // with code InvalidRequest and reason line_spacing_param_invalid naming layout path and pitch.
        let req1 = req_post_json(
            "/api/render",
            &serde_json::json!({
                "template": "tpl_pitch_batch",
                "labels": [
                    { "data": { "pitch": 1.2 } },
                    { "data": { "pitch": 0 } },
                    { "data": { "pitch": 1.5 } }
                ]
            })
            .to_string(),
        );
        let res1 = app.clone().oneshot(req1).await.unwrap();
        assert_eq!(res1.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body1 = body_json(res1).await;
        assert_eq!(body1["error"]["code"], "BatchInvalid");
        let failures1 = body1["error"]["details"]["failures"].as_array().unwrap();
        assert_eq!(failures1.len(), 1);
        assert_eq!(failures1[0]["index"], 1);
        assert_eq!(failures1[0]["code"], "InvalidRequest");
        assert_eq!(
            failures1[0]["details"]["reason"],
            "line_spacing_param_invalid"
        );
        let msg1 = failures1[0]["message"].as_str().unwrap();
        assert!(
            msg1.contains("layout[0]") && msg1.contains("pitch"),
            "message must name layout[0] and pitch: {msg1}"
        );

        // Task 7.3: a template declaring default: .nan is refused at load
        let err = crate::parse::parse_template(tpl_pitch_def_nan)
            .unwrap_err()
            .to_string();
        assert!(err.contains("params.pitch.default"), "{err}");
    }

    // Issue 235: dynamic width max resolved below min tests
    #[tokio::test]
    async fn http_render_dynamic_width_max_below_min_repro_and_ordered_cases() {
        let repro_yaml = r#"
name: Issue 235 Repro
unit: mm
dpi: 200
format:
  type: single
  width:
    min: 10.0
    max: "{max_width}"
  height: 18.1
params:
  - name: max_width
    type: length
    default: 120.0
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [5, 5]
    font_size: 8
"#;

        let (app, _state) = test_app_with_custom_templates(vec![("issue_235_repro", repro_yaml)]);

        let post_render = |data: serde_json::Value| {
            let app = app.clone();
            async move {
                let req = req_post_json(
                    "/api/render/label?format=png",
                    &serde_json::json!({
                        "template": "issue_235_repro",
                        "data": data
                    })
                    .to_string(),
                );
                app.oneshot(req).await.unwrap()
            }
        };

        // Task 1.1: max_width: 5 -> 400 InvalidRequest, width_bounds_inverted, message naming max_width, 5, 10
        // Against unchanged tree, this fails with 500 Internal (panic envelope).
        let res = post_render(serde_json::json!({ "max_width": 5 })).await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body = body_json(res).await;
        assert_eq!(body["error"]["code"], "InvalidRequest");
        assert_eq!(body["error"]["details"]["reason"], "width_bounds_inverted");
        let msg = body["error"]["message"].as_str().unwrap();
        assert!(
            msg.contains("max_width") && msg.contains("5") && msg.contains("10"),
            "message must name max_width, 5, and 10: {msg}"
        );

        // GET /api/health on the same app asserts 200 (service kept serving)
        let health_res = app.clone().oneshot(req_get("/api/health")).await.unwrap();
        assert_eq!(health_res.status(), StatusCode::OK);

        // Task 1.2: ordered cases against repro template
        // max_width: 10 asserts 200 (equal bounds render)
        let res_equal = post_render(serde_json::json!({ "max_width": 10 })).await;
        assert_eq!(res_equal.status(), StatusCode::OK);

        // max_width: 2000 exceeds the 1000 mm limit while staying above min
        let res_big = post_render(serde_json::json!({ "max_width": 2000 })).await;
        assert_eq!(res_big.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body_big = body_json(res_big).await;
        assert_eq!(body_big["error"]["code"], "UnsupportedLayoutItem");
        assert_eq!(
            body_big["error"]["details"]["reason"],
            "dimension_exceeds_limit"
        );
    }

    #[tokio::test]
    async fn http_render_dynamic_width_other_reference_shapes() {
        let tpl_min_ref = r#"
name: Min Ref
unit: mm
dpi: 200
format:
  type: single
  width:
    min: "{min_width}"
    max: 60.0
  height: 18.1
params:
  - name: min_width
    type: length
    default: 10.0
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [5, 5]
    font_size: 8
"#;

        let tpl_both_refs = r#"
name: Both Refs
unit: mm
dpi: 200
format:
  type: single
  width:
    min: "{lo}"
    max: "{hi}"
  height: 18.1
params:
  - name: lo
    type: length
    default: 10.0
  - name: hi
    type: length
    default: 50.0
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [5, 5]
    font_size: 8
"#;

        let (app, _state) = test_app_with_custom_templates(vec![
            ("tpl_min_ref", tpl_min_ref),
            ("tpl_both_refs", tpl_both_refs),
        ]);

        // min_width: 80 -> 400 width_bounds_inverted naming min_width, 80 and 60
        // Against unchanged tree, fails with 500 (panic envelope).
        let req1 = req_post_json(
            "/api/render/label?format=png",
            &serde_json::json!({
                "template": "tpl_min_ref",
                "data": { "min_width": 80 }
            })
            .to_string(),
        );
        let res1 = app.clone().oneshot(req1).await.unwrap();
        assert_eq!(res1.status(), StatusCode::BAD_REQUEST);
        let body1 = body_json(res1).await;
        assert_eq!(body1["error"]["code"], "InvalidRequest");
        assert_eq!(body1["error"]["details"]["reason"], "width_bounds_inverted");
        let msg1 = body1["error"]["message"].as_str().unwrap();
        assert!(
            msg1.contains("min_width") && msg1.contains("80") && msg1.contains("60"),
            "message must contain min_width, 80, and 60: {msg1}"
        );

        // lo: 30, hi: 20 -> 400 width_bounds_inverted naming lo, hi, 30 and 20
        // Against unchanged tree, fails with 500 (panic envelope).
        let req2 = req_post_json(
            "/api/render/label?format=png",
            &serde_json::json!({
                "template": "tpl_both_refs",
                "data": { "lo": 30, "hi": 20 }
            })
            .to_string(),
        );
        let res2 = app.clone().oneshot(req2).await.unwrap();
        assert_eq!(res2.status(), StatusCode::BAD_REQUEST);
        let body2 = body_json(res2).await;
        assert_eq!(body2["error"]["code"], "InvalidRequest");
        assert_eq!(body2["error"]["details"]["reason"], "width_bounds_inverted");
        let msg2 = body2["error"]["message"].as_str().unwrap();
        assert!(
            msg2.contains("lo")
                && msg2.contains("hi")
                && msg2.contains("30")
                && msg2.contains("20"),
            "message must contain lo, hi, 30, and 20: {msg2}"
        );
    }

    #[tokio::test]
    async fn http_render_dynamic_width_defaults() {
        let tpl_defaults = r#"
name: Defaults Inverted
unit: mm
dpi: 200
format:
  type: single
  width:
    min: 10.0
    max: "{max_width}"
  height: 18.1
params:
  - name: max_width
    type: length
    default: 5.0
layout:
  - type: text
    value: "Hi"
    at: [0, 0]
    size: [4, 4]
    font_size: 8
"#;

        let (app, _state) =
            test_app_with_custom_templates(vec![("tpl_defaults_inverted", tpl_defaults)]);

        // Omitted max_width uses default 5.0 -> 400 width_bounds_inverted naming max_width, 5, 10
        // Against unchanged tree, fails with 500 (panic envelope).
        let req1 = req_post_json(
            "/api/render/label?format=png",
            &serde_json::json!({
                "template": "tpl_defaults_inverted",
                "data": {}
            })
            .to_string(),
        );
        let res1 = app.clone().oneshot(req1).await.unwrap();
        assert_eq!(res1.status(), StatusCode::BAD_REQUEST);
        let body1 = body_json(res1).await;
        assert_eq!(body1["error"]["code"], "InvalidRequest");
        assert_eq!(body1["error"]["details"]["reason"], "width_bounds_inverted");
        let msg1 = body1["error"]["message"].as_str().unwrap();
        assert!(
            msg1.contains("max_width") && msg1.contains("5") && msg1.contains("10"),
            "message must contain max_width, 5, and 10: {msg1}"
        );

        // Supplying max_width: 20 -> 200 OK
        let req2 = req_post_json(
            "/api/render/label?format=png",
            &serde_json::json!({
                "template": "tpl_defaults_inverted",
                "data": { "max_width": 20 }
            })
            .to_string(),
        );
        let res2 = app.clone().oneshot(req2).await.unwrap();
        assert_eq!(res2.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn http_render_dynamic_width_precedence_over_measurement() {
        let tpl_precedence = r#"
name: Precedence
unit: mm
dpi: 200
format:
  type: single
  width:
    min: 10.0
    max: "{max_width}"
  height: 18.1
params:
  - name: max_width
    type: length
    default: 120.0
  - name: w
    type: length
    default: 5.0
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: ["{w}", 5]
    font_size: 8
"#;

        let (app, _state) =
            test_app_with_custom_templates(vec![("tpl_precedence", tpl_precedence)]);

        // max_width: 40, w: 0 asserts 422 UnsupportedLayoutItem, size_invalid
        let req2 = req_post_json(
            "/api/render/label?format=png",
            &serde_json::json!({
                "template": "tpl_precedence",
                "data": { "max_width": 40, "w": 0 }
            })
            .to_string(),
        );
        let res2 = app.clone().oneshot(req2).await.unwrap();
        assert_eq!(res2.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body2 = body_json(res2).await;
        assert_eq!(body2["error"]["code"], "UnsupportedLayoutItem");
        assert_eq!(body2["error"]["details"]["reason"], "size_invalid");
    }

    #[tokio::test]
    async fn http_batch_dynamic_width_inverted_bounds() {
        let repro_yaml = r#"
name: Issue 235 Batch Repro
unit: mm
dpi: 200
format:
  type: single
  width:
    min: 10.0
    max: "{max_width}"
  height: 18.1
params:
  - name: max_width
    type: length
    default: 120.0
layout:
  - type: text
    value: "Hello"
    at: [0, 0]
    size: [5, 5]
    font_size: 8
"#;

        let (app, _state) = test_app_with_custom_templates(vec![("issue_235_batch", repro_yaml)]);

        // POST /api/render with two labels: max_width: 40 then max_width: 5
        // assert status 422, JSON content-type, error.code BatchInvalid, and
        // error.details.failures holding exactly one entry at index 1 with code InvalidRequest, reason width_bounds_inverted.
        // Against unchanged tree, fails with 500 (panic envelope).
        let req = req_post_json(
            "/api/render",
            &serde_json::json!({
                "template": "issue_235_batch",
                "labels": [
                    { "data": { "max_width": 40 } },
                    { "data": { "max_width": 5 } }
                ]
            })
            .to_string(),
        );
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let content_type = res
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        assert!(
            content_type.contains("application/json"),
            "content-type must be json, got {content_type}"
        );
        let body = body_json(res).await;
        assert_eq!(body["error"]["code"], "BatchInvalid");
        let failures = body["error"]["details"]["failures"].as_array().unwrap();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0]["index"], 1);
        assert_eq!(failures[0]["code"], "InvalidRequest");
        assert_eq!(failures[0]["details"]["reason"], "width_bounds_inverted");
    }
}

/// Parameters contract (#413): published `control` and `default`, literal defaults judged at load,
/// supplied-value coercion, and template-environment faults (`reference_unresolved`).
#[cfg(test)]
mod parameters_http_tests {
    use super::store::Store;
    use super::{app, AppState, TemplateRegistry};
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use serde_json::{json, Value};
    use std::sync::Arc;
    use tower::ServiceExt;

    /// A fresh templates folder holding `templates` as `<id>.yaml`, loaded through the registry so
    /// a refused file is quarantined exactly as at startup. Auth is off.
    fn app_with(templates: &[(&str, &str)]) -> (axum::Router, Arc<AppState>, std::path::PathBuf) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "labeler-params-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("create templates dir");
        for (id, yaml) in templates {
            std::fs::write(dir.join(format!("{id}.yaml")), yaml).expect("write template");
        }
        let registry = TemplateRegistry::load_from_dir(&dir).expect("load templates");
        let store = Store::open_in_memory().expect("store");
        let state = Arc::new(AppState::new(registry, dir.clone(), store).with_no_auth(true));
        (app(state.clone()), state, dir)
    }

    /// The error `<id>.yaml` in `dir` is quarantined with, or `None` when it loads.
    fn quarantine_error(dir: &std::path::Path, id: &str) -> Option<String> {
        let registry = TemplateRegistry::load_from_dir(dir).expect("load templates");
        registry
            .broken()
            .iter()
            .find(|b| b.path == format!("{id}.yaml"))
            .map(|b| b.error.clone())
    }

    async fn send(
        app: &axum::Router,
        method: &str,
        uri: &str,
        content_type: &str,
        body: String,
    ) -> (StatusCode, Vec<u8>) {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", content_type)
            .body(Body::from(body))
            .unwrap();
        let res = app.clone().oneshot(req).await.expect("request");
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), 64 * 1024 * 1024)
            .await
            .expect("collect body")
            .to_vec();
        (status, bytes)
    }

    /// Send a JSON body (written out verbatim, so a test can spell `1e0`) and read a JSON answer;
    /// a non-JSON answer reads as `null`.
    async fn send_json(
        app: &axum::Router,
        method: &str,
        uri: &str,
        body: &str,
    ) -> (StatusCode, Value) {
        let (status, bytes) = send(app, method, uri, "application/json", body.to_string()).await;
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn get_json(app: &axum::Router, uri: &str) -> (StatusCode, Value) {
        let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
        let res = app.clone().oneshot(req).await.expect("request");
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), 64 * 1024 * 1024)
            .await
            .expect("collect body");
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn render(app: &axum::Router, template: &str, data: &str) -> (StatusCode, Value) {
        send_json(
            app,
            "POST",
            "/api/render/label",
            &format!(r#"{{"template": "{template}", "data": {data}}}"#),
        )
        .await
    }

    /// Render and return the raw answer, so a test can compare images.
    async fn render_bytes(app: &axum::Router, template: &str, data: &str) -> (StatusCode, Vec<u8>) {
        let body = format!(r#"{{"template": "{template}", "data": {data}}}"#);
        send(app, "POST", "/api/render/label", "application/json", body).await
    }

    /// A test's failed expectations, collected so one run reports every failing case.
    #[derive(Default)]
    struct Misses(Vec<String>);

    impl Misses {
        fn expect(&mut self, ok: bool, case: &str, body: &Value) {
            if !ok {
                self.0.push(format!("{case}: {body}"));
            }
        }

        fn reference_unresolved(
            &mut self,
            status: StatusCode,
            body: &Value,
            field: &str,
            case: &str,
        ) {
            let ok = status == StatusCode::UNPROCESSABLE_ENTITY
                && body["error"]["code"] == "TemplateInvalid"
                && body["error"]["details"]["reason"] == "reference_unresolved"
                && body["error"]["details"]["field"] == field;
            self.expect(ok, &format!("{case} (status {status})"), body);
        }

        fn param_value_invalid(
            &mut self,
            status: StatusCode,
            body: &Value,
            param: &str,
            case: &str,
        ) {
            let ok = status == StatusCode::BAD_REQUEST
                && body["error"]["code"] == "InvalidRequest"
                && body["error"]["details"]["reason"] == "param_value_invalid"
                && body["error"]["details"]["param"] == param;
            self.expect(ok, &format!("{case} (status {status})"), body);
        }

        fn ok(&mut self, status: StatusCode, body: &Value, case: &str) {
            self.expect(
                status == StatusCode::OK,
                &format!("{case} (status {status})"),
                body,
            );
        }

        fn assert_none(self) {
            assert!(
                self.0.is_empty(),
                "{} failing case(s):\n{}",
                self.0.len(),
                self.0.join("\n")
            );
        }
    }

    /// A one-text single label whose `params:` block is `params` and whose layout is `layout`
    /// (both already indented as YAML sequence items).
    fn single(params: &str, layout: &str) -> String {
        format!(
            "name: T\nunit: mm\ndpi: 100\nparams:\n{params}\nformat: {{ type: single, width: 60, height: 20 }}\nlayout:\n{layout}\n"
        )
    }

    fn text(value: &str) -> String {
        format!(
            "  - type: text\n    value: \"{value}\"\n    at: [0, 0]\n    size: [60, 10]\n    font_size: 6\n"
        )
    }

    /// A container no label activates (`gate` defaults to `off`), holding one text item.
    fn inactive_text(value: &str) -> String {
        format!(
            "  - type: container\n    when: {{ gate: shown }}\n    at: [0, 10]\n    size: [60, 10]\n    items:\n      - type: text\n        value: \"{value}\"\n        at: [0, 0]\n        size: [60, 10]\n        font_size: 6\n"
        )
    }

    const GATE_PARAM: &str =
        "  - name: gate\n    type: enum\n    values: [shown, hidden]\n    default: hidden\n";

    fn param_entry<'a>(params: &'a Value, name: &str) -> &'a Value {
        params
            .as_array()
            .unwrap_or_else(|| panic!("params is not an array: {params}"))
            .iter()
            .find(|p| p["name"] == name)
            .unwrap_or_else(|| panic!("no param '{name}' in {params}"))
    }

    // A1
    #[tokio::test]
    async fn list_and_detail_publish_each_parameters_control() {
        let params = "  - name: t\n    type: string\n  - name: ta\n    type: string\n    multiline: true\n  - name: i\n    type: integer\n  - name: n\n    type: number\n  - name: len\n    type: length\n  - name: b\n    type: boolean\n  - name: e\n    type: enum\n    values: [a, z]\n  - name: d\n    type: datetime\n  - name: dt\n    type: datetime\n    time: true\n  - name: l\n    type: list\n  - name: photo\n    type: string\n  - name: icon\n    type: string\n"
            .to_string()
            + GATE_PARAM;
        let layout = text("{t}")
            + "  - type: container\n    when: { gate: shown }\n    at: [0, 10]\n    size: [20, 10]\n    items:\n      - type: image\n        src: \"{photo}\"\n        at: [0, 0]\n        size: [10, 10]\n"
            + "  - type: image\n    src: \"x/{icon}.png\"\n    at: [40, 10]\n    size: [10, 10]\n";
        let yaml = single(&params, &layout);
        let (app, _state, _dir) = app_with(&[("controls", &yaml)]);
        let expected = [
            ("t", "text"),
            ("ta", "textarea"),
            ("i", "integer"),
            ("n", "number"),
            ("len", "number"),
            ("b", "checkbox"),
            ("e", "select"),
            ("d", "date"),
            ("dt", "datetime"),
            ("l", "list"),
            ("photo", "image"),
            ("icon", "text"),
        ];

        let (status, detail) = get_json(&app, "/api/templates/controls").await;
        assert_eq!(status, StatusCode::OK, "{detail}");
        let (status, list) = get_json(&app, "/api/templates").await;
        assert_eq!(status, StatusCode::OK, "{list}");
        let summary = list["templates"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["id"] == "controls")
            .unwrap_or_else(|| panic!("controls not listed: {list}"));
        for (where_, params) in [("detail", &detail["params"]), ("list", &summary["params"])] {
            for (name, control) in expected {
                assert_eq!(
                    param_entry(params, name)["control"],
                    control,
                    "{where_}: control of '{name}'"
                );
            }
        }
    }

    // A2
    #[tokio::test]
    async fn literal_defaults_and_values_are_judged_at_load_with_a_path() {
        let cases = [
            (
                "bool_yes",
                "  - name: b\n    type: boolean\n    default: \"yes\"\n",
                "params.b.default",
            ),
            (
                "int_frac",
                "  - name: n\n    type: integer\n    default: 2.7\n",
                "params.n.default",
            ),
            (
                "int_near",
                "  - name: n\n    type: integer\n    default: 2.00000001\n",
                "params.n.default",
            ),
            (
                "int_bound",
                "  - name: n\n    type: integer\n    min: 1\n    max: 10\n    default: 11\n",
                "params.n.default",
            ),
            (
                "num_blank",
                "  - name: n\n    type: number\n    default: \"\"\n",
                "params.n.default",
            ),
            (
                "enum_blank",
                "  - name: e\n    type: enum\n    values: [a, \"\"]\n",
                "params.e.values",
            ),
        ];
        let valid = single("  - name: x\n    type: string\n", &text("ok"));
        let mut misses = Misses::default();
        for (id, params, path) in cases {
            let yaml = single(params, &text("fixed"));
            let (app, _state, dir) = app_with(&[(id, &yaml), ("seed", &valid)]);
            let error = quarantine_error(&dir, id);
            misses.expect(
                error.as_deref().is_some_and(|e| e.contains(path)),
                &format!("{id}: quarantined naming {path}"),
                &json!(error),
            );

            let (status, body) = send(&app, "PUT", "/api/templates/seed", "text/yaml", yaml).await;
            let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
            misses.expect(
                status == StatusCode::UNPROCESSABLE_ENTITY
                    && body["error"]["code"] == "TemplateInvalid"
                    && body["error"]["message"]
                        .as_str()
                        .unwrap_or("")
                        .contains(path),
                &format!("{id}: PUT is 422 TemplateInvalid naming {path} (status {status})"),
                &body,
            );
            std::fs::remove_dir_all(dir).ok();
        }
        misses.assert_none();
    }

    const URL_PARAMS: &str = "  - name: url\n    type: string\n    default: \"{vars.base}\"\n";

    // A5
    #[tokio::test]
    async fn an_unresolved_default_fails_even_when_supplied_and_unread() {
        let yaml = single(URL_PARAMS, &text("fixed"));
        let (app, _state, _dir) = app_with(&[("url", &yaml)]);
        let (status, body) = render(&app, "url", r#"{"url": "https://given/"}"#).await;
        let mut misses = Misses::default();
        misses.reference_unresolved(status, &body, "vars.base", "supplied url");
        misses.assert_none();
    }

    // A6
    #[tokio::test]
    async fn an_unset_variable_in_an_inactive_item_fails_the_render() {
        let yaml = single(GATE_PARAM, &(text("fixed") + &inactive_text("{vars.base}")));
        let (app, _state, _dir) = app_with(&[("inactive_var", &yaml)]);
        let (status, body) = render(&app, "inactive_var", "{}").await;
        let mut misses = Misses::default();
        misses.reference_unresolved(status, &body, "vars.base", "inactive vars.base");
        misses.assert_none();
    }

    // A7
    #[tokio::test]
    async fn an_unknown_format_name_in_an_inactive_item_fails_the_render() {
        let params = GATE_PARAM.to_string() + "  - name: printed_on\n    type: datetime\n";
        let cases = [
            (
                "no_such",
                "{sys.now:no_such_format}",
                "no_such_format",
                false,
            ),
            ("seeded_time", "{sys.now:time}", "time", true),
            ("param_fmt", "{printed_on:nope}", "nope", false),
        ];
        let mut misses = Misses::default();
        for (id, token, field, empty_formats) in cases {
            let yaml = single(&params, &(text("fixed") + &inactive_text(token)));
            let (app, state, _dir) = app_with(&[(id, &yaml)]);
            if empty_formats {
                state
                    .store()
                    .set_setting(crate::settings::DATETIME_FORMATS, "{}")
                    .await
                    .unwrap();
            }
            let (status, body) = render(&app, id, "{}").await;
            misses.reference_unresolved(status, &body, field, id);
        }
        misses.assert_none();
    }

    // A8
    #[tokio::test]
    async fn a_tokened_default_its_type_refuses_fails_every_render() {
        let params = "  - name: size\n    type: enum\n    values: [small, large]\n    default: \"{vars.size}\"\n";
        let yaml = single(params, &text("{size}"));
        let (app, state, _dir) = app_with(&[("sized", &yaml)]);
        state.store().set_variable("size", "medium").await.unwrap();
        let mut misses = Misses::default();
        for data in ["{}", r#"{"size": "small"}"#] {
            let (status, body) = render(&app, "sized", data).await;
            misses.reference_unresolved(status, &body, "size", data);
        }
        misses.assert_none();
    }

    fn sheet(params: &str, layout: &str) -> String {
        format!(
            "name: S\nunit: mm\ndpi: 100\nparams:\n{params}\nformat:\n  type: sheet\n  paper_width: 100\n  paper_height: 50\n  label_width: 60\n  label_height: 20\n  positions:\n    - [0, 0]\n    - [0, 25]\nlayout:\n{layout}\n"
        )
    }

    // A8a
    #[tokio::test]
    async fn a_template_fault_answers_the_whole_request_before_any_label() {
        let single_yaml = single(URL_PARAMS, &text("fixed"));
        let sheet_yaml = sheet(URL_PARAMS, &text("fixed"));
        let (app, _state, _dir) =
            app_with(&[("url_single", &single_yaml), ("url_sheet", &sheet_yaml)]);
        let printer = json!({ "id": "p1", "name": "p1", "uri": "ipp://fake.test/" }).to_string();
        let (status, _) = send(&app, "POST", "/api/printers", "application/json", printer).await;
        assert_eq!(status, StatusCode::CREATED);

        let label = r#"{"data": {"url": "https://given/"}}"#;
        let three = format!("[{label}, {label}, {label}]");
        let cases = [
            (
                "single batch",
                "/api/render",
                format!(r#"{{"template": "url_single", "labels": {three}}}"#),
            ),
            (
                "sheet batch",
                "/api/render",
                format!(r#"{{"template": "url_sheet", "labels": {three}}}"#),
            ),
            (
                "print",
                "/api/print",
                r#"{"template": "url_single", "printer": "p1", "labels": [{"data": {"url": "https://given/"}}]}"#
                    .to_string(),
            ),
            (
                "batch with an undeclared key",
                "/api/render",
                format!(
                    r#"{{"template": "url_single", "labels": [{{"data": {{"url": "https://given/", "nope": 1}}}}, {label}]}}"#
                ),
            ),
        ];
        let mut misses = Misses::default();
        for (case, uri, body) in cases {
            let (status, body) = send_json(&app, "POST", uri, &body).await;
            misses.reference_unresolved(status, &body, "vars.base", case);
        }

        let (status, body) = get_json(&app, "/api/templates/url_single/thumbnail").await;
        misses.reference_unresolved(status, &body, "vars.base", "thumbnail");
        misses.assert_none();
    }

    // A8b
    #[tokio::test]
    async fn an_omitted_string_prints_as_empty() {
        let yaml = single("  - name: title\n    type: string\n", &text("{title}"));
        let (app, _state, _dir) = app_with(&[("needs_title", &yaml)]);
        let (status, omitted) = render_bytes(&app, "needs_title", "{}").await;
        assert_eq!(status, StatusCode::OK);
        let (status, empty) = render_bytes(&app, "needs_title", r#"{"title": ""}"#).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(omitted, empty);
    }

    const COERCE_PARAMS: &str = "  - name: copies\n    type: integer\n  - name: title\n    type: string\n  - name: code\n    type: enum\n    values: [\"1\", \"2\"]\n  - name: width\n    type: number\n  - name: bold\n    type: boolean\n";

    // A10
    #[tokio::test]
    async fn values_outside_their_types_form_are_refused() {
        let yaml = single(COERCE_PARAMS, &text("fixed"));
        let (app, _state, _dir) = app_with(&[("coerce", &yaml)]);
        let cases = [
            (r#"{"copies": 2.7}"#, "copies"),
            (r#"{"title": 5}"#, "title"),
            (r#"{"title": ["A", "B"]}"#, "title"),
            (r#"{"code": 1}"#, "code"),
            (r#"{"width": "3in"}"#, "width"),
        ];
        let mut misses = Misses::default();
        for (data, param) in cases {
            let (status, body) = render(&app, "coerce", data).await;
            misses.param_value_invalid(status, &body, param, data);
        }
        misses.assert_none();
    }

    // A11
    #[tokio::test]
    async fn a_boolean_takes_its_numeric_and_padded_spellings() {
        // `bold: true` activates an item reading `title`, so a spelling's image shows which value
        // it became. `bold` defaults to true, so a false spelling misread as an omission draws the
        // true image.
        let params = COERCE_PARAMS.replace(
            "    type: boolean\n",
            "    type: boolean\n    default: true\n",
        );
        let gated = "  - type: container\n    when: { bold: true }\n    at: [0, 10]\n    size: [60, 10]\n    items:\n      - type: text\n        value: \"{title}\"\n        at: [0, 0]\n        size: [60, 10]\n        font_size: 6\n";
        let yaml = single(&params, &(text("fixed") + gated));
        let (app, _state, _dir) = app_with(&[("coerce", &yaml)]);
        let data = |spelling: &str| format!(r#"{{"title": "T", "bold": {spelling}}}"#);
        let (_, true_png) = render_bytes(&app, "coerce", &data("true")).await;
        let (_, false_png) = render_bytes(&app, "coerce", &data("false")).await;
        assert_ne!(true_png, false_png, "the gate must change the image");
        let mut misses = Misses::default();
        for (spellings, reference) in [
            (&["1", "1.0", "1e0", r#""1""#, r#"" true ""#][..], &true_png),
            (&["0", "0.0", r#""0""#, r#"" false ""#][..], &false_png),
        ] {
            for spelling in spellings {
                let (status, png) = render_bytes(&app, "coerce", &data(spelling)).await;
                misses.expect(
                    status == StatusCode::OK && &png == reference,
                    &format!("{spelling} (status {status})"),
                    &Value::Null,
                );
            }
        }
        for spelling in ["2", "0.5"] {
            let (status, body) = render(&app, "coerce", &data(spelling)).await;
            misses.param_value_invalid(status, &body, "bold", spelling);
        }
        misses.assert_none();
    }

    // A12
    #[tokio::test]
    async fn an_invalid_value_only_an_inactive_branch_reads_fails_the_label() {
        let params = GATE_PARAM.to_string() + "  - name: copies\n    type: integer\n";
        let yaml = single(&params, &(text("fixed") + &inactive_text("{copies}")));
        let (app, _state, _dir) = app_with(&[("gated_copies", &yaml)]);
        let (status, body) = render(&app, "gated_copies", r#"{"copies": "many"}"#).await;
        let mut misses = Misses::default();
        misses.param_value_invalid(status, &body, "copies", "inactive copies");
        misses.assert_none();
    }

    // A14
    #[tokio::test]
    async fn numbers_are_held_to_min_and_max_exactly() {
        let params = "  - name: count\n    type: integer\n    min: 1\n    max: 10\n  - name: n\n    type: number\n    min: 0.1\n    max: 0.2\n  - name: i\n    type: integer\n    max: 9007199254740992\n";
        let yaml = single(params, &text("fixed"));
        let (app, _state, _dir) = app_with(&[("bounded", &yaml)]);
        let mut misses = Misses::default();
        for data in [r#"{"count": 1}"#, r#"{"count": 10}"#, r#"{"n": 0.1}"#] {
            let (status, body) = render(&app, "bounded", data).await;
            misses.ok(status, &body, data);
        }
        for (data, param) in [
            (r#"{"count": 0}"#, "count"),
            (r#"{"count": 11}"#, "count"),
            (r#"{"n": 0.2000000001}"#, "n"),
            (r#"{"i": 9007199254740993}"#, "i"),
        ] {
            let (status, body) = render(&app, "bounded", data).await;
            misses.param_value_invalid(status, &body, param, data);
        }

        let over_default = single(
            "  - name: n\n    type: number\n    min: 0.1\n    max: 0.2\n    default: 0.2000000001\n",
            &text("fixed"),
        );
        let (_app, _state, dir) = app_with(&[("over_default", &over_default)]);
        let error = quarantine_error(&dir, "over_default");
        misses.expect(
            error
                .as_deref()
                .is_some_and(|e| e.contains("params.n.default")),
            "a literal default above max is quarantined at params.n.default",
            &json!(error),
        );
        misses.assert_none();
    }

    // A17
    #[tokio::test]
    async fn the_inputs_route_and_detail_keys_are_gone() {
        let yaml = single("  - name: title\n    type: string\n", &text("{title}"));
        let (app, _state, _dir) = app_with(&[("plain", &yaml)]);
        let (status, body) = send_json(
            &app,
            "POST",
            "/api/templates/plain/inputs",
            r#"{"labels": [{"data": {}}]}"#,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        assert_eq!(body["error"]["code"], "NotFound", "{body}");
        assert_eq!(body["error"]["details"]["kind"], "route", "{body}");

        let (status, detail) = get_json(&app, "/api/templates/plain").await;
        assert_eq!(status, StatusCode::OK);
        assert!(detail.get("inputs").is_none(), "{detail}");
        assert!(detail.get("param_defaults").is_none(), "{detail}");
    }

    const PUBLISHED_PARAMS: &str = "  - name: bold\n    type: boolean\n    default: \"false\"\n  - name: copies\n    type: integer\n    default: \"3\"\n  - name: url\n    type: string\n    default: \"{vars.base}\"\n  - name: printed_on\n    type: datetime\n    default: \"{sys.now}\"\n  - name: tags\n    type: list\n    default: [A, B]\n";

    fn assert_published_by_value(params: &Value, case: &str) {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        assert_eq!(
            param_entry(params, "bold")["default"],
            json!(false),
            "{case}"
        );
        assert_eq!(param_entry(params, "copies")["default"], json!(3), "{case}");
        assert_eq!(
            param_entry(params, "url")["default"],
            json!("https://ex.co/"),
            "{case}"
        );
        assert_eq!(
            param_entry(params, "printed_on")["default"],
            json!(today),
            "{case}"
        );
        assert_eq!(
            param_entry(params, "tags")["default"],
            json!(["A", "B"]),
            "{case}"
        );
        for entry in params.as_array().unwrap() {
            let default = &entry["default"];
            assert!(
                !default.is_object(),
                "{case}: published default of {} must be a scalar or list: {default}",
                entry["name"]
            );
        }
    }

    // A22
    #[tokio::test]
    async fn defaults_are_published_by_value() {
        let yaml = single(PUBLISHED_PARAMS, &text("fixed"));
        let broken_layout = single(PUBLISHED_PARAMS, &text("{vars.other}"));
        let (app, state, _dir) = app_with(&[("published", &yaml), ("broken_env", &broken_layout)]);

        // Without `base` every list and detail answer is still 200, and only the tokened defaults
        // of the affected templates are left out.
        let (status, detail) = get_json(&app, "/api/templates/published").await;
        assert_eq!(status, StatusCode::OK, "{detail}");
        assert!(
            param_entry(&detail["params"], "url")
                .get("default")
                .is_none(),
            "{detail}"
        );
        assert_eq!(
            param_entry(&detail["params"], "copies")["default"],
            json!(3)
        );

        state
            .store()
            .set_variable("base", "https://ex.co/")
            .await
            .unwrap();
        let (status, detail) = get_json(&app, "/api/templates/published").await;
        assert_eq!(status, StatusCode::OK, "{detail}");
        assert_published_by_value(&detail["params"], "detail");
        // `base` is read only by a default, and still counts as a variable the template reads.
        assert_eq!(detail["variables"], json!(["base"]), "{detail}");
        let (status, list) = get_json(&app, "/api/templates").await;
        assert_eq!(status, StatusCode::OK, "{list}");
        let summary = |list: &Value, id: &str| {
            list["templates"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["id"] == id)
                .cloned()
                .unwrap_or_else(|| panic!("{id} not listed: {list}"))
        };
        assert_published_by_value(&summary(&list, "published")["params"], "list");

        // An unrelated unset variable in the layout leaves out every tokened default of that template.
        let (status, broken) = get_json(&app, "/api/templates/broken_env").await;
        assert_eq!(status, StatusCode::OK, "{broken}");
        for name in ["url", "printed_on"] {
            assert!(
                param_entry(&broken["params"], name)
                    .get("default")
                    .is_none(),
                "{name}: {broken}"
            );
        }
        assert_eq!(
            param_entry(&broken["params"], "bold")["default"],
            json!(false)
        );
        let broken_summary = summary(&list, "broken_env");
        assert!(
            param_entry(&broken_summary["params"], "url")
                .get("default")
                .is_none(),
            "{broken_summary}"
        );

        let (status, created) = send(
            &app,
            "POST",
            "/api/templates/fresh",
            "text/yaml",
            yaml.clone(),
        )
        .await;
        let created: Value = serde_json::from_slice(&created).unwrap();
        assert_eq!(status, StatusCode::CREATED, "{created}");
        assert_published_by_value(&created["params"], "create");
        let (status, replaced) = send(&app, "PUT", "/api/templates/fresh", "text/yaml", yaml).await;
        let replaced: Value = serde_json::from_slice(&replaced).unwrap();
        assert_eq!(status, StatusCode::OK, "{replaced}");
        assert_published_by_value(&replaced["params"], "replace");
    }

    // A22 (two templates, one name)
    #[tokio::test]
    async fn each_listed_template_publishes_its_own_resolved_default() {
        let first = single(
            "  - name: url\n    type: string\n    default: \"{vars.first}\"\n",
            &text("fixed"),
        );
        let second = single(
            "  - name: url\n    type: string\n    default: \"{vars.second}\"\n",
            &text("fixed"),
        );
        let (app, state, _dir) = app_with(&[("first", &first), ("second", &second)]);
        state.store().set_variable("first", "one").await.unwrap();
        state.store().set_variable("second", "two").await.unwrap();
        let (status, list) = get_json(&app, "/api/templates").await;
        assert_eq!(status, StatusCode::OK, "{list}");
        for (id, value) in [("first", "one"), ("second", "two")] {
            let summary = list["templates"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["id"] == id)
                .unwrap_or_else(|| panic!("{id} not listed: {list}"));
            assert_eq!(
                param_entry(&summary["params"], "url")["default"],
                json!(value),
                "{id}"
            );
        }
    }
}
