use std::net::IpAddr;
use std::time::Duration;

use reqwest::redirect::Policy;
use serde::de::DeserializeOwned;

const MAX_BYTES: usize = 8 * 1024 * 1024; // 8 MiB response cap
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug)]
pub enum EgressError {
    Blocked(String),
    Timeout,
    TooLarge,
    Status(u16),
    Transport(String),
}

/// Outbound IP allow-check: deny loopback, link-local (incl. cloud metadata), unspecified, multicast.
/// Private LAN ranges are allowed (that is where a self-hosted Homebox lives). `allow_loopback` is the
/// test override (true only for `Egress::with_loopback`); it relaxes ONLY loopback, never link-local etc.
pub fn ip_allowed(ip: IpAddr, allow_loopback: bool) -> bool {
    // Normalize IPv4-mapped IPv6 (::ffff:a.b.c.d) down to V4 first, otherwise a mapped loopback /
    // link-local / unspecified address (e.g. ::ffff:169.254.169.254) would skip the V4 checks below
    // and be wrongly allowed while the OS still connects to it as IPv4.
    let ip = match ip {
        IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(v6)),
        v4 => v4,
    };
    if ip.is_loopback() {
        return allow_loopback;
    }
    if ip.is_unspecified() || ip.is_multicast() {
        return false;
    }
    match ip {
        IpAddr::V4(v4) => !v4.is_link_local(), // 169.254.0.0/16 (covers metadata)
        IpAddr::V6(v6) => {
            // unicast link-local fe80::/10
            let seg = v6.segments()[0];
            (seg & 0xffc0) != 0xfe80
        }
    }
}

pub struct Egress {
    client: reqwest::Client,
    allow_loopback: bool,
}

impl Default for Egress {
    fn default() -> Self {
        Self::new()
    }
}

impl Egress {
    pub fn new() -> Self {
        Self::build(false)
    }

    /// Test-only: identical to `new()` but permits loopback so wiremock servers on 127.0.0.1 work.
    pub fn with_loopback() -> Self {
        Self::build(true)
    }

    fn build(allow_loopback: bool) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(READ_TIMEOUT)
            .redirect(Policy::none()) // do not follow redirects (no cross-host bounce)
            .no_proxy() // ignore proxy env vars
            .gzip(true)
            .https_only(false) // allow http for a LAN Homebox; scheme still checked below
            .build()
            .expect("build reqwest client");
        Self {
            client,
            allow_loopback,
        }
    }

    /// GET `<base><path>?<query>` with a bearer header, after resolving the host and refusing any
    /// disallowed IP. Deserializes the body into `T`. Read-only (GET only).
    pub async fn get_json<T: DeserializeOwned>(
        &self,
        base: &url::Url,
        path: &str,
        query: &[(String, String)],
        bearer: &str,
    ) -> Result<T, EgressError> {
        let scheme = base.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(EgressError::Blocked(format!(
                "scheme '{scheme}' not allowed"
            )));
        }
        let host = base
            .host_str()
            .ok_or_else(|| EgressError::Blocked("no host".into()))?;
        let port = base.port_or_known_default().unwrap_or(80);
        // Resolve and refuse if ANY resolved address is disallowed (conservative). NOTE: reqwest
        // re-resolves on connect, so a sub-millisecond DNS-rebind could still slip a vetted host to a
        // blocked IP. That residual TOCTOU is the accepted risk for this single-tenant authed LAN tool.
        let addrs = tokio::net::lookup_host((host, port))
            .await
            .map_err(|e| EgressError::Transport(format!("dns: {e}")))?;
        let mut any = false;
        for sa in addrs {
            any = true;
            if !ip_allowed(sa.ip(), self.allow_loopback) {
                return Err(EgressError::Blocked(format!(
                    "address {} not allowed",
                    sa.ip()
                )));
            }
        }
        if !any {
            return Err(EgressError::Blocked("host did not resolve".into()));
        }

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
        serde_json::from_slice(&bytes).map_err(|e| EgressError::Transport(format!("json: {e}")))
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
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    #[test]
    fn ip_policy_blocks_dangerous_allows_private() {
        assert!(!ip_allowed(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), false)); // loopback
        assert!(!ip_allowed(
            IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254)),
            false
        )); // metadata/link-local
        assert!(!ip_allowed(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), false)); // unspecified
        assert!(!ip_allowed(IpAddr::V6(Ipv6Addr::LOCALHOST), false)); // ::1
        assert!(ip_allowed(
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50)),
            false
        )); // private LAN: allowed
        assert!(ip_allowed(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)), false)); // private LAN: allowed
        assert!(ip_allowed(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)), false)); // public: allowed
                                                                           // loopback IS allowed when the test override is set
        assert!(ip_allowed(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), true));
        // but the override does NOT re-enable link-local / unspecified / multicast
        assert!(!ip_allowed(
            IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254)),
            true
        ));
        assert!(!ip_allowed(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), true));
        // IPv4-mapped IPv6 must NOT bypass the V4 checks (loopback/link-local/unspecified)
        assert!(!ip_allowed(
            IpAddr::V6("::ffff:127.0.0.1".parse().unwrap()),
            false
        ));
        assert!(!ip_allowed(
            IpAddr::V6("::ffff:169.254.169.254".parse().unwrap()),
            false
        ));
        assert!(!ip_allowed(
            IpAddr::V6("::ffff:0.0.0.0".parse().unwrap()),
            false
        ));
        // a mapped public address is still allowed
        assert!(ip_allowed(
            IpAddr::V6("::ffff:1.1.1.1".parse().unwrap()),
            false
        ));
    }

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
        let egress = Egress::with_loopback(); // wiremock is on 127.0.0.1
        let v: serde_json::Value = egress
            .get_json(&base, "/api/v1/ping", &[], "hb_abc")
            .await
            .unwrap();
        assert_eq!(v["ok"], true);
    }

    #[tokio::test]
    async fn blocks_loopback_host() {
        // wiremock binds to 127.0.0.1; the PRODUCTION egress must Block it.
        let server = wiremock::MockServer::start().await;
        let base = url::Url::parse(&server.uri()).unwrap();
        let egress = Egress::new();
        let err = egress
            .get_json::<serde_json::Value>(&base, "/x", &[], "hb_abc")
            .await
            .unwrap_err();
        assert!(matches!(err, EgressError::Blocked(_)));
    }
}
