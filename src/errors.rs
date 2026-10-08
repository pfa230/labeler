use axum::{
    extract::rejection::{JsonRejection, PathRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};

use crate::models::{ErrorBody, ErrorResponse};
use crate::reason::Reason;
use crate::store::StoreError;
use crate::templates::TemplateRegistryError;

const CODE_INVALID_REQUEST: &str = "InvalidRequest";
const CODE_UNAUTHORIZED: &str = "Unauthorized";
const CODE_FORBIDDEN: &str = "Forbidden";
const CODE_NOT_FOUND: &str = "NotFound";
const CODE_CONFLICT: &str = "Conflict";
const CODE_PAYLOAD_TOO_LARGE: &str = "PayloadTooLarge";
const CODE_UNSUPPORTED_MEDIA_TYPE: &str = "UnsupportedMediaType";
const CODE_TEMPLATE_INVALID: &str = "TemplateInvalid";
const CODE_UNSUPPORTED_LAYOUT: &str = "UnsupportedLayoutItem";
const CODE_BATCH_INVALID: &str = "BatchInvalid";
const CODE_INTERNAL: &str = "Internal";
const CODE_UPSTREAM: &str = "Upstream";

#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    code: &'static str,
    message: String,
    details: Option<Value>,
    reason: Option<Reason>,
    response_only_detail_keys: Vec<&'static str>,
}

/// What a `NotFound` names, serialized as `details.kind`.
#[derive(Debug, Clone, Copy)]
pub enum NotFoundKind {
    Route,
    Template,
    Printer,
    Connection,
    Setting,
    User,
    Token,
}

impl NotFoundKind {
    fn as_str(self) -> &'static str {
        match self {
            NotFoundKind::Route => "route",
            NotFoundKind::Template => "template",
            NotFoundKind::Printer => "printer",
            NotFoundKind::Connection => "connection",
            NotFoundKind::Setting => "setting",
            NotFoundKind::User => "user",
            NotFoundKind::Token => "token",
        }
    }
}

/// One label's failure within a batch: the label's 0-based index plus the error object it would
/// have produced on its own.
#[derive(Debug, serde::Serialize)]
pub struct BatchFailure {
    pub index: usize,
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl BatchFailure {
    pub fn new(index: usize, err: AppError) -> Self {
        Self {
            index,
            code: err.code,
            message: err.message,
            details: err.details,
        }
    }
}

impl AppError {
    fn new(
        status: StatusCode,
        code: &'static str,
        message: impl Into<String>,
        details: Option<Value>,
    ) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            details,
            reason: None,
            response_only_detail_keys: Vec::new(),
        }
    }

    /// Build an error that carries a reason. `extra` is merged alongside `reason` in
    /// `details`; taking a `Map` rather than a `Value` makes a non-object `details` unrepresentable,
    /// so the merge cannot lose data. This is the only writer of both `details` and `reason` for a
    /// reasoned error, so the two cannot diverge.
    fn reasoned(
        status: StatusCode,
        code: &'static str,
        reason: Reason,
        message: impl Into<String>,
        extra: Option<serde_json::Map<String, Value>>,
    ) -> Self {
        let mut details = extra.unwrap_or_default();
        details.insert("reason".to_string(), Value::from(reason.as_slug()));
        Self {
            status,
            code,
            message: message.into(),
            details: Some(Value::Object(details)),
            reason: Some(reason),
            response_only_detail_keys: Vec::new(),
        }
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// The `details.reason` slug, when this error carries one.
    pub fn reason(&self) -> Option<&'static str> {
        self.reason.map(Reason::as_slug)
    }

    pub fn details(&self) -> Option<&Value> {
        self.details.as_ref()
    }

    pub fn message_text(&self) -> String {
        self.message.clone()
    }

    /// The stable error `code` string (for tests / introspection).
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// A JSON body that axum could not parse. Keeps the parser's own text under `details.error`.
    pub fn malformed_json(parser_error: String) -> Self {
        let mut extra = serde_json::Map::new();
        extra.insert("error".to_string(), Value::from(parser_error));
        let mut err = Self::reasoned(
            StatusCode::BAD_REQUEST,
            CODE_INVALID_REQUEST,
            Reason::JsonMalformed,
            "Malformed JSON body",
            Some(extra),
        );
        err.response_only_detail_keys.push("error");
        err
    }

    pub fn batch_invalid(failures: Vec<BatchFailure>) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            CODE_BATCH_INVALID,
            "one or more labels in the batch are invalid",
            Some(json!({ "failures": failures })),
        )
    }

    pub fn batch_too_large(count: usize, max: usize) -> Self {
        Self::payload_too_large(format!("batch has {count} labels; the maximum is {max}"))
    }

    pub fn payload_too_large(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            CODE_PAYLOAD_TOO_LARGE,
            message,
            None,
        )
    }

    /// An unknown `/api/*` route (`id` is the path) or a named record that does not exist.
    pub fn not_found(kind: NotFoundKind, id: impl Into<String>) -> Self {
        let id = id.into();
        let message = match kind {
            NotFoundKind::Route => format!("no API route for '{id}'"),
            _ => format!("no {} '{id}' was found", kind.as_str()),
        };
        Self::new(
            StatusCode::NOT_FOUND,
            CODE_NOT_FOUND,
            message,
            Some(json!({ "kind": kind.as_str(), "id": id })),
        )
    }

    /// A value the render needs is absent; `field` names it.
    pub fn missing_field(field: &str) -> Self {
        let mut extra = serde_json::Map::new();
        extra.insert("field".to_string(), Value::from(field));
        Self::reasoned(
            StatusCode::UNPROCESSABLE_ENTITY,
            CODE_UNSUPPORTED_LAYOUT,
            Reason::MissingField,
            format!("Missing required field '{field}'"),
            Some(extra),
        )
    }

    /// A supplied parameter value its type refuses; `element` is a `list` element's position.
    pub fn param_value_invalid(
        param: &str,
        element: Option<usize>,
        message: impl Into<String>,
    ) -> Self {
        let mut extra = serde_json::Map::new();
        extra.insert("param".to_string(), Value::from(param));
        if let Some(element) = element {
            extra.insert("element".to_string(), Value::from(element));
        }
        Self::reasoned(
            StatusCode::BAD_REQUEST,
            CODE_INVALID_REQUEST,
            Reason::ParamValueInvalid,
            message,
            Some(extra),
        )
    }

    pub fn unsupported_layout_item(reason: Reason, message: impl Into<String>) -> Self {
        Self::reasoned(
            StatusCode::UNPROCESSABLE_ENTITY,
            CODE_UNSUPPORTED_LAYOUT,
            reason,
            message,
            None,
        )
    }

    pub fn field_value_not_scalar(field: impl Into<String>) -> Self {
        let f = field.into();
        let mut extra = serde_json::Map::new();
        extra.insert("field".to_string(), Value::from(f.clone()));
        Self::reasoned(
            StatusCode::UNPROCESSABLE_ENTITY,
            CODE_UNSUPPORTED_LAYOUT,
            Reason::FieldValueNotScalar,
            format!(
                "field '{f}' is an array; only scalar values can be rendered in this layout item"
            ),
            Some(extra),
        )
    }

    pub fn invalid_request(reason: Reason, message: impl Into<String>) -> Self {
        Self::reasoned(
            StatusCode::BAD_REQUEST,
            CODE_INVALID_REQUEST,
            reason,
            message,
            None,
        )
    }

    pub fn color_param_invalid(param: &str, detail: impl std::fmt::Display) -> Self {
        Self::invalid_request(
            Reason::ColorParamInvalid,
            format!("Invalid value for color parameter '{param}': {detail}"),
        )
    }

    pub fn line_spacing_param_invalid(
        path: &str,
        param: &str,
        detail: impl std::fmt::Display,
    ) -> Self {
        Self::invalid_request(
            Reason::LineSpacingParamInvalid,
            format!("Invalid line_spacing value for parameter '{param}' at {path}: {detail}"),
        )
    }

    pub fn width_bounds_inverted(
        min: f32,
        max: f32,
        unit: &str,
        min_param: Option<&str>,
        max_param: Option<&str>,
    ) -> Self {
        let max_part = match max_param {
            Some(param) => format!("format.width.max {max} {unit} (parameter '{param}')"),
            None => format!("format.width.max {max} {unit}"),
        };
        let min_part = match min_param {
            Some(param) => format!("format.width.min {min} {unit} (parameter '{param}')"),
            None => format!("format.width.min {min} {unit}"),
        };
        Self::invalid_request(
            Reason::WidthBoundsInverted,
            format!("{max_part} is below {min_part}"),
        )
    }

    pub fn printer_invalid(message: impl Into<String>) -> Self {
        Self::invalid_request(Reason::PrinterInvalid, message)
    }

    pub fn template_invalid(reason: Reason, message: impl Into<String>) -> Self {
        Self::reasoned(
            StatusCode::UNPROCESSABLE_ENTITY,
            CODE_TEMPLATE_INVALID,
            reason,
            message,
            None,
        )
    }

    pub fn param_default_unresolvable(failure: &crate::render::ParamDefaultFailure) -> Self {
        let mut extra = serde_json::Map::new();
        extra.insert(
            "param".to_string(),
            serde_json::Value::String(failure.param.clone()),
        );
        if let Some(token) = &failure.token {
            extra.insert(
                "token".to_string(),
                serde_json::Value::String(token.clone()),
            );
        }
        if let Some(val) = &failure.value {
            extra.insert("value".to_string(), serde_json::Value::String(val.clone()));
        }
        Self::reasoned(
            StatusCode::UNPROCESSABLE_ENTITY,
            CODE_TEMPLATE_INVALID,
            Reason::ParamDefaultUnresolvable,
            &failure.message,
            Some(extra),
        )
    }

    /// The service failed for a reason not attributable to the request. The message is logged.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            CODE_INTERNAL,
            message,
            None,
        )
    }

    pub fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            CODE_UNAUTHORIZED,
            "authentication required",
            None,
        )
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, CODE_FORBIDDEN, message, None)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, CODE_CONFLICT, message, None)
    }

    fn upstream(reason: Reason, message: impl Into<String>) -> Self {
        Self::reasoned(
            StatusCode::BAD_GATEWAY,
            CODE_UPSTREAM,
            reason,
            message,
            None,
        )
    }

    fn unsupported_media_type(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            CODE_UNSUPPORTED_MEDIA_TYPE,
            message,
            None,
        )
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status;
        let log_details = match &self.details {
            Some(Value::Object(map)) if !self.response_only_detail_keys.is_empty() => {
                let filtered: serde_json::Map<String, Value> = map
                    .iter()
                    .filter(|(k, _)| !self.response_only_detail_keys.contains(&k.as_str()))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                if filtered.is_empty() {
                    None
                } else {
                    Some(Value::Object(filtered))
                }
            }
            other => other.clone(),
        };

        if status.is_server_error() {
            tracing::error!(
                status = %status,
                code = self.code,
                message = %self.message,
                details = ?log_details,
                "request failed"
            );
        } else {
            tracing::warn!(
                status = %status,
                code = self.code,
                message = %self.message,
                details = ?log_details,
                "request rejected"
            );
        }

        let body = Json(ErrorResponse {
            error: ErrorBody {
                code: self.code.to_string(),
                message: self.message,
                details: self.details,
            },
        });
        (status, body).into_response()
    }
}

impl From<JsonRejection> for AppError {
    fn from(rejection: JsonRejection) -> Self {
        if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
            return AppError::payload_too_large("Request body too large");
        }
        let message = rejection.body_text();
        match rejection {
            JsonRejection::MissingJsonContentType(_) => {
                AppError::unsupported_media_type("Content-Type must be application/json")
            }
            JsonRejection::JsonSyntaxError(_) | JsonRejection::JsonDataError(_) => {
                AppError::malformed_json(message)
            }
            JsonRejection::BytesRejection(_) => {
                AppError::invalid_request(Reason::RequestBodyInvalid, "Invalid request body")
            }
            _ => AppError::invalid_request(Reason::RequestBodyInvalid, "Invalid JSON request"),
        }
    }
}

impl From<PathRejection> for AppError {
    fn from(rejection: PathRejection) -> Self {
        if rejection.status().is_server_error() {
            AppError::internal(rejection.body_text())
        } else {
            AppError::invalid_request(Reason::PathParamInvalid, "Invalid path parameter")
        }
    }
}

impl From<TemplateRegistryError> for AppError {
    fn from(err: TemplateRegistryError) -> Self {
        // Only `Io` is ever returned as an error; the other variants describe quarantined files.
        AppError::internal(err.to_string())
    }
}

impl From<crate::connector::ConnectorError> for AppError {
    fn from(err: crate::connector::ConnectorError) -> Self {
        use crate::connector::ConnectorError::*;
        match err {
            AuthFailed => AppError::upstream(Reason::Auth, "upstream authentication failed"),
            ConnectionFailed(m) => AppError::upstream(Reason::Unreachable, m),
            RateLimited => AppError::upstream(Reason::RateLimited, "upstream rate limited"),
            Upstream(m) => AppError::upstream(Reason::BadResponse, m),
            InvalidFilter(m) => AppError::invalid_request(Reason::FilterInvalid, m),
            RowKeyInvalid(m) => AppError::invalid_request(Reason::RowKeyInvalid, m),
            BudgetExceeded => {
                AppError::invalid_request(Reason::RowLimitExceeded, "too many rows requested")
            }
        }
    }
}

impl From<StoreError> for AppError {
    fn from(err: StoreError) -> Self {
        AppError::internal(err.to_string())
    }
}

#[derive(Debug, Clone)]
pub enum TemplateError {
    Yaml { path: String, msg: String },
    Validation { path: String, msg: String },
}

impl TemplateError {
    pub fn with_prefix(self, prefix: &str) -> Self {
        match self {
            TemplateError::Yaml { path, msg } => TemplateError::Yaml {
                path: join_path(prefix, &path),
                msg,
            },
            TemplateError::Validation { path, msg } => TemplateError::Validation {
                path: join_path(prefix, &path),
                msg,
            },
        }
    }

    pub fn at(self, segment: &str) -> Self {
        match self {
            TemplateError::Yaml { path, msg } => TemplateError::Yaml {
                path: join_path(&path, segment),
                msg,
            },
            TemplateError::Validation { path, msg } => TemplateError::Validation {
                path: join_path(&path, segment),
                msg,
            },
        }
    }
}

fn join_path(prefix: &str, suffix: &str) -> String {
    if prefix.is_empty() {
        return suffix.to_string();
    }
    if suffix.is_empty() {
        return prefix.to_string();
    }
    if suffix.starts_with('[') {
        format!("{prefix}{suffix}")
    } else {
        format!("{prefix}.{suffix}")
    }
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateError::Yaml { path, msg } => {
                if path.is_empty() {
                    write!(f, "yaml error: {msg}")
                } else {
                    write!(f, "yaml error at {path}: {msg}")
                }
            }
            TemplateError::Validation { path, msg } => {
                if path.is_empty() {
                    write!(f, "validation error: {msg}")
                } else {
                    write!(f, "validation error at {path}: {msg}")
                }
            }
        }
    }
}

impl std::error::Error for TemplateError {}

#[cfg(test)]
mod tests {
    use super::{AppError, BatchFailure};
    use axum::http::StatusCode;

    /// A per-label failure is the label's own error object plus `index`: its reason and any other
    /// details travel under `details`, never beside `code`.
    #[test]
    fn batch_failure_carries_the_error_details() {
        let failure = BatchFailure::new(2, AppError::missing_field("sku"));
        let json = serde_json::to_value(&failure).expect("serialize");
        assert_eq!(
            json,
            serde_json::json!({
                "index": 2,
                "code": "UnsupportedLayoutItem",
                "message": "Missing required field 'sku'",
                "details": { "reason": "missing_field", "field": "sku" },
            })
        );
    }

    #[test]
    fn connector_errors_map_to_upstream_or_invalid_request() {
        use crate::connector::ConnectorError;
        let cases = [
            (
                ConnectorError::AuthFailed,
                StatusCode::BAD_GATEWAY,
                "Upstream",
                "auth",
            ),
            (
                ConnectorError::ConnectionFailed("x".into()),
                StatusCode::BAD_GATEWAY,
                "Upstream",
                "unreachable",
            ),
            (
                ConnectorError::RateLimited,
                StatusCode::BAD_GATEWAY,
                "Upstream",
                "rate_limited",
            ),
            (
                ConnectorError::Upstream("x".into()),
                StatusCode::BAD_GATEWAY,
                "Upstream",
                "bad_response",
            ),
            (
                ConnectorError::InvalidFilter("x".into()),
                StatusCode::BAD_REQUEST,
                "InvalidRequest",
                "filter_invalid",
            ),
            (
                ConnectorError::RowKeyInvalid("x".into()),
                StatusCode::BAD_REQUEST,
                "InvalidRequest",
                "row_key_invalid",
            ),
            (
                ConnectorError::BudgetExceeded,
                StatusCode::BAD_REQUEST,
                "InvalidRequest",
                "row_limit_exceeded",
            ),
        ];
        for (err, status, code, reason) in cases {
            let app_err = AppError::from(err);
            // `status` is private, but this module is a child of `errors`, so it is visible here.
            assert_eq!(app_err.status, status, "status for {reason}");
            assert_eq!(app_err.code(), code, "code for {reason}");
            assert_eq!(app_err.reason(), Some(reason));
        }
    }

    #[tokio::test]
    async fn path_rejection_server_error_stays_internal() {
        use axum::extract::FromRequestParts;
        // MissingPathParams produces a 500-classified PathRejection
        let req = axum::http::Request::builder()
            .uri("/")
            .body(axum::body::Body::empty())
            .unwrap();
        let (mut parts, _) = req.into_parts();
        let res = axum::extract::Path::<String>::from_request_parts(&mut parts, &()).await;
        let rejection = res.expect_err("should reject");
        assert!(rejection.status().is_server_error());
        let app_err = AppError::from(rejection);
        assert_eq!(app_err.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(app_err.code(), "Internal");
        assert_eq!(app_err.reason(), None);
    }

    #[test]
    fn malformed_json_response_only_keys_preserved_on_wire() {
        let err = AppError::malformed_json("syntax error at line 1".into());
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert_eq!(err.code(), "InvalidRequest");
        assert_eq!(err.reason(), Some("json_malformed"));
        assert!(err.response_only_detail_keys.contains(&"error"));
        let details = err.details.as_ref().unwrap();
        assert_eq!(details["error"], "syntax error at line 1");
        assert_eq!(details["reason"], "json_malformed");
    }
}
