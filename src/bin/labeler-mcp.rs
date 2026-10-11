//! `labeler-mcp`: an MCP (stdio) adapter that fronts a running labeler REST service.
//!
//! It publishes four tools — `list_templates`, `render_label`, `list_printers` and
//! `print_labels` — over the official Rust MCP SDK, forwarding each call as an HTTP
//! request to the service origin in `LABELER_URL` (default `http://127.0.0.1:8080`),
//! with `LABELER_TOKEN` (when set) as a bearer token. The service owns validation,
//! rendering, printer connections and actor attribution; this process owns only the
//! MCP protocol and the HTTP forwarding.

use std::collections::HashMap;

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
        Err(_) => {
            parse_origin(DEFAULT_SERVICE_ORIGIN).expect("the built-in default origin is valid")
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
}

/// Deserialize a present key as `Some`; a key written as `null` fails rather than
/// being treated as absent, matching the REST `start_slot` rule.
fn non_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
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

impl LabelerMcp {
    fn url(&self, path: &str) -> Url {
        self.origin
            .join(path)
            .expect("fixed API paths join the configured origin")
    }

    /// Send one REST request and classify the outcome. Success bodies must be the
    /// expected JSON; non-success bodies must be labeler's error envelope, which is
    /// preserved as `{status, error}` with `isError: true`.
    async fn rest_json(
        &self,
        method: reqwest::Method,
        url: Url,
        body: Option<&Value>,
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

    /// Like `rest_json`, but a success body is returned as raw bytes (for PNG previews).
    async fn rest_bytes(
        &self,
        method: reqwest::Method,
        url: Url,
        body: Option<&Value>,
    ) -> Result<(reqwest::StatusCode, Vec<u8>), CallToolResult> {
        let (status, bytes) = self.send(method, url, body).await?;
        if !status.is_success() {
            return Err(self.service_error(status, &bytes));
        }
        Ok((status, bytes))
    }

    async fn send(
        &self,
        method: reqwest::Method,
        url: Url,
        body: Option<&Value>,
    ) -> Result<(reqwest::StatusCode, Vec<u8>), CallToolResult> {
        let mut req = self.http.request(method, url);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        if let Some(body) = body {
            req = req.json(body);
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
                Some(&body),
            )
            .await?;
        if !bytes.starts_with(PNG_MAGIC) {
            return Ok(error_text(
                "the service returned a non-PNG body for a preview request",
            ));
        }
        use base64::Engine as _;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
        Ok(CallToolResult::success(vec![ContentBlock::image(
            encoded,
            "image/png",
        )]))
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
        description = "Send physical print jobs for a batch of labels to a configured printer. This has a physical effect. The adapter does not retry: if the response is lost, the dispatch outcome is unknown.",
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
        let url = self.url("api/print");
        let (status, bytes) = match self
            .send(reqwest::Method::POST, url, Some(&Value::Object(body)))
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
    // One invocation may make at most one print POST: refuse redirects, which
    // would replay the request body, and disable the client's automatic retries.
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
}
