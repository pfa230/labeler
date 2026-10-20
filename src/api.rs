//! API routes and handler functions.
//!
//! Handlers in this module tree import `Json` and `Path` from `crate::extract` rather than
//! `axum::extract` so that request extraction failures automatically produce the standard
//! JSON error envelope with `AppError` rather than axum's plain text rejections (#225).
//!
//! The resource handlers live in child modules beside their route declarations; this parent keeps
//! the shared `AppState`, the application router and its composition point.

use arc_swap::ArcSwap;
use axum::{
    extract::State,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::{catch_panic::CatchPanicLayer, trace::TraceLayer};
use utoipa::OpenApi;
use utoipa_axum::{router::OpenApiRouter, routes};
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    errors::{AppError, NotFoundKind},
    extract::Json,
    models::HealthResponse,
    openapi::ApiMetadata,
    store::Store,
    templates::{TemplateRegistry, TemplateRegistryError},
};

#[cfg(test)]
use crate::extract::Path;

pub(crate) mod auth;
pub(crate) mod printers;
pub(crate) mod rendering;
pub(crate) mod settings;
pub(crate) mod templates;

pub use printers::valid_printer_id;

pub struct AppState {
    templates: ArcSwap<TemplateRegistry>,
    templates_dir: PathBuf,
    write_lock: Mutex<()>,
    store: Store,
    ui_dir: PathBuf,
    trust_proxy: bool,
    no_auth: bool,
    /// Fires in a create after validation and before the file is published, for the interleaving a
    /// request cannot stage: a file arriving at the destination name. Compiled out of the shipped
    /// binary; without it no test tells the exclusive publish from a stat-then-rename.
    #[cfg(test)]
    pre_publish_hook: std::sync::Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
}

impl AppState {
    pub fn new(registry: TemplateRegistry, templates_dir: PathBuf, store: Store) -> Self {
        Self {
            templates: ArcSwap::from_pointee(registry),
            templates_dir,
            write_lock: Mutex::new(()),
            store,
            ui_dir: std::env::var_os("LABELER_UI_DIR")
                .map(Into::into)
                .unwrap_or_else(|| PathBuf::from("ui/dist")),
            trust_proxy: std::env::var("LABELER_TRUST_PROXY")
                .map(|v| v == "true")
                .unwrap_or(false),
            #[cfg(test)]
            pre_publish_hook: std::sync::Mutex::new(None),
            no_auth: std::env::var("LABELER_NO_AUTH")
                .map(|v| v == "true")
                .unwrap_or(false),
        }
    }

    pub fn with_ui_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.ui_dir = dir.into();
        self
    }

    pub fn with_no_auth(mut self, no_auth: bool) -> Self {
        self.no_auth = no_auth;
        self
    }

    pub fn ui_dir(&self) -> &std::path::Path {
        &self.ui_dir
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn trust_proxy(&self) -> bool {
        self.trust_proxy
    }

    pub fn no_auth(&self) -> bool {
        self.no_auth
    }

    /// Install the validated-but-not-yet-published hook. Test-only; see the field.
    #[cfg(test)]
    pub fn set_pre_publish_hook(&self, hook: impl Fn() + Send + Sync + 'static) {
        *self.pre_publish_hook.lock().expect("hook lock") = Some(Box::new(hook));
    }

    /// Called by the create endpoint after validation and before it publishes.
    fn before_publish(&self) {
        #[cfg(test)]
        {
            let hook = self.pre_publish_hook.lock().expect("hook lock");
            if let Some(hook) = hook.as_ref() {
                hook();
            }
        }
    }

    /// Read the templates directory without publishing the result.
    fn read_templates(&self) -> Result<TemplateRegistry, TemplateRegistryError> {
        TemplateRegistry::load_from_dir(&self.templates_dir)
    }

    /// Make `registry` the served set, logging whatever it refused.
    fn publish(&self, registry: TemplateRegistry) -> (usize, usize) {
        let count = registry.len();
        let broken_count = registry.broken().len();
        if broken_count > 0 {
            for b in registry.broken() {
                tracing::warn!(path = %b.path, error = %b.error, "template failed to load");
            }
        }
        self.templates.store(Arc::new(registry));
        (count, broken_count)
    }

    // Synchronous filesystem I/O. Acceptable for the single-user, local-templates-dir target and
    // consistent with the synchronous Typst render path; revisit with spawn_blocking if it ever
    // serves large dirs or remote storage.
    fn reload(&self) -> Result<(usize, usize), TemplateRegistryError> {
        let registry = self.read_templates()?;
        Ok(self.publish(registry))
    }
}

pub(crate) fn documented_router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::with_openapi(ApiMetadata::openapi())
        .routes(routes!(health))
        .merge(templates::router())
        .merge(printers::router())
        .merge(settings::router())
        .merge(rendering::router())
        .merge(auth::router())
}

fn api_router() -> Router<Arc<AppState>> {
    let (router, document) = documented_router().split_for_parts();
    let router = router
        // Serve the OpenAPI doc from an explicit route so it resolves at /api/openapi.json under the
        // `/api` nest (SwaggerUi's own `.url()` serving route gets double-prefixed when nested).
        .route(
            "/openapi.json",
            get(openapi_json).with_state(document.clone()),
        )
        // SwaggerUi serves the UI at /api/docs/ (trailing slash).
        .merge(SwaggerUi::new("/docs").url("/api/openapi.json", document));

    #[cfg(test)]
    let router = router.route("/test/panic/{shape}", get(test_api_panic_h));

    router
}

pub const UNREADABLE_PANIC_MARKER: &str = "unreadable panic payload";
pub const PANIC_RESPONSE_MESSAGE: &str = "internal server error";

#[cfg(test)]
async fn test_api_panic_h(Path(shape): Path<String>) -> Response {
    match shape.as_str() {
        "formatted" => panic!("distinctive interpolated panic: {}", "payload-value-42"),
        "literal" => panic!("distinctive literal panic payload"),
        "any" => std::panic::panic_any(123_u32),
        other => panic!("unknown panic shape: {other}"),
    }
}

#[cfg(test)]
async fn test_outside_panic_h() -> Response {
    panic!("distinctive outside api panic");
}

async fn openapi_json(State(document): State<utoipa::openapi::OpenApi>) -> Response {
    Json(document).into_response()
}

fn handle_panic(err: Box<dyn std::any::Any + Send + 'static>) -> Response {
    if let Some(s) = err.downcast_ref::<String>() {
        tracing::error!("panic caught: {s}");
    } else if let Some(s) = err.downcast_ref::<&'static str>() {
        tracing::error!("panic caught: {s}");
    } else {
        tracing::error!("panic caught: {UNREADABLE_PANIC_MARKER}");
    }
    AppError::internal(PANIC_RESPONSE_MESSAGE).into_response()
}

pub fn app(state: Arc<AppState>) -> Router {
    let assets = tower_http::services::ServeDir::new(state.ui_dir().join("assets"));
    let api = api_router().layer(axum::middleware::from_fn_with_state(
        state.clone(),
        crate::middleware::require_auth,
    ));
    let router = Router::new()
        .nest("/api", api)
        .nest_service("/assets", assets);

    #[cfg(test)]
    let router = router.route("/test/panic", get(test_outside_panic_h));

    router
        .fallback(fallback)
        .layer(TraceLayer::new_for_http())
        .layer(CatchPanicLayer::custom(handle_panic as fn(_) -> _))
        .with_state(state)
}

async fn fallback(State(state): State<Arc<AppState>>, uri: axum::http::Uri) -> Response {
    if uri.path() == "/api" || uri.path().starts_with("/api/") {
        return AppError::not_found(NotFoundKind::Route, uri.path()).into_response();
    }
    // SPA: serve index.html for any non-API, non-asset route (client-side routing).
    match tokio::fs::read(state.ui_dir().join("index.html")).await {
        Ok(bytes) => (
            axum::http::StatusCode::OK,
            [("content-type", "text/html; charset=utf-8")],
            bytes,
        )
            .into_response(),
        Err(_) => (
            axum::http::StatusCode::NOT_FOUND,
            "UI not built; run `npm --prefix ui run build`",
        )
            .into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/health",
    tag = "api",
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse)
    )
)]
pub async fn health() -> impl IntoResponse {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}
