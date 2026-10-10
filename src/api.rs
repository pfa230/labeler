//! API routes and handler functions.
//!
//! Handlers in this module import `Json` and `Path` from `crate::extract` rather than
//! `axum::extract` so that request extraction failures automatically produce the standard
//! JSON error envelope with `AppError` rather than axum's plain text rejections (#225).

use arc_swap::ArcSwap;
use axum::{
    extract::{FromRequestParts, Query, State},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Router,
};
use axum_extra::extract::cookie::CookieJar;
use sha2::Digest;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::{catch_panic::CatchPanicLayer, trace::TraceLayer};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    connector::{BrowsePage, BrowseRequest, ConnectorSchema, LabelRow, MaterializeRequest},
    errors::{AppError, NotFoundKind},
    extract::{Json, Path},
    fs_safe::{self, PublishResult},
    models::{
        BatchRowError, BatchSummary, ErrorResponse, HealthResponse, NewPrinter, PrintRequest,
        Printer, PrinterConnection, PrinterUpdate, ReloadResponse, RenderLabelRequest,
        RenderRequest, TemplateDetail, TemplateList, VariableValue,
    },
    openapi::ApiDoc,
    parse::parse_template,
    reason::Reason,
    render::{
        render_single_label_image, render_single_label_pdf, resolve_environment, ColorMode,
        ImageRenderOptions,
    },
    store::Store,
    templates::{
        validate_template_id_stem, TemplateContent, TemplateDefinition, TemplateRegistry,
        TemplateRegistryError,
    },
};
use rustix::fd::AsFd;

const MAX_BATCH_LABELS: usize = 500;

#[derive(serde::Deserialize)]
pub struct RenderQuery {
    pub format: Option<String>,
    pub color_mode: Option<String>,
    pub resolution: Option<String>,
}

pub struct AppState {
    templates: ArcSwap<TemplateRegistry>,
    templates_dir: PathBuf,
    write_lock: Mutex<()>,
    store: Store,
    ui_dir: PathBuf,
    trust_proxy: bool,
    no_auth: bool,
    egress: crate::egress::Egress,
    connectors: crate::connector::ConnectorRegistry,
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
            egress: crate::egress::Egress::new(),
            connectors: crate::connector::ConnectorRegistry::default(),
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

    pub fn egress(&self) -> &crate::egress::Egress {
        &self.egress
    }

    pub fn connectors(&self) -> &crate::connector::ConnectorRegistry {
        &self.connectors
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

fn api_router() -> Router<Arc<AppState>> {
    let router = Router::new()
        .route("/health", get(health))
        .route("/templates", get(list_templates))
        .route("/templates/reload", post(reload_templates))
        .route(
            "/templates/{id}",
            get(get_template)
                .post(create_template)
                .put(replace_template)
                .delete(delete_template),
        )
        .route("/templates/{id}/source", get(template_source))
        .route("/templates/{id}/thumbnail", get(thumbnail))
        .route("/printers", get(list_printers).post(create_printer))
        .route("/printers/probe", post(probe_printer))
        .route(
            "/printers/{id}",
            get(get_printer).put(replace_printer).delete(delete_printer),
        )
        .route(
            "/connections",
            get(list_connections).post(create_connection),
        )
        .route(
            "/connections/{id}",
            get(get_connection_h)
                .put(update_connection_h)
                .delete(delete_connection_h),
        )
        .route("/connections/{id}/schema", get(connection_schema))
        .route("/connections/{id}/browse", post(connection_browse))
        .route(
            "/connections/{id}/materialize",
            post(connection_materialize),
        )
        .route("/variables", get(get_variables))
        .route("/variables/{key}", put(put_variable))
        .route("/settings", get(get_settings))
        .route("/settings/{key}", put(put_setting).delete(delete_setting))
        .route("/datetime-formats/preview", post(preview_datetime_format))
        .route("/render/label", post(render_label))
        .route("/render", post(render_labels))
        .route("/print", post(print_labels))
        .route("/favorites", get(list_favorites))
        .route(
            "/favorites/{template_id}",
            put(add_favorite).delete(remove_favorite),
        )
        .route("/recent-templates", get(recent_templates))
        .route("/auth/setup", post(setup))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/auth/password", post(change_password))
        .route("/users", get(list_users).post(create_user_h))
        .route("/users/{id}", axum::routing::delete(delete_user_h))
        .route("/tokens", get(list_tokens).post(create_token_h))
        .route("/tokens/{id}", axum::routing::delete(delete_token_h))
        // Serve the OpenAPI doc from an explicit route so it resolves at /api/openapi.json under the
        // `/api` nest (SwaggerUi's own `.url()` serving route gets double-prefixed when nested).
        .route("/openapi.json", get(openapi_json))
        // SwaggerUi serves the UI at /api/docs/ (trailing slash).
        .merge(SwaggerUi::new("/docs").url("/api/openapi.json", ApiDoc::openapi()));

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

async fn openapi_json() -> Response {
    Json(ApiDoc::openapi()).into_response()
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
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse)
    )
)]
pub async fn health() -> impl IntoResponse {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

#[utoipa::path(
    get,
    path = "/templates",
    responses(
        (status = 200, description = "List templates", body = TemplateList)
    )
)]
pub async fn list_templates(
    State(state): State<Arc<AppState>>,
) -> Result<Json<TemplateList>, AppError> {
    let context = detail_context(&state).await?;
    let dt_resolver = crate::datetime_fmt::DateTimeResolver {
        formats: &context.dt_formats,
        now: chrono::Local::now(),
    };
    let registry = state.templates.load_full();
    let templates = registry.summaries(&context.variables, &dt_resolver);
    let broken = registry
        .broken()
        .iter()
        .map(|b| crate::models::BrokenTemplateSummary {
            path: b.path.clone(),
            error: b.error.clone(),
        })
        .collect();
    Ok(Json(TemplateList { templates, broken }))
}

#[utoipa::path(
    post,
    path = "/templates/reload",
    responses(
        (status = 200, description = "Templates reloaded from disk", body = ReloadResponse),
        (status = 500, description = "Failed to read the templates directory", body = ErrorResponse)
    )
)]
pub async fn reload_templates(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ReloadResponse>, AppError> {
    // Under the same lock the write endpoints hold, so it cannot interleave with a write and the
    // reload that follows it.
    let _guard = state.write_lock.lock().await;
    let (count, broken_count) = state.reload()?;
    Ok(Json(ReloadResponse {
        count,
        broken_count,
    }))
}

fn check_template_id(id: &str) -> Result<(), AppError> {
    if validate_template_id_stem(id) {
        Ok(())
    } else {
        Err(AppError::invalid_request(
            Reason::TemplateIdInvalid,
            format!("template id '{id}' must be non-empty and match ^[a-zA-Z0-9_-]+$"),
        ))
    }
}

fn parse_and_validate(body: &str) -> Result<TemplateContent, AppError> {
    let content = parse_template(body).map_err(|err| {
        AppError::template_invalid(Reason::TemplateValidationFailed, err.to_string())
    })?;
    content
        .validate()
        .map_err(|err| AppError::template_invalid(Reason::TemplateValidationFailed, err))?;
    Ok(content)
}

/// The variables and datetime formats a response's published `params` are resolved against. A
/// write reads them before writing, so a store failure answers `500` with nothing written.
struct DetailContext {
    variables: BTreeMap<String, String>,
    dt_formats: BTreeMap<String, String>,
}

async fn detail_context(state: &AppState) -> Result<DetailContext, AppError> {
    Ok(DetailContext {
        variables: state.store().all_variables().await?,
        dt_formats: crate::settings::resolve_datetime_formats(state.store())
            .await
            .map_err(|e| AppError::internal(e.to_string()))?,
    })
}

/// Finish a template write: reload the registry and answer with the detail of exactly what was
/// written, built from the validated body rather than looked up in the reloaded registry.
fn reload_and_describe(
    state: &AppState,
    id: String,
    content: TemplateContent,
    context: &DetailContext,
    status: axum::http::StatusCode,
) -> Result<Response, AppError> {
    state.reload()?;
    let dt_resolver = crate::datetime_fmt::DateTimeResolver {
        formats: &context.dt_formats,
        now: chrono::Local::now(),
    };
    let detail = TemplateDefinition { id, content }.build_detail(&context.variables, &dt_resolver);
    Ok((status, Json(detail)).into_response())
}

#[utoipa::path(
    post,
    path = "/templates/{id}",
    params(("id" = String, Path, description = "Template ID")),
    request_body(content = String, description = "Template YAML", content_type = "text/yaml"),
    responses(
        (status = 201, description = "Template created", body = TemplateDetail),
        (status = 400, description = "Invalid id", body = ErrorResponse),
        (status = 409, description = "The template's file already exists", body = ErrorResponse),
        (status = 422, description = "Invalid template", body = ErrorResponse),
        (status = 500, description = "The write failed or the directory could not be re-read", body = ErrorResponse)
    )
)]
pub async fn create_template(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    body: String,
) -> Result<Response, AppError> {
    check_template_id(&id)?;
    let content = parse_and_validate(&body)?;

    let _guard = state.write_lock.lock().await;
    let context = detail_context(&state).await?;
    let root_fd = fs_safe::open_dir_handle(&state.templates_dir)?;
    state.before_publish();
    // The exclusive publish decides existence from the disk, atomically, so a file copied in out of
    // band or a broken one is never overwritten.
    match fs_safe::stage_and_publish_new(root_fd.as_fd(), &format!("{id}.yaml"), &body)? {
        PublishResult::Published => {}
        PublishResult::AlreadyExists => {
            return Err(AppError::conflict(format!(
                "template '{id}' already exists"
            )));
        }
    }
    reload_and_describe(
        &state,
        id,
        content,
        &context,
        axum::http::StatusCode::CREATED,
    )
}

#[utoipa::path(
    put,
    path = "/templates/{id}",
    params(("id" = String, Path, description = "Template ID")),
    request_body(content = String, description = "Template YAML", content_type = "text/yaml"),
    responses(
        (status = 200, description = "Template replaced", body = TemplateDetail),
        (status = 400, description = "Invalid id", body = ErrorResponse),
        (status = 404, description = "The template's file does not exist", body = ErrorResponse),
        (status = 422, description = "Invalid template", body = ErrorResponse),
        (status = 500, description = "The write failed or the directory could not be re-read", body = ErrorResponse)
    )
)]
pub async fn replace_template(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    body: String,
) -> Result<Response, AppError> {
    check_template_id(&id)?;
    let content = parse_and_validate(&body)?;

    let _guard = state.write_lock.lock().await;
    let filename = format!("{id}.yaml");
    // Decided from the disk, not the registry, so a broken file is replaceable.
    match std::fs::symlink_metadata(state.templates_dir.join(&filename)) {
        Ok(_) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(AppError::not_found(NotFoundKind::Template, id));
        }
        Err(err) => {
            return Err(AppError::internal(format!(
                "failed to check template file '{filename}': {err}"
            )));
        }
    }
    let context = detail_context(&state).await?;
    let root_fd = fs_safe::open_dir_handle(&state.templates_dir)?;
    fs_safe::stage_and_replace(root_fd.as_fd(), &filename, &body)?;
    reload_and_describe(&state, id, content, &context, axum::http::StatusCode::OK)
}

#[utoipa::path(
    delete,
    path = "/templates/{id}",
    params(("id" = String, Path, description = "Template ID")),
    responses(
        (status = 204, description = "Template deleted"),
        (status = 400, description = "Invalid id", body = ErrorResponse),
        (status = 404, description = "The template's file does not exist", body = ErrorResponse),
        (status = 500, description = "File removal, the favorites prune, or the directory re-read failed", body = ErrorResponse)
    )
)]
pub async fn delete_template(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    check_template_id(&id)?;
    let _guard = state.write_lock.lock().await;
    let root_fd = fs_safe::open_dir_handle(&state.templates_dir)?;
    fs_safe::unlink_file(root_fd.as_fd(), &id)?;
    state.store().remove_favorites_for_template(&id).await?;
    state.reload()?;
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

#[utoipa::path(
    get,
    path = "/templates/{id}",
    params(
        ("id" = String, Path, description = "Template ID")
    ),
    responses(
        (status = 200, description = "Template details", body = TemplateDetail),
        (status = 404, description = "Template not found", body = ErrorResponse)
    )
)]
pub async fn get_template(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<TemplateDetail>, AppError> {
    let variables = state.store().all_variables().await?;
    let dt_formats = crate::settings::resolve_datetime_formats(state.store())
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let now = chrono::Local::now();
    let dt_resolver = crate::datetime_fmt::DateTimeResolver {
        formats: &dt_formats,
        now,
    };
    state
        .templates
        .load_full()
        .detail(&id, &variables, &dt_resolver)
        .map(Json)
        .ok_or_else(|| AppError::not_found(NotFoundKind::Template, id))
}

#[utoipa::path(
    get,
    path = "/templates/{id}/source",
    params(("id" = String, Path, description = "Template ID")),
    responses(
        (status = 200, description = "The template file's bytes, served or broken", content_type = "text/yaml"),
        (status = 400, description = "Invalid id", body = ErrorResponse),
        (status = 404, description = "The template's file does not exist or cannot be read", body = ErrorResponse)
    )
)]
pub async fn template_source(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    check_template_id(&id)?;
    let yaml = std::fs::read(state.templates_dir.join(format!("{id}.yaml")))
        .map_err(|_| AppError::not_found(NotFoundKind::Template, id))?;
    Ok((
        axum::http::StatusCode::OK,
        [("content-type", "text/yaml; charset=utf-8")],
        yaml,
    )
        .into_response())
}

#[utoipa::path(
    get,
    path = "/templates/{id}/thumbnail",
    params(("id" = String, Path, description = "Template id")),
    responses(
        (status = 200, description = "Rendered PNG thumbnail", content_type = "image/png", body = Vec<u8>),
        (status = 304, description = "Not modified (ETag match)"),
        (status = 404, description = "Template not found", body = ErrorResponse),
        (status = 422, description = "Render/interpolation error", body = ErrorResponse),
    )
)]
pub async fn thumbnail(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let registry = state.templates.load_full();
    let template = registry
        .get(&id)
        .ok_or_else(|| AppError::not_found(NotFoundKind::Template, id.clone()))?;
    let dt_formats = crate::settings::resolve_datetime_formats(state.store())
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let now = chrono::Local::now();
    let dt = crate::datetime_fmt::DateTimeResolver {
        formats: &dt_formats,
        now,
    };
    let variables = state.store().all_variables().await?;
    let data = template.placeholder_data(now);
    let png = crate::render::render_thumbnail_png(template, &data, &variables, &dt)?;

    // #129: key the ETag on the rendered bytes, not the template YAML. The image depends on the
    // template AND the renderer AND the variables it interpolates AND the datetime formats, so a key
    // built from a list of inputs goes stale as soon as that list is incomplete — which is the bug
    // this replaces (the YAML hash covered one input of four). Hashing the payload cannot be
    // incomplete. The cost is that revalidation no longer skips the render, so a `304` saves the
    // transfer but not the work; at catalog scale that is well under a second per grid refresh.
    let etag = format!("\"{}\"", hex::encode(sha2::Sha256::digest(&png)));

    if let Some(inm) = headers.get(axum::http::header::IF_NONE_MATCH) {
        if inm.to_str().map(|v| v == "*" || v == etag).unwrap_or(false) {
            return Ok((
                axum::http::StatusCode::NOT_MODIFIED,
                [(axum::http::header::ETAG, etag.as_str())],
            )
                .into_response());
        }
    }

    Ok((
        axum::http::StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "image/png"),
            (axum::http::header::ETAG, etag.as_str()),
            (axum::http::header::CACHE_CONTROL, "no-cache"),
        ],
        png,
    )
        .into_response())
}

fn validate_printer(id: &str, name: &str, connection: &PrinterConnection) -> Result<(), AppError> {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::invalid_request(
            Reason::PrinterIdInvalid,
            format!(
                "printer id '{id}' must be non-empty and contain only letters, digits, '-' or '_'"
            ),
        ));
    }
    if name.trim().is_empty() {
        return Err(AppError::printer_invalid("printer name must not be empty"));
    }
    crate::driver::driver_for(connection)
        .map_err(|err| AppError::printer_invalid(err.to_string()))?;
    Ok(())
}

#[utoipa::path(
    get,
    path = "/printers",
    responses((status = 200, description = "List printers", body = [Printer]))
)]
pub async fn list_printers(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Printer>>, AppError> {
    Ok(Json(state.store().list_printers().await?))
}

#[utoipa::path(
    post,
    path = "/printers",
    request_body = NewPrinter,
    responses(
        (status = 201, description = "Printer created", body = Printer),
        (status = 400, description = "Invalid printer", body = ErrorResponse),
        (status = 409, description = "Printer id already exists", body = ErrorResponse)
    )
)]
pub async fn create_printer(
    State(state): State<Arc<AppState>>,
    Json(body): Json<NewPrinter>,
) -> Result<Response, AppError> {
    let connection = body.connection();
    validate_printer(&body.id, &body.name, &connection)?;
    let _guard = state.write_lock.lock().await;
    if state.store().get_printer(&body.id).await?.is_some() {
        return Err(AppError::conflict(format!(
            "A printer with id '{}' already exists",
            body.id
        )));
    }
    state
        .store()
        .insert_printer(&body.id, &body.name, &connection)
        .await?;
    let printer = Printer::new(body.id, body.name, connection);
    Ok((axum::http::StatusCode::CREATED, Json(printer)).into_response())
}

#[utoipa::path(
    get,
    path = "/printers/{id}",
    params(("id" = String, Path, description = "Printer ID")),
    responses(
        (status = 200, description = "Printer", body = Printer),
        (status = 404, description = "Printer not found", body = ErrorResponse)
    )
)]
pub async fn get_printer(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Printer>, AppError> {
    let printer = state
        .store()
        .get_printer(&id)
        .await?
        .ok_or_else(|| AppError::not_found(NotFoundKind::Printer, id))?;
    Ok(Json(printer))
}

#[utoipa::path(
    get,
    path = "/variables",
    responses((status = 200, description = "All variables", body = std::collections::BTreeMap<String, String>))
)]
pub async fn get_variables(
    State(state): State<Arc<AppState>>,
) -> Result<Json<std::collections::BTreeMap<String, String>>, AppError> {
    Ok(Json(state.store().all_variables().await?))
}

#[utoipa::path(
    put,
    path = "/variables/{key}",
    params(("key" = String, Path, description = "Variable key")),
    request_body = VariableValue,
    responses(
        (status = 200, description = "Variable stored", body = VariableValue),
        (status = 400, description = "Invalid key", body = ErrorResponse)
    )
)]
pub async fn put_variable(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
    Json(body): Json<VariableValue>,
) -> Result<Json<VariableValue>, AppError> {
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Err(AppError::invalid_request(
            Reason::VariableKeyInvalid,
            format!("variable key '{key}' must be non-empty and contain only letters, digits, '_', '-' or '.'"),
        ));
    }
    let _guard = state.write_lock.lock().await;
    state.store().set_variable(&key, &body.value).await?;
    Ok(Json(body))
}

/// A resolved application setting: its effective value and whether that is the in-code default.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ResolvedSetting {
    pub value: serde_json::Value,
    pub is_default: bool,
}

/// Request body for `PUT /settings/{key}`: the new value, validated per setting.
#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct SettingValue {
    pub value: serde_json::Value,
}

#[utoipa::path(
    get,
    path = "/settings",
    tag = "settings",
    responses((status = 200, description = "Resolved application settings", body = std::collections::BTreeMap<String, ResolvedSetting>))
)]
pub async fn get_settings(State(state): State<Arc<AppState>>) -> Result<Response, AppError> {
    let mut out = std::collections::BTreeMap::new();
    for key in crate::settings::KNOWN {
        let stored = state.store().get_setting(key).await?;
        out.insert(key.to_string(), resolved_setting(key, stored)?);
    }
    Ok(Json(out).into_response())
}

/// A known setting's effective value from its stored override (`None`: the in-code default). A
/// stored override that no longer parses is a `500`.
fn resolved_setting(key: &str, stored: Option<String>) -> Result<ResolvedSetting, AppError> {
    use crate::settings::{
        resolve_datetime_formats_from, resolve_default_id_from, DATETIME_FORMATS,
        DEFAULT_CONNECTION_ID, DEFAULT_PRINTER_ID,
    };
    let is_default = stored.is_none();
    let value = match key {
        DATETIME_FORMATS => resolve_datetime_formats_from(stored).map(|v| serde_json::json!(v)),
        DEFAULT_CONNECTION_ID | DEFAULT_PRINTER_ID => {
            resolve_default_id_from(key, stored).map(|v| serde_json::json!(v))
        }
        _ => return Err(AppError::internal(format!("unknown setting '{key}'"))),
    }
    .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(ResolvedSetting { value, is_default })
}

#[utoipa::path(
    put,
    path = "/settings/{key}",
    tag = "settings",
    params(("key" = String, Path, description = "Setting key")),
    request_body = SettingValue,
    responses(
        (status = 200, description = "Override stored", body = ResolvedSetting),
        (status = 400, description = "Invalid value", body = ErrorResponse),
        (status = 404, description = "Unknown setting", body = ErrorResponse)
    )
)]
pub async fn put_setting(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
    Json(body): Json<SettingValue>,
) -> Result<Response, AppError> {
    if !crate::settings::is_known(&key) {
        return Err(AppError::not_found(NotFoundKind::Setting, &key));
    }
    let canonical = crate::settings::validate(&key, &body.value)
        .map_err(|err| AppError::invalid_request(Reason::SettingValueInvalid, err))?;
    let _guard = state.write_lock.lock().await;
    let missing = match key.as_str() {
        crate::settings::DEFAULT_CONNECTION_ID => state
            .store()
            .get_connection(&canonical)
            .await?
            .is_none()
            .then_some("connection"),
        crate::settings::DEFAULT_PRINTER_ID => state
            .store()
            .get_printer(&canonical)
            .await?
            .is_none()
            .then_some("printer"),
        _ => None,
    };
    if let Some(kind) = missing {
        return Err(AppError::invalid_request(
            Reason::SettingValueInvalid,
            format!("{kind} '{canonical}' does not exist"),
        ));
    }
    state.store().set_setting(&key, &canonical).await?;
    Ok(Json(resolved_setting(&key, Some(canonical))?).into_response())
}

#[utoipa::path(
    delete,
    path = "/settings/{key}",
    tag = "settings",
    params(("key" = String, Path, description = "Setting key")),
    responses(
        (status = 204, description = "Reset to default"),
        (status = 404, description = "Unknown setting", body = ErrorResponse)
    )
)]
pub async fn delete_setting(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
) -> Result<Response, AppError> {
    if !crate::settings::is_known(&key) {
        return Err(AppError::not_found(NotFoundKind::Setting, &key));
    }
    let _guard = state.write_lock.lock().await;
    // idempotent: a known setting that was never overridden is already at its default
    state.store().delete_setting(&key).await?;
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

/// Request body for `POST /datetime-formats/preview`.
#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct DatetimePreviewRequest {
    pub pattern: String,
}

/// Response for `POST /datetime-formats/preview`: the pattern applied to the current local time.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct DatetimePreviewResponse {
    pub sample: String,
}

#[utoipa::path(
    post,
    path = "/datetime-formats/preview",
    tag = "settings",
    request_body = DatetimePreviewRequest,
    responses(
        (status = 200, description = "Rendered sample for the pattern", body = DatetimePreviewResponse),
        (status = 400, description = "Invalid strftime pattern", body = ErrorResponse),
    )
)]
pub async fn preview_datetime_format(
    Json(req): Json<DatetimePreviewRequest>,
) -> Result<Response, AppError> {
    crate::datetime_fmt::validate_pattern(&req.pattern)
        .map_err(|err| AppError::invalid_request(Reason::DatetimePatternInvalid, err))?;
    let sample = crate::datetime_fmt::format_now(&req.pattern, chrono::Local::now());
    Ok(Json(DatetimePreviewResponse { sample }).into_response())
}

#[utoipa::path(
    put,
    path = "/printers/{id}",
    params(("id" = String, Path, description = "Printer ID")),
    request_body = PrinterUpdate,
    responses(
        (status = 200, description = "Printer replaced", body = Printer),
        (status = 400, description = "Invalid printer", body = ErrorResponse),
        (status = 404, description = "Printer not found", body = ErrorResponse)
    )
)]
pub async fn replace_printer(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<PrinterUpdate>,
) -> Result<Response, AppError> {
    let mut connection = body.connection();
    validate_printer(&id, &body.name, &connection)?;
    let _guard = state.write_lock.lock().await;
    let Some(existing) = state.store().get_printer_connection(&id).await? else {
        return Err(AppError::not_found(NotFoundKind::Printer, id));
    };
    connection.password = match connection.password {
        None => existing.password,
        Some(p) if p.is_empty() => None,
        replaced => replaced,
    };
    state
        .store()
        .replace_printer(&id, &body.name, &connection)
        .await?;
    Ok(Json(Printer::new(id, body.name, connection)).into_response())
}

#[utoipa::path(
    delete,
    path = "/printers/{id}",
    params(("id" = String, Path, description = "Printer ID")),
    responses(
        (status = 204, description = "Printer deleted"),
        (status = 404, description = "Printer not found", body = ErrorResponse)
    )
)]
pub async fn delete_printer(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let _guard = state.write_lock.lock().await;
    if state.store().delete_printer_and_default(&id).await? {
        Ok(axum::http::StatusCode::NO_CONTENT.into_response())
    } else {
        Err(AppError::not_found(NotFoundKind::Printer, id))
    }
}

/// The printer's self-reported capabilities, shaped for UI feedback.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ProbeCapabilities {
    pub model: Option<String>,
    pub media_width_mm: Option<f32>,
    pub resolution_dpi: Option<u32>,
    /// `"color"`, `"bilevel"`, or `"unknown"` (printer advertised no color/raster attribute).
    pub color: String,
    pub accepts_png: bool,
}

/// Result of a probe. Always returned inside a `200`; reachability is data, not the HTTP status.
#[derive(serde::Serialize, utoipa::ToSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ProbeResponse {
    Ok { capabilities: ProbeCapabilities },
    Unreachable { detail: String },
}

impl ProbeResponse {
    fn ok(caps: crate::driver::PrinterCapabilities) -> Self {
        let color = if caps.bilevel {
            "bilevel"
        } else if caps.color_known {
            "color"
        } else {
            "unknown"
        };
        ProbeResponse::Ok {
            capabilities: ProbeCapabilities {
                model: caps.model,
                media_width_mm: caps.loaded_media_width_mm,
                resolution_dpi: caps.resolution_dpi,
                color: color.to_string(),
                accepts_png: caps.accepts_png,
            },
        }
    }
}

#[utoipa::path(
    post,
    path = "/printers/probe",
    request_body = PrinterConnection,
    responses(
        (status = 200, description = "Probe result (ok or unreachable)", body = ProbeResponse),
        (status = 400, description = "Invalid printer", body = ErrorResponse)
    )
)]
pub async fn probe_printer(
    Json(connection): Json<PrinterConnection>,
) -> Result<Json<ProbeResponse>, AppError> {
    let driver = crate::driver::driver_for(&connection)
        .map_err(|e| AppError::printer_invalid(e.to_string()))?;
    Ok(Json(match driver.probe().await {
        crate::driver::ProbeOutcome::Ok(c) => ProbeResponse::ok(c),
        crate::driver::ProbeOutcome::Unreachable(d) => ProbeResponse::Unreachable { detail: d },
    }))
}

#[derive(serde::Serialize, utoipa::ToSchema, Debug, PartialEq)]
pub struct ConnectionView {
    pub id: String,
    pub connector: String,
    pub name: String,
    pub base_url: String,
    pub public_url: Option<String>,
    pub has_credential: bool,
}

impl From<&crate::store::Connection> for ConnectionView {
    fn from(c: &crate::store::Connection) -> Self {
        Self {
            id: c.id.clone(),
            connector: c.connector.clone(),
            name: c.name.clone(),
            base_url: c.base_url.clone(),
            public_url: c.public_url.clone(),
            has_credential: !c.credential.is_empty(),
        }
    }
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectionCreate {
    pub connector: String,
    pub name: String,
    pub base_url: String,
    #[serde(default, deserialize_with = "crate::models::deserialize_some")]
    #[schema(nullable = false)]
    pub public_url: Option<String>,
    // Required; optional here only so an omitted one is refused as `credential_required`.
    #[serde(default, deserialize_with = "crate::models::deserialize_some")]
    #[schema(nullable = false, required = true)]
    pub credential: Option<String>,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectionUpdate {
    pub name: String,
    pub base_url: String,
    #[serde(default, deserialize_with = "crate::models::deserialize_some")]
    #[schema(nullable = false)]
    pub public_url: Option<String>,
    #[serde(default, deserialize_with = "crate::models::deserialize_some")]
    #[schema(nullable = false)]
    pub credential: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UrlField {
    Base,
    Public,
}

impl UrlField {
    pub(crate) fn wire_name(self) -> &'static str {
        match self {
            Self::Base => "base_url",
            Self::Public => "public_url",
        }
    }

    pub(crate) fn reason(self) -> Reason {
        match self {
            Self::Base => Reason::BaseUrlInvalid,
            Self::Public => Reason::PublicUrlInvalid,
        }
    }
}

pub(crate) fn validate_and_normalize_url(raw: &str, field: UrlField) -> Result<String, AppError> {
    let trimmed = raw.trim();
    // The parser drops empty userinfo (`https://@host`), so only its syntax violation shows it.
    let embedded_credentials = std::cell::Cell::new(false);
    let report_violation = |violation| {
        if violation == url::SyntaxViolation::EmbeddedCredentials {
            embedded_credentials.set(true);
        }
    };
    let parsed = url::Url::options()
        .syntax_violation_callback(Some(&report_violation))
        .parse(trimmed)
        .map_err(|_| {
            AppError::invalid_request(field.reason(), format!("invalid {}", field.wire_name()))
        })?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(AppError::invalid_request(
            field.reason(),
            format!("{} must use http or https scheme", field.wire_name()),
        ));
    }
    if parsed.host().is_none() {
        return Err(AppError::invalid_request(
            field.reason(),
            format!("{} must include a host", field.wire_name()),
        ));
    }
    if embedded_credentials.get() || !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(AppError::invalid_request(
            field.reason(),
            format!("{} must not contain userinfo", field.wire_name()),
        ));
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(AppError::invalid_request(
            field.reason(),
            format!(
                "{} must not contain query parameters or fragments",
                field.wire_name()
            ),
        ));
    }
    Ok(trimmed.trim_end_matches('/').to_string())
}

#[utoipa::path(
    get,
    path = "/connections",
    responses(
        (status = 200, description = "List connections (credential redacted; only has_credential exposed)", body = [ConnectionView])
    )
)]
pub async fn list_connections(State(state): State<Arc<AppState>>) -> Result<Response, AppError> {
    let cs = state.store().list_connections().await?;
    Ok(Json(cs.iter().map(ConnectionView::from).collect::<Vec<_>>()).into_response())
}

#[utoipa::path(
    post,
    path = "/connections",
    request_body = ConnectionCreate,
    responses(
        (status = 201, description = "Connection created (credential redacted in response)", body = ConnectionView),
        (status = 400, description = "Invalid request", body = ErrorResponse)
    )
)]
pub async fn create_connection(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ConnectionCreate>,
) -> Result<Response, AppError> {
    state
        .connectors()
        .get(&body.connector)
        .ok_or_else(|| AppError::invalid_request(Reason::ConnectorUnknown, "unknown connector"))?;
    let cred = body.credential.unwrap_or_default();
    if cred.is_empty() {
        return Err(AppError::invalid_request(
            Reason::CredentialRequired,
            "credential required",
        ));
    }
    let base_url = validate_and_normalize_url(&body.base_url, UrlField::Base)?;
    let pub_url = optional_public_url(body.public_url.as_deref())?;
    let _g = state.write_lock.lock().await;
    let c = state
        .store()
        .create_connection(crate::store::NewConnection {
            connector: &body.connector,
            name: &body.name,
            base_url: &base_url,
            public_url: pub_url.as_deref(),
            credential: &cred,
        })
        .await?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(ConnectionView::from(&c)),
    )
        .into_response())
}

/// A missing or blank `public_url` is none; anything else must be a valid URL.
fn optional_public_url(raw: Option<&str>) -> Result<Option<String>, AppError> {
    match raw.map(str::trim) {
        None | Some("") => Ok(None),
        Some(url) => validate_and_normalize_url(url, UrlField::Public).map(Some),
    }
}

#[utoipa::path(
    get,
    path = "/connections/{id}",
    params(("id" = String, Path, description = "Connection ID")),
    responses(
        (status = 200, description = "Connection (credential redacted)", body = ConnectionView),
        (status = 404, description = "Connection not found", body = ErrorResponse)
    )
)]
pub async fn get_connection_h(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let c = state
        .store()
        .get_connection(&id)
        .await?
        .ok_or_else(|| AppError::not_found(NotFoundKind::Connection, &id))?;
    Ok(Json(ConnectionView::from(&c)).into_response())
}

#[utoipa::path(
    put,
    path = "/connections/{id}",
    params(("id" = String, Path, description = "Connection ID")),
    request_body = ConnectionUpdate,
    responses(
        (status = 200, description = "Connection updated (credential redacted)", body = ConnectionView),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 404, description = "Connection not found", body = ErrorResponse)
    )
)]
pub async fn update_connection_h(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<ConnectionUpdate>,
) -> Result<Response, AppError> {
    let existing = state
        .store()
        .get_connection(&id)
        .await?
        .ok_or_else(|| AppError::not_found(NotFoundKind::Connection, &id))?;
    state.connectors().get(&existing.connector).ok_or_else(|| {
        AppError::invalid_request(Reason::ConnectionConnectorMissing, "unknown connector")
    })?;
    if body.credential.as_deref() == Some("") {
        return Err(AppError::invalid_request(
            Reason::CredentialRequired,
            "credential must not be empty; omit it to keep the stored one",
        ));
    }
    let base_url = validate_and_normalize_url(&body.base_url, UrlField::Base)?;
    let public_url = optional_public_url(body.public_url.as_deref())?;
    let _g = state.write_lock.lock().await;
    let ok = state
        .store()
        .update_connection(
            &id,
            crate::store::UpdateConnection {
                name: &body.name,
                base_url: &base_url,
                public_url: public_url.as_deref(),
                credential: body.credential.as_deref(),
            },
        )
        .await?;
    if !ok {
        return Err(AppError::not_found(NotFoundKind::Connection, &id));
    }
    let c = state
        .store()
        .get_connection(&id)
        .await?
        .ok_or_else(|| AppError::not_found(NotFoundKind::Connection, &id))?;
    Ok(Json(ConnectionView::from(&c)).into_response())
}

#[utoipa::path(
    delete,
    path = "/connections/{id}",
    params(("id" = String, Path, description = "Connection ID")),
    responses(
        (status = 204, description = "Connection deleted"),
        (status = 404, description = "Connection not found", body = ErrorResponse)
    )
)]
pub async fn delete_connection_h(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let _g = state.write_lock.lock().await;
    if !state.store().delete_connection_and_default(&id).await? {
        return Err(AppError::not_found(NotFoundKind::Connection, &id));
    }
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

async fn load_conn_and_connector<'a>(
    state: &'a AppState,
    id: &str,
) -> Result<(crate::store::Connection, &'a crate::connector::Connectors), AppError> {
    let conn = state
        .store()
        .get_connection(id)
        .await?
        .ok_or_else(|| AppError::not_found(NotFoundKind::Connection, id))?;
    let c = state.connectors().get(&conn.connector).ok_or_else(|| {
        AppError::invalid_request(Reason::ConnectionConnectorMissing, "unknown connector")
    })?;
    Ok((conn, c))
}

#[utoipa::path(
    get,
    path = "/connections/{id}/schema",
    params(("id" = String, Path, description = "Connection ID")),
    responses(
        (status = 200, description = "Connector schema (resources, fields, filters, relationships)", body = ConnectorSchema),
        (status = 404, description = "Connection not found", body = ErrorResponse),
        (status = 502, description = "Upstream failure", body = ErrorResponse)
    )
)]
pub async fn connection_schema(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let (conn, c) = load_conn_and_connector(&state, &id).await?;
    let schema = c
        .schema(&conn, state.egress())
        .await
        .map_err(AppError::from)?;
    Ok(Json(schema).into_response())
}

#[utoipa::path(
    post,
    path = "/connections/{id}/browse",
    params(("id" = String, Path, description = "Connection ID")),
    request_body = BrowseRequest,
    responses(
        (status = 200, description = "A page of browse rows", body = BrowsePage),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 404, description = "Connection not found", body = ErrorResponse),
        (status = 502, description = "Upstream failure", body = ErrorResponse)
    )
)]
pub async fn connection_browse(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<crate::connector::BrowseRequest>,
) -> Result<Response, AppError> {
    let (conn, c) = load_conn_and_connector(&state, &id).await?;
    let page = c
        .browse(&conn, state.egress(), req)
        .await
        .map_err(AppError::from)?;
    Ok(Json(page).into_response())
}

#[utoipa::path(
    post,
    path = "/connections/{id}/materialize",
    params(("id" = String, Path, description = "Connection ID")),
    request_body = MaterializeRequest,
    responses(
        (status = 200, description = "Materialized label rows", body = [LabelRow]),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 404, description = "Connection not found", body = ErrorResponse),
        (status = 502, description = "Upstream failure", body = ErrorResponse)
    )
)]
pub async fn connection_materialize(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<crate::connector::MaterializeRequest>,
) -> Result<Response, AppError> {
    let (conn, c) = load_conn_and_connector(&state, &id).await?;
    let rows = c
        .materialize(&conn, state.egress(), req)
        .await
        .map_err(AppError::from)?;
    Ok(Json(rows).into_response())
}

fn download_response(bytes: Vec<u8>, content_type: &'static str, filename: &str) -> Response {
    (
        axum::http::StatusCode::OK,
        [
            ("content-type", content_type.to_string()),
            (
                "content-disposition",
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        bytes,
    )
        .into_response()
}

/// Where a batch goes: a file for `/render`, a printer for `/print`.
enum BatchTarget<'a> {
    File { format: Option<&'a str> },
    Printer { id: &'a str, actor: &'a str },
}

/// Shared by `/render` and `/print`: decides the request-level faults, then renders every label
/// before anything is returned or sent (rendering, "All-or-nothing validation").
async fn run_batch(
    state: &Arc<AppState>,
    template: &TemplateDefinition,
    labels: &[crate::models::LabelInput],
    start_slot: Option<u32>,
    target: BatchTarget<'_>,
) -> Result<Response, AppError> {
    let is_single = matches!(
        template.format,
        crate::models::TemplateFormat::Single { .. }
    );
    if is_single && start_slot.is_some() {
        return Err(AppError::invalid_request(
            Reason::FieldNotApplicable,
            "start_slot applies only to sheet templates",
        ));
    }
    if !is_single && matches!(target, BatchTarget::File { format: Some(_) }) {
        return Err(AppError::invalid_request(
            Reason::FieldNotApplicable,
            "format applies only to single templates",
        ));
    }
    let start_slot = start_slot.unwrap_or(0);
    let variables = state.store().all_variables().await?;
    let dt_formats = crate::settings::resolve_datetime_formats(state.store())
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let dt = crate::datetime_fmt::DateTimeResolver {
        formats: &dt_formats,
        now: chrono::Local::now(),
    };
    let render_env = resolve_environment(template, &variables, &dt)?;
    match target {
        BatchTarget::File { format } => {
            let env = crate::batch::BatchEnv {
                render: &render_env,
                render_opts: crate::render::ImageRenderOptions::default(),
            };
            let rendered = crate::batch::render_batch(
                template,
                labels,
                crate::batch::BatchMode::Download,
                format,
                start_slot,
                &env,
                MAX_BATCH_LABELS,
            )?;
            let crate::batch::RenderedBatch::Download {
                bytes,
                content_type,
                filename,
            } = rendered
            else {
                return Err(AppError::internal(
                    "batch returned non-download for download mode",
                ));
            };
            Ok(download_response(bytes, content_type, &filename))
        }
        BatchTarget::Printer {
            id: printer_id,
            actor,
        } => {
            let connection = state
                .store()
                .get_printer_connection(printer_id)
                .await?
                .ok_or_else(|| {
                    AppError::not_found(NotFoundKind::Printer, printer_id.to_string())
                })?;
            let driver = crate::driver::driver_for(&connection)
                .map_err(|err| AppError::printer_invalid(err.to_string()))?;
            let ovr = driver.configured_render_override();
            let media_width_mm = match &template.format {
                crate::models::TemplateFormat::Single {
                    media_width: Some(w),
                    ..
                } => Some(crate::driver::media_width_mm(*w, &template.unit)),
                _ => None,
            };
            // Capabilities only feed negotiation (printing, "Render negotiation").
            let caps = if ovr.color_mode.is_none() || ovr.resolution_dpi.is_none() {
                driver.capabilities().await
            } else {
                None
            };
            let render_opts = crate::driver::effective_render(&ovr, caps.as_ref());
            let artifact_format =
                crate::driver::print_artifact_format(render_opts.color_mode, is_single);
            let render_format = match artifact_format {
                crate::driver::ArtifactFormat::Png => "png",
                _ => "pdf",
            };
            let env = crate::batch::BatchEnv {
                render: &render_env,
                render_opts,
            };
            // Validate-then-execute: render everything first; bad data => 422 before any send.
            let rendered = crate::batch::render_batch(
                template,
                labels,
                crate::batch::BatchMode::Print,
                Some(render_format),
                start_slot,
                &env,
                MAX_BATCH_LABELS,
            )?;
            let crate::batch::RenderedBatch::Print { units } = rendered else {
                return Err(AppError::internal(
                    "batch returned non-print for print mode",
                ));
            };
            let total = labels.len();
            let jobs = units.len();
            let mut failed = Vec::new();
            for unit in &units {
                let media_size =
                    media_width_mm
                        .zip(unit.width_mm)
                        .map(|(x, y)| crate::driver::MediaSize {
                            x_dimension: crate::driver::hundredths_mm(x),
                            y_dimension: crate::driver::hundredths_mm(y),
                        });
                match driver
                    .send(
                        &unit.bytes,
                        &crate::driver::PrintOptions {
                            artifact_format,
                            media_size,
                        },
                    )
                    .await
                {
                    Ok(()) => {
                        let _ = state
                            .store()
                            .record_job(&template.id, Some(printer_id), "ok", None, actor)
                            .await;
                    }
                    Err(err) => {
                        let msg = err.to_string();
                        let _ = state
                            .store()
                            .record_job(&template.id, Some(printer_id), "failed", Some(&msg), actor)
                            .await;
                        for &i in &unit.indices {
                            failed.push(BatchRowError {
                                index: i,
                                error: msg.clone(),
                            });
                        }
                    }
                }
            }
            let summary = BatchSummary {
                total,
                sent: total - failed.len(),
                failed,
                jobs,
            };
            Ok((axum::http::StatusCode::OK, Json(summary)).into_response())
        }
    }
}

#[utoipa::path(
    post,
    path = "/render",
    request_body = RenderRequest,
    responses(
        (status = 200, description = "ZIP (single) or paginated PDF (sheet), as an attachment"),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 404, description = "Template not found", body = ErrorResponse),
        (status = 413, description = "Batch too large", body = ErrorResponse),
        (status = 422, description = "One or more labels invalid", body = ErrorResponse)
    )
)]
pub async fn render_labels(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RenderRequest>,
) -> Result<Response, AppError> {
    let registry = state.templates.load_full();
    let template = registry
        .get(&req.template)
        .ok_or_else(|| AppError::not_found(NotFoundKind::Template, req.template.clone()))?;
    run_batch(
        &state,
        template,
        &req.labels,
        req.start_slot,
        BatchTarget::File {
            format: req.format.as_deref(),
        },
    )
    .await
}

#[utoipa::path(
    post,
    path = "/print",
    request_body = PrintRequest,
    responses(
        (status = 200, description = "Print summary", body = BatchSummary),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 404, description = "Template or printer not found", body = ErrorResponse),
        (status = 413, description = "Batch too large", body = ErrorResponse),
        (status = 422, description = "One or more labels invalid", body = ErrorResponse)
    )
)]
pub async fn print_labels(
    State(state): State<Arc<AppState>>,
    axum::Extension(principal): axum::Extension<crate::middleware::Principal>,
    Json(req): Json<PrintRequest>,
) -> Result<Response, AppError> {
    let registry = state.templates.load_full();
    let template = registry
        .get(&req.template)
        .ok_or_else(|| AppError::not_found(NotFoundKind::Template, req.template.clone()))?;
    run_batch(
        &state,
        template,
        &req.labels,
        req.start_slot,
        BatchTarget::Printer {
            id: &req.printer,
            actor: &principal.actor_id(),
        },
    )
    .await
}

#[utoipa::path(
    post,
    path = "/render/label",
    params(
        ("format" = Option<String>, Query, description = "Output format: png (default) or pdf"),
        ("color_mode" = Option<String>, Query, description = "Color mode for PNG: color (default) or bilevel"),
        ("resolution" = Option<String>, Query, description = "PNG raster DPI override (1-1200); defaults to template dpi")
    ),
    request_body = RenderLabelRequest,
    responses(
        (status = 200, description = "Rendered PNG bytes", content_type = "image/png", body = Vec<u8>),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 404, description = "Template not found", body = ErrorResponse),
        (status = 415, description = "Unsupported media type", body = ErrorResponse),
        (status = 422, description = "Validation error", body = ErrorResponse)
    )
)]
pub async fn render_label(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RenderQuery>,
    Json(req): Json<RenderLabelRequest>,
) -> Result<Response, AppError> {
    let registry = state.templates.load_full();
    let template = registry
        .get(&req.template)
        .ok_or_else(|| AppError::not_found(NotFoundKind::Template, req.template.clone()))?;

    tracing::debug!(
        template = %template.id,
        dpi = template.dpi,
        data_keys = req.data.len(),
        "render label request"
    );

    let color_mode = match query.color_mode.as_deref() {
        None | Some("") | Some("color") => ColorMode::Color,
        Some("bilevel") => ColorMode::BiLevel,
        Some(other) => {
            return Err(AppError::invalid_request(
                Reason::ColorModeUnknown,
                format!("unknown color_mode '{other}'; use color or bilevel"),
            ))
        }
    };
    let resolution_dpi = match query.resolution.as_deref() {
        None | Some("") => None,
        Some(s) => {
            let dpi: u32 = s.parse().map_err(|_| {
                AppError::invalid_request(
                    Reason::ResolutionInvalid,
                    format!("resolution must be a positive integer, got '{s}'"),
                )
            })?;
            if dpi == 0 || dpi > crate::render::MAX_RENDER_DPI {
                return Err(AppError::invalid_request(
                    Reason::ResolutionInvalid,
                    format!(
                        "resolution must be between 1 and {}",
                        crate::render::MAX_RENDER_DPI
                    ),
                ));
            }
            Some(dpi)
        }
    };
    let img_opts = ImageRenderOptions {
        color_mode,
        resolution_dpi,
    };

    let variables = state.store().all_variables().await?;
    let dt_formats = crate::settings::resolve_datetime_formats(state.store())
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let dt = crate::datetime_fmt::DateTimeResolver {
        formats: &dt_formats,
        now: chrono::Local::now(),
    };
    enum RenderFormat {
        Png,
        Pdf,
    }

    let format = match query.format.as_deref() {
        None | Some("") | Some("png") => RenderFormat::Png,
        Some("pdf") => {
            if color_mode == ColorMode::BiLevel {
                return Err(AppError::invalid_request(
                    Reason::BilevelRequiresPng,
                    "bilevel is only supported for png output",
                ));
            }
            RenderFormat::Pdf
        }
        Some(other) => {
            return Err(AppError::invalid_request(
                Reason::FormatUnknown,
                format!("unknown format '{other}'; use png or pdf"),
            ));
        }
    };

    let env = resolve_environment(template, &variables, &dt)?;
    crate::render::validate_label_data_keys(template, &req.data)?;

    let (bytes, content_type) = match format {
        RenderFormat::Png => (
            render_single_label_image(template, &req.data, &env, img_opts)?,
            "image/png",
        ),
        RenderFormat::Pdf => (
            render_single_label_pdf(template, &req.data, &env)?,
            "application/pdf",
        ),
    };

    Ok((
        axum::http::StatusCode::OK,
        [("content-type", content_type)],
        bytes,
    )
        .into_response())
}

#[utoipa::path(get, path = "/favorites", tag = "favorites",
    responses((status = 200, description = "Favorited template ids", body = Vec<String>)))]
pub async fn list_favorites(
    State(state): State<Arc<AppState>>,
    axum::Extension(principal): axum::Extension<crate::middleware::Principal>,
) -> Result<Json<Vec<String>>, AppError> {
    let ids = state.store().list_favorites(&principal.actor_id()).await?;
    let registry = state.templates.load_full();
    Ok(Json(
        ids.into_iter()
            .filter(|id| registry.get(id).is_some())
            .collect(),
    ))
}

#[utoipa::path(put, path = "/favorites/{template_id}", tag = "favorites",
    params(("template_id" = String, Path, description = "Template ID")),
    responses((status = 204, description = "Favorited"), (status = 404, description = "Unknown template", body = ErrorResponse)))]
pub async fn add_favorite(
    State(state): State<Arc<AppState>>,
    axum::Extension(principal): axum::Extension<crate::middleware::Principal>,
    Path(template_id): Path<String>,
) -> Result<Response, AppError> {
    let _guard = state.write_lock.lock().await;
    // Under the lock, not before it: an in-flight delete would otherwise prune between this check
    // and the insert below, leaving exactly the stale row the prune exists to remove (#140).
    if state.templates.load_full().get(&template_id).is_none() {
        return Err(AppError::not_found(NotFoundKind::Template, template_id));
    }
    state
        .store()
        .add_favorite(&principal.actor_id(), &template_id)
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

#[utoipa::path(delete, path = "/favorites/{template_id}", tag = "favorites",
    params(("template_id" = String, Path, description = "Template ID")),
    responses((status = 204, description = "Unfavorited (idempotent)")))]
pub async fn remove_favorite(
    State(state): State<Arc<AppState>>,
    axum::Extension(principal): axum::Extension<crate::middleware::Principal>,
    Path(template_id): Path<String>,
) -> Result<Response, AppError> {
    let _guard = state.write_lock.lock().await;
    state
        .store()
        .remove_favorite(&principal.actor_id(), &template_id)
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

#[utoipa::path(get, path = "/recent-templates", tag = "favorites",
    responses((status = 200, description = "The caller's 6 most recently printed template ids", body = Vec<String>)))]
pub async fn recent_templates(
    State(state): State<Arc<AppState>>,
    axum::Extension(principal): axum::Extension<crate::middleware::Principal>,
) -> Result<Json<Vec<String>>, AppError> {
    let ids = state
        .store()
        .recent_templates(&principal.actor_id())
        .await?;
    let registry = state.templates.load_full();
    Ok(Json(
        ids.into_iter()
            .filter(|id| registry.get(id).is_some())
            .collect(),
    ))
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

/// Validate a new account/password: non-empty username, non-empty password. Returns 400 otherwise
/// (an empty password is a footgun; run with LABELER_NO_AUTH instead of an empty-password account).
fn validate_new_account(username: &str, password: &str) -> Result<(), AppError> {
    if username.trim().is_empty() {
        return Err(AppError::invalid_request(
            Reason::UsernameEmpty,
            "username must not be empty",
        ));
    }
    validate_password(password)
}

fn validate_password(password: &str) -> Result<(), AppError> {
    if password.is_empty() {
        return Err(AppError::invalid_request(
            Reason::PasswordEmpty,
            "password must not be empty",
        ));
    }
    Ok(())
}

/// Authentication state for the SPA, returned by `GET /auth/me`.
/// This type is the OpenAPI schema only; the `me` handler constructs the JSON response directly with `serde_json::json!`, so changes here must be mirrored in the handler.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct AuthStatus {
    pub authed: bool,
    #[serde(rename = "needsSetup")]
    pub needs_setup: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub me: Option<UserSummary>,
    #[serde(rename = "noAuth", skip_serializing_if = "std::ops::Not::not")]
    pub no_auth: bool,
}

/// A user as exposed by the API (never includes the password hash).
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct UserSummary {
    pub id: String,
    pub username: String,
}

/// An API token's public metadata (the secret is only ever returned once, at creation).
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct TokenSummary {
    pub id: String,
    pub name: String,
    pub last_used_at: Option<String>,
    pub created_at: String,
}

/// The one-time response to `POST /tokens`, carrying the plaintext secret.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct TokenCreated {
    pub id: String,
    pub name: String,
    pub secret: String,
}

/// A trivial `{ "ok": true }` acknowledgement.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct OkResponse {
    pub ok: bool,
}

#[utoipa::path(
    post,
    path = "/auth/setup",
    tag = "auth",
    request_body = Credentials,
    responses(
        (status = 200, description = "First user created and logged in", body = OkResponse),
        (status = 409, description = "Setup already completed", body = ErrorResponse)
    )
)]
pub async fn setup(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    req_https: HttpsHint,
    Json(body): Json<Credentials>,
) -> Result<Response, AppError> {
    let _guard = state.write_lock.lock().await;
    if state.store().count_users().await.map_err(AppError::from)? > 0 {
        return Err(AppError::conflict("setup already completed"));
    }
    validate_new_account(&body.username, &body.password)?;
    let hash = crate::auth::hash_password(&body.password)
        .map_err(|_| AppError::internal("hash failed"))?;
    let user = state
        .store()
        .create_user(&body.username, &hash)
        .await
        .map_err(AppError::from)?;
    start_session(&state, jar, &user.id, req_https.0).await
}

#[utoipa::path(
    post,
    path = "/auth/login",
    tag = "auth",
    request_body = Credentials,
    responses(
        (status = 200, description = "Logged in; sets a session cookie", body = OkResponse),
        (status = 401, description = "Invalid credentials", body = ErrorResponse),
        (status = 403, description = "Cross-origin request rejected", body = ErrorResponse)
    )
)]
pub async fn login(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    req_https: HttpsHint,
    Json(body): Json<Credentials>,
) -> Result<Response, AppError> {
    match state
        .store()
        .get_user_by_username(&body.username)
        .await
        .map_err(AppError::from)?
    {
        Some(user) if crate::auth::verify_password(&body.password, &user.password_hash) => {
            start_session(&state, jar, &user.id, req_https.0).await
        }
        Some(_) => Err(AppError::unauthorized()),
        None => {
            crate::auth::dummy_verify(&body.password);
            Err(AppError::unauthorized())
        }
    }
}

async fn start_session(
    state: &AppState,
    jar: CookieJar,
    user_id: &str,
    https: bool,
) -> Result<Response, AppError> {
    // Rotate: invalidate any session the incoming cookie referenced (session-fixation defense).
    if let Some(old) = jar.get(crate::middleware::SESSION_COOKIE) {
        let _ = state
            .store()
            .delete_session(&crate::auth::sha256_hex(old.value()))
            .await;
    }
    let secret = crate::auth::random_secret();
    state
        .store()
        .create_session(&crate::auth::sha256_hex(&secret), user_id, "+30 days")
        .await
        .map_err(AppError::from)?;
    let jar = jar.add(crate::middleware::session_cookie(secret, https));
    Ok((jar, Json(serde_json::json!({"ok": true}))).into_response())
}

#[utoipa::path(
    post,
    path = "/auth/logout",
    tag = "auth",
    responses(
        (status = 200, description = "Session cleared", body = OkResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 403, description = "Cross-origin request rejected", body = ErrorResponse)
    )
)]
pub async fn logout(State(state): State<Arc<AppState>>, jar: CookieJar) -> Response {
    if let Some(c) = jar.get(crate::middleware::SESSION_COOKIE) {
        let _ = state
            .store()
            .delete_session(&crate::auth::sha256_hex(c.value()))
            .await;
    }
    (
        jar.add(crate::middleware::clear_cookie()),
        Json(serde_json::json!({"ok": true})),
    )
        .into_response()
}

/// Mark a response uncacheable so the browser and any reverse proxy never serve a stale auth state
/// (a cached `/auth/me` would strand the SPA on `/setup` after first-account setup; see #103).
fn no_store(mut resp: Response) -> Response {
    resp.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    resp
}

// `/auth/me` is AUTH-EXEMPT (it must answer for logged-OUT callers too), so it resolves auth itself
// (optional) and always returns 200 with the auth state the SPA needs.
#[utoipa::path(
    get,
    path = "/auth/me",
    tag = "auth",
    responses(
        (status = 200, description = "Current auth state (authed flag, needsSetup, optional user)", body = AuthStatus)
    )
)]
pub async fn me(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Response, AppError> {
    if state.no_auth() {
        return Ok(no_store(
            Json(serde_json::json!({
                "authed": true,
                "needsSetup": false,
                "me": {"id": "local", "username": "local"},
                "noAuth": true
            }))
            .into_response(),
        ));
    }
    if let Some(crate::middleware::Principal::User { id, username, .. }) =
        crate::middleware::resolve_optional(&state, &headers).await
    {
        return Ok(no_store(
            Json(serde_json::json!({
                "authed": true,
                "needsSetup": false,
                "me": {"id": id, "username": username}
            }))
            .into_response(),
        ));
    }
    let needs_setup = state.store().count_users().await.map_err(AppError::from)? == 0;
    Ok(no_store(
        Json(serde_json::json!({"authed": false, "needsSetup": needs_setup})).into_response(),
    ))
}

#[utoipa::path(
    get,
    path = "/users",
    tag = "auth",
    responses(
        (status = 200, description = "List users", body = [UserSummary]),
        (status = 401, description = "Not authenticated", body = ErrorResponse)
    )
)]
pub async fn list_users(State(state): State<Arc<AppState>>) -> Result<Response, AppError> {
    let users = state.store().list_users().await.map_err(AppError::from)?;
    Ok(Json(
        users
            .into_iter()
            .map(|u| serde_json::json!({"id": u.id, "username": u.username}))
            .collect::<Vec<_>>(),
    )
    .into_response())
}

#[utoipa::path(
    post,
    path = "/users",
    tag = "auth",
    request_body = Credentials,
    responses(
        (status = 201, description = "User created", body = UserSummary),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 409, description = "Username already exists", body = ErrorResponse)
    )
)]
pub async fn create_user_h(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Credentials>,
) -> Result<Response, AppError> {
    validate_new_account(&body.username, &body.password)?;
    let _guard = state.write_lock.lock().await;
    // The write-lock serializes writers, so a check-then-insert is race-free here and yields a clean 409
    // instead of a 500 from the UNIQUE constraint.
    if state
        .store()
        .get_user_by_username(&body.username)
        .await
        .map_err(AppError::from)?
        .is_some()
    {
        return Err(AppError::conflict("username already exists"));
    }
    let hash = crate::auth::hash_password(&body.password)
        .map_err(|_| AppError::internal("hash failed"))?;
    let u = state
        .store()
        .create_user(&body.username, &hash)
        .await
        .map_err(AppError::from)?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({"id": u.id, "username": u.username})),
    )
        .into_response())
}

#[utoipa::path(
    delete,
    path = "/users/{id}",
    tag = "auth",
    params(("id" = String, Path, description = "User ID")),
    responses(
        (status = 204, description = "User deleted"),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 404, description = "User not found", body = ErrorResponse),
        (status = 409, description = "Cannot delete the last user or your own account", body = ErrorResponse)
    )
)]
pub async fn delete_user_h(
    State(state): State<Arc<AppState>>,
    axum::Extension(p): axum::Extension<crate::middleware::Principal>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let _guard = state.write_lock.lock().await;
    if state.store().count_users().await.map_err(AppError::from)? <= 1 {
        return Err(AppError::conflict("cannot delete the last user"));
    }
    // Deleting your own account cascades your session (FK ON DELETE CASCADE), silently logging you out;
    // block it so the action is refused with a clear message rather than bouncing the caller to login.
    if let crate::middleware::Principal::User { id: me, .. } = &p {
        if me == &id {
            return Err(AppError::conflict("cannot delete your own account"));
        }
    }
    if !state
        .store()
        .delete_user(&id)
        .await
        .map_err(AppError::from)?
    {
        return Err(AppError::not_found(NotFoundKind::User, &id));
    }
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct PasswordChange {
    pub current_password: String,
    pub new_password: String,
}

#[utoipa::path(
    post,
    path = "/auth/password",
    tag = "auth",
    request_body = PasswordChange,
    responses(
        (status = 200, description = "Password changed; other sessions revoked", body = OkResponse),
        (status = 401, description = "Current password incorrect or not authenticated", body = ErrorResponse),
        (status = 403, description = "An API token cannot change a password", body = ErrorResponse)
    )
)]
pub async fn change_password(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    axum::Extension(p): axum::Extension<crate::middleware::Principal>,
    Json(body): Json<PasswordChange>,
) -> Result<Response, AppError> {
    let crate::middleware::Principal::User {
        id,
        by_token: false,
        ..
    } = p
    else {
        return Err(AppError::forbidden("token cannot change a password"));
    };
    let user = state
        .store()
        .get_user_by_id(&id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::unauthorized)?;
    if !crate::auth::verify_password(&body.current_password, &user.password_hash) {
        return Err(AppError::unauthorized());
    }
    validate_password(&body.new_password)?;
    let _guard = state.write_lock.lock().await;
    let hash = crate::auth::hash_password(&body.new_password)
        .map_err(|_| AppError::internal("hash failed"))?;
    state
        .store()
        .set_user_password(&id, &hash)
        .await
        .map_err(AppError::from)?;
    let keep = jar
        .get(crate::middleware::SESSION_COOKIE)
        .map(|c| crate::auth::sha256_hex(c.value()))
        .unwrap_or_default();
    state
        .store()
        .delete_user_sessions_except(&id, &keep)
        .await
        .map_err(AppError::from)?;
    Ok(Json(serde_json::json!({"ok": true})).into_response())
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct TokenCreate {
    pub name: String,
}

/// The calling user's id for the token endpoints. No-auth mode refuses `/tokens*` in the middleware,
/// so `Local` cannot reach a handler; it is refused rather than trusted.
fn token_owner(principal: &crate::middleware::Principal) -> Result<&str, AppError> {
    match principal {
        crate::middleware::Principal::User { id, .. } => Ok(id),
        crate::middleware::Principal::Local => {
            Err(AppError::forbidden("authentication is disabled"))
        }
    }
}

#[utoipa::path(
    get,
    path = "/tokens",
    tag = "auth",
    responses(
        (status = 200, description = "The caller's own API tokens (never the secret)", body = [TokenSummary]),
        (status = 401, description = "Not authenticated", body = ErrorResponse)
    )
)]
pub async fn list_tokens(
    State(state): State<Arc<AppState>>,
    axum::Extension(p): axum::Extension<crate::middleware::Principal>,
) -> Result<Response, AppError> {
    let t = state
        .store()
        .list_tokens(token_owner(&p)?)
        .await
        .map_err(AppError::from)?;
    Ok(Json(
        t.into_iter()
            .map(|t| {
                serde_json::json!({"id": t.id, "name": t.name, "last_used_at": t.last_used_at, "created_at": t.created_at})
            })
            .collect::<Vec<_>>(),
    )
    .into_response())
}

#[utoipa::path(
    post,
    path = "/tokens",
    tag = "auth",
    request_body = TokenCreate,
    responses(
        (status = 201, description = "Token created for the caller; secret returned once", body = TokenCreated),
        (status = 401, description = "Not authenticated", body = ErrorResponse)
    )
)]
pub async fn create_token_h(
    State(state): State<Arc<AppState>>,
    axum::Extension(p): axum::Extension<crate::middleware::Principal>,
    Json(body): Json<TokenCreate>,
) -> Result<Response, AppError> {
    let owner = token_owner(&p)?;
    let _guard = state.write_lock.lock().await;
    let secret = format!("lbl_{}", crate::auth::random_secret());
    let t = state
        .store()
        .create_token(owner, &body.name, &crate::auth::sha256_hex(&secret))
        .await
        .map_err(AppError::from)?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({"id": t.id, "name": t.name, "secret": secret})),
    )
        .into_response())
}

#[utoipa::path(
    delete,
    path = "/tokens/{id}",
    tag = "auth",
    params(("id" = String, Path, description = "Token ID")),
    responses(
        (status = 204, description = "Token revoked"),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 404, description = "No such token of the caller's", body = ErrorResponse)
    )
)]
pub async fn delete_token_h(
    State(state): State<Arc<AppState>>,
    axum::Extension(p): axum::Extension<crate::middleware::Principal>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let owner = token_owner(&p)?;
    let _guard = state.write_lock.lock().await;
    if !state
        .store()
        .delete_token(owner, &id)
        .await
        .map_err(AppError::from)?
    {
        return Err(AppError::not_found(NotFoundKind::Token, &id));
    }
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

// Effective-https extractor for the cookie Secure flag (proxy-aware), used by setup/login.
pub struct HttpsHint(pub bool);
impl FromRequestParts<Arc<AppState>> for HttpsHint {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        Ok(HttpsHint(crate::middleware::effective_https(
            &parts.headers,
            &parts.uri,
            state.trust_proxy(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_and_normalize_url_accepts_valid_urls() {
        assert_eq!(
            validate_and_normalize_url("http://example.com", UrlField::Base).unwrap(),
            "http://example.com"
        );
        assert_eq!(
            validate_and_normalize_url("https://example.com/", UrlField::Base).unwrap(),
            "https://example.com"
        );
        assert_eq!(
            validate_and_normalize_url("  https://example.com/sub/path///  ", UrlField::Public)
                .unwrap(),
            "https://example.com/sub/path"
        );
        assert_eq!(
            validate_and_normalize_url("http://hb.lan:7745", UrlField::Base).unwrap(),
            "http://hb.lan:7745"
        );
    }

    #[test]
    fn validate_and_normalize_url_rejects_invalid_urls() {
        // Bad scheme
        let err = validate_and_normalize_url("ftp://example.com", UrlField::Public).unwrap_err();
        assert_eq!(err.reason(), Some("public_url_invalid"));
        assert!(err.message_text().contains("must use http or https scheme"));

        // Missing host
        let err = validate_and_normalize_url("http://", UrlField::Base).unwrap_err();
        assert_eq!(err.reason(), Some("base_url_invalid"));

        // Userinfo with password
        let err = validate_and_normalize_url("https://user:pass@example.com", UrlField::Public)
            .unwrap_err();
        assert_eq!(err.reason(), Some("public_url_invalid"));
        assert!(err.message_text().contains("must not contain userinfo"));

        // Userinfo without password (username only)
        let err =
            validate_and_normalize_url("https://user@example.com", UrlField::Base).unwrap_err();
        assert_eq!(err.reason(), Some("base_url_invalid"));
        assert!(err.message_text().contains("must not contain userinfo"));

        // Query parameters
        let err =
            validate_and_normalize_url("http://example.com?query=1", UrlField::Public).unwrap_err();
        assert_eq!(err.reason(), Some("public_url_invalid"));
        assert!(err
            .message_text()
            .contains("must not contain query parameters or fragments"));

        // Fragments
        let err =
            validate_and_normalize_url("http://example.com#section", UrlField::Base).unwrap_err();
        assert_eq!(err.reason(), Some("base_url_invalid"));
        assert!(err
            .message_text()
            .contains("must not contain query parameters or fragments"));

        // Parse failure
        let err = validate_and_normalize_url("not a url", UrlField::Base).unwrap_err();
        assert_eq!(err.reason(), Some("base_url_invalid"));
        assert_eq!(err.message_text(), "invalid base_url");

        let err = validate_and_normalize_url("not a url", UrlField::Public).unwrap_err();
        assert_eq!(err.reason(), Some("public_url_invalid"));
        assert_eq!(err.message_text(), "invalid public_url");
    }

    #[test]
    fn connection_view_from_connection() {
        let conn = crate::store::Connection {
            id: "conn-1".into(),
            connector: "homebox".into(),
            name: "Homebox".into(),
            base_url: "http://hb.lan:7745".into(),
            public_url: Some("https://homebox.example.com".into()),
            credential: "secret".into(),
        };
        let view = ConnectionView::from(&conn);
        assert_eq!(
            view,
            ConnectionView {
                id: "conn-1".into(),
                connector: "homebox".into(),
                name: "Homebox".into(),
                base_url: "http://hb.lan:7745".into(),
                public_url: Some("https://homebox.example.com".into()),
                has_credential: true,
            }
        );

        let conn_no_cred = crate::store::Connection {
            id: "conn-2".into(),
            connector: "homebox".into(),
            name: "Homebox".into(),
            base_url: "http://hb.lan:7745".into(),
            public_url: None,
            credential: "".into(),
        };
        let view2 = ConnectionView::from(&conn_no_cred);
        assert!(!view2.has_credential);
        assert_eq!(view2.public_url, None);
    }
}
