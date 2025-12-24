use axum::{
    extract::{Path, State, Json},
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
    render::render_single_label,
    render::render_sheet_labels,
    templates::TemplateRegistry,
};

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
pub async fn list_templates(
    State(registry): State<Arc<TemplateRegistry>>,
) -> impl IntoResponse {
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
    Json(req): Json<RenderLabelRequest>,
) -> Result<Response, AppError> {
    let template = registry
        .get(&req.template)
        .ok_or_else(|| AppError::template_not_found(req.template.clone()))?;

    let option_value = req.label.option.as_deref();

    tracing::debug!(
        template = %template.id,
        option = option_value.unwrap_or(""),
        dpi = template.dpi,
        data_keys = req.label.data.len(),
        "render label request"
    );

    let mut selected_option = option_value;
    if let Some(options) = &template.options {
        selected_option = selected_option.or_else(|| options.0.first().map(|v| v.as_str()));
        let selected = selected_option.unwrap_or("");
        if !options.0.iter().any(|opt| opt == selected) {
            return Err(AppError::invalid_option_value(selected, &options.0));
        }
    }

    let png = render_single_label(template, &req.label.data, selected_option)?;

    Ok((
        axum::http::StatusCode::OK,
        [("content-type", "image/png")],
        png,
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
    Json(req): Json<RenderBatchRequest>,
) -> Result<Response, AppError> {
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
