//! Rendering and printing routes: single-label render and the shared batch download/print path.

use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    errors::{AppError, NotFoundKind},
    extract::Json,
    models::{
        BatchRowError, BatchSummary, ErrorResponse, JobRecord, PrintRequest, RenderLabelRequest,
        RenderRequest,
    },
    reason::Reason,
    render::{
        render_single_label_image, render_single_label_pdf, resolve_environment, ColorMode,
        ImageRenderOptions,
    },
    templates::TemplateDefinition,
};

use super::AppState;

const MAX_BATCH_LABELS: usize = 500;

#[derive(serde::Deserialize)]
pub(crate) struct RenderQuery {
    format: Option<String>,
    color_mode: Option<String>,
    resolution: Option<String>,
}

pub(super) fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(render_label))
        .routes(routes!(render_labels))
        .routes(routes!(print_labels))
        .routes(routes!(list_jobs))
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
    File {
        format: Option<&'a str>,
    },
    Printer {
        id: &'a str,
        actor: &'a str,
        chain_and_continue: bool,
    },
}

/// Shared by `/render` and `/print`: decides the request-level faults, then renders every label
/// before anything is returned or sent (rendering, "All-or-nothing validation").
/// Typst compilation/rasterization run inline on the calling thread, assuming small local
/// operator workloads without latency isolation; only printer I/O yields.
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
    let variables = state.store().all_variables()?;
    let dt_formats = crate::settings::resolve_datetime_formats(state.store())
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
            chain_and_continue,
        } => {
            let connection = state
                .store()
                .get_printer_connection(printer_id)?
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
            // One query feeds job finishing and render negotiation (printing, "Render negotiation").
            let caps = driver.capabilities().await;
            let finishing = if chain_and_continue {
                if !caps.as_ref().is_some_and(|c| c.supports_chain_and_continue) {
                    return Err(AppError::chain_and_continue_unsupported());
                }
                Some(crate::driver::Finishing::ChainAndContinue)
            } else {
                caps.as_ref()
                    .filter(|c| c.supports_trim)
                    .map(|_| crate::driver::Finishing::Trim)
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
            let total = units.iter().map(|u| u.indices.len()).sum();
            let jobs = units.len();
            let mut failed = Vec::new();
            for unit in &units {
                let media_size =
                    media_width_mm
                        .zip(unit.y_dimension)
                        .map(|(x, y)| crate::driver::MediaSize {
                            x_dimension: crate::driver::hundredths_mm(x),
                            y_dimension: y,
                        });
                match driver
                    .send(
                        &unit.bytes,
                        &crate::driver::PrintOptions {
                            artifact_format,
                            media_size,
                            copies: unit.copies,
                            finishing,
                        },
                    )
                    .await
                {
                    Ok(()) => {
                        let _ = state.store().record_job(
                            &template.id,
                            Some(printer_id),
                            "ok",
                            None,
                            actor,
                        );
                    }
                    Err(err) => {
                        let msg = err.to_string();
                        let _ = state.store().record_job(
                            &template.id,
                            Some(printer_id),
                            "failed",
                            Some(&msg),
                            actor,
                        );
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
    tag = "api",
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
    tag = "api",
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
            chain_and_continue: req.chain_and_continue,
        },
    )
    .await
}

#[utoipa::path(
    post,
    path = "/render/label",
    tag = "api",
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

    let variables = state.store().all_variables()?;
    let dt_formats = crate::settings::resolve_datetime_formats(state.store())
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

    // Typst compilation/rasterization run inline on this request's thread, assuming small
    // local operator workloads without latency isolation for concurrent requests.
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

#[utoipa::path(get, path = "/jobs", tag = "printing",
    responses((status = 200, description = "Every recorded print job, newest first", body = Vec<JobRecord>)))]
pub async fn list_jobs(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<JobRecord>>, AppError> {
    Ok(Json(state.store().list_jobs()?))
}
