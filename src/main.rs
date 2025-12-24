use axum::{
    extract::{Path, Json},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use axum::extract::rejection::{JsonRejection, PathRejection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, net::SocketAddr};
use tracing_subscriber::EnvFilter;
use utoipa::{OpenApi, ToSchema};
use utoipa::openapi::OpenApi as OpenApiSpec;

const CODE_TEMPLATE_NOT_FOUND: &str = "TemplateNotFound";
const CODE_INVALID_REQUEST: &str = "InvalidRequest";
const CODE_UNSUPPORTED_MEDIA_TYPE: &str = "UnsupportedMediaType";
const CODE_NOT_IMPLEMENTED: &str = "NotImplemented";

#[derive(OpenApi)]
#[openapi(
    paths(
        health,
        list_templates,
        get_template,
        render_label,
        render_batch
    ),
    components(
        schemas(
            HealthResponse,
            TemplateList,
            TemplateSummary,
            TemplateFormat,
            TemplateDetail,
            OptionDetail,
            FieldSpec,
            RenderLabelRequest,
            RenderBatchRequest,
            BatchLabel,
            OutputOptions,
            ErrorResponse,
            ErrorBody
        )
    ),
    tags(
        (name = "labeler", description = "Label rendering service")
    )
)]
struct ApiDoc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("labeler=info,tower_http=info")),
        )
        .init();

    let app = app();

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr: SocketAddr = format!("0.0.0.0:{}", port)
        .parse()
        .expect("invalid PORT");

    tracing::info!(%addr, "labeler service listening");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind listener");

    axum::serve(listener, app)
        .await
        .expect("server error");
}

fn app() -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/openapi.json", get(openapi_json))
        .route("/templates", get(list_templates))
        .route("/templates/:id", get(get_template))
        .route("/render/label", post(render_label))
        .route("/render/batch", post(render_batch))
}

#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse)
    )
)]
async fn health() -> impl IntoResponse {
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
async fn list_templates() -> impl IntoResponse {
    let templates = template_summaries();
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
async fn get_template(Path(id): Path<String>) -> Result<Json<TemplateDetail>, AppError> {
    match template_detail(&id) {
        Some(detail) => Ok(Json(detail)),
        None => Err(AppError::template_not_found(id)),
    }
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
async fn render_label(Json(_req): Json<RenderLabelRequest>) -> Result<Response, AppError> {
    Err(AppError::not_implemented("render_label"))
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
async fn render_batch(Json(_req): Json<RenderBatchRequest>) -> Result<Response, AppError> {
    Err(AppError::not_implemented("render_batch"))
}

async fn openapi_json() -> Json<OpenApiSpec> {
    Json(ApiDoc::openapi())
}

#[derive(Debug)]
struct AppError {
    status: StatusCode,
    code: &'static str,
    message: String,
    details: Option<Value>,
}

impl AppError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>, details: Option<Value>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            details,
        }
    }

    fn template_not_found(id: String) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            CODE_TEMPLATE_NOT_FOUND,
            format!("No template with id '{}' was found", id),
            Some(json!({ "template": id })),
        )
    }

    fn not_implemented(endpoint: &str) -> Self {
        Self::new(
            StatusCode::NOT_IMPLEMENTED,
            CODE_NOT_IMPLEMENTED,
            "Rendering pipeline not implemented yet",
            Some(json!({ "endpoint": endpoint })),
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
        let body = Json(ErrorResponse {
            error: ErrorBody {
                code: self.code.to_string(),
                message: self.message,
                details: self.details,
            },
        });
        (self.status, body).into_response()
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

#[derive(Serialize, ToSchema)]
struct ErrorResponse {
    error: ErrorBody,
}

#[derive(Serialize, ToSchema)]
struct ErrorBody {
    code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<Value>,
}

#[derive(Serialize, ToSchema)]
struct HealthResponse {
    status: String,
}

#[derive(Serialize, ToSchema)]
struct TemplateList {
    templates: Vec<TemplateSummary>,
}

#[derive(Serialize, ToSchema)]
struct TemplateSummary {
    id: String,
    name: String,
    description: String,
    options: HashMap<String, Vec<String>>,
    format: TemplateFormat,
}

#[derive(Serialize, ToSchema)]
struct TemplateDetail {
    id: String,
    name: String,
    description: String,
    format: TemplateFormat,
    options: HashMap<String, OptionDetail>,
    fields: Vec<FieldSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
}

#[derive(Serialize, ToSchema)]
struct OptionDetail {
    values: Vec<String>,
    default: String,
}

#[derive(Serialize, ToSchema)]
struct FieldSpec {
    name: String,
    #[serde(rename = "type")]
    #[schema(rename = "type")]
    field_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_length: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    multiline: Option<bool>,
}

#[derive(Serialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TemplateFormat {
    Sheet {
        labels_per_sheet: u32,
        paper_size: String,
        label_size: String,
    },
    Continuous {
        width_mm: f32,
    },
}

#[derive(Debug, Deserialize, ToSchema)]
struct RenderLabelRequest {
    template: String,
    data: HashMap<String, Value>,
    #[serde(default)]
    options: HashMap<String, Value>,
    #[serde(default)]
    output: OutputOptions,
}

#[derive(Debug, Deserialize, ToSchema)]
struct RenderBatchRequest {
    template: String,
    labels: Vec<BatchLabel>,
    #[serde(default)]
    start_slot: u32,
    #[serde(default)]
    output: OutputOptions,
}

#[derive(Debug, Deserialize, ToSchema)]
struct BatchLabel {
    data: HashMap<String, Value>,
    #[serde(default)]
    options: HashMap<String, Value>,
}

#[derive(Debug, Deserialize, Default, ToSchema)]
struct OutputOptions {
    dpi: Option<u32>,
}

fn template_summaries() -> Vec<TemplateSummary> {
    vec![
        TemplateSummary {
            id: "avery5163".to_string(),
            name: "Avery 5163 2x4 Shipping Label".to_string(),
            description: "10 labels per US Letter sheet (8.5x11)".to_string(),
            options: HashMap::from([(
                "orientation".to_string(),
                vec!["horizontal".to_string(), "vertical".to_string()],
            )]),
            format: TemplateFormat::Sheet {
                labels_per_sheet: 10,
                paper_size: "8.5x11".to_string(),
                label_size: "2x4".to_string(),
            },
        },
        TemplateSummary {
            id: "brother12mm".to_string(),
            name: "Brother 12mm Continuous Label".to_string(),
            description: "Continuous label roll (12mm width)".to_string(),
            options: HashMap::from([(
                "color".to_string(),
                vec!["black".to_string(), "red".to_string()],
            )]),
            format: TemplateFormat::Continuous { width_mm: 12.0 },
        },
    ]
}

fn template_detail(id: &str) -> Option<TemplateDetail> {
    match id {
        "avery5163" => Some(TemplateDetail {
            id: "avery5163".to_string(),
            name: "Avery 5163 2x4".to_string(),
            description: "Standard shipping labels on US Letter sheets".to_string(),
            format: TemplateFormat::Sheet {
                labels_per_sheet: 10,
                paper_size: "8.5x11".to_string(),
                label_size: "2x4".to_string(),
            },
            options: HashMap::from([(
                "orientation".to_string(),
                OptionDetail {
                    values: vec!["horizontal".to_string(), "vertical".to_string()],
                    default: "horizontal".to_string(),
                },
            )]),
            fields: vec![
                FieldSpec {
                    name: "id".to_string(),
                    field_type: "string".to_string(),
                    max_length: Some(10),
                    multiline: None,
                },
                FieldSpec {
                    name: "name".to_string(),
                    field_type: "string".to_string(),
                    max_length: Some(50),
                    multiline: None,
                },
                FieldSpec {
                    name: "address".to_string(),
                    field_type: "string".to_string(),
                    max_length: Some(100),
                    multiline: Some(true),
                },
            ],
            version: None,
        }),
        "brother12mm" => Some(TemplateDetail {
            id: "brother12mm".to_string(),
            name: "Brother 12mm".to_string(),
            description: "Continuous 12mm label tape".to_string(),
            format: TemplateFormat::Continuous { width_mm: 12.0 },
            options: HashMap::from([(
                "color".to_string(),
                OptionDetail {
                    values: vec!["black".to_string(), "red".to_string()],
                    default: "black".to_string(),
                },
            )]),
            fields: vec![FieldSpec {
                name: "message".to_string(),
                field_type: "string".to_string(),
                max_length: Some(200),
                multiline: Some(true),
            }],
            version: None,
        }),
        _ => None,
    }
}
