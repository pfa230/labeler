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

use common::{error_object_text, first_text, is_error, McpProcess};
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
        let templates = dir.join("templates");
        std::fs::create_dir_all(&templates).expect("create config dir");
        std::fs::write(templates.join("brother_18mm_qr.yaml"), TEMPLATE_YAML)
            .expect("write template");
        Self { dir }
    }
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

    // The four tools are what a client discovers against the real service.
    let tools = mcp.list_tools().await;
    let mut names: Vec<String> = tools["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .map(|t| t["name"].as_str().expect("tool name").to_string())
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            "list_printers",
            "list_templates",
            "print_labels",
            "render_label"
        ]
    );

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

    // Filled preview: the decoded image equals a PNG produced by the service.
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
