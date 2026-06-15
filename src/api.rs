use arc_swap::ArcSwap;
use axum::{
    extract::rejection::JsonRejection,
    extract::{Json, Path, Query, State},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    errors::AppError,
    models::{
        ErrorResponse, HealthResponse, ReloadResponse, RenderBatchRequest, RenderLabelRequest,
        TemplateDetail, TemplateList,
    },
    openapi::ApiDoc,
    parse::parse_template,
    render::render_sheet_labels,
    render::render_single_label,
    render::render_single_label_pdf,
    store::Store,
    templates::{TemplateDefinition, TemplateRegistry, TemplateRegistryError},
};

#[derive(serde::Deserialize)]
pub struct RenderQuery {
    pub format: Option<String>,
}

pub struct AppState {
    templates: ArcSwap<TemplateRegistry>,
    templates_dir: PathBuf,
    write_lock: Mutex<()>,
    store: Store,
}

impl AppState {
    pub fn new(registry: TemplateRegistry, templates_dir: PathBuf, store: Store) -> Self {
        Self {
            templates: ArcSwap::from_pointee(registry),
            templates_dir,
            write_lock: Mutex::new(()),
            store,
        }
    }

    pub fn store(&self) -> &Store {
        &self.store
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

pub fn app(state: Arc<AppState>) -> Router {
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
        .route("/render/label", post(render_label))
        .route("/render/batch", post(render_batch))
        .merge(SwaggerUi::new("/docs").url("/openapi.json", ApiDoc::openapi()))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
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
    post,
    path = "/render/label",
    params(
        ("format" = Option<String>, Query, description = "Output format: png (default) or pdf")
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

    let (bytes, content_type) = match query.format.as_deref() {
        None | Some("") | Some("png") => (
            render_single_label(template, &req.label.data, option_value)?,
            "image/png",
        ),
        Some("pdf") => (
            render_single_label_pdf(template, &req.label.data, option_value)?,
            "application/pdf",
        ),
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
    path = "/render/batch",
    request_body = RenderBatchRequest,
    responses(
        (status = 200, description = "Rendered PDF bytes", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 404, description = "Template not found", body = ErrorResponse),
        (status = 415, description = "Unsupported media type", body = ErrorResponse),
        (status = 422, description = "Validation error", body = ErrorResponse),
        (status = 501, description = "Not implemented", body = ErrorResponse)
    )
)]
pub async fn render_batch(
    State(state): State<Arc<AppState>>,
    payload: Result<Json<RenderBatchRequest>, JsonRejection>,
) -> Result<Response, AppError> {
    let Json(req) = payload.map_err(AppError::from)?;
    tracing::debug!(
        template = %req.template,
        labels = req.labels.len(),
        start_slot = req.start_slot,
        "render batch request"
    );
    let registry = state.templates.load_full();
    let template = registry
        .get(&req.template)
        .ok_or_else(|| AppError::template_not_found(req.template.clone()))?;

    let pdf = render_sheet_labels(template, &req.labels, req.start_slot)?;

    Ok((
        axum::http::StatusCode::OK,
        [("content-type", "application/pdf")],
        pdf,
    )
        .into_response())
}
