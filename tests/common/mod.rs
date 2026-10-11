//! Shared driver for `labeler-mcp` integration tests: spawn the real binary and
//! speak newline-delimited JSON-RPC over its stdio.

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// A spawned `labeler-mcp` process driven over stdio.
pub struct McpProcess {
    child: tokio::process::Child,
    stdin: Option<tokio::process::ChildStdin>,
    stdout: BufReader<tokio::process::ChildStdout>,
    stderr_task: tokio::task::JoinHandle<String>,
    next_id: i64,
    stdout_lines: Vec<String>,
}

impl McpProcess {
    /// Spawn the binary with a cleared environment plus the given variables.
    pub async fn spawn(envs: &[(&str, &str)]) -> Self {
        let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_labeler-mcp"));
        cmd.env_clear();
        if let Ok(path) = std::env::var("PATH") {
            cmd.env("PATH", path);
        }
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        for (key, value) in envs {
            cmd.env(key, value);
        }
        let mut child = cmd.spawn().expect("spawn labeler-mcp");
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
        let mut stderr = child.stderr.take().expect("piped stderr");
        let stderr_task = tokio::spawn(async move {
            let mut buf = String::new();
            use tokio::io::AsyncReadExt;
            let _ = stderr.read_to_string(&mut buf).await;
            buf
        });
        Self {
            child,
            stdin: Some(stdin),
            stdout,
            stderr_task,
            next_id: 1,
            stdout_lines: Vec::new(),
        }
    }

    async fn write_line(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().expect("stdin is open");
        stdin
            .write_all(line.as_bytes())
            .await
            .expect("write to stdin");
        stdin.write_all(b"\n").await.expect("write newline");
        stdin.flush().await.expect("flush stdin");
    }

    async fn read_line(&mut self) -> Option<String> {
        let mut line = String::new();
        match self.stdout.read_line(&mut line).await {
            Ok(0) | Err(_) => None,
            Ok(_) => {
                let trimmed = line.trim_end_matches(['\n', '\r']).to_string();
                self.stdout_lines.push(trimmed.clone());
                Some(trimmed)
            }
        }
    }

    async fn send_raw(&mut self, message: Value) {
        self.write_line(&message.to_string()).await;
    }

    /// Send a request and wait for the response with this id, skipping notifications.
    pub async fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send_raw(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))
        .await;
        self.read_response(id).await
    }

    async fn notify(&mut self, method: &str, params: Value) {
        self.send_raw(json!({ "jsonrpc": "2.0", "method": method, "params": params }))
            .await;
    }

    async fn read_response(&mut self, id: i64) -> Value {
        loop {
            let line = tokio::time::timeout(std::time::Duration::from_secs(30), self.read_line())
                .await
                .expect("timed out waiting for a protocol message")
                .expect("stdout closed before the response arrived");
            let message: Value = serde_json::from_str(&line)
                .unwrap_or_else(|e| panic!("stdout line is not JSON ({e}): {line}"));
            if message.get("id").and_then(Value::as_i64) == Some(id) {
                return message;
            }
        }
    }

    pub async fn initialize(&mut self) -> Value {
        let response = self
            .request(
                "initialize",
                json!({
                    "protocolVersion": "2025-11-25",
                    "capabilities": {},
                    "clientInfo": { "name": "adapter-tests", "version": "0" }
                }),
            )
            .await;
        self.notify("notifications/initialized", json!({})).await;
        response
    }

    pub async fn list_tools(&mut self) -> Value {
        let response = self.request("tools/list", json!({})).await;
        response["result"].clone()
    }

    /// Call a tool and return the full JSON-RPC message (result or protocol error).
    pub async fn call_tool(&mut self, name: &str, arguments: Value) -> Value {
        self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )
        .await
    }

    /// A completed execution result: asserts no protocol error, returns `result`.
    pub async fn tool_result(&mut self, name: &str, arguments: Value) -> Value {
        let message = self.call_tool(name, arguments).await;
        assert!(
            message.get("error").is_none(),
            "expected a tool execution result, got a protocol error: {message}"
        );
        message["result"].clone()
    }

    async fn captured_stderr(&mut self) -> String {
        (&mut self.stderr_task).await.expect("stderr collector")
    }

    /// Close stdin and wait for the process to exit; returns its success flag.
    pub async fn shutdown(mut self) -> (bool, Vec<String>, String) {
        drop(self.stdin.take());
        let status = tokio::time::timeout(std::time::Duration::from_secs(15), self.child.wait())
            .await
            .expect("process did not exit after stdin closed")
            .expect("wait failed");
        self.finish(status).await
    }

    async fn finish(mut self, status: std::process::ExitStatus) -> (bool, Vec<String>, String) {
        loop {
            let mut line = String::new();
            match self.stdout.read_line(&mut line).await {
                Ok(0) | Err(_) => break,
                Ok(_) => self
                    .stdout_lines
                    .push(line.trim_end_matches(['\n', '\r']).to_string()),
            }
        }
        let stderr = self.captured_stderr().await;
        (
            status.success(),
            std::mem::take(&mut self.stdout_lines),
            stderr,
        )
    }
}

pub fn is_error(result: &Value) -> bool {
    result.get("isError").and_then(Value::as_bool) == Some(true)
}

pub fn first_text(result: &Value) -> String {
    result["content"][0]["text"]
        .as_str()
        .expect("first content block is text")
        .to_string()
}

pub fn error_object_text(result: &Value) -> Value {
    serde_json::from_str(&first_text(result)).expect("execution error text is JSON")
}
