use axum::{
    extract::rejection::{JsonRejection, PathRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};

use crate::models::{ErrorBody, ErrorResponse};
use crate::store::StoreError;
use crate::templates::TemplateRegistryError;

const CODE_TEMPLATE_NOT_FOUND: &str = "TemplateNotFound";
const CODE_INVALID_REQUEST: &str = "InvalidRequest";
const CODE_UNSUPPORTED_MEDIA_TYPE: &str = "UnsupportedMediaType";
const CODE_NOT_IMPLEMENTED: &str = "NotImplemented";
const CODE_INVALID_OPTION_VALUE: &str = "InvalidOptionValue";
const CODE_MISSING_FIELD: &str = "MissingField";
const CODE_UNSUPPORTED_LAYOUT: &str = "UnsupportedLayoutItem";
const CODE_UNSUPPORTED_FORMAT: &str = "UnsupportedFormat";
const CODE_RENDER_FAILED: &str = "RenderFailed";
const CODE_TEMPLATE_INVALID: &str = "TemplateInvalid";
const CODE_TEMPLATE_EXISTS: &str = "TemplateExists";
const CODE_PRINTER_NOT_FOUND: &str = "PrinterNotFound";
const CODE_PRINTER_EXISTS: &str = "PrinterExists";
const CODE_PRINTER_INVALID: &str = "PrinterInvalid";
const CODE_PRINTER_DISABLED: &str = "PrinterDisabled";
const CODE_PRINT_FAILED: &str = "PrintFailed";
const CODE_INTERNAL: &str = "Internal";
const CODE_BATCH_INVALID: &str = "BatchInvalid";
const CODE_BATCH_TOO_LARGE: &str = "BatchTooLarge";

#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    code: &'static str,
    message: String,
    details: Option<Value>,
}

/// One label's validation failure within a batch (its 0-based index + the error code/message).
#[derive(Debug, serde::Serialize)]
pub struct BatchFailure {
    pub index: usize,
    pub code: &'static str,
    pub message: String,
}

impl AppError {
    pub fn new(
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
        }
    }

    pub fn message_text(&self) -> String {
        self.message.clone()
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
        Self::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            CODE_BATCH_TOO_LARGE,
            format!("batch has {count} labels; the maximum is {max}"),
            Some(json!({ "count": count, "max": max })),
        )
    }

    /// The stable error `code` string (for tests / introspection).
    pub fn code(&self) -> &'static str {
        self.code
    }

    pub fn template_not_found(id: String) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            CODE_TEMPLATE_NOT_FOUND,
            format!("No template with id '{}' was found", id),
            Some(json!({ "template": id })),
        )
    }

    pub fn not_implemented(endpoint: &str) -> Self {
        Self::new(
            StatusCode::NOT_IMPLEMENTED,
            CODE_NOT_IMPLEMENTED,
            "Rendering pipeline not implemented yet",
            Some(json!({ "endpoint": endpoint })),
        )
    }

    pub fn invalid_option_value(
        selection: &std::collections::BTreeMap<String, String>,
        allowed: &std::collections::BTreeMap<String, Vec<String>>,
    ) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            CODE_INVALID_OPTION_VALUE,
            "Invalid option selection".to_string(),
            Some(json!({ "selection": selection, "allowed": allowed })),
        )
    }

    pub fn missing_field(field: &str) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            CODE_MISSING_FIELD,
            format!("Missing required field '{field}'"),
            Some(json!({ "field": field })),
        )
    }

    pub fn unsupported_layout_item(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            CODE_UNSUPPORTED_LAYOUT,
            message,
            None,
        )
    }

    pub fn unsupported_format(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            CODE_UNSUPPORTED_FORMAT,
            message,
            None,
        )
    }

    pub fn render_failed(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            CODE_RENDER_FAILED,
            message,
            None,
        )
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, CODE_INVALID_REQUEST, message, None)
    }

    pub fn template_invalid(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            CODE_TEMPLATE_INVALID,
            message,
            None,
        )
    }

    pub fn template_exists(id: &str) -> Self {
        Self::new(
            StatusCode::CONFLICT,
            CODE_TEMPLATE_EXISTS,
            format!("A template with id '{id}' already exists"),
            Some(json!({ "template": id })),
        )
    }

    pub fn printer_not_found(id: String) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            CODE_PRINTER_NOT_FOUND,
            format!("No printer with id '{id}' was found"),
            Some(json!({ "printer": id })),
        )
    }

    pub fn printer_exists(id: &str) -> Self {
        Self::new(
            StatusCode::CONFLICT,
            CODE_PRINTER_EXISTS,
            format!("A printer with id '{id}' already exists"),
            Some(json!({ "printer": id })),
        )
    }

    pub fn printer_invalid(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            CODE_PRINTER_INVALID,
            message,
            None,
        )
    }

    pub fn printer_disabled(id: &str) -> Self {
        Self::new(
            StatusCode::CONFLICT,
            CODE_PRINTER_DISABLED,
            format!("printer '{id}' is disabled"),
            Some(json!({ "printer": id })),
        )
    }

    pub fn print_failed(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, CODE_PRINT_FAILED, message, None)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            CODE_INTERNAL,
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
        if status.is_server_error() {
            tracing::error!(
                status = %status,
                code = self.code,
                message = %self.message,
                details = ?self.details,
                "request failed"
            );
        } else {
            tracing::warn!(
                status = %status,
                code = self.code,
                message = %self.message,
                details = ?self.details,
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
        let message = rejection.body_text();
        tracing::warn!(message = %message, "json request rejected");
        match rejection {
            JsonRejection::MissingJsonContentType(_) => {
                AppError::unsupported_media_type("Content-Type must be application/json")
            }
            JsonRejection::JsonSyntaxError(_) | JsonRejection::JsonDataError(_) => AppError::new(
                StatusCode::BAD_REQUEST,
                CODE_INVALID_REQUEST,
                "Malformed JSON body",
                Some(json!({ "error": message })),
            ),
            JsonRejection::BytesRejection(_) => AppError::invalid_request("Invalid request body"),
            _ => AppError::invalid_request("Invalid JSON request"),
        }
    }
}

impl From<PathRejection> for AppError {
    fn from(_rejection: PathRejection) -> Self {
        AppError::invalid_request("Invalid path parameter")
    }
}

impl From<TemplateRegistryError> for AppError {
    fn from(err: TemplateRegistryError) -> Self {
        let message = err.to_string();
        match err {
            TemplateRegistryError::Io { .. } => AppError::render_failed(message),
            _ => AppError::template_invalid(message),
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
