use axum::extract::{rejection::QueryRejection, Query, State};
use serde::Deserialize;
use std::sync::Arc;
use utoipa_axum::{router::OpenApiRouter, routes};

use super::AppState;
use crate::{
    errors::AppError,
    extract::Json,
    models::{ActivityPage, ErrorResponse},
    reason::Reason,
};

pub(super) fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new().routes(routes!(list_activity))
}

#[derive(Deserialize)]
pub struct ActivityQuery {
    before_id: Option<i64>,
}

#[utoipa::path(get, path = "/activity", tag = "api",
    params(("before_id" = Option<i64>, Query, description = "Exclusive older-entry cursor; positive signed 64-bit integer")),
    responses(
        (status = 200, description = "Shared activity, newest first, at most 100 entries", body = ActivityPage),
        (status = 400, description = "Invalid cursor", body = ErrorResponse),
        (status = 401, description = "Authentication required", body = ErrorResponse),
        (status = 500, description = "Activity read failed", body = ErrorResponse)
    ))]
pub async fn list_activity(
    State(state): State<Arc<AppState>>,
    query: Result<Query<ActivityQuery>, QueryRejection>,
) -> Result<Json<ActivityPage>, AppError> {
    let invalid_cursor = || {
        AppError::invalid_request(
            Reason::ActivityCursorInvalid,
            "before_id must be a positive signed 64-bit integer",
        )
    };
    let Query(query) = query.map_err(|_| invalid_cursor())?;
    if query.before_id.is_some_and(|id| id <= 0) {
        return Err(invalid_cursor());
    }
    Ok(Json(state.store().list_activity(query.before_id)?))
}
