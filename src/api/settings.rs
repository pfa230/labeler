//! Variable and application-setting routes, with the datetime-format preview.

use axum::{
    extract::State,
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Router,
};
use std::sync::Arc;

use crate::{
    errors::{AppError, NotFoundKind},
    extract::{Json, Path},
    models::{ErrorResponse, VariableValue},
    reason::Reason,
};

use super::AppState;

pub(super) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/variables", get(get_variables))
        .route("/variables/{key}", put(put_variable))
        .route("/settings", get(get_settings))
        .route("/settings/{key}", put(put_setting).delete(delete_setting))
        .route("/datetime-formats/preview", post(preview_datetime_format))
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
#[serde(deny_unknown_fields)]
pub struct SettingValue {
    #[serde(deserialize_with = "crate::models::non_null_value")]
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
        DEFAULT_PRINTER_ID,
    };
    let is_default = stored.is_none();
    let value = match key {
        DATETIME_FORMATS => resolve_datetime_formats_from(stored).map(|v| serde_json::json!(v)),
        DEFAULT_PRINTER_ID => resolve_default_id_from(key, stored).map(|v| serde_json::json!(v)),
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
#[serde(deny_unknown_fields)]
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
