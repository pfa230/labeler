//! End-to-end exercise of the `labeler-mcp` adapter against a local real labeler
//! service and a controlled fake IPP receiver (task 3.1).
//!
//! Each assertion names the observable break it detects:
//! - filled preview output          → the adapter cannot turn a template plus
//!   parameter values into the service's PNG image content
//! - successful batch dispatch      → the adapter cannot carry a print batch
//!   through to a printer (no Print-Job reaches the receiver)
//! - zero dispatch for invalid batch → validation no longer happens before any
//!   physical send (a bad batch would reach the printer)
//! - job-log attribution            → the service records the wrong actor for
//!   prints made through a personal token, or through a no-auth service
//!   (expected actor `local`)

mod common;

use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use common::{error_object_text, first_text, is_error, template_management_tool_names, McpProcess};
use serde_json::{json, Value};

const TEMPLATE_YAML: &str = include_str!("fixtures/templates/brother_18mm_qr.yaml");

/// A unique temp config dir holding the template; removed on drop.
struct TempConfig {
    dir: PathBuf,
}

impl TempConfig {
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "labeler-mcp-it-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join("templates")).expect("create config dir");
        let config = Self { dir };
        config.write_template("brother_18mm_qr", TEMPLATE_YAML);
        config
    }

    fn template_path(&self, id: &str) -> PathBuf {
        self.dir.join("templates").join(format!("{id}.yaml"))
    }

    /// Write a template file straight to disk, as an authoring client or a
    /// stray editor would.
    fn write_template(&self, id: &str, body: &str) {
        std::fs::write(self.template_path(id), body).expect("write template");
    }
}

/// The width and height a PNG's IHDR chunk declares.
fn png_dimensions(png: &[u8]) -> (u32, u32) {
    assert!(
        png.starts_with(b"\x89PNG\r\n\x1a\n"),
        "PNG signature: {} bytes",
        png.len()
    );
    let word = |at: usize| u32::from_be_bytes([png[at], png[at + 1], png[at + 2], png[at + 3]]);
    (word(16), word(20))
}

impl Drop for TempConfig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A controlled fake IPP receiver: an HTTP peer that answers every IPP request
/// with a successful empty response and counts Print-Jobs.
struct FakeIpp {
    uri: String,
    print_jobs: Arc<AtomicUsize>,
}

fn spawn_fake_ipp() -> FakeIpp {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake IPP port");
    let port = listener.local_addr().unwrap().port();
    let print_jobs = Arc::new(AtomicUsize::new(0));
    let counter = print_jobs.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let counter = counter.clone();
            std::thread::spawn(move || {
                let _ = handle_ipp_http(stream, &counter);
            });
        }
    });
    FakeIpp {
        uri: format!("ipp://127.0.0.1:{port}/"),
        print_jobs,
    }
}

/// Read one HTTP request carrying an IPP body, count Print-Jobs, and answer with
/// a successful IPP response wrapped in HTTP. The IPP client sends the body with
/// `Transfer-Encoding: chunked`.
fn handle_ipp_http(mut stream: TcpStream, print_jobs: &AtomicUsize) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let mut read_buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut read_more = |buf: &mut Vec<u8>| -> std::io::Result<bool> {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Ok(false);
        }
        buf.extend_from_slice(&chunk[..n]);
        Ok(true)
    };

    // Headers first.
    let header_end = loop {
        if let Some(pos) = read_buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
        if !read_more(&mut read_buf)? {
            return Ok(());
        }
    };
    let head = String::from_utf8_lossy(&read_buf[..header_end - 4]).to_ascii_lowercase();
    let mut rest = read_buf[header_end..].to_vec();

    let body = if head.contains("transfer-encoding: chunked") {
        // Decode `<hex-size>\r\n<data>\r\n ... 0\r\n\r\n`.
        let mut decoded = Vec::new();
        loop {
            let Some(line_end) = rest.windows(2).position(|w| w == b"\r\n") else {
                if !read_more(&mut rest)? {
                    return Ok(());
                }
                continue;
            };
            let size = usize::from_str_radix(String::from_utf8_lossy(&rest[..line_end]).trim(), 16)
                .unwrap_or(0);
            let data_start = line_end + 2;
            if size == 0 {
                break;
            }
            while rest.len() < data_start + size + 2 {
                if !read_more(&mut rest)? {
                    return Ok(());
                }
            }
            decoded.extend_from_slice(&rest[data_start..data_start + size]);
            rest = rest[data_start + size + 2..].to_vec();
        }
        decoded
    } else {
        let content_length = head
            .lines()
            .find_map(|l| {
                let (key, value) = l.split_once(':')?;
                if key.trim() == "content-length" {
                    value.trim().parse::<usize>().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);
        while rest.len() < content_length {
            if !read_more(&mut rest)? {
                return Ok(());
            }
        }
        rest.truncate(content_length);
        rest
    };

    // IPP header: version (2), operation (2), request-id (4).
    if body.len() >= 8 {
        let opcode = u16::from_be_bytes([body[2], body[3]]);
        if opcode == 0x0002 {
            // Print-Job
            print_jobs.fetch_add(1, Ordering::SeqCst);
        }
        let request_id = &body[4..8];
        let mut ipp = Vec::with_capacity(9);
        ipp.extend_from_slice(&body[0..2]); // version
        ipp.extend_from_slice(&0u16.to_be_bytes()); // successful-ok
        ipp.extend_from_slice(request_id);
        ipp.push(0x03); // end-of-attributes
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/ipp\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            ipp.len()
        );
        stream.write_all(response.as_bytes())?;
        stream.write_all(&ipp)?;
        stream.flush()?;
    }
    Ok(())
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind a free port")
        .local_addr()
        .unwrap()
        .port()
}

/// Spawn the real labeler service binary and wait until it answers /api/health.
async fn spawn_service(
    config: &TempConfig,
    port: u16,
    extra: &[(&str, &str)],
) -> tokio::process::Child {
    let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_labeler"));
    cmd.env("LABELER_CONFIG_DIR", &config.dir)
        .env("PORT", port.to_string())
        .env("RUST_LOG", "error")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    for (key, value) in extra {
        cmd.env(key, value);
    }
    let child = cmd.spawn().expect("spawn labeler service");
    let health = format!("http://127.0.0.1:{port}/api/health");
    let client = reqwest::Client::new();
    for _ in 0..150 {
        if client
            .get(&health)
            .timeout(Duration::from_secs(1))
            .send()
            .await
            .is_ok_and(|r| r.status().is_success())
        {
            return child;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("labeler service did not become healthy on {health}");
}

fn cookie_from(response: &reqwest::Response) -> String {
    response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|s| {
            s.split(";")
                .next()?
                .strip_prefix("labeler_session=")
                .map(|v| format!("labeler_session={v}"))
        })
        .expect("setup sets a session cookie")
}

fn job_rows(db: &Path) -> Vec<(String, String, String)> {
    let conn =
        rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("open the job log");
    let mut stmt = conn
        .prepare("SELECT user_id, template, status FROM jobs ORDER BY rowid")
        .expect("jobs query");
    stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })
    .expect("map jobs")
    .collect::<Result<Vec<_>, _>>()
    .expect("read jobs")
}

/// Authenticated service path: setup a user, create a printer and a personal
/// token, then drive preview, print, invalid batch and attribution through MCP.
#[tokio::test(flavor = "multi_thread")]
async fn personal_token_preview_print_and_attribution() {
    let config = TempConfig::new("token");
    let ipp = spawn_fake_ipp();
    let port = free_port();
    let mut service = spawn_service(&config, port, &[]).await;
    let base = format!("http://127.0.0.1:{port}");
    let origin = base.clone();
    let client = reqwest::Client::new();

    // First-run setup (cookie-authenticated, so carry a matching Origin).
    let setup = client
        .post(format!("{base}/api/auth/setup"))
        .header("origin", &origin)
        .json(&json!({"username": "alice", "password": "correct-horse"}))
        .send()
        .await
        .expect("setup request");
    assert!(setup.status().is_success(), "setup: {}", setup.status());
    let cookie = cookie_from(&setup);

    // Configure the fake IPP printer.
    let printer = client
        .post(format!("{base}/api/printers"))
        .header("cookie", &cookie)
        .header("origin", &origin)
        .json(&json!({"id": "fake", "name": "Fake", "uri": ipp.uri}))
        .send()
        .await
        .expect("create printer");
    assert!(
        printer.status().is_success(),
        "printer: {}",
        printer.status()
    );

    // A personal token owned by the setup user.
    let token_response = client
        .post(format!("{base}/api/tokens"))
        .header("cookie", &cookie)
        .header("origin", &origin)
        .json(&json!({"name": "mcp"}))
        .send()
        .await
        .expect("create token");
    assert!(
        token_response.status().is_success(),
        "token: {}",
        token_response.status()
    );
    let token_body: Value = token_response.json().await.expect("token JSON");
    let secret = token_body["secret"]
        .as_str()
        .expect("token secret")
        .to_string();
    let user_id = {
        let me = client
            .get(format!("{base}/api/auth/me"))
            .header("cookie", &cookie)
            .send()
            .await
            .expect("auth/me");
        let me: Value = me.json().await.expect("auth/me JSON");
        me["me"]["id"].as_str().expect("user id").to_string()
    };

    let mut mcp = McpProcess::spawn(&[
        ("LABELER_URL", base.as_str()),
        ("LABELER_TOKEN", secret.as_str()),
    ])
    .await;
    mcp.initialize().await;

    // The eleven tools are what a client discovers against the real service.
    let tools = mcp.list_tools().await;
    let mut names: Vec<String> = tools["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .map(|t| t["name"].as_str().expect("tool name").to_string())
        .collect();
    names.sort();
    assert_eq!(names, template_management_tool_names());

    // Discovery: the template's published parameters reach the agent.
    let templates = mcp.tool_result("list_templates", json!({})).await;
    assert!(!is_error(&templates), "{templates}");
    let listed: Value = serde_json::from_str(&first_text(&templates)).expect("templates JSON");
    let summary = listed["templates"]
        .as_array()
        .expect("templates array")
        .iter()
        .find(|t| t["id"] == "brother_18mm_qr")
        .expect("the fixture template is listed");
    assert!(
        summary["params"].as_array().is_some_and(|p| !p.is_empty()),
        "published parameters are available: {summary}"
    );

    // Filled preview: the decoded image is a PNG (byte equality with the
    // service's own PNG is asserted in tests/mcp.rs).
    let preview = mcp
        .tool_result(
            "render_label",
            json!({
                "template": "brother_18mm_qr",
                "data": {"code": "https://example.com/1", "message": "Hello"}
            }),
        )
        .await;
    assert!(!is_error(&preview), "{preview}");
    let image = &preview["content"][0];
    assert_eq!(image["type"], "image", "{preview}");
    assert_eq!(image["mimeType"], "image/png", "{preview}");
    use base64::Engine as _;
    let png = base64::engine::general_purpose::STANDARD
        .decode(image["data"].as_str().expect("base64"))
        .expect("decode png");
    assert!(
        png.starts_with(b"\x89PNG\r\n\x1a\n"),
        "the preview is a real PNG ({} bytes)",
        png.len()
    );

    // Successful batch dispatch: one Print-Job reaches the fake receiver.
    let before = ipp.print_jobs.load(Ordering::SeqCst);
    let printed = mcp
        .tool_result(
            "print_labels",
            json!({
                "template": "brother_18mm_qr",
                "printer": "fake",
                "labels": [{"data": {"code": "A1", "message": "one"}}]
            }),
        )
        .await;
    assert!(!is_error(&printed), "{printed}");
    let summary: Value = serde_json::from_str(&first_text(&printed)).expect("print summary");
    assert_eq!(summary["sent"], 1, "{summary}");
    assert_eq!(
        ipp.print_jobs.load(Ordering::SeqCst),
        before + 1,
        "the batch reached the printer as one Print-Job"
    );

    // Job-log attribution through the personal token: the service records alice.
    let rows = job_rows(&config.dir.join("labeler.db"));
    assert!(
        rows.iter().any(|(actor, template, status)| {
            actor == &user_id && template == "brother_18mm_qr" && status == "ok"
        }),
        "the print job is attributed to the token's user {user_id}: {rows:?}"
    );

    // Invalid batch: refused before any physical send, no new job row.
    let jobs_before = ipp.print_jobs.load(Ordering::SeqCst);
    let rows_before = job_rows(&config.dir.join("labeler.db")).len();
    let invalid = mcp
        .tool_result(
            "print_labels",
            json!({
                "template": "brother_18mm_qr",
                "printer": "fake",
                "labels": [{"data": {"code": ["not-a-string"], "message": "x"}}]
            }),
        )
        .await;
    assert!(is_error(&invalid), "an invalid batch fails: {invalid}");
    let carried = error_object_text(&invalid);
    assert_eq!(carried["status"], 422, "{carried}");
    assert_eq!(carried["error"]["code"], "BatchInvalid", "{carried}");
    assert_eq!(
        ipp.print_jobs.load(Ordering::SeqCst),
        jobs_before,
        "an invalid batch sends nothing to the printer"
    );
    assert_eq!(
        job_rows(&config.dir.join("labeler.db")).len(),
        rows_before,
        "an invalid batch records no job"
    );

    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
    service.kill().await.expect("stop the service");
}

/// No-auth service path: prints through the adapter are attributed to `local`.
#[tokio::test(flavor = "multi_thread")]
async fn no_auth_prints_are_attributed_to_local() {
    let config = TempConfig::new("noauth");
    let ipp = spawn_fake_ipp();
    let port = free_port();
    let mut service = spawn_service(&config, port, &[("LABELER_NO_AUTH", "true")]).await;
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::new();

    // Printers are manageable in no-auth mode (credential routes are not).
    let printer = client
        .post(format!("{base}/api/printers"))
        .json(&json!({"id": "fake", "name": "Fake", "uri": ipp.uri}))
        .send()
        .await
        .expect("create printer");
    assert!(
        printer.status().is_success(),
        "printer: {}",
        printer.status()
    );

    // No LABELER_TOKEN: the adapter carries no credential.
    let mut mcp = McpProcess::spawn(&[("LABELER_URL", base.as_str())]).await;
    mcp.initialize().await;

    let printed = mcp
        .tool_result(
            "print_labels",
            json!({
                "template": "brother_18mm_qr",
                "printer": "fake",
                "labels": [{"data": {"code": "B2", "message": "local"}}]
            }),
        )
        .await;
    assert!(!is_error(&printed), "{printed}");
    assert_eq!(ipp.print_jobs.load(Ordering::SeqCst), 1, "one Print-Job");

    let rows = job_rows(&config.dir.join("labeler.db"));
    assert!(
        rows.iter().any(|(actor, template, status)| actor == "local"
            && template == "brother_18mm_qr"
            && status == "ok"),
        "the print job is attributed to actor local: {rows:?}"
    );

    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
    service.kill().await.expect("stop the service");
}

// ---------------------------------------------------------------------------
// mcp-template-management 4: real-service authoring, repair, reload and auth
// ---------------------------------------------------------------------------

const AUTHORING_YAML: &str = r#"# authored through MCP
name: Authoring One
unit: mm
dpi: 200
params:
  - name: message
    type: string
format: {type: single, width: 50, height: 20}
layout:
  - type: text
    value: "{message}"
    at: [0.0, 0.0]
    size: [50, 20]
    font_size: 12
"#;

const AUTHORING_EDITED_YAML: &str = r#"# edited through MCP
name: Authoring One Edited
unit: mm
dpi: 200
params:
  - name: message
    type: string
format: {type: single, width: 50, height: 20}
layout:
  - type: text
    value: "{message}"
    at: [1.0, 1.0]
    size: [48, 18]
    font_size: 11
"#;

const RELOAD_ID_YAML: &str = r#"name: Reload As A Template
unit: mm
dpi: 200
format: {type: single, width: 40, height: 15}
layout:
  - type: text
    value: "reload"
    at: [0.0, 0.0]
    size: [40, 15]
    font_size: 10
"#;

const SEED_YAML: &str = r#"name: Seeded Offline
unit: mm
dpi: 200
format: {type: single, width: 30, height: 12}
layout: []
"#;

const BROKEN_YAML: &str = "name: Needs Repair\nnot_a_template_key: true\n";

const REPAIRED_YAML: &str = r#"name: Needs Repair Fixed
unit: mm
dpi: 200
format: {type: single, width: 30, height: 12}
layout: []
"#;

const SHEET_YAML: &str = include_str!("fixtures/templates/avery5163_asset_tag.yaml");

/// The PNG bytes of an image content block, decoded from its base64 data.
fn image_bytes(result: &Value) -> Vec<u8> {
    use base64::Engine as _;
    assert_eq!(result["content"][0]["type"], "image", "{result}");
    assert_eq!(result["content"][0]["mimeType"], "image/png", "{result}");
    base64::engine::general_purpose::STANDARD
        .decode(result["content"][0]["data"].as_str().expect("base64"))
        .expect("decode image")
}

/// The authoring workflow against a real service in no-auth mode: create, read
/// detail and source, replace with the edited source, preview and thumbnail the
/// result, refuse a duplicate create and an invalid edit, then delete.
#[tokio::test(flavor = "multi_thread")]
async fn template_authoring_lifecycle_against_the_real_service() {
    let config = TempConfig::new("authoring");
    let port = free_port();
    let mut service = spawn_service(&config, port, &[("LABELER_NO_AUTH", "true")]).await;
    let base = format!("http://127.0.0.1:{port}");
    let mut mcp = McpProcess::spawn(&[("LABELER_URL", base.as_str())]).await;
    mcp.initialize().await;

    // Create: the stored file holds the supplied bytes and the response is the
    // published detail of the new template.
    let created = mcp
        .tool_result(
            "create_template",
            json!({"template": "authoring_one", "yaml": AUTHORING_YAML}),
        )
        .await;
    assert!(!is_error(&created), "create: {created}");
    let created_detail: Value = serde_json::from_str(&first_text(&created)).expect("detail JSON");
    assert_eq!(created_detail["id"], "authoring_one", "{created_detail}");
    assert!(
        created_detail["params"]
            .as_array()
            .is_some_and(|p| !p.is_empty()),
        "the published detail carries the parameters: {created_detail}"
    );
    assert_eq!(
        std::fs::read_to_string(config.template_path("authoring_one")).expect("stored file"),
        AUTHORING_YAML,
        "the stored source is the YAML the agent supplied"
    );

    // Read: detail and source agree with the file, comments included.
    let detail = mcp
        .tool_result("get_template", json!({"template": "authoring_one"}))
        .await;
    assert!(!is_error(&detail), "detail: {detail}");
    let detail: Value = serde_json::from_str(&first_text(&detail)).expect("detail JSON");
    assert_eq!(detail["name"], "Authoring One", "{detail}");
    let source = mcp
        .tool_result("get_template_source", json!({"template": "authoring_one"}))
        .await;
    assert!(!is_error(&source), "source: {source}");
    assert_eq!(
        first_text(&source),
        AUTHORING_YAML,
        "the source reads back byte-for-byte"
    );

    // Replace: the edit is stored whole and published under the same id.
    let replaced = mcp
        .tool_result(
            "replace_template",
            json!({"template": "authoring_one", "yaml": AUTHORING_EDITED_YAML}),
        )
        .await;
    assert!(!is_error(&replaced), "replace: {replaced}");
    let replaced_detail: Value = serde_json::from_str(&first_text(&replaced)).expect("detail JSON");
    assert_eq!(
        replaced_detail["name"], "Authoring One Edited",
        "{replaced_detail}"
    );
    assert_eq!(
        std::fs::read_to_string(config.template_path("authoring_one")).expect("stored file"),
        AUTHORING_EDITED_YAML
    );
    let edited_source = mcp
        .tool_result("get_template_source", json!({"template": "authoring_one"}))
        .await;
    assert_eq!(first_text(&edited_source), AUTHORING_EDITED_YAML);

    // Filled preview and thumbnail both produce a PNG of the edited template.
    let preview = mcp
        .tool_result(
            "render_label",
            json!({"template": "authoring_one", "data": {"message": "Hello"}}),
        )
        .await;
    assert!(!is_error(&preview), "preview: {preview}");
    assert!(
        image_bytes(&preview).starts_with(b"\x89PNG\r\n\x1a\n"),
        "the preview is a real PNG"
    );
    let thumbnail = mcp
        .tool_result(
            "get_template_thumbnail",
            json!({"template": "authoring_one"}),
        )
        .await;
    assert!(!is_error(&thumbnail), "thumbnail: {thumbnail}");
    assert!(
        image_bytes(&thumbnail).starts_with(b"\x89PNG\r\n\x1a\n"),
        "the thumbnail is a real PNG"
    );

    // A duplicate create is refused and leaves the edited file untouched.
    let duplicate = mcp
        .tool_result(
            "create_template",
            json!({"template": "authoring_one", "yaml": AUTHORING_YAML}),
        )
        .await;
    assert!(is_error(&duplicate), "a taken id is refused: {duplicate}");
    assert_eq!(error_object_text(&duplicate)["status"], 409, "{duplicate}");
    assert_eq!(
        std::fs::read_to_string(config.template_path("authoring_one")).expect("stored file"),
        AUTHORING_EDITED_YAML,
        "a refused create leaves the file unchanged"
    );

    // An invalid replacement is refused and leaves the file untouched too.
    let invalid = mcp
        .tool_result(
            "replace_template",
            json!({"template": "authoring_one", "yaml": "not: a template\n"}),
        )
        .await;
    assert!(is_error(&invalid), "invalid YAML is refused: {invalid}");
    assert_eq!(error_object_text(&invalid)["status"], 422, "{invalid}");
    assert_eq!(
        std::fs::read_to_string(config.template_path("authoring_one")).expect("stored file"),
        AUTHORING_EDITED_YAML,
        "a refused replace leaves the file unchanged"
    );

    // Delete removes the file and drops it from the served registry.
    let deleted = mcp
        .tool_result("delete_template", json!({"template": "authoring_one"}))
        .await;
    assert!(!is_error(&deleted), "delete: {deleted}");
    assert!(
        deleted["content"]
            .as_array()
            .is_some_and(|blocks| blocks.is_empty()),
        "deletion answers with empty content: {deleted}"
    );
    assert!(
        !config.template_path("authoring_one").exists(),
        "the file is gone"
    );
    let listed = mcp.tool_result("list_templates", json!({})).await;
    let listed: Value = serde_json::from_str(&first_text(&listed)).expect("templates JSON");
    assert!(
        !listed["templates"]
            .as_array()
            .expect("templates")
            .iter()
            .any(|t| t["id"] == "authoring_one"),
        "the registry dropped it: {listed}"
    );
    let gone = mcp
        .tool_result("get_template", json!({"template": "authoring_one"}))
        .await;
    assert!(is_error(&gone), "the detail of a deleted template: {gone}");
    assert_eq!(error_object_text(&gone)["status"], 404, "{gone}");

    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
    service.kill().await.expect("stop the service");
}

/// The valid identifier `reload` addresses a template through all six
/// path-based tools, and `reload_templates` still replaces the registry.
#[tokio::test(flavor = "multi_thread")]
async fn the_valid_id_reload_is_a_template_and_reload_still_replaces_the_registry() {
    let config = TempConfig::new("reload-id");
    let port = free_port();
    let mut service = spawn_service(&config, port, &[("LABELER_NO_AUTH", "true")]).await;
    let base = format!("http://127.0.0.1:{port}");
    let mut mcp = McpProcess::spawn(&[("LABELER_URL", base.as_str())]).await;
    mcp.initialize().await;

    // Creating `reload` writes the file instead of answering with reload counts.
    let created = mcp
        .tool_result(
            "create_template",
            json!({"template": "reload", "yaml": RELOAD_ID_YAML}),
        )
        .await;
    assert!(!is_error(&created), "create reload: {created}");
    assert_eq!(
        serde_json::from_str::<Value>(&first_text(&created)).expect("detail JSON")["id"],
        "reload",
        "the create answered with a template, not registry counts: {created}"
    );
    assert!(
        config.template_path("reload").exists(),
        "creation wrote reload.yaml rather than only reloading the registry"
    );
    assert_eq!(
        std::fs::read_to_string(config.template_path("reload")).expect("stored file"),
        RELOAD_ID_YAML
    );

    let listed = mcp.tool_result("list_templates", json!({})).await;
    let listed: Value = serde_json::from_str(&first_text(&listed)).expect("templates JSON");
    assert!(
        listed["templates"]
            .as_array()
            .expect("templates")
            .iter()
            .any(|t| t["id"] == "reload"),
        "the template is served: {listed}"
    );

    let detail = mcp
        .tool_result("get_template", json!({"template": "reload"}))
        .await;
    assert!(!is_error(&detail), "detail of the reload id: {detail}");
    assert_eq!(
        serde_json::from_str::<Value>(&first_text(&detail)).expect("detail JSON")["name"],
        "Reload As A Template"
    );

    let source = mcp
        .tool_result("get_template_source", json!({"template": "reload"}))
        .await;
    assert!(!is_error(&source), "source of the reload id: {source}");
    assert_eq!(first_text(&source), RELOAD_ID_YAML);

    let thumbnail = mcp
        .tool_result("get_template_thumbnail", json!({"template": "reload"}))
        .await;
    assert!(
        !is_error(&thumbnail),
        "thumbnail of the reload id: {thumbnail}"
    );
    assert!(image_bytes(&thumbnail).starts_with(b"\x89PNG\r\n\x1a\n"));

    let replaced = mcp
        .tool_result(
            "replace_template",
            json!({"template": "reload", "yaml": SEED_YAML}),
        )
        .await;
    assert!(!is_error(&replaced), "replace of the reload id: {replaced}");
    assert_eq!(
        std::fs::read_to_string(config.template_path("reload")).expect("stored file"),
        SEED_YAML,
        "the replacement overwrote the file"
    );

    let deleted = mcp
        .tool_result("delete_template", json!({"template": "reload"}))
        .await;
    assert!(!is_error(&deleted), "delete of the reload id: {deleted}");
    assert!(
        !config.template_path("reload").exists(),
        "the reload id's file is deleted"
    );

    // A file seeded straight to disk is served only once the registry is
    // reloaded, which is what the reload tool does.
    config.write_template("seeded_offline", SEED_YAML);
    let before = mcp.tool_result("list_templates", json!({})).await;
    let before: Value = serde_json::from_str(&first_text(&before)).expect("templates JSON");
    assert!(
        !before["templates"]
            .as_array()
            .expect("templates")
            .iter()
            .any(|t| t["id"] == "seeded_offline"),
        "nothing serves a file the service has not reloaded: {before}"
    );

    let reload = mcp.tool_result("reload_templates", json!({})).await;
    assert!(!is_error(&reload), "reload: {reload}");
    let counts: Value = serde_json::from_str(&first_text(&reload)).expect("reload JSON");
    assert_eq!(counts["count"], 2, "the served templates: {counts}");
    assert_eq!(counts["broken_count"], 0, "{counts}");

    let after = mcp.tool_result("list_templates", json!({})).await;
    let after: Value = serde_json::from_str(&first_text(&after)).expect("templates JSON");
    assert!(
        after["templates"]
            .as_array()
            .expect("templates")
            .iter()
            .any(|t| t["id"] == "seeded_offline"),
        "reload replaced the registry from disk: {after}"
    );

    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
    service.kill().await.expect("stop the service");
}

/// A quarantined file is readable and repairable through the adapter, and a
/// sheet template's thumbnail is one slot rather than the page.
#[tokio::test(flavor = "multi_thread")]
async fn a_quarantined_template_is_repaired_and_a_sheet_thumbnail_is_one_slot() {
    let config = TempConfig::new("repair");
    config.write_template("needs_repair", BROKEN_YAML);
    let port = free_port();
    let mut service = spawn_service(&config, port, &[("LABELER_NO_AUTH", "true")]).await;
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::new();

    // A fake printer proves that producing a thumbnail sends nothing to print.
    let ipp = spawn_fake_ipp();
    let printer = client
        .post(format!("{base}/api/printers"))
        .json(&json!({"id": "fake", "name": "Fake", "uri": ipp.uri}))
        .send()
        .await
        .expect("create printer");
    assert!(
        printer.status().is_success(),
        "printer: {}",
        printer.status()
    );

    let mut mcp = McpProcess::spawn(&[("LABELER_URL", base.as_str())]).await;
    mcp.initialize().await;

    // The broken file is quarantined: listed as broken, no detail, but its
    // source is readable — the starting point of a repair.
    let listed = mcp.tool_result("list_templates", json!({})).await;
    let listed: Value = serde_json::from_str(&first_text(&listed)).expect("templates JSON");
    assert!(
        !listed["templates"]
            .as_array()
            .expect("templates")
            .iter()
            .any(|t| t["id"] == "needs_repair"),
        "{listed}"
    );
    assert!(
        listed["broken"]
            .as_array()
            .expect("broken")
            .iter()
            .any(|b| b["path"] == "needs_repair.yaml"),
        "the broken file is quarantined: {listed}"
    );

    let detail = mcp
        .tool_result("get_template", json!({"template": "needs_repair"}))
        .await;
    assert!(
        is_error(&detail),
        "a quarantined template has no detail: {detail}"
    );
    assert_eq!(error_object_text(&detail)["status"], 404, "{detail}");

    let source = mcp
        .tool_result("get_template_source", json!({"template": "needs_repair"}))
        .await;
    assert!(!is_error(&source), "its source is still readable: {source}");
    assert_eq!(first_text(&source), BROKEN_YAML);

    // The reload counts report what disk currently holds: one served, broken.
    let reload = mcp.tool_result("reload_templates", json!({})).await;
    assert!(!is_error(&reload), "reload: {reload}");
    let counts: Value = serde_json::from_str(&first_text(&reload)).expect("reload JSON");
    assert_eq!(counts["count"], 1, "{counts}");
    assert_eq!(counts["broken_count"], 1, "{counts}");

    // Repair by replacement: the same id becomes served.
    let repaired = mcp
        .tool_result(
            "replace_template",
            json!({"template": "needs_repair", "yaml": REPAIRED_YAML}),
        )
        .await;
    assert!(!is_error(&repaired), "repair: {repaired}");
    let served = mcp
        .tool_result("get_template", json!({"template": "needs_repair"}))
        .await;
    assert!(
        !is_error(&served),
        "the repaired template is served: {served}"
    );
    assert_eq!(
        serde_json::from_str::<Value>(&first_text(&served)).expect("detail JSON")["name"],
        "Needs Repair Fixed"
    );
    let reload_after = mcp.tool_result("reload_templates", json!({})).await;
    let counts: Value = serde_json::from_str(&first_text(&reload_after)).expect("reload JSON");
    assert_eq!(counts["count"], 2, "the repaired file is served: {counts}");
    assert_eq!(
        counts["broken_count"], 0,
        "nothing is quarantined: {counts}"
    );

    // A sheet thumbnail is one slot — the 4in x 2in label at 300 dpi — and no
    // print is sent to produce it.
    config.write_template("sheet", SHEET_YAML);
    let reload_sheet = mcp.tool_result("reload_templates", json!({})).await;
    assert!(!is_error(&reload_sheet), "reload the sheet: {reload_sheet}");
    let thumbnail = mcp
        .tool_result("get_template_thumbnail", json!({"template": "sheet"}))
        .await;
    assert!(!is_error(&thumbnail), "sheet thumbnail: {thumbnail}");
    let png = image_bytes(&thumbnail);
    assert_eq!(png_dimensions(&png), (1200, 600), "one slot, not the page");
    assert_eq!(
        ipp.print_jobs.load(Ordering::SeqCst),
        0,
        "a thumbnail never prints"
    );

    let (success, _, _) = mcp.shutdown().await;
    assert!(success);
    service.kill().await.expect("stop the service");
}

/// Template writes are authorized like every other call: a personal token is
/// accepted, a revoked one is refused before any file changes, and the secret
/// never reaches the adapter's diagnostics or results.
#[tokio::test(flavor = "multi_thread")]
async fn template_writes_follow_token_authorization_and_keep_the_secret_quiet() {
    let config = TempConfig::new("token-write");
    let port = free_port();
    let mut service = spawn_service(&config, port, &[]).await;
    let base = format!("http://127.0.0.1:{port}");
    let origin = base.clone();
    let client = reqwest::Client::new();

    // First-run setup (cookie-authenticated, so carry a matching Origin).
    let setup = client
        .post(format!("{base}/api/auth/setup"))
        .header("origin", &origin)
        .json(&json!({"username": "alice", "password": "correct-horse"}))
        .send()
        .await
        .expect("setup request");
    assert!(setup.status().is_success(), "setup: {}", setup.status());
    let cookie = cookie_from(&setup);

    let token_response = client
        .post(format!("{base}/api/tokens"))
        .header("cookie", &cookie)
        .header("origin", &origin)
        .json(&json!({"name": "mcp-writes"}))
        .send()
        .await
        .expect("create token");
    assert!(
        token_response.status().is_success(),
        "token: {}",
        token_response.status()
    );
    let token_body: Value = token_response.json().await.expect("token JSON");
    let secret = token_body["secret"]
        .as_str()
        .expect("token secret")
        .to_string();
    let token_id = token_body["id"].as_str().expect("token id").to_string();

    let mut mcp = McpProcess::spawn(&[
        ("LABELER_URL", base.as_str()),
        ("LABELER_TOKEN", secret.as_str()),
    ])
    .await;
    mcp.initialize().await;

    let created = mcp
        .tool_result(
            "create_template",
            json!({"template": "token_written", "yaml": SEED_YAML}),
        )
        .await;
    assert!(
        !is_error(&created),
        "a personal token authorizes template writes: {created}"
    );
    assert!(
        config.template_path("token_written").exists(),
        "the write landed"
    );

    // Revoking the token refuses the next write before any file changes.
    let revoke = client
        .delete(format!("{base}/api/tokens/{token_id}"))
        .header("cookie", &cookie)
        .header("origin", &origin)
        .send()
        .await
        .expect("revoke token");
    assert!(revoke.status().is_success(), "revoke: {}", revoke.status());

    let refused = mcp
        .tool_result(
            "create_template",
            json!({"template": "after_revoke", "yaml": SEED_YAML}),
        )
        .await;
    assert!(is_error(&refused), "a revoked token is refused: {refused}");
    assert_eq!(error_object_text(&refused)["status"], 401, "{refused}");
    assert!(
        !config.template_path("after_revoke").exists(),
        "a revoked token changes nothing"
    );

    let (success, lines, stderr) = mcp.shutdown().await;
    assert!(success);
    let captured = lines.join("\n") + &stderr;
    assert!(
        !captured.contains(&secret),
        "the token secret never appears in the adapter's diagnostics or results"
    );
    service.kill().await.expect("stop the service");
}
