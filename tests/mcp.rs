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

use common::{error_object_text, first_text, is_error, template_management_tool_names, McpProcess};
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

/// The raw path segment the adapter must send for a template id: the id
/// unchanged, since every permitted character is literal in a URL path segment.
fn template_path(id: &str, suffix: &str) -> String {
    format!("/api/templates/{id}{suffix}")
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
    assert_eq!(tool_names(&tools), template_management_tool_names());

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
async fn discovery_publishes_twelve_tools_with_schemas_and_annotations() {
    let mut mcp = McpProcess::spawn(&[]).await;
    mcp.initialize().await;
    let tools = mcp.list_tools().await;
    assert_eq!(tool_names(&tools), template_management_tool_names());
    assert_eq!(tools["tools"].as_array().unwrap().len(), 12);

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
    assert!(
        schema["properties"].get("chain_and_continue").is_some(),
        "chain_and_continue is published: {schema}"
    );
    assert!(
        !schema["required"]
            .as_array()
            .is_some_and(|r| r.iter().any(|v| v == "chain_and_continue")),
        "chain_and_continue is optional: {schema}"
    );
    let chain = &schema["properties"]["chain_and_continue"];
    assert_eq!(
        chain["type"], "boolean",
        "chain_and_continue is a nonnullable boolean: {chain}"
    );
    assert!(
        chain.get("default").is_none(),
        "chain_and_continue has no null default: {chain}"
    );
    let description = print["description"].as_str().expect("description");
    assert!(
        description.contains("physical"),
        "print_labels states the physical effect: {description}"
    );
    assert!(
        description.contains("unknown"),
        "print_labels states lost-response uncertainty: {description}"
    );
    assert!(
        description.contains("chain_and_continue") && description.contains("ordinary"),
        "print_labels explains both finishing selections: {description}"
    );

    for name in [
        "list_templates",
        "get_template",
        "get_template_source",
        "get_template_thumbnail",
        "render_label",
        "list_printers",
    ] {
        assert_eq!(
            find_tool(&tools, name)["annotations"]["readOnlyHint"],
            true,
            "{name} is read-only"
        );
    }
    for name in [
        "create_template",
        "replace_template",
        "delete_template",
        "reload_templates",
        "print_labels",
        "cut_printer",
    ] {
        let annotations = &find_tool(&tools, name)["annotations"];
        assert_eq!(annotations["readOnlyHint"], false, "{name} is a mutation");
        assert_eq!(annotations["destructiveHint"], true, "{name}");
        assert_eq!(annotations["idempotentHint"], false, "{name}");
        let description = find_tool(&tools, name)["description"]
            .as_str()
            .expect("description");
        assert!(
            description.contains("unknown"),
            "{name} explains lost-response uncertainty: {description}"
        );
    }

    // Every path-based template tool requires the identifier and publishes the
    // service's rule for it, so a client can refuse an impossible id itself.
    for name in [
        "get_template",
        "get_template_source",
        "get_template_thumbnail",
        "create_template",
        "replace_template",
        "delete_template",
    ] {
        let schema = &find_tool(&tools, name)["inputSchema"];
        assert!(
            schema["required"]
                .as_array()
                .is_some_and(|r| r.iter().any(|v| v == "template")),
            "template is required for {name}: {schema}"
        );
        let id_description = schema["properties"]["template"]["description"]
            .as_str()
            .expect("template description");
        assert!(
            id_description.contains("a-zA-Z0-9_-"),
            "{name} publishes the id rule: {id_description}"
        );
    }
    for name in ["create_template", "replace_template"] {
        let schema = &find_tool(&tools, name)["inputSchema"];
        assert!(
            schema["required"]
                .as_array()
                .is_some_and(|r| r.iter().any(|v| v == "yaml")),
            "yaml is required for {name}: {schema}"
        );
        assert_eq!(
            schema["properties"]["yaml"]["type"], "string",
            "yaml is a string for {name}: {schema}"
        );
    }

    // The write descriptions tell the three operations apart and name the
    // registry replacement reload performs.
    let create = find_tool(&tools, "create_template")["description"]
        .as_str()
        .unwrap()
        .to_lowercase();
    assert!(
        create.contains("create") && create.contains("never overwritten"),
        "create_template states creation and the no-overwrite rule: {create}"
    );
    let replace = find_tool(&tools, "replace_template")["description"]
        .as_str()
        .unwrap()
        .to_lowercase();
    assert!(
        replace.contains("replace") && replace.contains("never created"),
        "replace_template states complete replacement and the no-create rule: {replace}"
    );
    let delete = find_tool(&tools, "delete_template")["description"]
        .as_str()
        .unwrap()
        .to_lowercase();
    assert!(delete.contains("delete"), "delete_template: {delete}");
    let reload = find_tool(&tools, "reload_templates")["description"]
        .as_str()
        .unwrap()
        .to_lowercase();
    assert!(
        reload.contains("disk") && reload.contains("registry"),
        "reload_templates states the registry replacement: {reload}"
    );
    let cut = find_tool(&tools, "cut_printer")["description"]
        .as_str()
        .unwrap()
        .to_lowercase();
    assert!(
        cut.contains("release") && cut.contains("chain and continue"),
        "cut_printer states the release of the held label: {cut}"
    );

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
async fn template_tools_refuse_bad_arguments_without_calling_rest() {
    // No mocks: any HTTP call would 404, so an isError result with an empty
    // request log proves the adapter refused before calling REST.
    let mock = wiremock::MockServer::start().await;
    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;

    let cases = [
        ("get_template", json!({})),
        ("get_template", json!({"template": Value::Null})),
        ("get_template", json!({"template": 7})),
        ("get_template", json!({"template": "t", "extra": 1})),
        ("get_template_source", json!({})),
        ("get_template_thumbnail", json!({"template": Value::Null})),
        ("create_template", json!({"template": "t"})),
        ("create_template", json!({"template": "t", "yaml": 5})),
        (
            "create_template",
            json!({"template": "t", "yaml": "name: T", "extra": true}),
        ),
        ("replace_template", json!({"yaml": "name: T"})),
        ("delete_template", json!({})),
        ("delete_template", json!({"template": "t", "extra": 1})),
        ("reload_templates", json!({"template": "t"})),
    ];
    for (tool, arguments) in &cases {
        let result = mcp.tool_result(tool, arguments.clone()).await;
        assert!(is_error(&result), "{tool} refuses {arguments}: {result}");
    }

    // The refusals name the offending argument where the schema can.
    let missing_template = mcp.tool_result("get_template", json!({})).await;
    assert!(
        first_text(&missing_template).contains("template"),
        "the message names the missing argument: {missing_template}"
    );
    let missing_yaml = mcp
        .tool_result("create_template", json!({"template": "t"}))
        .await;
    assert!(
        first_text(&missing_yaml).contains("yaml"),
        "the message names the missing argument: {missing_yaml}"
    );

    assert!(
        mock.received_requests().await.unwrap().is_empty(),
        "argument refusals make no REST call"
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

    for invalid in [json!(Value::Null), json!("true"), json!(1)] {
        let invalid_chain = mcp
            .tool_result(
                "print_labels",
                json!({
                    "template": "t",
                    "printer": "p",
                    "labels": [{"data": {}}],
                    "chain_and_continue": invalid
                }),
            )
            .await;
        assert!(
            is_error(&invalid_chain),
            "non-boolean chain_and_continue {invalid}: {invalid_chain}"
        );
    }

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
    assert!(
        !body
            .as_object()
            .is_some_and(|o| o.contains_key("chain_and_continue")),
        "omitted chain_and_continue stays absent: {body}"
    );
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn chain_and_continue_is_forwarded_exactly_as_supplied() {
    let summary = json!({"total": 1, "sent": 1, "failed": [], "jobs": 1});
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/print"))
        .and(wiremock::matchers::body_partial_json(json!({
            "chain_and_continue": true
        })))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(&summary))
        .expect(1)
        .mount(&mock)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/print"))
        .and(wiremock::matchers::body_partial_json(json!({
            "chain_and_continue": false
        })))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(&summary))
        .expect(1)
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;

    let chain = mcp
        .tool_result(
            "print_labels",
            json!({
                "template": "t",
                "printer": "p",
                "labels": [{"data": {}}],
                "chain_and_continue": true
            }),
        )
        .await;
    assert!(!is_error(&chain), "{chain}");

    let ordinary = mcp
        .tool_result(
            "print_labels",
            json!({
                "template": "t",
                "printer": "p",
                "labels": [{"data": {}}],
                "chain_and_continue": false
            }),
        )
        .await;
    assert!(!is_error(&ordinary), "{ordinary}");

    let requests = mock.received_requests().await.unwrap();
    assert_eq!(
        requests.len(),
        2,
        "one chain and one ordinary print request"
    );
    let first: Value = serde_json::from_slice(&requests[0].body).expect("JSON body");
    assert_eq!(
        first["chain_and_continue"], true,
        "first request carries true: {first}"
    );
    let second: Value = serde_json::from_slice(&requests[1].body).expect("JSON body");
    assert_eq!(
        second["chain_and_continue"], false,
        "second request carries false: {second}"
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
async fn cut_printer_refuses_an_invalid_printer_id_without_calling_rest() {
    // No mocks: any HTTP call would 404, so an isError result with an empty
    // request log proves the adapter refused before calling REST.
    let mock = wiremock::MockServer::start().await;
    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;

    let result = mcp
        .tool_result("cut_printer", json!({"printer": "../other"}))
        .await;
    assert!(
        is_error(&result),
        "an impossible printer id is refused: {result}"
    );
    assert!(
        first_text(&result).contains("printer id"),
        "the message names the refused argument: {result}"
    );
    assert!(
        mock.received_requests().await.unwrap().is_empty(),
        "the refusal makes no REST call"
    );
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn a_failed_cut_returns_the_json_with_is_error_and_an_ok_cut_does_not() {
    let failed_body = json!({
        "status": "failed",
        "detail": "printer returned IPP status ServerErrorOperationNotSupported"
    });
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/printers/p1/cut"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(&failed_body))
        .expect(1)
        .mount(&mock)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/printers/p2/cut"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({"status": "ok"})))
        .expect(1)
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;

    let refused = mcp
        .tool_result("cut_printer", json!({"printer": "p1"}))
        .await;
    assert!(is_error(&refused), "a failed cut sets isError: {refused}");
    let returned: Value = serde_json::from_str(&first_text(&refused)).expect("cut JSON");
    assert_eq!(returned, failed_body, "the JSON is unchanged");

    let ok = mcp
        .tool_result("cut_printer", json!({"printer": "p2"}))
        .await;
    assert!(!is_error(&ok), "an ok cut is not an error: {ok}");
    let returned: Value = serde_json::from_str(&first_text(&ok)).expect("cut JSON");
    assert_eq!(returned, json!({"status": "ok"}));

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
    assert_eq!(tools["tools"].as_array().unwrap().len(), 12);

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

// ---------------------------------------------------------------------------
// mcp-template-management 1.2 identifier routing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ordinary_and_punctuation_ids_reach_the_captured_template_route() {
    let detail = json!({"id": "plain", "name": "Plain"});
    let source = "# keep me\nname: Bro\n";
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(template_path("plain", "")))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(&detail))
        .expect(1)
        .mount(&mock)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(template_path(
            "brother_24mm-qr",
            "/source",
        )))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_string(source)
                .insert_header("content-type", "text/yaml"),
        )
        .expect(1)
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;

    let detail_result = mcp
        .tool_result("get_template", json!({"template": "plain"}))
        .await;
    assert!(!is_error(&detail_result), "{detail_result}");
    let returned: Value = serde_json::from_str(&first_text(&detail_result)).expect("detail JSON");
    assert_eq!(returned, detail);

    let source_result = mcp
        .tool_result(
            "get_template_source",
            json!({"template": "brother_24mm-qr"}),
        )
        .await;
    assert!(!is_error(&source_result), "{source_result}");
    assert_eq!(first_text(&source_result), source);

    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn the_valid_id_reload_addresses_the_template_not_the_registry_route() {
    let detail = json!({"id": "reload", "name": "Reload, the template"});
    let source = "name: Reload, the template\n";
    let png = png_bytes();
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(template_path("reload", "")))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(&detail))
        .expect(1)
        .mount(&mock)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(template_path("reload", "/source")))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_string(source)
                .insert_header("content-type", "text/yaml"),
        )
        .expect(1)
        .mount(&mock)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(template_path(
            "reload",
            "/thumbnail",
        )))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_bytes(png.clone())
                .insert_header("content-type", "image/png"),
        )
        .expect(1)
        .mount(&mock)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(template_path("reload", "")))
        .respond_with(wiremock::ResponseTemplate::new(201).set_body_json(&detail))
        .expect(1)
        .mount(&mock)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("PUT"))
        .and(wiremock::matchers::path(template_path("reload", "")))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(&detail))
        .expect(1)
        .mount(&mock)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path(template_path("reload", "")))
        .respond_with(wiremock::ResponseTemplate::new(204))
        .expect(1)
        .mount(&mock)
        .await;
    // The registry-reload route must never answer for the id `reload`.
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/template-registry/reload"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(json!({"count": 1, "broken_count": 0})),
        )
        .expect(0)
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;

    let created = mcp
        .tool_result(
            "create_template",
            json!({"template": "reload", "yaml": "name: Reload, the template\n"}),
        )
        .await;
    assert!(!is_error(&created), "{created}");
    let created_body: Value = serde_json::from_str(&first_text(&created)).expect("detail JSON");
    assert_eq!(
        created_body["id"], "reload",
        "create answers with the template detail, not reload counts: {created}"
    );

    let detail_result = mcp
        .tool_result("get_template", json!({"template": "reload"}))
        .await;
    assert!(!is_error(&detail_result), "{detail_result}");
    assert_eq!(
        serde_json::from_str::<Value>(&first_text(&detail_result)).expect("detail JSON"),
        detail
    );

    let source_result = mcp
        .tool_result("get_template_source", json!({"template": "reload"}))
        .await;
    assert!(!is_error(&source_result), "{source_result}");
    assert_eq!(first_text(&source_result), source);

    let thumbnail = mcp
        .tool_result("get_template_thumbnail", json!({"template": "reload"}))
        .await;
    assert!(!is_error(&thumbnail), "{thumbnail}");
    assert_eq!(thumbnail["content"][0]["type"], "image", "{thumbnail}");

    let replaced = mcp
        .tool_result(
            "replace_template",
            json!({"template": "reload", "yaml": "name: Reload, the template\n"}),
        )
        .await;
    assert!(!is_error(&replaced), "{replaced}");

    let deleted = mcp
        .tool_result("delete_template", json!({"template": "reload"}))
        .await;
    assert!(!is_error(&deleted), "{deleted}");

    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn invalid_template_ids_and_traversal_are_refused_before_http() {
    let mock = wiremock::MockServer::start().await;
    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;

    for id in [
        "",
        "../other",
        "has space",
        "has.dot",
        "reload/../x",
        "..%2fevil",
    ] {
        let result = mcp
            .tool_result("get_template_source", json!({"template": id}))
            .await;
        assert!(is_error(&result), "id {id:?} is refused: {result}");
        assert!(
            first_text(&result).contains("template id"),
            "the message names the problem: {result}"
        );
    }
    let create = mcp
        .tool_result(
            "create_template",
            json!({"template": "../other", "yaml": "name: X\n"}),
        )
        .await;
    assert!(is_error(&create), "traversal create is refused: {create}");
    let delete = mcp
        .tool_result("delete_template", json!({"template": "bad.dot"}))
        .await;
    assert!(is_error(&delete), "invalid delete is refused: {delete}");

    assert!(
        mock.received_requests().await.unwrap().is_empty(),
        "identifier refusals make no REST call"
    );
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

// ---------------------------------------------------------------------------
// mcp-template-management 2 read operations
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_template_returns_the_complete_detail_json() {
    // The response carries a field beyond the ones the adapter knows about: it
    // must survive the round trip rather than be dropped on reserialization.
    let detail = json!({
        "id": "label1",
        "name": "Label One",
        "categories": ["brother"],
        "unit": "mm",
        "dpi": 200,
        "format": {"type": "single", "width": 50.0, "height": 20.0},
        "params": [{"name": "message", "label": "Message", "type": "string"}],
        "variables": [{"key": "shop.name", "value": "Acme"}],
        "recently_printed": true
    });
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(template_path("label1", "")))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(&detail))
        .expect(1)
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;
    let result = mcp
        .tool_result("get_template", json!({"template": "label1"}))
        .await;
    assert!(!is_error(&result), "{result}");
    let returned: Value = serde_json::from_str(&first_text(&result)).expect("detail JSON");
    assert_eq!(returned, detail, "the detail passes through unchanged");
    assert!(returned["params"].as_array().is_some_and(|p| !p.is_empty()));
    assert!(returned["variables"]
        .as_array()
        .is_some_and(|v| !v.is_empty()));
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn get_template_preserves_a_rest_not_found_error() {
    let envelope = json!({
        "error": {
            "code": "NotFound",
            "message": "no template 'nope'",
            "details": {"kind": "template", "id": "nope"}
        }
    });
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(template_path("nope", "")))
        .respond_with(wiremock::ResponseTemplate::new(404).set_body_json(&envelope))
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;
    let result = mcp
        .tool_result("get_template", json!({"template": "nope"}))
        .await;
    assert!(is_error(&result), "{result}");
    let carried = error_object_text(&result);
    assert_eq!(carried["status"], 404);
    assert_eq!(carried["error"], envelope["error"]);
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn get_template_source_returns_the_stored_text_exactly() {
    let source = "# leading comment\nname: Commented\n   \n# trailing comment\n";
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(template_path(
            "commented",
            "/source",
        )))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_string(source)
                .insert_header("content-type", "text/yaml; charset=utf-8"),
        )
        .expect(1)
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;
    let result = mcp
        .tool_result("get_template_source", json!({"template": "commented"}))
        .await;
    assert!(!is_error(&result), "{result}");
    let blocks = result["content"].as_array().expect("content array");
    assert_eq!(blocks.len(), 1, "one text block: {result}");
    assert_eq!(blocks[0]["type"], "text", "{result}");
    assert_eq!(
        blocks[0]["text"].as_str().expect("text content"),
        source,
        "comments and trailing whitespace survive unchanged"
    );
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn the_source_of_a_quarantined_template_succeeds_while_its_detail_fails() {
    let source = "name: Broken\nformat: {type: ";
    let detail_envelope = json!({
        "error": {"code": "NotFound", "message": "no template 'broken'"}
    });
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(template_path("broken", "/source")))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_string(source)
                .insert_header("content-type", "text/yaml; charset=utf-8"),
        )
        .mount(&mock)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(template_path("broken", "")))
        .respond_with(wiremock::ResponseTemplate::new(404).set_body_json(&detail_envelope))
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;
    let detail_result = mcp
        .tool_result("get_template", json!({"template": "broken"}))
        .await;
    assert!(is_error(&detail_result), "{detail_result}");
    assert_eq!(error_object_text(&detail_result)["status"], 404);

    let source_result = mcp
        .tool_result("get_template_source", json!({"template": "broken"}))
        .await;
    assert!(!is_error(&source_result), "{source_result}");
    assert_eq!(first_text(&source_result), source);
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn source_not_found_and_non_utf8_bodies_are_execution_errors() {
    let envelope = json!({
        "error": {
            "code": "NotFound",
            "message": "no template 'missing'",
            "details": {"kind": "template", "id": "missing"}
        }
    });
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(template_path(
            "missing", "/source",
        )))
        .respond_with(wiremock::ResponseTemplate::new(404).set_body_json(&envelope))
        .mount(&mock)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(template_path("binary", "/source")))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_bytes(vec![0xff, 0xfe, 0xfd, 0x80]),
        )
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;

    let missing = mcp
        .tool_result("get_template_source", json!({"template": "missing"}))
        .await;
    assert!(is_error(&missing), "{missing}");
    let carried = error_object_text(&missing);
    assert_eq!(carried["status"], 404);
    assert_eq!(carried["error"], envelope["error"]);

    let binary = mcp
        .tool_result("get_template_source", json!({"template": "binary"}))
        .await;
    assert!(is_error(&binary), "{binary}");
    assert!(
        first_text(&binary).contains("UTF-8"),
        "the message names the decode failure: {binary}"
    );
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn get_template_thumbnail_returns_the_service_png_without_printing() {
    let png = png_bytes();
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(template_path(
            "sheet1",
            "/thumbnail",
        )))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_bytes(png.clone())
                .insert_header("content-type", "image/png"),
        )
        .expect(1)
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;
    let result = mcp
        .tool_result("get_template_thumbnail", json!({"template": "sheet1"}))
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

    // One unconditional GET and nothing else: no cache probe, no print request.
    let requests = mock.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1, "one thumbnail request and no print");
    assert_eq!(requests[0].method.as_str(), "GET");
    assert_eq!(
        requests[0].url.path(),
        template_path("sheet1", "/thumbnail")
    );
    assert!(
        !requests[0]
            .headers
            .iter()
            .any(|(name, _)| name.as_str().eq_ignore_ascii_case("if-none-match")),
        "the thumbnail request is unconditional"
    );
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn thumbnail_non_png_bodies_and_service_errors_are_execution_errors() {
    let envelope = json!({
        "error": {
            "code": "NotFound",
            "message": "no template 'nope'",
            "details": {"kind": "template", "id": "nope"}
        }
    });
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(template_path(
            "html",
            "/thumbnail",
        )))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_string("<html>not a png</html>")
                .insert_header("content-type", "text/html"),
        )
        .mount(&mock)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(template_path(
            "nope",
            "/thumbnail",
        )))
        .respond_with(wiremock::ResponseTemplate::new(404).set_body_json(&envelope))
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;

    let html = mcp
        .tool_result("get_template_thumbnail", json!({"template": "html"}))
        .await;
    assert!(is_error(&html), "{html}");
    assert!(
        first_text(&html).to_lowercase().contains("png"),
        "the message names the problem: {html}"
    );

    let missing = mcp
        .tool_result("get_template_thumbnail", json!({"template": "nope"}))
        .await;
    assert!(is_error(&missing), "{missing}");
    let carried = error_object_text(&missing);
    assert_eq!(carried["status"], 404);
    assert_eq!(carried["error"], envelope["error"]);
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

// ---------------------------------------------------------------------------
// mcp-template-management 3 mutation operations
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_and_replace_send_the_exact_yaml_and_return_the_complete_detail() {
    let created = json!({
        "id": "new_one",
        "name": "Created",
        "unit": "mm",
        "dpi": 200,
        "format": {"type": "single", "width": 50.0, "height": 20.0},
        "params": [],
        "variables": [],
        "printers": ["desk"]
    });
    let replaced = json!({
        "id": "new_one",
        "name": "Edited",
        "unit": "mm",
        "dpi": 200,
        "format": {"type": "single", "width": 50.0, "height": 20.0},
        "params": [],
        "variables": [],
        "printers": ["desk"]
    });
    let yaml = "# authored with a comment\nname: Created\nunit: mm\n\n  \n";
    let edited = "# edited, still commented\nname: Edited\n";

    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(template_path("new_one", "")))
        .and(wiremock::matchers::body_string(yaml))
        .and(wiremock::matchers::header("content-type", "text/yaml"))
        .respond_with(wiremock::ResponseTemplate::new(201).set_body_json(&created))
        .expect(1)
        .mount(&mock)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("PUT"))
        .and(wiremock::matchers::path(template_path("new_one", "")))
        .and(wiremock::matchers::body_string(edited))
        .and(wiremock::matchers::header("content-type", "text/yaml"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(&replaced))
        .expect(1)
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;

    let create_result = mcp
        .tool_result(
            "create_template",
            json!({"template": "new_one", "yaml": yaml}),
        )
        .await;
    assert!(!is_error(&create_result), "{create_result}");
    let create_body: Value =
        serde_json::from_str(&first_text(&create_result)).expect("detail JSON");
    assert_eq!(create_body, created, "the 201 detail is unchanged");

    let replace_result = mcp
        .tool_result(
            "replace_template",
            json!({"template": "new_one", "yaml": edited}),
        )
        .await;
    assert!(!is_error(&replace_result), "{replace_result}");
    let replace_body: Value =
        serde_json::from_str(&first_text(&replace_result)).expect("detail JSON");
    assert_eq!(replace_body, replaced, "the 200 detail is unchanged");

    // The body reached the service as the exact bytes supplied: no YAML was
    // parsed and rewritten on the way.
    let requests = mock.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2, "one create and one replace");
    assert_eq!(requests[0].body.as_slice(), yaml.as_bytes());
    assert_eq!(requests[1].body.as_slice(), edited.as_bytes());
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

async fn refused_write(
    tool: &str,
    arguments: Value,
    status: u16,
    envelope: &Value,
    expected_requests: usize,
) {
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::path(template_path("t", "")))
        .respond_with(wiremock::ResponseTemplate::new(status).set_body_json(envelope))
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;
    let result = mcp.tool_result(tool, arguments).await;
    assert!(is_error(&result), "{tool} HTTP {status}: {result}");
    let carried = error_object_text(&result);
    assert_eq!(carried["status"], status as u64, "{carried}");
    assert_eq!(
        carried["error"], envelope["error"],
        "the error object is unchanged"
    );
    assert_eq!(
        mock.received_requests().await.unwrap().len(),
        expected_requests,
        "{tool} sends no fallback write"
    );
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn template_write_refusals_are_preserved_without_fallback() {
    refused_write(
        "create_template",
        json!({"template": "t", "yaml": "name: T\n"}),
        409,
        &json!({"error": {"code": "Conflict", "message": "template 't' already exists"}}),
        1,
    )
    .await;
    refused_write(
        "replace_template",
        json!({"template": "t", "yaml": "name: T\n"}),
        404,
        &json!({"error": {
            "code": "NotFound",
            "message": "no template 't'",
            "details": {"kind": "template", "id": "t"}
        }}),
        1,
    )
    .await;
    refused_write(
        "create_template",
        json!({"template": "t", "yaml": "not: a template\n"}),
        422,
        &json!({"error": {
            "code": "TemplateInvalid",
            "message": "invalid template",
            "details": {"reason": "template_validation_failed", "path": "format"}
        }}),
        1,
    )
    .await;
    refused_write(
        "replace_template",
        json!({"template": "t", "yaml": "name: T\n"}),
        500,
        &json!({"error": {"code": "Internal", "message": "the write failed"}}),
        1,
    )
    .await;
}

#[tokio::test]
async fn delete_returns_empty_content_and_later_calls_still_run() {
    let envelope = json!({
        "error": {
            "code": "NotFound",
            "message": "no template 'missing'",
            "details": {"kind": "template", "id": "missing"}
        }
    });
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path(template_path("gone", "")))
        .respond_with(wiremock::ResponseTemplate::new(204))
        .expect(1)
        .mount(&mock)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path(template_path("missing", "")))
        .respond_with(wiremock::ResponseTemplate::new(404).set_body_json(&envelope))
        .expect(1)
        .mount(&mock)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/api/templates"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "templates": [], "broken": []
        })))
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;

    let deleted = mcp
        .tool_result("delete_template", json!({"template": "gone"}))
        .await;
    assert!(!is_error(&deleted), "{deleted}");
    assert!(
        deleted["content"]
            .as_array()
            .is_some_and(|blocks| blocks.is_empty()),
        "204 becomes an empty content array: {deleted}"
    );

    let missing = mcp
        .tool_result("delete_template", json!({"template": "missing"}))
        .await;
    assert!(is_error(&missing), "{missing}");
    let carried = error_object_text(&missing);
    assert_eq!(carried["status"], 404);
    assert_eq!(carried["error"], envelope["error"]);

    let later = mcp.tool_result("list_templates", json!({})).await;
    assert!(!is_error(&later), "a later call still works: {later}");
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn reload_returns_the_complete_json_even_with_quarantined_files() {
    let body = json!({"count": 2, "broken_count": 1, "reloaded_at": "2026-10-12T00:00:00Z"});
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/template-registry/reload"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(&body))
        .expect(1)
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;
    let result = mcp.tool_result("reload_templates", json!({})).await;
    assert!(
        !is_error(&result),
        "a nonzero broken_count is not an error: {result}"
    );
    let returned: Value = serde_json::from_str(&first_text(&result)).expect("reload JSON");
    assert_eq!(
        returned, body,
        "counts and extra fields pass through unchanged"
    );
    assert_eq!(returned["broken_count"], 1);
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn a_failed_reload_preserves_the_service_error() {
    let envelope = json!({
        "error": {"code": "Internal", "message": "failed to read the templates directory"}
    });
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/template-registry/reload"))
        .respond_with(wiremock::ResponseTemplate::new(500).set_body_json(&envelope))
        .expect(1)
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;
    let result = mcp.tool_result("reload_templates", json!({})).await;
    assert!(is_error(&result), "{result}");
    let carried = error_object_text(&result);
    assert_eq!(carried["status"], 500);
    assert_eq!(carried["error"], envelope["error"]);
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

/// An HTTP peer that answers GET with a small JSON body and closes every other
/// request without responding, counting the ones it drops.
async fn spawn_dropping_peer() -> (String, Arc<AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let dropped = Arc::new(AtomicUsize::new(0));
    let counter = dropped.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let counter = counter.clone();
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
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
                let head =
                    String::from_utf8_lossy(&collected[..collected.len().min(24)]).to_string();
                if head.starts_with("GET") {
                    let body = br#"{"templates":[]}"#;
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.write_all(body).await;
                    let _ = socket.flush().await;
                } else {
                    // Close without a response: the mutation's outcome is lost.
                    counter.fetch_add(1, Ordering::SeqCst);
                }
            });
        }
    });
    (format!("http://{addr}"), dropped)
}

#[tokio::test]
async fn template_mutations_send_one_request_per_call_and_report_unknown_outcomes() {
    let yaml =
        "name: T\nunit: mm\ndpi: 200\nformat: {type: single, width: 50, height: 20}\nlayout: []\n";
    let (origin, dropped) = spawn_dropping_peer().await;
    let mut mcp = McpProcess::spawn(&[("LABELER_URL", origin.as_str())]).await;
    mcp.initialize().await;

    let mutations = [
        ("create_template", json!({"template": "t", "yaml": yaml})),
        ("replace_template", json!({"template": "t", "yaml": yaml})),
        ("delete_template", json!({"template": "t"})),
        ("reload_templates", json!({})),
    ];
    let mut seen = 0;
    for (tool, arguments) in mutations {
        let result = mcp.tool_result(tool, arguments).await;
        assert!(is_error(&result), "{tool}: {result}");
        assert!(
            first_text(&result).contains("unknown"),
            "{tool} reports an unknown outcome: {result}"
        );
        seen += 1;
        // Give any (wrong) replay a chance to arrive before counting.
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        assert_eq!(
            dropped.load(Ordering::SeqCst),
            seen,
            "{tool} emitted exactly one request"
        );
    }

    // The adapter is still usable after a lost mutation response.
    let ok = mcp.tool_result("list_templates", json!({})).await;
    assert!(!is_error(&ok), "a later read still succeeds: {ok}");
    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}

#[tokio::test]
async fn template_mutation_redirects_are_not_followed() {
    let mock = wiremock::MockServer::start().await;
    for (method, path) in [
        ("POST", template_path("t", "")),
        ("PUT", template_path("t", "")),
        ("DELETE", template_path("t", "")),
        ("POST", "/api/template-registry/reload".to_string()),
    ] {
        wiremock::Mock::given(wiremock::matchers::method(method))
            .and(wiremock::matchers::path(path))
            .respond_with(
                wiremock::ResponseTemplate::new(308).insert_header("location", "/api/elsewhere"),
            )
            .expect(1)
            .mount(&mock)
            .await;
    }
    // Whatever method carries the redirected request, it must never arrive.
    wiremock::Mock::given(wiremock::matchers::path("/api/elsewhere"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(0)
        .mount(&mock)
        .await;

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", mock.uri().as_str())]).await;
    mcp.initialize().await;

    let yaml = "name: T\n";
    for (tool, arguments) in [
        ("create_template", json!({"template": "t", "yaml": yaml})),
        ("replace_template", json!({"template": "t", "yaml": yaml})),
        ("delete_template", json!({"template": "t"})),
        ("reload_templates", json!({})),
    ] {
        let result = mcp.tool_result(tool, arguments).await;
        assert!(
            is_error(&result),
            "{tool} reports the redirect as a failure: {result}"
        );
    }

    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
}
