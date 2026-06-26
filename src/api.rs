use arc_swap::ArcSwap;
use axum::{
    extract::rejection::JsonRejection,
    extract::{DefaultBodyLimit, FromRequestParts, Json, Path, Query, State},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Router,
};
use axum_extra::extract::cookie::CookieJar;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    connector::{BrowsePage, BrowseRequest, ConnectorSchema, LabelRow, MaterializeRequest},
    errors::AppError,
    models::{
        BatchRequest, BatchRowError, BatchSummary, ErrorResponse, HealthResponse, PrintRequest,
        ReloadResponse, RenderLabelRequest, TemplateDetail, TemplateList, VariableValue,
    },
    openapi::ApiDoc,
    parse::parse_template,
    render::{render_single_label_image, render_single_label_pdf, ColorMode, ImageRenderOptions},
    store::{Printer, Store},
    templates::{TemplateDefinition, TemplateRegistry, TemplateRegistryError},
};

const MAX_BATCH_LABELS: usize = 500;
const MAX_PRINT_COPIES: u32 = 100;
const MAX_RENDER_DPI: u32 = 1200;

#[derive(serde::Deserialize)]
pub struct RenderQuery {
    pub format: Option<String>,
    pub color_mode: Option<String>,
    pub resolution: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct ImportCsvQuery {
    pub template: String,
    pub mode: Option<String>,
    pub printer: Option<String>,
    pub format: Option<String>,
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
    cursor_key: crate::connector::cursor::SigningKey,
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
            no_auth: std::env::var("LABELER_NO_AUTH")
                .map(|v| v == "true")
                .unwrap_or(false),
            egress: crate::egress::Egress::new(),
            connectors: crate::connector::ConnectorRegistry::default(),
            cursor_key: crate::connector::cursor::SigningKey::random(),
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

    pub fn cursor_key(&self) -> &crate::connector::cursor::SigningKey {
        &self.cursor_key
    }

    #[cfg(test)]
    pub fn with_loopback_egress(mut self) -> Self {
        self.egress = crate::egress::Egress::with_loopback();
        self
    }

    // Synchronous filesystem I/O. Acceptable for the single-user, local-templates-dir target and
    // consistent with the synchronous Typst render path; revisit with spawn_blocking if it ever
    // serves large dirs or remote storage.
    fn reload(&self) -> Result<usize, TemplateRegistryError> {
        let registry = TemplateRegistry::load_from_dir(&self.templates_dir)?;
        let count = registry.len();
        self.templates.store(Arc::new(registry));
        Ok(count)
    }
}

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
        .route("/templates/{id}/thumbnail", get(thumbnail))
        .route("/printers", get(list_printers).post(create_printer))
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
        .route("/batch", post(batch))
        .route(
            "/print",
            post(print_label).layer(DefaultBodyLimit::max(64 * 1024)),
        )
        .route("/import/csv", post(import_csv))
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
        .merge(SwaggerUi::new("/docs").url("/api/openapi.json", ApiDoc::openapi()))
}

async fn openapi_json() -> Response {
    Json(ApiDoc::openapi()).into_response()
}

pub fn app(state: Arc<AppState>) -> Router {
    let assets = tower_http::services::ServeDir::new(state.ui_dir().join("assets"));
    let api = api_router().layer(axum::middleware::from_fn_with_state(
        state.clone(),
        crate::middleware::require_auth,
    ));
    Router::new()
        .nest("/api", api)
        .nest_service("/assets", assets)
        .fallback(fallback)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn fallback(State(state): State<Arc<AppState>>, uri: axum::http::Uri) -> Response {
    if uri.path() == "/api" || uri.path().starts_with("/api/") {
        return AppError::not_found(uri.path()).into_response();
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
pub async fn list_templates(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let templates = state.templates.load_full().summaries();
    Json(TemplateList { templates })
}

#[utoipa::path(
    post,
    path = "/templates/reload",
    responses(
        (status = 200, description = "Templates reloaded from disk", body = ReloadResponse),
        (status = 422, description = "A template on disk is invalid; previous set kept", body = ErrorResponse),
        (status = 500, description = "Failed to read the templates directory", body = ErrorResponse)
    )
)]
pub async fn reload_templates(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ReloadResponse>, AppError> {
    let count = state.reload()?;
    Ok(Json(ReloadResponse { count }))
}

fn template_file_path(dir: &std::path::Path, id: &str) -> Result<PathBuf, AppError> {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::invalid_request(format!(
            "template id '{id}' must be non-empty and contain only letters, digits, '-' or '_'"
        )));
    }
    Ok(dir.join(format!("{id}.yaml")))
}

fn parse_and_validate(body: &str) -> Result<TemplateDefinition, AppError> {
    let template =
        parse_template(body).map_err(|err| AppError::template_invalid(err.to_string()))?;
    template.validate().map_err(AppError::template_invalid)?;
    Ok(template)
}

fn write_template_file(path: &std::path::Path, body: &str) -> Result<(), AppError> {
    let dir = path
        .parent()
        .ok_or_else(|| AppError::render_failed("invalid template path"))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::render_failed("invalid template path"))?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = dir.join(format!(".{file_name}.{nonce}.tmp"));
    std::fs::write(&tmp, body)
        .map_err(|err| AppError::render_failed(format!("failed to write template: {err}")))?;
    std::fs::rename(&tmp, path)
        .map_err(|err| AppError::render_failed(format!("failed to persist template: {err}")))?;
    Ok(())
}

#[utoipa::path(
    post,
    path = "/templates",
    request_body(content = String, description = "Template YAML", content_type = "text/yaml"),
    responses(
        (status = 201, description = "Template created", body = TemplateDetail),
        (status = 409, description = "Template id already exists", body = ErrorResponse),
        (status = 422, description = "Invalid template", body = ErrorResponse)
    )
)]
pub async fn create_template(
    State(state): State<Arc<AppState>>,
    body: String,
) -> Result<Response, AppError> {
    let template = parse_and_validate(&body)?;
    let id = template.id.clone();
    let path = template_file_path(&state.templates_dir, &id)?;
    let _guard = state.write_lock.lock().await;
    if path.exists() {
        return Err(AppError::template_exists(&id));
    }
    write_template_file(&path, &body)?;
    state.reload()?;
    let detail = state
        .templates
        .load_full()
        .detail(&id)
        .ok_or_else(|| AppError::render_failed("template missing after write"))?;
    Ok((axum::http::StatusCode::CREATED, Json(detail)).into_response())
}

#[utoipa::path(
    put,
    path = "/templates/{id}",
    params(("id" = String, Path, description = "Template ID")),
    request_body(content = String, description = "Template YAML", content_type = "text/yaml"),
    responses(
        (status = 200, description = "Template replaced", body = TemplateDetail),
        (status = 400, description = "Body id does not match path id", body = ErrorResponse),
        (status = 404, description = "Template not found", body = ErrorResponse),
        (status = 422, description = "Invalid template", body = ErrorResponse)
    )
)]
pub async fn replace_template(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    body: String,
) -> Result<Response, AppError> {
    let template = parse_and_validate(&body)?;
    if template.id != id {
        return Err(AppError::invalid_request(format!(
            "template id in body ('{}') must match path id ('{id}')",
            template.id
        )));
    }
    let path = template_file_path(&state.templates_dir, &id)?;
    let _guard = state.write_lock.lock().await;
    if !path.exists() {
        return Err(AppError::template_not_found(id));
    }
    write_template_file(&path, &body)?;
    state.reload()?;
    let detail = state
        .templates
        .load_full()
        .detail(&id)
        .ok_or_else(|| AppError::render_failed("template missing after write"))?;
    Ok((axum::http::StatusCode::OK, Json(detail)).into_response())
}

#[utoipa::path(
    delete,
    path = "/templates/{id}",
    params(("id" = String, Path, description = "Template ID")),
    responses(
        (status = 204, description = "Template deleted"),
        (status = 404, description = "Template not found", body = ErrorResponse)
    )
)]
pub async fn delete_template(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let path = template_file_path(&state.templates_dir, &id)?;
    let _guard = state.write_lock.lock().await;
    if !path.exists() {
        return Err(AppError::template_not_found(id));
    }
    std::fs::remove_file(&path)
        .map_err(|err| AppError::render_failed(format!("failed to delete template: {err}")))?;
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
    state
        .templates
        .load_full()
        .detail(&id)
        .map(Json)
        .ok_or_else(|| AppError::template_not_found(id))
}

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
        .ok_or_else(|| AppError::template_not_found(id.clone()))?;
    let hash = registry
        .content_hash(&id)
        .expect("content_hash present for a loaded template");
    let etag = format!("\"{}\"", hash);

    if let Some(inm) = headers.get(axum::http::header::IF_NONE_MATCH) {
        if inm.to_str().map(|v| v == "*" || v == etag).unwrap_or(false) {
            return Ok((
                axum::http::StatusCode::NOT_MODIFIED,
                [(axum::http::header::ETAG, etag.as_str())],
            )
                .into_response());
        }
    }

    let data = crate::render::placeholder_data(template);
    let option = crate::render::default_option_selection(template);
    let variables = state.store().all_variables().await?;
    let dt_formats = crate::settings::resolve_datetime_formats(state.store())
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let dt = crate::datetime_fmt::DateTimeResolver {
        formats: &dt_formats,
        now: chrono::Local::now(),
    };
    let png =
        crate::render::render_thumbnail_png(template, &data, option.as_ref(), &variables, &dt)?;

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

fn validate_printer(printer: &Printer) -> Result<(), AppError> {
    if printer.id.is_empty()
        || !printer
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::invalid_request(format!(
            "printer id '{}' must be non-empty and contain only letters, digits, '-' or '_'",
            printer.id
        )));
    }
    if printer.name.trim().is_empty() {
        return Err(AppError::printer_invalid("printer name must not be empty"));
    }
    crate::driver::validate_config(&printer.kind, &printer.config)
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
    let mut printers = state.store().list_printers().await?;
    for p in &mut printers {
        p.config = crate::driver::redact_config(&p.kind, &p.config);
    }
    Ok(Json(printers))
}

#[utoipa::path(
    post,
    path = "/printers",
    request_body = Printer,
    responses(
        (status = 201, description = "Printer created", body = Printer),
        (status = 409, description = "Printer id already exists", body = ErrorResponse),
        (status = 422, description = "Invalid printer", body = ErrorResponse)
    )
)]
pub async fn create_printer(
    State(state): State<Arc<AppState>>,
    Json(mut printer): Json<Printer>,
) -> Result<Response, AppError> {
    validate_printer(&printer)?;
    let _guard = state.write_lock.lock().await;
    if state.store().get_printer(&printer.id).await?.is_some() {
        return Err(AppError::printer_exists(&printer.id));
    }
    crate::driver::merge_secrets(&printer.kind, &mut printer.config, None);
    state.store().upsert_printer(&printer).await?;
    printer.config = crate::driver::redact_config(&printer.kind, &printer.config);
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
    let mut printer = state
        .store()
        .get_printer(&id)
        .await?
        .ok_or_else(|| AppError::printer_not_found(id))?;
    printer.config = crate::driver::redact_config(&printer.kind, &printer.config);
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
        return Err(AppError::invalid_request(format!(
            "variable key '{key}' must be non-empty and contain only letters, digits, '_', '-' or '.'"
        )));
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
    use std::collections::BTreeMap;
    let stored = state
        .store()
        .get_setting(crate::settings::JOB_LOG_RETENTION_DAYS)
        .await?;
    let is_default = stored.is_none();
    let days = crate::settings::resolve_retention_days_from(stored)
        .map_err(|e| AppError::internal(e.to_string()))?;
    let mut out: BTreeMap<String, ResolvedSetting> = BTreeMap::new();
    out.insert(
        crate::settings::JOB_LOG_RETENTION_DAYS.to_string(),
        ResolvedSetting {
            value: serde_json::json!(days),
            is_default,
        },
    );
    let dt_stored = state
        .store()
        .get_setting(crate::settings::DATETIME_FORMATS)
        .await?;
    let dt_is_default = dt_stored.is_none();
    let dt_formats = crate::settings::resolve_datetime_formats_from(dt_stored)
        .map_err(|e| AppError::internal(e.to_string()))?;
    out.insert(
        crate::settings::DATETIME_FORMATS.to_string(),
        ResolvedSetting {
            value: serde_json::json!(dt_formats),
            is_default: dt_is_default,
        },
    );
    Ok(Json(out).into_response())
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
        return Err(AppError::setting_not_found(&key));
    }
    let canonical =
        crate::settings::validate(&key, &body.value).map_err(AppError::invalid_request)?;
    let _guard = state.write_lock.lock().await;
    state.store().set_setting(&key, &canonical).await?;
    // canonical is the validated integer text; reflect it back as a JSON number
    let value: serde_json::Value = canonical
        .parse::<u32>()
        .map(serde_json::Value::from)
        .unwrap_or(body.value);
    Ok(Json(ResolvedSetting {
        value,
        is_default: false,
    })
    .into_response())
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
        return Err(AppError::setting_not_found(&key));
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
    crate::datetime_fmt::validate_pattern(&req.pattern).map_err(AppError::invalid_request)?;
    let sample = crate::datetime_fmt::format_now(&req.pattern, chrono::Local::now());
    Ok(Json(DatetimePreviewResponse { sample }).into_response())
}

#[utoipa::path(
    put,
    path = "/printers/{id}",
    params(("id" = String, Path, description = "Printer ID")),
    request_body = Printer,
    responses(
        (status = 200, description = "Printer replaced", body = Printer),
        (status = 400, description = "Body id does not match path id", body = ErrorResponse),
        (status = 404, description = "Printer not found", body = ErrorResponse),
        (status = 422, description = "Invalid printer", body = ErrorResponse)
    )
)]
pub async fn replace_printer(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(mut printer): Json<Printer>,
) -> Result<Response, AppError> {
    if printer.id != id {
        return Err(AppError::invalid_request(format!(
            "printer id in body ('{}') must match path id ('{id}')",
            printer.id
        )));
    }
    validate_printer(&printer)?;
    let _guard = state.write_lock.lock().await;
    let existing = state.store().get_printer(&id).await?;
    let Some(existing) = existing else {
        return Err(AppError::printer_not_found(id));
    };
    crate::driver::merge_secrets(&printer.kind, &mut printer.config, Some(&existing.config));
    state.store().upsert_printer(&printer).await?;
    printer.config = crate::driver::redact_config(&printer.kind, &printer.config);
    Ok((axum::http::StatusCode::OK, Json(printer)).into_response())
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
    if state.store().delete_printer(&id).await? {
        Ok(axum::http::StatusCode::NO_CONTENT.into_response())
    } else {
        Err(AppError::printer_not_found(id))
    }
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct ConnectionInput {
    pub connector: String,
    pub name: String,
    pub base_url: String,
    pub credential: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

fn connection_view(c: &crate::store::Connection) -> serde_json::Value {
    serde_json::json!({
        "id": c.id, "connector": c.connector, "name": c.name,
        "base_url": c.base_url, "enabled": c.enabled,
        "has_credential": !c.credential.is_empty()
    })
}

#[utoipa::path(
    get,
    path = "/connections",
    responses(
        (status = 200, description = "List connections (credential redacted; only has_credential exposed)", body = Object)
    )
)]
pub async fn list_connections(State(state): State<Arc<AppState>>) -> Result<Response, AppError> {
    let cs = state.store().list_connections().await?;
    Ok(Json(cs.iter().map(connection_view).collect::<Vec<_>>()).into_response())
}

#[utoipa::path(
    post,
    path = "/connections",
    request_body = ConnectionInput,
    responses(
        (status = 201, description = "Connection created (credential redacted in response)", body = Object),
        (status = 400, description = "Invalid request", body = ErrorResponse)
    )
)]
pub async fn create_connection(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ConnectionInput>,
) -> Result<Response, AppError> {
    if state.connectors().get(&body.connector).is_none() {
        return Err(AppError::invalid_request("unknown connector"));
    }
    let cred = body.credential.unwrap_or_default();
    if cred.is_empty() {
        return Err(AppError::invalid_request("credential required"));
    }
    url::Url::parse(&body.base_url).map_err(|_| AppError::invalid_request("invalid base_url"))?;
    let _g = state.write_lock.lock().await;
    let c = state
        .store()
        .create_connection(&body.connector, &body.name, &body.base_url, &cred)
        .await?;
    Ok((axum::http::StatusCode::CREATED, Json(connection_view(&c))).into_response())
}

#[utoipa::path(
    get,
    path = "/connections/{id}",
    params(("id" = String, Path, description = "Connection ID")),
    responses(
        (status = 200, description = "Connection (credential redacted)", body = Object),
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
        .ok_or_else(|| AppError::not_found(&id))?;
    Ok(Json(connection_view(&c)).into_response())
}

#[utoipa::path(
    put,
    path = "/connections/{id}",
    params(("id" = String, Path, description = "Connection ID")),
    request_body = ConnectionInput,
    responses(
        (status = 200, description = "Connection updated (credential redacted)", body = Object),
        (status = 404, description = "Connection not found", body = ErrorResponse)
    )
)]
pub async fn update_connection_h(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<ConnectionInput>,
) -> Result<Response, AppError> {
    url::Url::parse(&body.base_url).map_err(|_| AppError::invalid_request("invalid base_url"))?;
    let _g = state.write_lock.lock().await;
    let cred = body.credential.filter(|c| !c.is_empty());
    let ok = state
        .store()
        .update_connection(
            &id,
            &body.name,
            &body.base_url,
            cred.as_deref(),
            body.enabled,
        )
        .await?;
    if !ok {
        return Err(AppError::not_found(&id));
    }
    let c = state.store().get_connection(&id).await?.unwrap();
    Ok(Json(connection_view(&c)).into_response())
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
    if !state.store().delete_connection(&id).await? {
        return Err(AppError::not_found(&id));
    }
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

fn connector_status(
    e: &crate::connector::ConnectorError,
) -> (axum::http::StatusCode, &'static str, String) {
    use crate::connector::ConnectorError::*;
    use axum::http::StatusCode;
    match e {
        AuthFailed => (
            StatusCode::BAD_GATEWAY,
            "ConnectorAuthFailed",
            "upstream authentication failed".into(),
        ),
        Forbidden => (
            StatusCode::BAD_GATEWAY,
            "ConnectorForbidden",
            "upstream forbidden".into(),
        ),
        ConnectionFailed(m) => (StatusCode::BAD_GATEWAY, "ConnectorUnreachable", m.clone()),
        InvalidFilter(m) => (StatusCode::BAD_REQUEST, "InvalidFilter", m.clone()),
        UpstreamSchemaMismatch(m) => (StatusCode::BAD_GATEWAY, "UpstreamSchemaMismatch", m.clone()),
        RateLimited => (
            StatusCode::TOO_MANY_REQUESTS,
            "RateLimited",
            "upstream rate limited".into(),
        ),
        BudgetExceeded => (
            StatusCode::BAD_REQUEST,
            "BudgetExceeded",
            "too many rows requested".into(),
        ),
        Upstream(m) => (StatusCode::BAD_GATEWAY, "Upstream", m.clone()),
    }
}

fn connector_err(e: crate::connector::ConnectorError) -> AppError {
    let (status, code, msg) = connector_status(&e);
    AppError::new(status, code, msg, None)
}

async fn load_conn_and_connector<'a>(
    state: &'a AppState,
    id: &str,
) -> Result<(crate::store::Connection, &'a crate::connector::Connectors), AppError> {
    let conn = state
        .store()
        .get_connection(id)
        .await?
        .ok_or_else(|| AppError::not_found(id))?;
    let c = state
        .connectors()
        .get(&conn.connector)
        .ok_or_else(|| AppError::invalid_request("unknown connector"))?;
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
        .map_err(connector_err)?;
    Ok(Json(schema).into_response())
}

#[utoipa::path(
    post,
    path = "/connections/{id}/browse",
    params(("id" = String, Path, description = "Connection ID")),
    request_body = BrowseRequest,
    responses(
        (status = 200, description = "A page of browse rows with an opaque cursor", body = BrowsePage),
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
        .browse(&conn, state.egress(), state.cursor_key(), req)
        .await
        .map_err(connector_err)?;
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
        .map_err(connector_err)?;
    Ok(Json(rows).into_response())
}

struct ParsedCsvRow {
    data: std::collections::HashMap<String, serde_json::Value>,
    option: std::collections::BTreeMap<String, String>,
}

fn parse_csv_rows(body: &str) -> Result<Vec<ParsedCsvRow>, AppError> {
    let body = body.strip_prefix('\u{feff}').unwrap_or(body);
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .trim(csv::Trim::All)
        .from_reader(body.as_bytes());
    let headers = reader
        .headers()
        .map_err(|err| AppError::invalid_request(format!("invalid CSV header: {err}")))?
        .clone();
    let mut seen = std::collections::HashSet::new();
    for header in headers.iter() {
        let header = header.trim();
        if header.is_empty() || !seen.insert(header) {
            return Err(AppError::invalid_request(
                "CSV header has empty or duplicate column names",
            ));
        }
    }
    let mut rows = Vec::new();
    for record in reader.records() {
        let record =
            record.map_err(|err| AppError::invalid_request(format!("invalid CSV row: {err}")))?;
        let mut data = std::collections::HashMap::new();
        let mut option = std::collections::BTreeMap::new();
        for (key, val) in headers.iter().zip(record.iter()) {
            if let Some(name) = key.strip_prefix("option.") {
                option.insert(name.to_string(), val.to_string());
            } else {
                data.insert(key.to_string(), serde_json::Value::String(val.to_string()));
            }
        }
        rows.push(ParsedCsvRow { data, option });
    }
    if rows.is_empty() {
        return Err(AppError::invalid_request("CSV has no data rows"));
    }
    Ok(rows)
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

fn parse_batch_mode(mode: &str) -> Result<crate::batch::BatchMode, AppError> {
    match mode {
        "download" => Ok(crate::batch::BatchMode::Download),
        "print" => Ok(crate::batch::BatchMode::Print),
        other => Err(AppError::invalid_request(format!(
            "unknown mode '{other}'; use download or print"
        ))),
    }
}

/// Shared batch dispatch for `/batch` and `/import/csv`: validates constraints, then either renders a
/// download blob or runs the print send loop and returns a `BatchSummary`.
async fn run_batch(
    state: &Arc<AppState>,
    template: &TemplateDefinition,
    labels: &[crate::models::LabelInput],
    mode: crate::batch::BatchMode,
    printer: Option<&str>,
    format: Option<&str>,
    start_slot: u32,
) -> Result<Response, AppError> {
    let is_single = matches!(
        template.format,
        crate::models::TemplateFormat::Single { .. }
    );
    if start_slot > 0 && is_single {
        return Err(AppError::invalid_request(
            "start_slot applies only to sheet templates",
        ));
    }
    let variables = state.store().all_variables().await?;
    let dt_formats = crate::settings::resolve_datetime_formats(state.store())
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let dt = crate::datetime_fmt::DateTimeResolver {
        formats: &dt_formats,
        now: chrono::Local::now(),
    };
    let env = crate::batch::BatchEnv {
        settings: &variables,
        datetime: &dt,
    };

    match mode {
        crate::batch::BatchMode::Download => {
            let rendered = crate::batch::render_batch(
                template,
                labels,
                mode,
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
        crate::batch::BatchMode::Print => {
            if format.is_some() {
                return Err(AppError::invalid_request(
                    "format applies only to download; omit it when printing",
                ));
            }
            let printer_id = printer
                .ok_or_else(|| AppError::invalid_request("mode=print requires a printer"))?;
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
            let driver_format = match driver.accepted_format() {
                crate::driver::ArtifactFormat::Pdf => "pdf",
                crate::driver::ArtifactFormat::Png => "png",
                fmt => {
                    return Err(AppError::print_failed(format!(
                        "no renderer for artifact format {fmt:?}"
                    )))
                }
            };
            // Validate-then-execute: render everything first; bad data => 422 before any send.
            let rendered = crate::batch::render_batch(
                template,
                labels,
                mode,
                Some(driver_format),
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
                match driver
                    .send(&unit.bytes, &crate::driver::PrintOptions::default())
                    .await
                {
                    Ok(()) => {
                        let _ = state
                            .store()
                            .record_job(&template.id, Some(printer_id), "ok", None)
                            .await;
                    }
                    Err(err) => {
                        let msg = err.to_string();
                        let _ = state
                            .store()
                            .record_job(&template.id, Some(printer_id), "failed", Some(&msg))
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
                succeeded: total - failed.len(),
                failed,
                jobs,
            };
            Ok((axum::http::StatusCode::OK, Json(summary)).into_response())
        }
    }
}

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
    let mode = parse_batch_mode(&req.mode)?;
    run_batch(
        &state,
        template,
        &req.labels,
        mode,
        req.printer.as_deref(),
        req.format.as_deref(),
        req.start_slot,
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
        (status = 409, description = "Printer disabled", body = ErrorResponse),
        (status = 413, description = "Request body too large", body = ErrorResponse),
        (status = 502, description = "Printer transport failure", body = ErrorResponse)
    )
)]
pub async fn print_label(
    State(state): State<Arc<AppState>>,
    payload: Result<Json<PrintRequest>, JsonRejection>,
) -> Result<Response, AppError> {
    let Json(req) = payload.map_err(AppError::from)?;
    if !(1..=MAX_PRINT_COPIES).contains(&req.copies) {
        return Err(AppError::invalid_request(format!(
            "copies must be between 1 and {MAX_PRINT_COPIES}"
        )));
    }
    let registry = state.templates.load_full();
    let template = registry
        .get(&req.template)
        .ok_or_else(|| AppError::template_not_found(req.template.clone()))?;
    let label = crate::models::LabelInput {
        data: req.fields,
        option: req.option,
    };
    let labels = vec![label; req.copies as usize];
    run_batch(
        &state,
        template,
        &labels,
        crate::batch::BatchMode::Print,
        Some(&req.printer),
        None,
        0,
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
        (status = 422, description = "Validation error", body = ErrorResponse),
        (status = 501, description = "Not implemented", body = ErrorResponse)
    )
)]
pub async fn render_label(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RenderQuery>,
    payload: Result<Json<RenderLabelRequest>, JsonRejection>,
) -> Result<Response, AppError> {
    let Json(req) = payload.map_err(AppError::from)?;
    let registry = state.templates.load_full();
    let template = registry
        .get(&req.template)
        .ok_or_else(|| AppError::template_not_found(req.template.clone()))?;

    let option_value = req.label.option.as_ref();

    tracing::debug!(
        template = %template.id,
        option_count = option_value.map(|selection| selection.len()).unwrap_or(0),
        dpi = template.dpi,
        data_keys = req.label.data.len(),
        "render label request"
    );

    if let Some(options) = &template.options {
        if let Some(selection) = option_value {
            if !options.is_valid_selection(selection) {
                return Err(AppError::invalid_option_value(selection, options.allowed()));
            }
        }
    } else if option_value.is_some() {
        return Err(AppError::invalid_request(
            "template does not support options",
        ));
    }

    let color_mode = match query.color_mode.as_deref() {
        None | Some("") | Some("color") => ColorMode::Color,
        Some("bilevel") => ColorMode::BiLevel,
        Some(other) => {
            return Err(AppError::invalid_request(format!(
                "unknown color_mode '{other}'; use color or bilevel"
            )))
        }
    };
    let resolution_dpi = match query.resolution.as_deref() {
        None | Some("") => None,
        Some(s) => {
            let dpi: u32 = s.parse().map_err(|_| {
                AppError::invalid_request(format!(
                    "resolution must be a positive integer, got '{s}'"
                ))
            })?;
            if dpi == 0 || dpi > MAX_RENDER_DPI {
                return Err(AppError::invalid_request(format!(
                    "resolution must be between 1 and {MAX_RENDER_DPI}"
                )));
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
    let (bytes, content_type) = match query.format.as_deref() {
        None | Some("") | Some("png") => (
            render_single_label_image(
                template,
                &req.label.data,
                option_value,
                &variables,
                &dt,
                img_opts,
            )?,
            "image/png",
        ),
        Some("pdf") => {
            if color_mode == ColorMode::BiLevel {
                return Err(AppError::invalid_request(
                    "bilevel is only supported for png output",
                ));
            }
            (
                render_single_label_pdf(template, &req.label.data, option_value, &variables, &dt)?,
                "application/pdf",
            )
        }
        Some(other) => {
            return Err(AppError::invalid_request(format!(
                "unknown format '{other}'; use png or pdf"
            )))
        }
    };

    Ok((
        axum::http::StatusCode::OK,
        [("content-type", content_type)],
        bytes,
    )
        .into_response())
}

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
        (status = 200, description = "Download blob (zip/pdf) or print summary (BatchSummary)"),
        (status = 400, description = "Invalid CSV or request", body = ErrorResponse),
        (status = 404, description = "Template or printer not found", body = ErrorResponse),
        (status = 413, description = "Batch too large", body = ErrorResponse),
        (status = 422, description = "One or more rows invalid (batch is atomic)", body = ErrorResponse),
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
    let mode = parse_batch_mode(params.mode.as_deref().unwrap_or("download"))?;
    // Declared options for this template (name -> allowed values); the first value is the default.
    let empty = std::collections::BTreeMap::new();
    let declared: &std::collections::BTreeMap<String, Vec<String>> = template
        .options
        .as_ref()
        .map(|o| o.allowed())
        .unwrap_or(&empty);
    let parsed_rows = parse_csv_rows(&body)?;
    // Per SPEC section E, an unknown option.<name> column is an error, not silently ignored.
    for row in &parsed_rows {
        for name in row.option.keys() {
            if !declared.contains_key(name) {
                return Err(AppError::invalid_request(format!(
                    "CSV column 'option.{name}' is not a declared option of template '{}'",
                    template.id
                )));
            }
        }
    }
    let labels: Vec<crate::models::LabelInput> = parsed_rows
        .into_iter()
        .map(|row| {
            let mut option = std::collections::BTreeMap::new();
            for (name, vals) in declared {
                let v = row.option.get(name).cloned();
                let v = match v {
                    Some(s) if !s.is_empty() => s,
                    _ => vals.first().cloned().unwrap_or_default(),
                };
                option.insert(name.clone(), v);
            }
            crate::models::LabelInput {
                data: row.data,
                option: if option.is_empty() {
                    None
                } else {
                    Some(option)
                },
            }
        })
        .collect();
    run_batch(
        &state,
        template,
        &labels,
        mode,
        params.printer.as_deref(),
        params.format.as_deref(),
        0,
    )
    .await
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
        return Err(AppError::invalid_request("username must not be empty"));
    }
    validate_password(password)
}

fn validate_password(password: &str) -> Result<(), AppError> {
    if password.is_empty() {
        return Err(AppError::invalid_request("password must not be empty"));
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
        return Ok(Json(serde_json::json!({
            "authed": true,
            "needsSetup": false,
            "me": {"id": "local", "username": "local"},
            "noAuth": true
        }))
        .into_response());
    }
    if let Some(p) = crate::middleware::resolve_optional(&state, &headers).await {
        let me = match p {
            crate::middleware::Principal::User { id, username } => {
                serde_json::json!({"id": id, "username": username})
            }
            crate::middleware::Principal::Token { .. } => {
                serde_json::json!({"id": "token", "username": "api-token"})
            }
            // resolve_optional never returns Local, but the match must be exhaustive.
            crate::middleware::Principal::Local => {
                serde_json::json!({"id": "local", "username": "local"})
            }
        };
        return Ok(
            Json(serde_json::json!({"authed": true, "needsSetup": false, "me": me}))
                .into_response(),
        );
    }
    let needs_setup = state.store().count_users().await.map_err(AppError::from)? == 0;
    Ok(Json(serde_json::json!({"authed": false, "needsSetup": needs_setup})).into_response())
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
        return Err(AppError::not_found(&id));
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
    let crate::middleware::Principal::User { id, .. } = p else {
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

#[utoipa::path(
    get,
    path = "/tokens",
    tag = "auth",
    responses(
        (status = 200, description = "List API tokens (never the secret)", body = [TokenSummary]),
        (status = 401, description = "Not authenticated", body = ErrorResponse)
    )
)]
pub async fn list_tokens(State(state): State<Arc<AppState>>) -> Result<Response, AppError> {
    let t = state.store().list_tokens().await.map_err(AppError::from)?;
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
        (status = 201, description = "Token created; secret returned once", body = TokenCreated),
        (status = 401, description = "Not authenticated", body = ErrorResponse)
    )
)]
pub async fn create_token_h(
    State(state): State<Arc<AppState>>,
    Json(body): Json<TokenCreate>,
) -> Result<Response, AppError> {
    let _guard = state.write_lock.lock().await;
    let secret = format!("lbl_{}", crate::auth::random_secret());
    let t = state
        .store()
        .create_token(&body.name, &crate::auth::sha256_hex(&secret))
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
        (status = 404, description = "Token not found", body = ErrorResponse)
    )
)]
pub async fn delete_token_h(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let _guard = state.write_lock.lock().await;
    if !state
        .store()
        .delete_token(&id)
        .await
        .map_err(AppError::from)?
    {
        return Err(AppError::not_found(&id));
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
