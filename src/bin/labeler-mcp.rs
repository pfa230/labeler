//! `labeler-mcp`: an MCP (stdio) adapter that fronts a running labeler REST service.
//!
//! It publishes twelve tools — template listing, detail, source, thumbnail, create,
//! replace, delete, reload, `render_label`, `list_printers`, `print_labels` and
//! `cut_printer` — over the official Rust MCP SDK, forwarding each call as an HTTP
//! request to the service origin in `LABELER_URL` (default `http://127.0.0.1:8080`),
//! with `LABELER_TOKEN` (when set) as a bearer token. The service owns validation,
//! rendering, printer connections and actor attribution; this process owns only the
//! MCP protocol and the HTTP forwarding.

use std::collections::HashMap;

use labeler::api::valid_printer_id;
use labeler::templates::validate_template_id_stem;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock},
    schemars::JsonSchema,
    tool, tool_handler, tool_router,
    transport::stdio,
    ServerHandler, ServiceExt,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use url::Url;

const DEFAULT_SERVICE_ORIGIN: &str = "http://127.0.0.1:8080";
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

/// The adapter's configuration, parsed once from the environment at startup.
struct Config {
    origin: Url,
    token: Option<String>,
}

/// Parse a service origin: absolute HTTP(S), no userinfo, query or fragment, and
/// only the root path (empty or `/`). Anything else is a startup failure. The
/// diagnostic names the reason but never echoes the raw value, which may hold
/// credentials.
fn parse_origin(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|e| format!("invalid LABELER_URL: {e}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("invalid LABELER_URL: scheme must be http or https".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("invalid LABELER_URL: userinfo is not allowed".to_string());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("invalid LABELER_URL: query and fragment are not allowed".to_string());
    }
    if !(url.path().is_empty() || url.path() == "/") {
        return Err("invalid LABELER_URL: only the root path is allowed".to_string());
    }
    Ok(url)
}

fn config_from_env() -> Result<Config, String> {
    let origin = match std::env::var("LABELER_URL") {
        Ok(raw) => parse_origin(&raw)?,
        Err(std::env::VarError::NotPresent) => {
            parse_origin(DEFAULT_SERVICE_ORIGIN).expect("the built-in default origin is valid")
        }
        // An origin that is set but not UTF-8 is invalid, not absent: the default
        // could silently target the wrong service. The value itself may hold
        // credentials, so it is never echoed.
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err("LABELER_URL is not valid UTF-8".to_string());
        }
    };
    let token = std::env::var("LABELER_TOKEN").ok();
    Ok(Config { origin, token })
}

/// No-argument tool parameters: any supplied key is unlisted and refused.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NoArgs {}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RenderLabelArgs {
    template: String,
    /// Parameter values for the template; names and JSON values pass through unchanged.
    data: HashMap<String, Value>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct LabelEntry {
    data: HashMap<String, Value>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PrintLabelsArgs {
    template: String,
    printer: String,
    labels: Vec<LabelEntry>,
    /// Present only when supplied; explicit `null` is refused (the REST spelling).
    // `required` publishes the integer's own schema and `skip_serializing_if`
    // drops the Option default, so discovery offers an optional, nonnullable
    // integer with no `null` default.
    #[serde(
        default,
        deserialize_with = "non_null",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(required)]
    start_slot: Option<u32>,
    /// Present only when supplied; explicit `null` is refused.
    #[serde(
        default,
        deserialize_with = "non_null",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(required)]
    chain_and_continue: Option<bool>,
}

/// Deserialize a present key as `Some`; a key written as `null` fails rather than
/// being treated as absent, matching the REST `start_slot` and `chain_and_continue`
/// rule.
fn non_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// The arguments of the cut tool: the printer's id, checked against the service's
/// printer id rule before any request, like template ids are.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CutPrinterArgs {
    /// Printer id: non-empty, matching ^[a-zA-Z0-9_-]+$ (letters, digits, '-' and '_').
    printer: String,
}

/// The template argument shared by the read and delete tools. The id rule is the
/// service's, published here so a client can refuse an impossible id before any
/// request is sent.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TemplateArgs {
    /// Template id: non-empty, matching ^[a-zA-Z0-9_-]+$ (letters, digits, '-' and '_').
    template: String,
}

/// The arguments of the complete-source write tools: the id plus the entire YAML
/// document to store under it.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TemplateWriteArgs {
    /// Template id: non-empty, matching ^[a-zA-Z0-9_-]+$ (letters, digits, '-' and '_').
    template: String,
    /// The template's complete YAML source, sent unchanged as the request body.
    yaml: String,
}

/// A request body: service JSON, or the raw YAML bytes of a template write.
enum Body {
    Json(Value),
    Yaml(String),
}

/// Labeler's error envelope, carried through unchanged into execution results.
#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    error: ErrorObject,
}

#[derive(Debug, Deserialize, Serialize)]
struct ErrorObject {
    code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<Value>,
}

#[derive(Clone)]
struct LabelerMcp {
    http: reqwest::Client,
    origin: Url,
    token: Option<String>,
}

fn json_text(value: &Value) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(value.to_string())])
}

fn error_text(message: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message.into())])
}

/// The service's PNG bytes as one MCP image block. A body without the PNG
/// signature is an execution error, never an advertised image.
fn png_image(bytes: &[u8], request: &str) -> CallToolResult {
    if !bytes.starts_with(PNG_MAGIC) {
        return error_text(format!(
            "the service returned a non-PNG body for a {request} request"
        ));
    }
    use base64::Engine as _;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    CallToolResult::success(vec![ContentBlock::image(encoded, "image/png")])
}

/// The single path segment of a template id. The id must satisfy the service's
/// rule; every permitted character is already valid literally in a URL path
/// segment, so the returned segment is the id unchanged.
fn id_segment(id: &str) -> Result<String, CallToolResult> {
    if !validate_template_id_stem(id) {
        return Err(error_text(format!(
            "invalid template id '{id}': must be non-empty and match ^[a-zA-Z0-9_-]+$"
        )));
    }
    Ok(id.to_string())
}

impl LabelerMcp {
    fn url(&self, path: &str) -> Url {
        self.origin
            .join(path)
            .expect("fixed API paths join the configured origin")
    }

    /// The URL of one path-based template tool: the validated literal id segment
    /// plus the tool's fixed suffix.
    fn template_url(&self, id: &str, suffix: &str) -> Result<Url, CallToolResult> {
        Ok(self.url(&format!("api/templates/{}{suffix}", id_segment(id)?)))
    }

    /// Send one REST request and classify the outcome. Success bodies must be the
    /// expected JSON; non-success bodies must be labeler's error envelope, which is
    /// preserved as `{status, error}` with `isError: true`.
    async fn rest_json(
        &self,
        method: reqwest::Method,
        url: Url,
        body: Option<&Body>,
    ) -> Result<Value, CallToolResult> {
        let (status, bytes) = self.send(method, url, body).await?;
        if !status.is_success() {
            return Err(self.service_error(status, &bytes));
        }
        serde_json::from_slice(&bytes).map_err(|_| {
            error_text(format!(
                "unexpected response from the labeler service (HTTP {status}): body is not JSON"
            ))
        })
    }

    /// Like `rest_json`, but a success body is returned as raw bytes (for PNG and
    /// source responses).
    async fn rest_bytes(
        &self,
        method: reqwest::Method,
        url: Url,
        body: Option<&Body>,
    ) -> Result<(reqwest::StatusCode, Vec<u8>), CallToolResult> {
        let (status, bytes) = self.send(method, url, body).await?;
        if !status.is_success() {
            return Err(self.service_error(status, &bytes));
        }
        Ok((status, bytes))
    }

    /// Send one mutation. A transport failure means the request may already have
    /// been applied when its response was lost, so it is reported as an unknown
    /// outcome; nothing is replayed.
    async fn mutate(
        &self,
        method: reqwest::Method,
        url: Url,
        body: Option<&Body>,
    ) -> Result<(reqwest::StatusCode, Vec<u8>), CallToolResult> {
        self.send(method, url, body).await.map_err(|err| {
            let text = err
                .content
                .first()
                .and_then(|c| c.as_text())
                .map(|t| t.text.clone())
                .unwrap_or_else(|| "the request failed".to_string());
            error_text(format!(
                "{text}; the mutation outcome is unknown, the change may already be applied \
                 and is never replayed"
            ))
        })
    }

    /// A mutation whose success body must be JSON, classified like `rest_json`.
    async fn mutate_json(
        &self,
        method: reqwest::Method,
        url: Url,
        body: Option<&Body>,
    ) -> Result<Value, CallToolResult> {
        let (status, bytes) = self.mutate(method, url, body).await?;
        if !status.is_success() {
            return Err(self.service_error(status, &bytes));
        }
        serde_json::from_slice(&bytes).map_err(|_| {
            error_text(format!(
                "unexpected response from the labeler service (HTTP {status}): body is not JSON"
            ))
        })
    }

    async fn send(
        &self,
        method: reqwest::Method,
        url: Url,
        body: Option<&Body>,
    ) -> Result<(reqwest::StatusCode, Vec<u8>), CallToolResult> {
        let mut req = self.http.request(method, url);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        match body {
            None => {}
            Some(Body::Json(value)) => req = req.json(value),
            Some(Body::Yaml(text)) => {
                req = req
                    .header(reqwest::header::CONTENT_TYPE, "text/yaml")
                    .body(text.clone())
            }
        }
        let resp = req
            .send()
            .await
            .map_err(|e| error_text(format!("labeler service unreachable: {e}")))?;
        let status = resp.status();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| error_text(format!("failed to read the service response: {e}")))?
            .to_vec();
        Ok((status, bytes))
    }

    fn service_error(&self, status: reqwest::StatusCode, bytes: &[u8]) -> CallToolResult {
        match serde_json::from_slice::<ErrorEnvelope>(bytes) {
            Ok(envelope) => error_text(
                json!({ "status": status.as_u16(), "error": envelope.error }).to_string(),
            ),
            Err(_) => error_text(format!(
                "labeler service returned HTTP {status} without a labeler error body"
            )),
        }
    }
}

#[tool_router]
impl LabelerMcp {
    #[tool(
        name = "list_templates",
        description = "List the labeler service's label templates; each summary includes the template's published parameters.",
        annotations(read_only_hint = true)
    )]
    async fn list_templates(
        &self,
        Parameters(_args): Parameters<NoArgs>,
    ) -> Result<CallToolResult, CallToolResult> {
        let body = self
            .rest_json(reqwest::Method::GET, self.url("api/templates"), None)
            .await?;
        Ok(json_text(&body))
    }

    #[tool(
        name = "get_template",
        description = "Read one template's published detail as JSON: identity, format, categories, parameters and referenced variables. A quarantined (broken) template has no detail; read its source with get_template_source.",
        annotations(read_only_hint = true)
    )]
    async fn get_template(
        &self,
        Parameters(args): Parameters<TemplateArgs>,
    ) -> Result<CallToolResult, CallToolResult> {
        let url = self.template_url(&args.template, "")?;
        let body = self.rest_json(reqwest::Method::GET, url, None).await?;
        Ok(json_text(&body))
    }

    #[tool(
        name = "get_template_source",
        description = "Read the stored YAML source of one template as exact text, comments and trailing whitespace included, without parsing or reformatting it. Works for quarantined (broken) templates too.",
        annotations(read_only_hint = true)
    )]
    async fn get_template_source(
        &self,
        Parameters(args): Parameters<TemplateArgs>,
    ) -> Result<CallToolResult, CallToolResult> {
        let url = self.template_url(&args.template, "/source")?;
        let (status, bytes) = self.rest_bytes(reqwest::Method::GET, url, None).await?;
        let source = String::from_utf8(bytes).map_err(|_| {
            error_text(format!(
                "unexpected response from the labeler service (HTTP {status}): body is not UTF-8"
            ))
        })?;
        Ok(CallToolResult::success(vec![ContentBlock::text(source)]))
    }

    #[tool(
        name = "get_template_thumbnail",
        description = "Fetch a template's placeholder thumbnail as a PNG image; a sheet template yields one slot. The request is unconditional and never prints.",
        annotations(read_only_hint = true)
    )]
    async fn get_template_thumbnail(
        &self,
        Parameters(args): Parameters<TemplateArgs>,
    ) -> Result<CallToolResult, CallToolResult> {
        let url = self.template_url(&args.template, "/thumbnail")?;
        let (_, bytes) = self.rest_bytes(reqwest::Method::GET, url, None).await?;
        Ok(png_image(&bytes, "thumbnail"))
    }

    #[tool(
        name = "create_template",
        description = "Create a new template from a complete YAML source and republish the live registry. The service validates the source; an id that already exists is refused with a conflict and never overwritten. The adapter does not retry: if the response is lost the outcome is unknown, the template may already exist.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false
        )
    )]
    async fn create_template(
        &self,
        Parameters(args): Parameters<TemplateWriteArgs>,
    ) -> Result<CallToolResult, CallToolResult> {
        let url = self.template_url(&args.template, "")?;
        let body = Body::Yaml(args.yaml);
        let detail = self
            .mutate_json(reqwest::Method::POST, url, Some(&body))
            .await?;
        Ok(json_text(&detail))
    }

    #[tool(
        name = "replace_template",
        description = "Replace an existing template's stored YAML wholesale with the supplied complete source: the whole file is overwritten, never merged or patched, and an id that does not exist is refused and never created. The adapter does not retry: if the response is lost the outcome is unknown, the file may already be changed.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false
        )
    )]
    async fn replace_template(
        &self,
        Parameters(args): Parameters<TemplateWriteArgs>,
    ) -> Result<CallToolResult, CallToolResult> {
        let url = self.template_url(&args.template, "")?;
        let body = Body::Yaml(args.yaml);
        let detail = self
            .mutate_json(reqwest::Method::PUT, url, Some(&body))
            .await?;
        Ok(json_text(&detail))
    }

    #[tool(
        name = "delete_template",
        description = "Delete a template's stored file and drop it from the live registry. The adapter does not retry: if the response is lost the outcome is unknown, the template may already be gone.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false
        )
    )]
    async fn delete_template(
        &self,
        Parameters(args): Parameters<TemplateArgs>,
    ) -> Result<CallToolResult, CallToolResult> {
        let url = self.template_url(&args.template, "")?;
        let (status, bytes) = self.mutate(reqwest::Method::DELETE, url, None).await?;
        if !status.is_success() {
            return Err(self.service_error(status, &bytes));
        }
        // The service answers 204 with no body; an empty content array is the
        // acknowledgement, and inventing JSON would claim more than it returned.
        Ok(CallToolResult::success(vec![]))
    }

    #[tool(
        name = "reload_templates",
        description = "Re-read the templates folder and replace the live registry from disk: valid files are served, broken ones quarantined. The adapter does not retry: if the response is lost the outcome is unknown, the registry may already be replaced.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false
        )
    )]
    async fn reload_templates(
        &self,
        Parameters(_args): Parameters<NoArgs>,
    ) -> Result<CallToolResult, CallToolResult> {
        let body = self
            .mutate_json(
                reqwest::Method::POST,
                self.url("api/template-registry/reload"),
                None,
            )
            .await?;
        Ok(json_text(&body))
    }

    #[tool(
        name = "render_label",
        description = "Preview a filled label as a PNG image without printing. Only single-label templates can be previewed; sheet templates are refused by the service (print them with print_labels).",
        annotations(read_only_hint = true)
    )]
    async fn render_label(
        &self,
        Parameters(args): Parameters<RenderLabelArgs>,
    ) -> Result<CallToolResult, CallToolResult> {
        let body = json!({ "template": args.template, "data": args.data });
        let (_, bytes) = self
            .rest_bytes(
                reqwest::Method::POST,
                self.url("api/render/label?format=png"),
                Some(&Body::Json(body)),
            )
            .await?;
        Ok(png_image(&bytes, "preview"))
    }

    #[tool(
        name = "list_printers",
        description = "List the labeler service's configured printers.",
        annotations(read_only_hint = true)
    )]
    async fn list_printers(
        &self,
        Parameters(_args): Parameters<NoArgs>,
    ) -> Result<CallToolResult, CallToolResult> {
        let body = self
            .rest_json(reqwest::Method::GET, self.url("api/printers"), None)
            .await?;
        Ok(json_text(&body))
    }

    #[tool(
        name = "print_labels",
        description = "Send physical print jobs for a batch of labels to a configured printer. This has a physical effect. chain_and_continue: true selects chain and continue, where each label separates and the final end stays attached for the next print; false or omission selects ordinary printing, and the service refuses chain-and-continue when the printer does not support it. The adapter does not retry: if the response is lost, the dispatch outcome is unknown.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false
        )
    )]
    async fn print_labels(
        &self,
        Parameters(args): Parameters<PrintLabelsArgs>,
    ) -> Result<CallToolResult, CallToolResult> {
        let mut body = serde_json::Map::new();
        body.insert("template".into(), Value::String(args.template));
        body.insert("printer".into(), Value::String(args.printer));
        body.insert(
            "labels".into(),
            serde_json::to_value(&args.labels).expect("label entries serialize"),
        );
        // Absent when omitted; never written as null (explicit null was refused above).
        if let Some(slot) = args.start_slot {
            body.insert("start_slot".into(), json!(slot));
        }
        if let Some(chain) = args.chain_and_continue {
            body.insert("chain_and_continue".into(), json!(chain));
        }
        let url = self.url("api/print");
        let (status, bytes) = match self
            .send(
                reqwest::Method::POST,
                url,
                Some(&Body::Json(Value::Object(body))),
            )
            .await
        {
            Ok(outcome) => outcome,
            Err(e) => {
                // One invocation makes at most one print POST; a lost response is
                // reported as unknown rather than retried.
                let text = match e.content.first().and_then(|c| c.as_text()) {
                    Some(t) => t.text.clone(),
                    None => "the print request failed".to_string(),
                };
                return Ok(error_text(format!(
                    "{text}; the print dispatch outcome is unknown"
                )));
            }
        };
        if !status.is_success() {
            return Err(self.service_error(status, &bytes));
        }
        // The complete summary passes through unchanged; only its failed list
        // decides isError.
        let value: Value = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(_) => {
                return Ok(error_text(format!(
                    "unexpected response from the labeler service (HTTP {status}): body is not a print summary"
                )));
            }
        };
        let Some(failed) = value.get("failed").and_then(Value::as_array) else {
            return Ok(error_text(format!(
                "unexpected response from the labeler service (HTTP {status}): body is not a print summary"
            )));
        };
        let mut result = json_text(&value);
        if !failed.is_empty() {
            result.is_error = Some(true);
        }
        Ok(result)
    }

    #[tool(
        name = "cut_printer",
        description = "Cut the tape to release the last label held by chain and continue, on printers whose probe reports cut. The adapter does not retry: if the response is lost the outcome is unknown.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false
        )
    )]
    async fn cut_printer(
        &self,
        Parameters(args): Parameters<CutPrinterArgs>,
    ) -> Result<CallToolResult, CallToolResult> {
        if !valid_printer_id(&args.printer) {
            return Err(error_text(format!(
                "invalid printer id '{}': must be non-empty and match ^[a-zA-Z0-9_-]+$",
                args.printer
            )));
        }
        let url = self.url(&format!("api/printers/{}/cut", args.printer));
        let body = self.mutate_json(reqwest::Method::POST, url, None).await?;
        // The complete answer passes through unchanged; only its status decides isError.
        let mut result = json_text(&body);
        if body.get("status").and_then(Value::as_str) == Some("failed") {
            result.is_error = Some(true);
        }
        Ok(result)
    }
}

#[tool_handler(name = "labeler-mcp")]
impl ServerHandler for LabelerMcp {}

#[tokio::main]
async fn main() {
    let config = match config_from_env() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("labeler-mcp: {err}");
            std::process::exit(1);
        }
    };
    // One invocation may make at most one request per tool call: refuse redirects,
    // which would replay the request body, and disable the client's automatic
    // retries, so a print or a template write is never sent twice.
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()
        .expect("the HTTP client builds without special configuration");
    let server = LabelerMcp {
        http,
        origin: config.origin,
        token: config.token,
    };
    let service = match server.serve(stdio()).await {
        Ok(service) => service,
        Err(err) => {
            eprintln!("labeler-mcp: failed to start the MCP server: {err}");
            std::process::exit(1);
        }
    };
    if let Err(err) = service.waiting().await {
        eprintln!("labeler-mcp: MCP server stopped with an error: {err}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_origin_parses() {
        let url = parse_origin(DEFAULT_SERVICE_ORIGIN).unwrap();
        assert_eq!(url.as_str(), "http://127.0.0.1:8080/");
    }

    #[test]
    fn explicit_http_and_https_origins_parse() {
        assert!(parse_origin("http://127.0.0.1:9090").is_ok());
        assert!(parse_origin("https://labeler.example.com/").is_ok());
    }

    #[test]
    fn file_scheme_is_refused() {
        let err = parse_origin("file:///tmp/labeler").unwrap_err();
        assert!(err.contains("scheme"), "{err}");
    }

    #[test]
    fn userinfo_query_fragment_and_path_are_refused() {
        assert!(parse_origin("http://user@127.0.0.1:8080")
            .unwrap_err()
            .contains("userinfo"));
        assert!(parse_origin("http://127.0.0.1:8080?a=1")
            .unwrap_err()
            .contains("query"));
        assert!(parse_origin("http://127.0.0.1:8080/#x")
            .unwrap_err()
            .contains("fragment"));
        assert!(parse_origin("http://127.0.0.1:8080/api")
            .unwrap_err()
            .contains("root path"));
    }

    #[test]
    fn valid_ids_are_literal_and_invalid_ids_are_refused() {
        assert_eq!(id_segment("reload").ok().as_deref(), Some("reload"));
        assert_eq!(
            id_segment("brother_24mm-qr").ok().as_deref(),
            Some("brother_24mm-qr")
        );
        assert!(id_segment("").is_err());
        assert!(id_segment("../other").is_err());
        assert!(id_segment("has space").is_err());
        assert!(id_segment("has.dot").is_err());
    }
}
