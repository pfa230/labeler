//! Printer routes: list, CRUD, connection validation, probe and cut.

use axum::{
    extract::State,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use std::sync::Arc;

use crate::{
    errors::{AppError, NotFoundKind},
    extract::{Json, Path},
    models::{ErrorResponse, NewPrinter, Printer, PrinterConnection, PrinterUpdate},
    reason::Reason,
};

use super::AppState;

pub(super) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/printers", get(list_printers).post(create_printer))
        .route("/printers/probe", post(probe_printer))
        .route("/printers/{id}/probe", post(probe_stored_printer))
        .route("/printers/{id}/cut", post(cut_printer))
        .route(
            "/printers/{id}",
            get(get_printer).put(replace_printer).delete(delete_printer),
        )
}

/// The printer id rule (printing, "Printer fields"): non-empty, letters, digits, `'-'` or `'_'`.
/// Shared with the MCP adapter, which checks a `cut_printer` id before building its request.
pub fn valid_printer_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn validate_printer(id: &str, name: &str, connection: &PrinterConnection) -> Result<(), AppError> {
    if !valid_printer_id(id) {
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
    tag = "api",
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
    tag = "api",
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
    tag = "api",
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
    put,
    path = "/printers/{id}",
    tag = "api",
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
    tag = "api",
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
    /// The printer advertises the `ptouch-chain` finishing template.
    pub chain_and_continue: bool,
    /// The printer advertises the `Ptouch-Cut` operation.
    pub cut: bool,
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
                chain_and_continue: caps.supports_chain_and_continue,
                cut: caps.supports_cut,
            },
        }
    }
}

#[utoipa::path(
    post,
    path = "/printers/probe",
    tag = "api",
    request_body = PrinterConnection,
    responses(
        (status = 200, description = "Probe result (ok or unreachable)", body = ProbeResponse),
        (status = 400, description = "Invalid printer", body = ErrorResponse)
    )
)]
pub async fn probe_printer(
    Json(connection): Json<PrinterConnection>,
) -> Result<Json<ProbeResponse>, AppError> {
    probe_connection(&connection).await
}

#[utoipa::path(
    post,
    path = "/printers/{id}/probe",
    tag = "api",
    params(("id" = String, Path, description = "Printer ID")),
    responses(
        (status = 200, description = "Probe result (ok or unreachable)", body = ProbeResponse),
        (status = 404, description = "Printer not found", body = ErrorResponse)
    )
)]
pub async fn probe_stored_printer(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ProbeResponse>, AppError> {
    let Some(connection) = state.store().get_printer_connection(&id).await? else {
        return Err(AppError::not_found(NotFoundKind::Printer, id));
    };
    probe_connection(&connection).await
}

async fn probe_connection(connection: &PrinterConnection) -> Result<Json<ProbeResponse>, AppError> {
    let driver = crate::driver::driver_for(connection)
        .map_err(|e| AppError::printer_invalid(e.to_string()))?;
    Ok(Json(match driver.probe().await {
        crate::driver::ProbeOutcome::Ok(c) => ProbeResponse::ok(c),
        crate::driver::ProbeOutcome::Unreachable(d) => ProbeResponse::Unreachable { detail: d },
    }))
}

/// Result of a cut. Always returned inside a `200`; a printer-side failure is data, not the HTTP
/// status.
#[derive(serde::Serialize, utoipa::ToSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CutResponse {
    Ok,
    Failed { detail: String },
}

#[utoipa::path(
    post,
    path = "/printers/{id}/cut",
    tag = "api",
    params(("id" = String, Path, description = "Printer ID")),
    responses(
        (status = 200, description = "Cut result (ok or failed)", body = CutResponse),
        (status = 404, description = "Printer not found", body = ErrorResponse)
    )
)]
pub async fn cut_printer(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<CutResponse>, AppError> {
    let Some(connection) = state.store().get_printer_connection(&id).await? else {
        return Err(AppError::not_found(NotFoundKind::Printer, id));
    };
    let driver = crate::driver::driver_for(&connection)
        .map_err(|e| AppError::printer_invalid(e.to_string()))?;
    Ok(Json(match driver.cut().await {
        Ok(()) => CutResponse::Ok,
        Err(err) => CutResponse::Failed {
            detail: err.to_string(),
        },
    }))
}
