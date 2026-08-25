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
const CODE_TEMPLATE_ID_COLLISION: &str = "TemplateIdCollision";
const CODE_PRINTER_NOT_FOUND: &str = "PrinterNotFound";
const CODE_PRINTER_EXISTS: &str = "PrinterExists";
const CODE_PRINTER_INVALID: &str = "PrinterInvalid";
const CODE_MEDIA_MISMATCH: &str = "MediaMismatch";
const CODE_PRINT_FAILED: &str = "PrintFailed";
const CODE_INTERNAL: &str = "Internal";
const CODE_BATCH_INVALID: &str = "BatchInvalid";
const CODE_BATCH_TOO_LARGE: &str = "BatchTooLarge";
const CODE_PAYLOAD_TOO_LARGE: &str = "PayloadTooLarge";
const CODE_NOT_FOUND: &str = "NotFound";
const CODE_UNAUTHORIZED: &str = "Unauthorized";
const CODE_FORBIDDEN: &str = "Forbidden";
const CODE_CONFLICT: &str = "Conflict";
const CODE_SETTING_NOT_FOUND: &str = "SettingNotFound";

#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    code: &'static str,
    message: String,
    details: Option<Value>,
    reason: Option<Reason>,
    response_only_detail_keys: Vec<&'static str>,
}

/// One label's validation failure within a batch (its 0-based index + the error code/reason/message).
#[derive(Debug, serde::Serialize)]
pub struct BatchFailure {
    pub index: usize,
    pub code: &'static str,
    /// Present exactly when the failure's code carries a reason (SPEC §10.1). A per-label failure
    /// can be a code outside the migrated four, so this is optional rather than required.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
    pub message: String,
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

    /// Build an error that carries a reason (ADR-0052). `extra` is merged alongside `reason` in
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

    /// The `details.reason` slug, when this error carries one (SPEC §10.1).
    pub fn reason(&self) -> Option<&'static str> {
        self.reason.map(Reason::as_slug)
    }

    pub fn message_text(&self) -> String {
        self.message.clone()
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
        Self::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            CODE_BATCH_TOO_LARGE,
            format!("batch has {count} labels; the maximum is {max}"),
            Some(json!({ "count": count, "max": max })),
        )
    }

    pub fn payload_too_large(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            CODE_PAYLOAD_TOO_LARGE,
            message,
            None,
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

    pub fn not_found(path: &str) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            CODE_NOT_FOUND,
            format!("no API route for '{path}'"),
            Some(json!({ "path": path })),
        )
    }

    pub fn setting_not_found(key: &str) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            CODE_SETTING_NOT_FOUND,
            format!("No setting named '{key}'"),
            Some(json!({ "setting": key })),
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

    pub fn unsupported_layout_item(reason: Reason, message: impl Into<String>) -> Self {
        Self::reasoned(
            StatusCode::UNPROCESSABLE_ENTITY,
            CODE_UNSUPPORTED_LAYOUT,
            reason,
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

    pub fn render_failed(reason: Reason, message: impl Into<String>) -> Self {
        Self::reasoned(
            StatusCode::INTERNAL_SERVER_ERROR,
            CODE_RENDER_FAILED,
            reason,
            message,
            None,
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

    pub fn template_invalid(reason: Reason, message: impl Into<String>) -> Self {
        Self::reasoned(
            StatusCode::UNPROCESSABLE_ENTITY,
            CODE_TEMPLATE_INVALID,
            reason,
            message,
            None,
        )
    }

    pub fn template_group_invalid(message: impl Into<String>) -> Self {
        Self::template_invalid(Reason::TemplateGroupInvalid, message)
    }

    pub fn template_group_unpatchable(message: impl Into<String>) -> Self {
        Self::template_invalid(Reason::TemplateGroupUnpatchable, message)
    }

    pub fn template_exists(id: &str) -> Self {
        Self::new(
            StatusCode::CONFLICT,
            CODE_TEMPLATE_EXISTS,
            format!("A template with id '{id}' already exists"),
            Some(json!({ "template": id })),
        )
    }

    /// Two files on disk declare one template id, and the service will not guess which was meant.
    ///
    /// `files` are bare filenames from the directory reading the decision was made on, never paths:
    /// the templates directory's location is server configuration. This code carries no
    /// `details.reason` on purpose. ADR-0052 scopes `reason` to `RenderFailed`, `InvalidRequest`,
    /// `UnsupportedLayoutItem` and `TemplateInvalid`, and a `409` is none of them (#183, #184).
    pub fn template_id_collision(id: &str, files: Vec<String>, message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::CONFLICT,
            CODE_TEMPLATE_ID_COLLISION,
            message,
            Some(json!({ "template": id, "files": files })),
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

    pub fn media_mismatch(want_mm: f32, got_mm: f32) -> Self {
        Self::new(
            StatusCode::CONFLICT,
            CODE_MEDIA_MISMATCH,
            format!("template requires {want_mm}mm media but {got_mm}mm is loaded"),
            None,
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
        let message = err.to_string();
        match err {
            TemplateRegistryError::Io { .. } => {
                AppError::render_failed(Reason::TemplateRegistryIo, message)
            }
            TemplateRegistryError::Parse { .. } => {
                AppError::template_invalid(Reason::TemplateParseFailed, message)
            }
            TemplateRegistryError::Validation { .. } => {
                AppError::template_invalid(Reason::TemplateValidationFailed, message)
            }
            TemplateRegistryError::DuplicateId { .. } => {
                AppError::template_invalid(Reason::TemplateDuplicateId, message)
            }
        }
    }
}

impl From<crate::connector::ConnectorError> for AppError {
    fn from(err: crate::connector::ConnectorError) -> Self {
        use crate::connector::ConnectorError::*;
        // These codes sit outside the four that carry a `details.reason` (ADR-0052): they describe
        // upstream transport, not a layout or request fault.
        let (status, code, message): (StatusCode, &'static str, String) = match err {
            AuthFailed => (
                StatusCode::BAD_GATEWAY,
                "ConnectorAuthFailed",
                "upstream authentication failed".into(),
            ),
            Forbidden => (
                StatusCode::BAD_GATEWAY,
                "ConnectorForbidden",
                "upstream forbidden".into(),
            ),
            ConnectionFailed(m) => (StatusCode::BAD_GATEWAY, "ConnectorUnreachable", m),
            InvalidFilter(m) => (StatusCode::BAD_REQUEST, "InvalidFilter", m),
            UpstreamSchemaMismatch(m) => (StatusCode::BAD_GATEWAY, "UpstreamSchemaMismatch", m),
            RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                "RateLimited",
                "upstream rate limited".into(),
            ),
            BudgetExceeded => (
                StatusCode::BAD_REQUEST,
                "BudgetExceeded",
                "too many rows requested".into(),
            ),
            Upstream(m) => (StatusCode::BAD_GATEWAY, "Upstream", m),
        };
        Self::new(status, code, message, None)
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
    use crate::reason::Reason;
    use axum::http::StatusCode;

    #[test]
    fn container_padding_no_room_reason_is_registered() {
        assert_eq!(
            Reason::ContainerPaddingNoRoom.as_slug(),
            "container_padding_no_room"
        );
    }

    /// The SPEC §10.1 table is the published contract; the enum is what the code emits. If they
    /// drift, clients switch on slugs that either no longer exist or were never documented. Checked
    /// in both directions deliberately: an undocumented slug is as bad as a phantom one.
    #[test]
    fn spec_documents_every_reason_and_invents_none() {
        use crate::reason::Reason;
        use std::collections::HashSet;

        let spec = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/SPEC.md"))
            .expect("read SPEC.md");
        let section = spec
            .split("### 10.1")
            .nth(1)
            .expect("SPEC.md must have a §10.1 reason section");
        let section = section.split("\n## ").next().unwrap_or(section);

        // Column 2 is the slug, in backticks. The header cell reads "Reason" without backticks and
        // the separator row has none either, so both drop out here.
        let spec_table: HashSet<&str> = section
            .lines()
            .filter(|line| line.starts_with('|'))
            .filter_map(|line| line.split('|').nth(2))
            .map(str::trim)
            .filter_map(|cell| cell.strip_prefix('`')?.strip_suffix('`'))
            .collect();

        let declared: HashSet<&str> = Reason::ALL.iter().map(|r| r.as_slug()).collect();

        // A reason added after `docs/SPEC.md` was frozen (ADR-0057) cannot be listed in §10.1, so its
        // documented home is the OpenSpec spec that introduced it. Only `openspec/specs/**/spec.md`
        // counts: a proposal, a design note, or an archived change folder records what was planned,
        // not a published contract, and accepting one lets a reason ship documented nowhere a client
        // reads (#164 diff review). The phantom half below still runs off the §10.1 table alone, so
        // widening the undocumented half does not weaken it.
        // Active deltas count too, otherwise a change that adds a reason cannot pass this
        // test until archive publishes its delta, and the workflow demands clean gates
        // before archiving (#217). A delta is not a plan: it is the text archive publishes
        // verbatim, so a slug documented there is documented in the contract a step early.
        // `changes/archive/` stays excluded, per the reasoning above.
        let openspec_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("openspec");
        let specs_dir = openspec_dir.join("specs");
        let mut delta_dirs: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(openspec_dir.join("changes")) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.file_name().and_then(|n| n.to_str()) != Some("archive") {
                    delta_dirs.push(path.join("specs"));
                }
            }
        }
        fn scan_specs(dir: &std::path::Path, declared: &HashSet<&str>, out: &mut HashSet<String>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    scan_specs(&path, declared, out);
                } else if path.file_name().and_then(|n| n.to_str()) == Some("spec.md") {
                    let Ok(content) = std::fs::read_to_string(&path) else {
                        continue;
                    };
                    for slug in declared {
                        if content.contains(&format!("`{slug}`")) {
                            out.insert((*slug).to_string());
                        }
                    }
                }
            }
        }
        let mut documented: HashSet<String> =
            spec_table.iter().map(|slug| (*slug).to_string()).collect();
        scan_specs(&specs_dir, &declared, &mut documented);
        for delta in &delta_dirs {
            scan_specs(delta, &declared, &mut documented);
        }
        let documented_refs: HashSet<&str> = documented.iter().map(String::as_str).collect();

        let mut undocumented: Vec<_> = declared.difference(&documented_refs).collect();
        undocumented.sort_unstable();
        assert!(
            undocumented.is_empty(),
            "reasons documented in neither SPEC \u{a7}10.1 nor openspec/specs: {undocumented:?}"
        );

        // Phantom check: §10.1 only. `documented` also holds slugs harvested from OpenSpec specs,
        // but those are filtered through `declared` on the way in, so they can never be phantoms.
        let mut phantom: Vec<_> = spec_table.difference(&declared).collect();
        phantom.sort_unstable();
        assert!(
            phantom.is_empty(),
            "SPEC \u{a7}10.1 documents reasons that do not exist: {phantom:?}"
        );
    }

    /// Decision 4 of ADR-0052: a per-label failure carries `reason` exactly when its code is one of
    /// the migrated four. Both halves matter — a required field would contradict the scoping, and a
    /// missing one would leave the nested failures prose-discriminated.
    #[test]
    fn batch_failure_carries_reason_only_for_reasoned_codes() {
        use crate::reason::Reason;

        let reasoned = AppError::template_invalid(Reason::TemplateParseFailed, "boom");
        let failure = BatchFailure {
            index: 0,
            code: reasoned.code(),
            reason: reasoned.reason(),
            message: reasoned.message_text(),
        };
        let json = serde_json::to_value(&failure).expect("serialize");
        assert_eq!(json["reason"], "template_parse_failed");

        let unreasoned = AppError::missing_field("code");
        let failure = BatchFailure {
            index: 1,
            code: unreasoned.code(),
            reason: unreasoned.reason(),
            message: unreasoned.message_text(),
        };
        let json = serde_json::to_value(&failure).expect("serialize");
        assert!(
            json.get("reason").is_none(),
            "an unreasoned code must omit the key, got {json}"
        );
    }

    #[test]
    fn connector_errors_keep_their_codes_and_statuses() {
        use crate::connector::ConnectorError;
        let cases = [
            (
                ConnectorError::AuthFailed,
                StatusCode::BAD_GATEWAY,
                "ConnectorAuthFailed",
            ),
            (
                ConnectorError::Forbidden,
                StatusCode::BAD_GATEWAY,
                "ConnectorForbidden",
            ),
            (
                ConnectorError::RateLimited,
                StatusCode::TOO_MANY_REQUESTS,
                "RateLimited",
            ),
            (
                ConnectorError::BudgetExceeded,
                StatusCode::BAD_REQUEST,
                "BudgetExceeded",
            ),
            (
                ConnectorError::InvalidFilter("x".into()),
                StatusCode::BAD_REQUEST,
                "InvalidFilter",
            ),
            (
                ConnectorError::ConnectionFailed("x".into()),
                StatusCode::BAD_GATEWAY,
                "ConnectorUnreachable",
            ),
            (
                ConnectorError::UpstreamSchemaMismatch("x".into()),
                StatusCode::BAD_GATEWAY,
                "UpstreamSchemaMismatch",
            ),
            (
                ConnectorError::Upstream("x".into()),
                StatusCode::BAD_GATEWAY,
                "Upstream",
            ),
        ];
        for (err, status, code) in cases {
            let app_err = AppError::from(err);
            // `status` is private, but this module is a child of `errors`, so it is visible here.
            assert_eq!(app_err.status, status, "status for {code}");
            assert_eq!(app_err.code(), code);
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
