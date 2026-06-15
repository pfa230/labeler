use axum::{
    extract::rejection::JsonRejection,
    extract::{Json, Path, Query, State},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    errors::AppError,
    models::{
        ErrorResponse, HealthResponse, RenderBatchRequest, RenderLabelRequest, TemplateDetail,
        TemplateList,
    },
    openapi::ApiDoc,
    render::render_sheet_labels,
    render::render_single_label,
    render::render_single_label_pdf,
    templates::TemplateRegistry,
};

#[derive(serde::Deserialize)]
pub struct RenderQuery {
    pub format: Option<String>,
}

pub fn app(state: Arc<TemplateRegistry>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/templates", get(list_templates))
        .route("/templates/{id}", get(get_template))
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
pub async fn list_templates(State(registry): State<Arc<TemplateRegistry>>) -> impl IntoResponse {
    let templates = registry.summaries();
    Json(TemplateList { templates })
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
    State(registry): State<Arc<TemplateRegistry>>,
    Path(id): Path<String>,
) -> Result<Json<TemplateDetail>, AppError> {
    registry
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
    State(registry): State<Arc<TemplateRegistry>>,
    Query(query): Query<RenderQuery>,
    payload: Result<Json<RenderLabelRequest>, JsonRejection>,
) -> Result<Response, AppError> {
    let Json(req) = payload.map_err(AppError::from)?;
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
    State(registry): State<Arc<TemplateRegistry>>,
    payload: Result<Json<RenderBatchRequest>, JsonRejection>,
) -> Result<Response, AppError> {
    let Json(req) = payload.map_err(AppError::from)?;
    tracing::debug!(
        template = %req.template,
        labels = req.labels.len(),
        start_slot = req.start_slot,
        "render batch request"
    );
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
