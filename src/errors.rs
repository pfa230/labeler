use axum::{
    extract::rejection::{JsonRejection, PathRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};

use crate::models::{ErrorBody, ErrorResponse};

const CODE_TEMPLATE_NOT_FOUND: &str = "TemplateNotFound";
const CODE_INVALID_REQUEST: &str = "InvalidRequest";
const CODE_UNSUPPORTED_MEDIA_TYPE: &str = "UnsupportedMediaType";
const CODE_NOT_IMPLEMENTED: &str = "NotImplemented";
const CODE_INVALID_OPTION_VALUE: &str = "InvalidOptionValue";
const CODE_MISSING_FIELD: &str = "MissingField";
const CODE_UNSUPPORTED_LAYOUT: &str = "UnsupportedLayoutItem";
const CODE_UNSUPPORTED_FORMAT: &str = "UnsupportedFormat";
const CODE_RENDER_FAILED: &str = "RenderFailed";

#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    code: &'static str,
    message: String,
    details: Option<Value>,
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

    pub fn invalid_option_value(option: &str, allowed: &[String]) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            CODE_INVALID_OPTION_VALUE,
            format!("Invalid option value '{option}'"),
            Some(json!({ "allowed": allowed })),
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

    fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, CODE_INVALID_REQUEST, message, None)
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
            tracing::debug!(
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
        match rejection {
            JsonRejection::MissingJsonContentType(_) => {
                AppError::unsupported_media_type("Content-Type must be application/json")
            }
            JsonRejection::JsonSyntaxError(_) | JsonRejection::JsonDataError(_) => {
                AppError::invalid_request("Malformed JSON body")
            }
            JsonRejection::BytesRejection(_) => {
                AppError::invalid_request("Invalid request body")
            }
            _ => AppError::invalid_request("Invalid JSON request"),
        }
    }
}

impl From<PathRejection> for AppError {
    fn from(_rejection: PathRejection) -> Self {
        AppError::invalid_request("Invalid path parameter")
    }
}
