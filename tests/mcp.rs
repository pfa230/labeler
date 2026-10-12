//! Adapter tests for the `labeler-mcp` stdio binary: protocol handshake, tool
//! surface, configuration and REST forwarding against mock services.
//!
//! Every test spawns the real binary in its own process environment and speaks
//! newline-delimited JSON-RPC over its stdio, so the assertions cover the shipped
//! adapter rather than its internals.

mod common;

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use common::{error_object_text, first_text, is_error, McpProcess};
use serde_json::{json, Value};

const PNG_B64: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

fn png_bytes() -> Vec<u8> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(PNG_B64)
        .expect("fixture PNG is valid base64")
}

fn tool_names(tools: &Value) -> Vec<String> {
    tools["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .map(|t| t["name"].as_str().expect("tool name").to_string())
        .collect()
}

fn find_tool<'a>(tools: &'a Value, name: &str) -> &'a Value {
    tools["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .find(|t| t["name"] == name)
        .unwrap_or_else(|| panic!("tool {name} is published"))
}

// ---------------------------------------------------------------------------
// 1.1 stdio adapter: initialization, tools capability, clean stdout, exit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn initializes_negotiates_tools_and_exits_on_stdin_closure() {
    let mut mcp = McpProcess::spawn(&[]).await;
    let init = mcp.initialize().await;
    let result = &init["result"];
    assert_eq!(
        result["serverInfo"]["name"], "labeler-mcp",
        "initialize names the server: {init}"
    );
    assert!(
        result["capabilities"].get("tools").is_some(),
        "tools capability is advertised: {init}"
    );

    let tools = mcp.list_tools().await;
    assert_eq!(
        tool_names(&tools),
        vec![
            "list_printers",
            "list_templates",
            "print_labels",
            "render_label"
        ]
    );

    let (success, lines, _) = mcp.shutdown().await;
    assert!(success, "stdin closure ends the process successfully");
    assert!(!lines.is_empty(), "the protocol produced messages");
    for line in &lines {
        let message: Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("stdout carries non-protocol text ({e}): {line}"));
        assert!(
            message.get("jsonrpc").is_some(),
            "stdout line is an MCP message: {line}"
        );
    }
}

// ---------------------------------------------------------------------------
// 1.2 service origin and bearer forwarding
// ---------------------------------------------------------------------------

#[tokio::test]
async fn default_origin_serves_requests_when_labeler_url_is_unset() {
    let listener = std::net::TcpListener::bind("127.0.0.1:8080")
        .expect("bind 127.0.0.1:8080 for the default-origin test; stop whatever is on it");
    let mock = wiremock::MockServer::builder()
        .listener(listener)
        .start()
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/api/templates"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "templates": []
        })))
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[]).await;
    mcp.initialize().await;
    let result = mcp.tool_result("list_templates", json!({})).await;
    assert_eq!(first_text(&result), r#"{"templates":[]}"#);
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn explicit_http_origin_reaches_the_mock_service() {
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/api/printers"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;
    let result = mcp.tool_result("list_printers", json!({})).await;
    assert_eq!(first_text(&result), "[]");
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn invalid_service_origin_exits_before_serving() {
    let mcp = McpProcess::spawn(&[("LABELER_URL", "file:///tmp/labeler")]).await;
    // No initialize: the process must refuse to start rather than speak MCP.
    // Closing stdin is a no-op for an already-exited process; wait returns at once.
    let (success, lines, stderr) = mcp.shutdown().await;
    assert!(!success, "invalid origin exits nonzero");
    assert!(
        lines.is_empty(),
        "no protocol output before exit: {lines:?}"
    );
    assert!(
        stderr.contains("LABELER_URL"),
        "diagnostic names the setting: {stderr}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn non_utf8_service_origin_exits_before_serving() {
    use std::os::unix::ffi::OsStrExt as _;
    let origin = std::ffi::OsStr::from_bytes(b"http://\xff\xfe");
    let mcp = McpProcess::spawn_os(&[("LABELER_URL", origin)]).await;
    let (success, lines, stderr) = mcp.shutdown().await;
    assert!(!success, "non-UTF-8 origin exits nonzero");
    assert!(
        lines.is_empty(),
        "no protocol output before exit: {lines:?}"
    );
    assert!(
        stderr.contains("LABELER_URL"),
        "diagnostic names the setting: {stderr}"
    );
}

#[tokio::test]
async fn rejected_credentials_are_absent_from_the_startup_diagnostic() {
    let mcp = McpProcess::spawn(&[(
        "LABELER_URL",
        "http://secret-user:secret-pass@127.0.0.1:8080",
    )])
    .await;
    let (success, lines, stderr) = mcp.shutdown().await;
    assert!(!success, "userinfo exits nonzero");
    assert!(lines.is_empty(), "no protocol output: {lines:?}");
    assert!(stderr.contains("userinfo"), "the reason is named: {stderr}");
    assert!(
        !stderr.contains("secret-user") && !stderr.contains("secret-pass"),
        "the rejected credentials never appear in diagnostics: {stderr}"
    );
}

#[tokio::test]
async fn token_is_sent_as_bearer_and_never_appears_in_output() {
    const TOKEN: &str = "test-secret-token-value";
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/api/templates"))
        .and(wiremock::matchers::header(
            "authorization",
            format!("Bearer {TOKEN}"),
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "templates": []
        })))
        .expect(1)
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[
        ("LABELER_URL", mock.uri().as_str()),
        ("LABELER_TOKEN", TOKEN),
    ])
    .await;
    mcp.initialize().await;
    let result = mcp.tool_result("list_templates", json!({})).await;
    assert!(!is_error(&result));
    let (success, lines, stderr) = mcp.shutdown().await;
    assert!(success);
    let all_output = lines.join("\n") + &stderr;
    assert!(
        !all_output.contains(TOKEN),
        "the token never appears in diagnostics or results"
    );
}

#[tokio::test]
async fn absent_token_sends_no_authorization_header() {
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/api/templates"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "templates": []
        })))
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;
    let result = mcp.tool_result("list_templates", json!({})).await;
    assert!(!is_error(&result));
    let requests = mock.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert!(
        !requests[0]
            .headers
            .iter()
            .any(|(name, _)| name.as_str().eq_ignore_ascii_case("authorization")),
        "no Authorization header without a token"
    );
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

// ---------------------------------------------------------------------------
// 1.3 tool surface: schemas, annotations, argument refusal, forwarding
// ---------------------------------------------------------------------------

#[tokio::test]
async fn discovery_publishes_four_tools_with_schemas_and_annotations() {
    let mut mcp = McpProcess::spawn(&[]).await;
    mcp.initialize().await;
    let tools = mcp.list_tools().await;
    assert_eq!(tools["tools"].as_array().unwrap().len(), 4);

    let print = find_tool(&tools, "print_labels");
    let schema = &print["inputSchema"];
    for field in ["template", "printer", "labels"] {
        assert!(
            schema["properties"].get(field).is_some(),
            "print_labels schema requires {field}: {schema}"
        );
        assert!(
            schema["required"]
                .as_array()
                .is_some_and(|r| r.iter().any(|v| v == field)),
            "{field} is required: {schema}"
        );
    }
    assert!(
        schema["properties"].get("start_slot").is_some(),
        "start_slot is published: {schema}"
    );
    assert!(
        !schema["required"]
            .as_array()
            .is_some_and(|r| r.iter().any(|v| v == "start_slot")),
        "start_slot is optional: {schema}"
    );
    let start_slot = &schema["properties"]["start_slot"];
    assert_eq!(
        start_slot["type"], "integer",
        "start_slot is a nonnullable integer: {start_slot}"
    );
    assert!(
        start_slot.get("default").is_none(),
        "start_slot has no null default: {start_slot}"
    );
    let annotations = &print["annotations"];
    assert_eq!(annotations["readOnlyHint"], false);
    assert_eq!(annotations["destructiveHint"], true);
    assert_eq!(annotations["idempotentHint"], false);
    let description = print["description"].as_str().expect("description");
    assert!(
        description.contains("physical"),
        "print_labels states the physical effect: {description}"
    );
    assert!(
        description.contains("unknown"),
        "print_labels states lost-response uncertainty: {description}"
    );

    for name in ["list_templates", "render_label", "list_printers"] {
        let tool = find_tool(&tools, name);
        assert_eq!(
            tool["annotations"]["readOnlyHint"], true,
            "{name} is read-only"
        );
    }
    let templates_desc = find_tool(&tools, "list_templates")["description"]
        .as_str()
        .unwrap();
    assert!(
        templates_desc.to_lowercase().contains("parameter"),
        "list_templates mentions published parameters: {templates_desc}"
    );
    let render = find_tool(&tools, "render_label");
    assert!(
        render["description"].as_str().unwrap().contains("single"),
        "render_label states the single-template limit"
    );
    let render_schema = &render["inputSchema"];
    assert_eq!(
        render_schema["properties"]["data"]["type"], "object",
        "data is an object: {render_schema}"
    );

    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn unknown_tool_and_malformed_envelope_are_protocol_errors() {
    let mut mcp = McpProcess::spawn(&[]).await;
    mcp.initialize().await;

    let unknown = mcp.call_tool("no_such_tool", json!({})).await;
    assert!(
        unknown.get("error").is_some(),
        "unknown tool is a protocol error: {unknown}"
    );
    assert!(
        unknown.get("result").is_none(),
        "unknown tool produces no execution result: {unknown}"
    );

    let malformed = mcp.request("tools/call", json!({ "arguments": {} })).await;
    assert!(
        malformed.get("error").is_some(),
        "a call without a tool name is a protocol error: {malformed}"
    );

    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn unlisted_arguments_missing_fields_and_null_start_slot_are_execution_errors() {
    // No mocks: any HTTP call would 404, so an isError result with an empty
    // request log proves the adapter refused before calling REST.
    let mock = wiremock::MockServer::start().await;
    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;

    let unexpected = mcp
        .tool_result(
            "print_labels",
            json!({
                "template": "t",
                "printer": "p",
                "labels": [{"data": {}}],
                "copies": 3
            }),
        )
        .await;
    assert!(is_error(&unexpected), "unlisted argument: {unexpected}");
    assert!(
        first_text(&unexpected).to_lowercase().contains("copies"),
        "the message names the unlisted key: {unexpected}"
    );

    let missing = mcp
        .tool_result("print_labels", json!({ "template": "t", "printer": "p" }))
        .await;
    assert!(is_error(&missing), "missing labels: {missing}");
    assert!(
        first_text(&missing).contains("labels"),
        "the message names the missing argument: {missing}"
    );

    let null_slot = mcp
        .tool_result(
            "print_labels",
            json!({
                "template": "t",
                "printer": "p",
                "labels": [{"data": {}}],
                "start_slot": Value::Null
            }),
        )
        .await;
    assert!(is_error(&null_slot), "null start_slot: {null_slot}");

    assert!(
        mock.received_requests().await.unwrap().is_empty(),
        "argument refusals make no REST call"
    );
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn nested_json_values_and_reserved_names_survive_forwarding() {
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/render/label"))
        .and(wiremock::matchers::query_param("format", "png"))
        .and(wiremock::matchers::body_partial_json(json!({
            "template": "t1",
            "data": {
                "name": "Widget",
                "count": 3,
                "active": true,
                "tags": ["a", "b"],
                "nested": {"deep": [1, 2.5, null]},
                "__proto__": "polluted",
                "constructor": {"x": 1}
            }
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_bytes(png_bytes())
                .insert_header("content-type", "image/png"),
        )
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;
    let result = mcp
        .tool_result(
            "render_label",
            json!({
                "template": "t1",
                "data": {
                    "name": "Widget",
                    "count": 3,
                    "active": true,
                    "tags": ["a", "b"],
                    "nested": {"deep": [1, 2.5, null]},
                    "__proto__": "polluted",
                    "constructor": {"x": 1}
                }
            }),
        )
        .await;
    assert!(!is_error(&result), "{result}");

    // Body fidelity beyond the partial match: the mock only matched if the whole
    // data object arrived, including nested nulls and reserved-looking names.
    let requests = mock.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

// ---------------------------------------------------------------------------
// 2.1 catalog reads
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_templates_returns_service_json_with_params_and_broken() {
    let body = json!({
        "templates": [{
            "id": "brother_24mm",
            "name": "Brother 24mm",
            "description": "",
            "categories": ["brother"],
            "unit": "mm",
            "dpi": 200,
            "params": [{"name": "message", "label": "Message", "type": "string"}],
            "format": {"type": "single", "width": 24, "height": 50}
        }],
        "broken": [{"path": "bad.yaml", "error": "invalid template"}]
    });
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/api/templates"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(&body))
        .expect(1)
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;
    let result = mcp.tool_result("list_templates", json!({})).await;
    assert!(!is_error(&result), "{result}");
    let text = first_text(&result);
    let returned: Value = serde_json::from_str(&text).expect("JSON text content");
    assert_eq!(returned, body);
    let params = &returned["templates"][0]["params"];
    assert!(
        params.as_array().is_some_and(|p| !p.is_empty()),
        "published parameters pass through: {returned}"
    );
    assert_eq!(
        returned["broken"][0]["path"], "bad.yaml",
        "quarantined templates are retained: {returned}"
    );
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn list_printers_returns_service_json() {
    let body = json!([{
        "id": "p1",
        "name": "Desk",
        "uri": "ipp://printer.local/",
        "insecure": false
    }]);
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/api/printers"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(&body))
        .expect(1)
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;
    let result = mcp.tool_result("list_printers", json!({})).await;
    assert!(!is_error(&result), "{result}");
    let returned: Value = serde_json::from_str(&first_text(&result)).expect("JSON");
    assert_eq!(returned, body);
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

// ---------------------------------------------------------------------------
// 2.2 PNG preview
// ---------------------------------------------------------------------------

#[tokio::test]
async fn render_label_returns_the_service_png_as_an_image_block() {
    let png = png_bytes();
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/render/label"))
        .and(wiremock::matchers::query_param("format", "png"))
        .and(wiremock::matchers::body_json(json!({
            "template": "label1",
            "data": {"message": "Hello"}
        })))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_bytes(png.clone())
                .insert_header("content-type", "image/png"),
        )
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;
    let result = mcp
        .tool_result(
            "render_label",
            json!({ "template": "label1", "data": {"message": "Hello"} }),
        )
        .await;
    assert!(!is_error(&result), "{result}");
    let image = &result["content"][0];
    assert_eq!(image["type"], "image", "{result}");
    assert_eq!(image["mimeType"], "image/png", "{result}");
    use base64::Engine as _;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(image["data"].as_str().expect("base64 data"))
        .expect("decodes");
    assert_eq!(decoded, png, "decoded MCP image equals the service PNG");

    let requests = mock.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1, "one preview request, no print request");
    assert_eq!(requests[0].method.as_str(), "POST");
    assert_eq!(requests[0].url.path(), "/api/render/label");
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn render_label_rejects_a_successful_non_png_body() {
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/render/label"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_string("<html>not a png</html>")
                .insert_header("content-type", "text/html"),
        )
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;
    let result = mcp
        .tool_result("render_label", json!({ "template": "t", "data": {} }))
        .await;
    assert!(is_error(&result), "{result}");
    assert!(
        first_text(&result).to_lowercase().contains("png"),
        "the message names the problem: {result}"
    );
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn sheet_refusal_propagates_as_the_service_error() {
    let envelope = json!({
        "error": {
            "code": "TemplateInvalid",
            "message": "sheet templates cannot be rendered as a single label",
            "details": {"reason": "format_unsupported"}
        }
    });
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/render/label"))
        .respond_with(wiremock::ResponseTemplate::new(422).set_body_json(&envelope))
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;
    let result = mcp
        .tool_result("render_label", json!({ "template": "sheet", "data": {} }))
        .await;
    assert!(is_error(&result), "{result}");
    let carried = error_object_text(&result);
    assert_eq!(carried["status"], 422);
    assert_eq!(carried["error"]["code"], "TemplateInvalid");
    assert_eq!(carried["error"]["details"]["reason"], "format_unsupported");
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

// ---------------------------------------------------------------------------
// 2.3 print_labels
// ---------------------------------------------------------------------------

#[tokio::test]
async fn print_labels_forwards_arguments_and_returns_a_full_summary() {
    // The summary carries an additional field beyond the four known keys: the
    // result must preserve it unchanged rather than drop it on reserialization.
    let summary = json!({
        "total": 2,
        "sent": 2,
        "failed": [],
        "jobs": 2,
        "printer": "p1"
    });
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/print"))
        .and(wiremock::matchers::body_json(json!({
            "template": "sheet1",
            "printer": "p1",
            "labels": [
                {"data": {"id": "A1"}},
                {"data": {"id": "A2"}}
            ],
            "start_slot": 2
        })))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(&summary))
        .expect(1)
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;
    let result = mcp
        .tool_result(
            "print_labels",
            json!({
                "template": "sheet1",
                "printer": "p1",
                "labels": [{"data": {"id": "A1"}}, {"data": {"id": "A2"}}],
                "start_slot": 2
            }),
        )
        .await;
    assert!(
        !is_error(&result),
        "a full summary is not an error: {result}"
    );
    let returned: Value = serde_json::from_str(&first_text(&result)).expect("summary JSON");
    assert_eq!(returned, summary);
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn omitted_start_slot_is_absent_from_the_forwarded_body() {
    let summary = json!({"total": 1, "sent": 1, "failed": [], "jobs": 1});
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/print"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(&summary))
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;
    let result = mcp
        .tool_result(
            "print_labels",
            json!({
                "template": "t",
                "printer": "p",
                "labels": [{"data": {"m": "x"}}]
            }),
        )
        .await;
    assert!(!is_error(&result), "{result}");
    let requests = mock.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: Value = serde_json::from_slice(&requests[0].body).expect("JSON body");
    assert!(
        !body
            .as_object()
            .is_some_and(|o| o.contains_key("start_slot")),
        "omitted start_slot stays absent: {body}"
    );
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn partial_print_failure_returns_the_summary_with_is_error() {
    let summary = json!({
        "total": 2,
        "sent": 1,
        "failed": [{"index": 1, "error": "printer refused the job"}],
        "jobs": 2
    });
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/print"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(&summary))
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;
    let result = mcp
        .tool_result(
            "print_labels",
            json!({
                "template": "t",
                "printer": "p",
                "labels": [{"data": {}}, {"data": {}}]
            }),
        )
        .await;
    assert!(is_error(&result), "partial failure sets isError: {result}");
    let returned: Value = serde_json::from_str(&first_text(&result)).expect("summary JSON");
    assert_eq!(returned, summary, "the summary is unchanged");
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn dropped_print_response_reports_unknown_outcome_without_resending() {
    // A raw HTTP peer that reads each request fully, counts it, then closes the
    // connection without responding: the adapter must report an unknown outcome
    // and must not send a second POST.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let print_posts = Arc::new(AtomicUsize::new(0));
    let counter = print_posts.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let counter = counter.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncReadExt as _;
                let mut collected = Vec::new();
                let mut chunk = [0u8; 8192];
                let mut header_end = None;
                let mut content_length = 0usize;
                loop {
                    let n = match socket.read(&mut chunk).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => n,
                    };
                    collected.extend_from_slice(&chunk[..n]);
                    if header_end.is_none() {
                        if let Some(pos) = collected.windows(4).position(|w| w == b"\r\n\r\n") {
                            header_end = Some(pos + 4);
                            let head = String::from_utf8_lossy(&collected[..pos]);
                            content_length = head
                                .lines()
                                .find_map(|l| {
                                    let (k, v) = l.split_once(':')?;
                                    k.eq_ignore_ascii_case("content-length")
                                        .then(|| v.trim().parse::<usize>().ok())
                                        .flatten()
                                })
                                .unwrap_or(0);
                        }
                    }
                    if let Some(end) = header_end {
                        if collected.len() >= end + content_length {
                            break;
                        }
                    }
                }
                let is_print = String::from_utf8_lossy(&collected[..collected.len().min(16)])
                    .starts_with("POST");
                if is_print {
                    counter.fetch_add(1, Ordering::SeqCst);
                }
                // Close without a response: the client sees a dropped connection.
            });
        }
    });

    let origin = format!("http://{addr}");
    let mut mcp = McpProcess::spawn(&[("LABELER_URL", origin.as_str())]).await;
    mcp.initialize().await;
    let result = mcp
        .tool_result(
            "print_labels",
            json!({
                "template": "t",
                "printer": "p",
                "labels": [{"data": {}}]
            }),
        )
        .await;
    assert!(is_error(&result), "{result}");
    assert!(
        first_text(&result).contains("unknown"),
        "the outcome is reported unknown: {result}"
    );
    // Give any (wrong) retry a chance to arrive before counting.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert_eq!(
        print_posts.load(Ordering::SeqCst),
        1,
        "exactly one print POST was sent"
    );
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn print_redirect_is_not_followed() {
    // A 308 pointing elsewhere must not replay the print POST at its Location:
    // the redirect itself is reported as a failure and no second POST is sent.
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/print"))
        .respond_with(
            wiremock::ResponseTemplate::new(308).insert_header("location", "/api/elsewhere"),
        )
        .expect(1)
        .mount(&mock)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/elsewhere"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "total": 1, "sent": 1, "failed": [], "jobs": 1
        })))
        .expect(0)
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;
    let result = mcp
        .tool_result(
            "print_labels",
            json!({ "template": "t", "printer": "p", "labels": [{"data": {}}] }),
        )
        .await;
    assert!(
        is_error(&result),
        "a redirect is not a print summary: {result}"
    );
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

// ---------------------------------------------------------------------------
// 2.4 service and connection failures map to execution errors
// ---------------------------------------------------------------------------

async fn assert_service_error_mapping(status: u16, envelope: Value, expect_code: &str) {
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/print"))
        .respond_with(wiremock::ResponseTemplate::new(status).set_body_json(&envelope))
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;
    let result = mcp
        .tool_result(
            "print_labels",
            json!({ "template": "t", "printer": "p", "labels": [{"data": {}}] }),
        )
        .await;
    assert!(is_error(&result), "HTTP {status}: {result}");
    let carried = error_object_text(&result);
    assert_eq!(carried["status"], status as u64, "{carried}");
    assert_eq!(carried["error"]["code"], expect_code, "{carried}");
    assert_eq!(
        carried["error"]["code"], envelope["error"]["code"],
        "the service error object is unchanged"
    );
    assert_eq!(
        carried["error"]["message"], envelope["error"]["message"],
        "the service message is unchanged"
    );
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn unauthorized_forbidden_too_large_and_batch_invalid_map_to_execution_errors() {
    assert_service_error_mapping(
        401,
        json!({"error": {"code": "Unauthorized", "message": "invalid or missing token"}}),
        "Unauthorized",
    )
    .await;
    assert_service_error_mapping(
        404,
        json!({"error": {
            "code": "NotFound",
            "message": "no printer 'p' was found",
            "details": {"kind": "printer", "id": "p"}
        }}),
        "NotFound",
    )
    .await;
    assert_service_error_mapping(
        413,
        json!({"error": {"code": "PayloadTooLarge", "message": "batch has 9999 labels; the maximum is 500"}}),
        "PayloadTooLarge",
    )
    .await;
    assert_service_error_mapping(
        422,
        json!({"error": {
            "code": "BatchInvalid",
            "message": "one or more labels in the batch are invalid",
            "details": {"reason": "batch_invalid", "failures": [
                {"index": 1, "code": "ParamValueInvalid", "message": "bad value"}
            ]}
        }}),
        "BatchInvalid",
    )
    .await;
}

#[tokio::test]
async fn batch_invalid_details_keep_every_indexed_failure() {
    let envelope = json!({"error": {
        "code": "BatchInvalid",
        "message": "one or more labels in the batch are invalid",
        "details": {"failures": [
            {"index": 0, "code": "ParamValueInvalid", "message": "first"},
            {"index": 2, "code": "ParamValueInvalid", "message": "third"}
        ]}
    }});
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/print"))
        .respond_with(wiremock::ResponseTemplate::new(422).set_body_json(&envelope))
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;
    let result = mcp
        .tool_result(
            "print_labels",
            json!({ "template": "t", "printer": "p", "labels": [{"data": {}}] }),
        )
        .await;
    assert!(is_error(&result));
    let carried = error_object_text(&result);
    let failures = carried["error"]["details"]["failures"]
        .as_array()
        .expect("failures array");
    assert_eq!(failures.len(), 2);
    assert_eq!(failures[0]["index"], 0);
    assert_eq!(failures[1]["index"], 2);
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn undecodable_success_body_is_an_execution_error() {
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/api/templates"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("this is not json"))
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;
    let result = mcp.tool_result("list_templates", json!({})).await;
    assert!(is_error(&result), "{result}");
    assert!(
        first_text(&result).contains("not JSON"),
        "the message explains the decode failure: {result}"
    );
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn unavailable_service_is_an_execution_error_and_later_calls_still_run() {
    // Bind and immediately release a port so nothing listens there.
    let dead = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap()
    };
    let origin = format!("http://{dead}");
    let mut mcp = McpProcess::spawn(&[("LABELER_URL", origin.as_str())]).await;
    mcp.initialize().await;

    let first = mcp.tool_result("list_templates", json!({})).await;
    assert!(is_error(&first), "{first}");
    assert!(
        first_text(&first).contains("unreachable"),
        "the message names the connection failure: {first}"
    );

    // The failure must not terminate the adapter: a later tool call still runs.
    let second = mcp.tool_result("list_printers", json!({})).await;
    assert!(
        is_error(&second),
        "still an execution error, not a crash: {second}"
    );
    let tools = mcp.list_tools().await;
    assert_eq!(tools["tools"].as_array().unwrap().len(), 4);

    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn failures_do_not_block_a_following_success_on_the_same_process() {
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/api/templates"))
        .respond_with(wiremock::ResponseTemplate::new(401).set_body_json(json!({
            "error": {"code": "Unauthorized", "message": "invalid or missing token"}
        })))
        .mount(&mock)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/api/printers"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;
    let failed = mcp.tool_result("list_templates", json!({})).await;
    assert!(is_error(&failed));
    assert_eq!(error_object_text(&failed)["status"], 401);
    let ok = mcp.tool_result("list_printers", json!({})).await;
    assert!(!is_error(&ok), "a later call succeeds: {ok}");
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}
