use std::time::Duration;

use reqwest::redirect::Policy;
use serde::de::DeserializeOwned;

const MAX_BYTES: usize = 8 * 1024 * 1024; // 8 MiB response cap
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug)]
pub enum EgressError {
    Timeout,
    TooLarge,
    Status(u16),
    /// The body is not the JSON expected. Carries nothing: a parse error quotes upstream values, and
    /// one of them can be the credential.
    Malformed,
    Transport(String),
}

pub struct Egress {
    client: reqwest::Client,
}

impl Default for Egress {
    fn default() -> Self {
        Self::new()
    }
}

impl Egress {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(READ_TIMEOUT)
            .redirect(Policy::none()) // do not follow redirects (no cross-host bounce)
            .no_proxy() // ignore proxy env vars
            .gzip(true)
            .https_only(false) // allow http for a LAN Homebox
            .build()
            .expect("build reqwest client");
        Self { client }
    }

    /// GET `<base><path>?<query>` with a bearer header. Deserializes the body into `T`. Read-only
    /// (GET only).
    pub async fn get_json<T: DeserializeOwned>(
        &self,
        base: &url::Url,
        path: &str,
        query: &[(String, String)],
        bearer: &str,
    ) -> Result<T, EgressError> {
        // Append `path` to the base path (preserves a base hosted under a subpath, e.g. `/homebox/`).
        let mut u = base.clone();
        let joined = format!("{}{}", base.path().trim_end_matches('/'), path);
        u.set_path(&joined);
        {
            let mut qp = u.query_pairs_mut();
            for (k, v) in query {
                qp.append_pair(k, v);
            }
        }

        let resp = self
            .client
            .get(u)
            .bearer_auth(bearer)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    EgressError::Timeout
                } else {
                    EgressError::Transport(redact(&e.to_string()))
                }
            })?;
        let status = resp.status();
        if !status.is_success() {
            return Err(EgressError::Status(status.as_u16()));
        }
        // Enforce the byte cap WHILE streaming, so a malicious/huge body cannot OOM us before a check.
        let bytes = read_capped(resp).await?;
        serde_json::from_slice(&bytes).map_err(|_| EgressError::Malformed)
    }
}

/// Read the body chunk-by-chunk, aborting as soon as the running total exceeds `MAX_BYTES`. Uses
/// `reqwest::Response::chunk` (built in, so no `futures-util` dependency). Do NOT replace this with
/// `resp.bytes().await` + a post-hoc length check: that buffers the ENTIRE body first, so a server
/// streaming gigabytes OOMs the process before the size check ever runs.
async fn read_capped(mut resp: reqwest::Response) -> Result<Vec<u8>, EgressError> {
    let mut out = Vec::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| EgressError::Transport(redact(&e.to_string())))?
    {
        if out.len() + chunk.len() > MAX_BYTES {
            return Err(EgressError::TooLarge);
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

/// Strip anything that looks like a bearer token from an error string before it can be logged.
fn redact(s: &str) -> String {
    // crude but sufficient: drop occurrences of "hb_" tokens.
    s.split_whitespace()
        .map(|w| if w.starts_with("hb_") { "hb_***" } else { w })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn get_json_fetches_and_sends_bearer() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/api/v1/ping"))
            .and(wiremock::matchers::header("authorization", "Bearer hb_abc"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})),
            )
            .mount(&server)
            .await;
        let base = url::Url::parse(&server.uri()).unwrap();
        let egress = Egress::new(); // wiremock is on 127.0.0.1
        let v: serde_json::Value = egress
            .get_json(&base, "/api/v1/ping", &[], "hb_abc")
            .await
            .unwrap();
        assert_eq!(v["ok"], true);
    }
}
