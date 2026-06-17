# Homebox Integration Plan A (Backend) Implementation Plan

> **Status: DONE** — implemented and merged to `main` (`40dcc72`, 2026-06-17). All 7 tasks shipped; #35 closed.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the backend spine for browsing a Homebox inventory as a label data source: a hardened egress client, a connections store + CRUD, a `Connector` trait + registry, the Homebox connector, and the `schema`/`browse`/`materialize` endpoints.

**Architecture:** One shared hardened `Egress` HTTP client (reqwest/rustls) does all outbound calls with an IP allow-check. A `connections` SQLite table stores `{connector, base_url, credential}` (the Homebox API key, redacted on read). A `Connector` trait normalizes an external API into a browse model; the Homebox impl talks to the unified `/v1/entities` API with `Authorization: Bearer <key>`. Three protected endpoints expose `schema`/`browse`/`materialize`; `browse` paginates with server-issued HMAC-bound cursors.

**Tech Stack:** Rust/axum 0.8, rusqlite, `reqwest` (rustls), `url`, `serde_json`, `hmac`+`sha2` (cursor signing), `wiremock` (dev, mock Homebox). Behind the shipped app-auth middleware.

## Global Constraints
- Rust edition 2021; `cargo fmt` + `cargo clippy --all-targets --all-features` + `cargo test` must be clean before each commit; never `#[allow(clippy::...)]` (fix root cause).
- No em dashes in code or docs.
- All new routes are added to `api_router()` and are therefore automatically behind `require_auth` (the auth layer is applied to the `/api` nest in `app()`); do not add anything to the exemption list.
- `reqwest` must use **rustls**, not OpenSSL: `reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "json", "gzip"] }`.
- Egress policy (from the spec): deny loopback `127.0.0.0/8`+`::1`, link-local `169.254.0.0/16`+`fe80::/10`, unspecified `0.0.0.0`+`::`, multicast; **allow** private LAN ranges. `http`/`https` only; connect+read timeouts; max response bytes; no cross-host redirects; ignore proxy env; redact secrets in logs.
- The connector is **read-only** (GET only); the credential is the pasted Homebox API key sent verbatim as `Authorization: Bearer <key>` (Homebox keys already carry the `hb_` prefix; do NOT prepend).
- Spec: `docs/superpowers/specs/2026-06-16-homebox-integration-design.md`; framework: `docs/superpowers/specs/2026-06-16-api-integration-framework-design.md`. Verified Homebox API facts (against `/tmp/hb-swagger.json`): `GET /v1/entities` query params are `q`, `page`, `pageSize`, `tags` (array, `collectionFormat: multi`, i.e. a **repeated bare key** `?tags=a&tags=b`, NO `[]` suffix), `parentIds` (same), and there is NO server-side entityType filter; the 200 body is `repo.EntityListResult { items: [EntitySummary], page, pageSize, total, totalPrice }`; `GET /v1/entities/tree?withItems=`; `GET /v1/entities/{id}`; `GET /v1/entities/fields` returns a JSON **array of strings** (custom-field names); API keys at `/v1/users/self/api-keys`; auth header `Authorization: Bearer`. `EntitySummary` properties include `id, name, description, assetId, quantity, purchasePrice, itemCount, entityType (nullable {name,...}), parent (nullable {name,...})`.

---

## Context the implementer needs
- **Store (`src/store.rs`):** `migrations()` returns `Migrations::new(vec![M::up(...), M::up(...)])`; append a THIRD `M::up` for `connections` (never edit existing). `Store { conn: Mutex<Connection> }`; methods lock + are async-shaped. `Store::open_in_memory()` for tests. `use rusqlite::OptionalExtension;` is already imported.
- **Router/state (`src/api.rs`):** `api_router() -> Router<Arc<AppState>>`; `app()` nests it under `/api` with the auth layer. `AppState { templates, templates_dir, write_lock: Mutex<()>, store, ui_dir, trust_proxy }`; add `egress: Arc<Egress>` and `connectors: ConnectorRegistry` built inside `AppState::new` (no call-site change). Handlers take `State<Arc<AppState>>`; write ops take `let _guard = state.write_lock.lock().await;`. Errors are `AppError` (`src/errors.rs`) with `unauthorized`/`forbidden`/`internal`/`conflict`/`invalid_request`/`not_found` constructors and the `{ error: { code, message, details } }` schema.
- **Auth principal:** protected handlers may read `axum::Extension<crate::middleware::Principal>` if they need the caller; connections/browse handlers do not need the identity (flat auth), just being reachable means authenticated.
- **Tests (`src/lib.rs`):** integration tests build `app(Arc::new(AppState::new(...)))` and drive via `tower::ServiceExt::oneshot`; existing tests authenticate with a seeded API token (`with_auth` wrapper). Connector unit tests live next to the connector and use `wiremock`.
- **Branch:** `homebox-backend`; final task merges to `main`.

---

### Task 0: Branch + dependencies
**Files:** `Cargo.toml`

- [ ] **Step 1: Branch + deps**
```bash
git checkout main && git pull && git checkout -b homebox-backend
```
Add to `Cargo.toml` `[dependencies]` (verify current minors + a quick web check on reqwest 0.12 rustls + `Client::builder` and wiremock 0.6 API):
```toml
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "json", "gzip"] }
url = "2"
hmac = "0.12"
```
(`sha2` is already a direct dep.) Add to `[dev-dependencies]`:
```toml
wiremock = "0.6"
```

- [ ] **Step 2: Build + commit**
Run: `cargo build` (expected: compiles, no usage yet).
```bash
git add Cargo.toml Cargo.lock && git commit -m "build: add reqwest(rustls), url, hmac, wiremock for the Homebox connector"
```

---

### Task 1: Hardened egress client (`src/egress.rs`)

**Files:** Create `src/egress.rs`; register `pub mod egress;` in `src/lib.rs`; Test: inline `#[cfg(test)]`.

**Interfaces:**
- Produces: `pub struct Egress`; `Egress::new() -> Egress` (production, blocks loopback); `Egress::with_loopback() -> Egress` (test-only, allows loopback so wiremock servers on 127.0.0.1 are reachable); `async fn Egress::get_json<T: DeserializeOwned>(&self, base: &url::Url, path: &str, query: &[(String, String)], bearer: &str) -> Result<T, EgressError>` (generic over the deserialized type — connectors pass typed structs, callers wanting raw JSON pass `serde_json::Value`); `pub enum EgressError { Blocked(String), Timeout, TooLarge, Status(u16), Transport(String) }`; `fn ip_allowed(ip: std::net::IpAddr, allow_loopback: bool) -> bool`.

> **Why `with_loopback`:** every wiremock-backed test in this plan (this task's success test, all of Task 4, and Task 6's endpoint happy-paths) hits a server bound to `127.0.0.1`. The production `Egress::new()` blocks loopback, so those tests MUST construct the egress with `Egress::with_loopback()`. Only the `blocks_loopback_host` negative test uses `Egress::new()`.

- [ ] **Step 1: Write failing tests**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    #[test]
    fn ip_policy_blocks_dangerous_allows_private() {
        assert!(!ip_allowed(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), false));     // loopback
        assert!(!ip_allowed(IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254)), false)); // metadata/link-local
        assert!(!ip_allowed(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), false));        // unspecified
        assert!(!ip_allowed(IpAddr::V6(Ipv6Addr::LOCALHOST), false));              // ::1
        assert!(ip_allowed(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50)), false));    // private LAN: allowed
        assert!(ip_allowed(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)), false));        // private LAN: allowed
        assert!(ip_allowed(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)), false));         // public: allowed
        // loopback IS allowed when the test override is set
        assert!(ip_allowed(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), true));
        // but the override does NOT re-enable link-local / unspecified / multicast
        assert!(!ip_allowed(IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254)), true));
        assert!(!ip_allowed(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), true));
    }

    #[tokio::test]
    async fn get_json_fetches_and_sends_bearer() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/api/v1/ping"))
            .and(wiremock::matchers::header("authorization", "Bearer hb_abc"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
            .mount(&server)
            .await;
        let base = url::Url::parse(&server.uri()).unwrap();
        let egress = Egress::with_loopback(); // wiremock is on 127.0.0.1
        let v: serde_json::Value = egress.get_json(&base, "/api/v1/ping", &[], "hb_abc").await.unwrap();
        assert_eq!(v["ok"], true);
    }

    #[tokio::test]
    async fn blocks_loopback_host() {
        // wiremock binds to 127.0.0.1; the PRODUCTION egress must Block it.
        let server = wiremock::MockServer::start().await;
        let base = url::Url::parse(&server.uri()).unwrap();
        let egress = Egress::new();
        let err = egress.get_json::<serde_json::Value>(&base, "/x", &[], "hb_abc").await.unwrap_err();
        assert!(matches!(err, EgressError::Blocked(_)));
    }
}
```

- [ ] **Step 2: Run to verify it fails** — `cargo test --lib egress::` (FAIL, module missing).

- [ ] **Step 3: Implement `src/egress.rs`**
```rust
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
    if ip.is_loopback() {
        return allow_loopback;
    }
    if ip.is_unspecified() || ip.is_multicast() {
        return false;
    }
    match ip {
        IpAddr::V4(v4) => !v4.is_link_local(),       // 169.254.0.0/16 (covers metadata)
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
        Self { client, allow_loopback }
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
            return Err(EgressError::Blocked(format!("scheme '{scheme}' not allowed")));
        }
        let host = base.host_str().ok_or_else(|| EgressError::Blocked("no host".into()))?;
        let port = base.port_or_known_default().unwrap_or(80);
        // Resolve and refuse if ANY resolved address is disallowed (conservative). NOTE: reqwest
        // re-resolves on connect, so a sub-millisecond DNS-rebind could still slip a vetted host to a
        // blocked IP. That residual TOCTOU is the accepted risk for this single-tenant authed LAN tool
        // (see the egress decision in the spec / ADR-0018); tightening it would mean a custom
        // `reqwest::dns::Resolve` resolver, deliberately out of scope here.
        let addrs = tokio::net::lookup_host((host, port))
            .await
            .map_err(|e| EgressError::Transport(format!("dns: {e}")))?;
        let mut any = false;
        for sa in addrs {
            any = true;
            if !ip_allowed(sa.ip(), self.allow_loopback) {
                return Err(EgressError::Blocked(format!("address {} not allowed", sa.ip())));
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
            .map_err(|e| if e.is_timeout() { EgressError::Timeout } else { EgressError::Transport(redact(&e.to_string())) })?;
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
        .map_err(|e| EgressError::Transport(e.to_string()))?
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
```
Register the module: add `pub mod egress;` to `src/lib.rs`. No `futures-util` dependency is needed (`read_capped` uses the built-in `Response::chunk`).

- [ ] **Step 4: Run tests** — `cargo test --lib egress::` (3 tests pass). `cargo clippy` clean.

- [ ] **Step 5: Commit**
```bash
git add src/egress.rs src/lib.rs Cargo.toml Cargo.lock
git commit -m "feat(connector): hardened egress client (IP allow-check, timeouts, size cap, no redirects)"
```

---

### Task 2: Connections store (`src/store.rs`)

**Files:** Modify `src/store.rs`; Test: inline `#[cfg(test)]`.

**Interfaces:**
- Produces: `pub struct Connection { pub id, pub connector, pub name, pub base_url, pub credential: String, pub enabled: bool }`; `Store::create_connection(&self, connector, name, base_url, credential) -> Result<Connection>`; `get_connection(&self, id) -> Result<Option<Connection>>`; `list_connections() -> Result<Vec<Connection>>`; `update_connection(&self, id, name, base_url, credential: Option<&str>, enabled) -> Result<bool>`; `delete_connection(&self, id) -> Result<bool>`.

- [ ] **Step 1: Write failing tests**
```rust
#[cfg(test)]
mod connection_tests {
    use super::*;
    fn store() -> Store { Store::open_in_memory().unwrap() }

    #[tokio::test]
    async fn connection_crud() {
        let s = store();
        let c = s.create_connection("homebox", "home", "http://hb.lan:7745", "hb_secret").await.unwrap();
        assert_eq!(c.connector, "homebox");
        assert_eq!(c.credential, "hb_secret");
        assert!(s.get_connection(&c.id).await.unwrap().is_some());
        assert_eq!(s.list_connections().await.unwrap().len(), 1);
        // update name + keep credential (None = unchanged)
        assert!(s.update_connection(&c.id, "renamed", "http://hb.lan:7745", None, true).await.unwrap());
        let g = s.get_connection(&c.id).await.unwrap().unwrap();
        assert_eq!(g.name, "renamed");
        assert_eq!(g.credential, "hb_secret"); // unchanged
        // update credential
        assert!(s.update_connection(&c.id, "renamed", "http://hb.lan:7745", Some("hb_new"), true).await.unwrap());
        assert_eq!(s.get_connection(&c.id).await.unwrap().unwrap().credential, "hb_new");
        assert!(s.delete_connection(&c.id).await.unwrap());
        assert!(s.get_connection(&c.id).await.unwrap().is_none());
    }
}
```

- [ ] **Step 2: Run to verify it fails** — `cargo test --lib store::connection_tests` (FAIL).

- [ ] **Step 3: Implement**
Append a THIRD `M::up` to the `migrations()` vec:
```rust
M::up(
    "CREATE TABLE connections (
        id         TEXT PRIMARY KEY,
        connector  TEXT NOT NULL,
        name       TEXT NOT NULL,
        base_url   TEXT NOT NULL,
        credential TEXT NOT NULL,
        enabled    INTEGER NOT NULL DEFAULT 1,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );",
),
```
Add the struct + methods (mirror the printer/user patterns):
```rust
#[derive(Debug, Clone)]
pub struct Connection {
    pub id: String,
    pub connector: String,
    pub name: String,
    pub base_url: String,
    pub credential: String,
    pub enabled: bool,
}

impl Store {
    pub async fn create_connection(&self, connector: &str, name: &str, base_url: &str, credential: &str) -> Result<Connection, StoreError> {
        let id = crate::auth::random_secret();
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO connections (id, connector, name, base_url, credential) VALUES (?1,?2,?3,?4,?5)",
            rusqlite::params![id, connector, name, base_url, credential],
        )?;
        Ok(Connection { id, connector: connector.into(), name: name.into(), base_url: base_url.into(), credential: credential.into(), enabled: true })
    }
    pub async fn get_connection(&self, id: &str) -> Result<Option<Connection>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        conn.query_row(
            "SELECT id, connector, name, base_url, credential, enabled FROM connections WHERE id = ?1",
            [id],
            row_to_connection,
        ).optional().map_err(Into::into)
    }
    pub async fn list_connections(&self) -> Result<Vec<Connection>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare("SELECT id, connector, name, base_url, credential, enabled FROM connections ORDER BY name")?;
        let rows = stmt.query_map([], row_to_connection)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
    pub async fn update_connection(&self, id: &str, name: &str, base_url: &str, credential: Option<&str>, enabled: bool) -> Result<bool, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        let n = match credential {
            Some(cred) => conn.execute(
                "UPDATE connections SET name=?1, base_url=?2, credential=?3, enabled=?4 WHERE id=?5",
                rusqlite::params![name, base_url, cred, enabled as i64, id])?,
            None => conn.execute(
                "UPDATE connections SET name=?1, base_url=?2, enabled=?3 WHERE id=?4",
                rusqlite::params![name, base_url, enabled as i64, id])?,
        };
        Ok(n > 0)
    }
    pub async fn delete_connection(&self, id: &str) -> Result<bool, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        Ok(conn.execute("DELETE FROM connections WHERE id = ?1", [id])? > 0)
    }
}

fn row_to_connection(r: &rusqlite::Row) -> rusqlite::Result<Connection> {
    Ok(Connection {
        id: r.get(0)?, connector: r.get(1)?, name: r.get(2)?, base_url: r.get(3)?,
        credential: r.get(4)?, enabled: r.get::<_, i64>(5)? != 0,
    })
}
```

- [ ] **Step 4: Run tests** — `cargo test --lib store::connection_tests` (pass) + full `cargo test --lib` (existing green).

- [ ] **Step 5: Commit**
```bash
git add src/store.rs && git commit -m "feat(connector): connections store schema + CRUD methods"
```

---

### Task 3: Connector trait, browse-model types, registry, cursor (`src/connector/`)

**Files:** Create `src/connector/mod.rs`, `src/connector/cursor.rs`; register `pub mod connector;` in `src/lib.rs`. Test: inline in `cursor.rs`.

**Interfaces:**
- Produces the browse model + trait (used by Task 4/6): see code. `ConnectorRegistry::default()`; `ConnectorRegistry::get(&self, id: &str) -> Option<&Connectors>`; `enum Connectors { Homebox(crate::connector::homebox::HomeboxConnector) }` with async `schema`/`browse`/`materialize` delegating to the inner impl. `cursor::sign(&CursorClaims) -> String`; `cursor::verify(token, expect: &CursorBinding) -> Result<CursorClaims, ConnectorError>`.

- [ ] **Step 1: Write failing test (cursor round-trip)**
In `src/connector/cursor.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cursor_round_trips_and_rejects_mismatch() {
        let key = SigningKey::random();
        let claims = CursorClaims { connector: "homebox".into(), connection: "c1".into(), resource: "entities".into(), filter_hash: "h".into(), page: 2, page_size: 50 };
        let token = sign(&key, &claims);
        let bind = CursorBinding { connector: "homebox", connection: "c1", resource: "entities", filter_hash: "h" };
        let back = verify(&key, &token, &bind).unwrap();
        assert_eq!(back.page, 2);
        // wrong connection -> rejected
        let bad = CursorBinding { connector: "homebox", connection: "OTHER", resource: "entities", filter_hash: "h" };
        assert!(verify(&key, &token, &bad).is_err());
        // tampered token -> rejected
        assert!(verify(&key, &(token + "x"), &bind).is_err());
    }
}
```

- [ ] **Step 2: Run to verify it fails** — `cargo test --lib connector::cursor` (FAIL).

- [ ] **Step 3: Implement `src/connector/cursor.rs`** (HMAC-SHA256 over the JSON claims; key is process-random so cursors do not survive a restart, which is fine, the UI re-browses):
```rust
use base64::Engine;
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;

use super::ConnectorError;

type HmacSha256 = Hmac<Sha256>;

/// Process-lifetime signing key for browse cursors (regenerated each start).
#[derive(Clone)]
pub struct SigningKey([u8; 32]);
impl SigningKey {
    pub fn random() -> Self {
        let mut k = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut k);
        Self(k)
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct CursorClaims {
    pub connector: String,
    pub connection: String,
    pub resource: String,
    pub filter_hash: String,
    pub page: u32,
    pub page_size: u32,
}

pub struct CursorBinding<'a> {
    pub connector: &'a str,
    pub connection: &'a str,
    pub resource: &'a str,
    pub filter_hash: &'a str,
}

const B64: base64::engine::general_purpose::GeneralPurpose = base64::engine::general_purpose::URL_SAFE_NO_PAD;

pub fn sign(key: &SigningKey, claims: &CursorClaims) -> String {
    let body = serde_json::to_vec(claims).expect("serialize cursor");
    let mut mac = HmacSha256::new_from_slice(&key.0).expect("hmac key");
    mac.update(&body);
    let tag = mac.finalize().into_bytes();
    format!("{}.{}", B64.encode(&body), B64.encode(tag))
}

pub fn verify(key: &SigningKey, token: &str, bind: &CursorBinding) -> Result<CursorClaims, ConnectorError> {
    let (b, t) = token.split_once('.').ok_or_else(|| ConnectorError::InvalidFilter("bad cursor".into()))?;
    let body = B64.decode(b).map_err(|_| ConnectorError::InvalidFilter("bad cursor".into()))?;
    let tag = B64.decode(t).map_err(|_| ConnectorError::InvalidFilter("bad cursor".into()))?;
    let mut mac = HmacSha256::new_from_slice(&key.0).expect("hmac key");
    mac.update(&body);
    mac.verify_slice(&tag).map_err(|_| ConnectorError::InvalidFilter("cursor signature".into()))?;
    let claims: CursorClaims = serde_json::from_slice(&body).map_err(|_| ConnectorError::InvalidFilter("bad cursor".into()))?;
    if claims.connector != bind.connector || claims.connection != bind.connection
        || claims.resource != bind.resource || claims.filter_hash != bind.filter_hash {
        return Err(ConnectorError::InvalidFilter("cursor does not match request".into()));
    }
    Ok(claims)
}
```

- [ ] **Step 4: Implement `src/connector/mod.rs`** (the browse model + trait + registry; `pub mod cursor;` and `pub mod homebox;` declared here):
```rust
pub mod cursor;
pub mod homebox;

use std::collections::BTreeMap;

use crate::egress::Egress;
use crate::store::Connection;

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum View { Table, Tree }

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType { Text, Number, Money, Date, Badge }

#[derive(serde::Serialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Tier { Cheap, Hydrated, Derived }

#[derive(serde::Serialize)]
pub struct FieldSpec { pub key: String, pub label: String, pub ty: FieldType, pub tier: Tier }

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterType { Search, LocationId, LabelId }

#[derive(serde::Serialize)]
pub struct FilterSpec { pub key: String, pub label: String, pub ty: FilterType }

#[derive(serde::Serialize)]
pub struct ResourceSpec { pub id: String, pub label: String, pub view: View, pub columns: Vec<FieldSpec>, pub filters: Vec<FilterSpec> }

#[derive(serde::Serialize)]
pub struct RelationshipSpec { pub id: String, pub label: String, pub from: String, pub to: String }

#[derive(serde::Serialize)]
pub struct ConnectorSchema { pub version: String, pub resources: Vec<ResourceSpec>, pub relationships: Vec<RelationshipSpec> }

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct RowRef { pub resource: String, pub key: String }

#[derive(serde::Serialize)]
#[serde(untagged)]
pub enum CellValue { Text(String), Number(f64) }

#[derive(serde::Serialize)]
pub struct DisplayRow { pub id: RowRef, pub cells: BTreeMap<String, CellValue> }

#[derive(serde::Deserialize)]
pub struct BrowseParent { pub relationship: String, pub key: String } // Direct mode only for Homebox

#[derive(serde::Deserialize)]
pub struct BrowseRequest {
    pub resource: String,
    #[serde(default)] pub filters: BTreeMap<String, String>,
    #[serde(default)] pub parent: Option<BrowseParent>,
    #[serde(default)] pub cursor: Option<String>,
    #[serde(default)] pub page_size: Option<u32>,
}

#[derive(serde::Serialize)]
pub struct BrowsePage { pub rows: Vec<DisplayRow>, pub next_cursor: Option<String>, pub has_more: bool, pub count: Option<u64> }

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpansionPolicy { AsListed }

#[derive(serde::Deserialize)]
pub struct MaterializeRequest { pub rows: Vec<RowRef>, pub fields: Vec<String>, pub expansion: ExpansionPolicy }

#[derive(serde::Serialize)]
pub struct LabelRow { pub source: RowRef, pub data: BTreeMap<String, String> }

#[derive(Debug)]
pub enum ConnectorError {
    AuthFailed,
    Forbidden,
    ConnectionFailed(String),
    InvalidFilter(String),
    UpstreamSchemaMismatch(String),
    RateLimited,
    BudgetExceeded,
    Upstream(String),
}

impl From<crate::egress::EgressError> for ConnectorError {
    fn from(e: crate::egress::EgressError) -> Self {
        use crate::egress::EgressError::*;
        match e {
            Status(401) | Status(403) => ConnectorError::AuthFailed,
            Status(429) => ConnectorError::RateLimited,
            Blocked(m) => ConnectorError::ConnectionFailed(m),
            Timeout => ConnectorError::ConnectionFailed("timeout".into()),
            TooLarge => ConnectorError::Upstream("response too large".into()),
            Status(s) => ConnectorError::Upstream(format!("upstream status {s}")),
            Transport(m) => ConnectorError::ConnectionFailed(m),
        }
    }
}

/// Static-dispatch registry (one connector for now). Avoids `dyn` + async-trait; add arms for more.
pub enum Connectors { Homebox(homebox::HomeboxConnector) }

impl Connectors {
    pub async fn schema(&self, conn: &Connection, egress: &Egress) -> Result<ConnectorSchema, ConnectorError> {
        match self { Connectors::Homebox(c) => c.schema(conn, egress).await }
    }
    pub async fn browse(&self, conn: &Connection, egress: &Egress, key: &cursor::SigningKey, req: BrowseRequest) -> Result<BrowsePage, ConnectorError> {
        match self { Connectors::Homebox(c) => c.browse(conn, egress, key, req).await }
    }
    pub async fn materialize(&self, conn: &Connection, egress: &Egress, req: MaterializeRequest) -> Result<Vec<LabelRow>, ConnectorError> {
        match self { Connectors::Homebox(c) => c.materialize(conn, egress, req).await }
    }
}

pub struct ConnectorRegistry { homebox: Connectors }
impl Default for ConnectorRegistry {
    fn default() -> Self { Self { homebox: Connectors::Homebox(homebox::HomeboxConnector::default()) } }
}
impl ConnectorRegistry {
    pub fn get(&self, id: &str) -> Option<&Connectors> {
        match id { "homebox" => Some(&self.homebox), _ => None }
    }
}
```
Register `pub mod connector;` in `src/lib.rs`.

> **REQUIRED for Task 7 (OpenAPI):** add `utoipa::ToSchema` to the derive list of every type above that is serialized into an API response or deserialized from a request body, because Task 7 registers them in `openapi.rs` and a `ToSchema` type may only reference other `ToSchema` types. Concretely, change `#[derive(serde::Serialize)]` → `#[derive(serde::Serialize, utoipa::ToSchema)]` (and `Deserialize` variants likewise) on: `View`, `FieldType`, `Tier`, `FieldSpec`, `FilterType`, `FilterSpec`, `ResourceSpec`, `RelationshipSpec`, `ConnectorSchema`, `RowRef`, `CellValue`, `DisplayRow`, `BrowseParent`, `BrowseRequest`, `BrowsePage`, `ExpansionPolicy`, `MaterializeRequest`, `LabelRow`. (`CellValue` is `#[serde(untagged)]`; utoipa derives an untagged `oneOf` for it, which is correct.) Do NOT derive `ToSchema` on `ConnectorError` (internal, never serialized) or the `cursor` types.

- [ ] **Step 5: Run** — `cargo test --lib connector::cursor` (pass), `cargo build`. The `homebox` module is referenced by the registry, so create a minimal compiling stub now (Task 4 replaces it). The stub method signatures MUST match what `Connectors` calls in mod.rs:
```rust
// src/connector/homebox.rs (stub; replaced in Task 4)
use super::{BrowsePage, BrowseRequest, ConnectorError, ConnectorSchema, LabelRow, MaterializeRequest};
use super::cursor::SigningKey;
use crate::egress::Egress;
use crate::store::Connection;

#[derive(Default)]
pub struct HomeboxConnector;

impl HomeboxConnector {
    pub async fn schema(&self, _conn: &Connection, _egress: &Egress) -> Result<ConnectorSchema, ConnectorError> {
        Err(ConnectorError::Upstream("unimplemented".into()))
    }
    pub async fn browse(&self, _conn: &Connection, _egress: &Egress, _key: &SigningKey, _req: BrowseRequest) -> Result<BrowsePage, ConnectorError> {
        Err(ConnectorError::Upstream("unimplemented".into()))
    }
    pub async fn materialize(&self, _conn: &Connection, _egress: &Egress, _req: MaterializeRequest) -> Result<Vec<LabelRow>, ConnectorError> {
        Err(ConnectorError::Upstream("unimplemented".into()))
    }
}
```
(Returns a real `Err`, never `todo!()`, so the build is green and harmless until Task 4.)

- [ ] **Step 6: Commit**
```bash
git add src/connector/ src/lib.rs
git commit -m "feat(connector): Connector trait, browse-model types, registry, signed cursors"
```

---

### Task 4: Homebox connector (`src/connector/homebox.rs`)

**Files:** Replace the stub `src/connector/homebox.rs`; Test: inline `#[cfg(test)]` with `wiremock`.

**Interfaces:**
- Consumes: `Egress::get_json`, the browse-model types, `cursor::{sign, verify, SigningKey, CursorClaims, CursorBinding}`, `store::Connection`.
- Produces: `HomeboxConnector` with `schema`/`browse`/`materialize` (signatures as called by `Connectors`).

- [ ] **Step 1: Write failing tests** (wiremock Homebox; fixtures shaped like `repo.EntitySummary`):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Connection;
    use wiremock::matchers::{method, path, header};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn conn(base: &str) -> Connection {
        Connection { id: "c1".into(), connector: "homebox".into(), name: "h".into(), base_url: base.into(), credential: "hb_key".into(), enabled: true }
    }

    #[tokio::test]
    async fn browse_sends_bearer_and_maps_rows() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/api/v1/entities"))
            .and(header("authorization", "Bearer hb_key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {"id":"e1","name":"Drill","description":"","entityType":{"name":"item"},"assetId":"000-001","quantity":1},
                    {"id":"e2","name":"Shelf","entityType":{"name":"location"}}
                ],
                "total": 2
            })))
            .mount(&server).await;
        let egress = crate::egress::Egress::with_loopback(); // wiremock on 127.0.0.1
        let key = crate::connector::cursor::SigningKey::random();
        let c = HomeboxConnector::default();
        let page = c.browse(&conn(&server.uri()), &egress, &key, crate::connector::BrowseRequest{
            resource: "entities".into(), filters: Default::default(), parent: None, cursor: None, page_size: Some(50),
        }).await.unwrap();
        assert_eq!(page.rows.len(), 2);
        assert_eq!(page.rows[0].id.key, "e1");
    }

    #[tokio::test]
    async fn auth_failure_maps_to_authfailed() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(401)).mount(&server).await;
        let egress = crate::egress::Egress::with_loopback(); // wiremock on 127.0.0.1
        let key = crate::connector::cursor::SigningKey::random();
        let err = HomeboxConnector::default().browse(&conn(&server.uri()), &egress, &key, crate::connector::BrowseRequest{
            resource:"entities".into(), filters:Default::default(), parent:None, cursor:None, page_size:None,
        }).await.unwrap_err();
        assert!(matches!(err, crate::connector::ConnectorError::AuthFailed));
    }

    #[tokio::test]
    async fn schema_discovers_custom_fields() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/api/v1/entities/fields"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!(["Calibration Date","Internal SKU"])))
            .mount(&server).await;
        let egress = crate::egress::Egress::with_loopback(); // wiremock on 127.0.0.1
        let s = HomeboxConnector::default().schema(&conn(&server.uri()), &egress).await.unwrap();
        let entities = s.resources.iter().find(|r| r.id == "entities").unwrap();
        assert!(entities.columns.iter().any(|f| f.label == "Calibration Date"));
    }

    #[tokio::test]
    async fn materialize_hydrates_selected_fields() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/api/v1/entities/e1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id":"e1","name":"Drill","manufacturer":"Acme","serialNumber":"SN9","entityType":{"name":"item"}
            })))
            .mount(&server).await;
        let egress = crate::egress::Egress::with_loopback(); // wiremock on 127.0.0.1
        let rows = HomeboxConnector::default().materialize(&conn(&server.uri()), &egress, crate::connector::MaterializeRequest{
            rows: vec![crate::connector::RowRef{resource:"entities".into(), key:"e1".into()}],
            fields: vec!["name".into(),"manufacturer".into(),"item_url".into()],
            expansion: crate::connector::ExpansionPolicy::AsListed,
        }).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].data["manufacturer"], "Acme");
        assert!(rows[0].data["item_url"].ends_with("/entity/e1"));
    }
}
```

- [ ] **Step 2: Run to verify it fails** — `cargo test --lib connector::homebox` (FAIL: stub returns errors).

- [ ] **Step 3: Implement `src/connector/homebox.rs`**
```rust
use std::collections::BTreeMap;

use url::Url;

use super::cursor::{self, CursorBinding, CursorClaims, SigningKey};
use super::{
    BrowsePage, BrowseRequest, CellValue, ConnectorError, ConnectorSchema, DisplayRow, FieldSpec,
    FieldType, FilterSpec, FilterType, LabelRow, MaterializeRequest, RelationshipSpec, ResourceSpec,
    RowRef, Tier, View,
};
use crate::egress::Egress;
use crate::store::Connection;

#[derive(Default)]
pub struct HomeboxConnector;

const PAGE_DEFAULT: u32 = 50;
const MATERIALIZE_CAP: usize = 200;

fn base(conn: &Connection) -> Result<Url, ConnectorError> {
    Url::parse(&conn.base_url).map_err(|_| ConnectorError::ConnectionFailed("invalid base_url".into()))
}

impl HomeboxConnector {
    pub async fn schema(&self, conn: &Connection, egress: &Egress) -> Result<ConnectorSchema, ConnectorError> {
        // Static cheap/hydrated fields + dynamic custom fields from /v1/entities/fields.
        let mut columns = vec![
            field("name", "Name", FieldType::Text, Tier::Cheap),
            field("description", "Description", FieldType::Text, Tier::Cheap),
            field("entityType", "Type", FieldType::Badge, Tier::Cheap),
            field("assetId", "Asset ID", FieldType::Text, Tier::Cheap),
            field("quantity", "Quantity", FieldType::Number, Tier::Cheap),
            field("purchasePrice", "Price", FieldType::Money, Tier::Cheap),
            field("location", "Location", FieldType::Text, Tier::Cheap),
            field("manufacturer", "Manufacturer", FieldType::Text, Tier::Hydrated),
            field("modelNumber", "Model", FieldType::Text, Tier::Hydrated),
            field("serialNumber", "Serial", FieldType::Text, Tier::Hydrated),
            field("item_url", "Homebox URL", FieldType::Text, Tier::Derived),
        ];
        let b = base(conn)?;
        let custom: Vec<String> = egress
            .get_json(&b, "/api/v1/entities/fields", &[], &conn.credential)
            .await
            .unwrap_or_default();
        for name in custom {
            columns.push(field(&format!("custom:{name}"), &name, FieldType::Text, Tier::Hydrated));
        }
        Ok(ConnectorSchema {
            version: "homebox-1".into(),
            resources: vec![
                ResourceSpec {
                    id: "entities".into(), label: "Items & Locations".into(), view: View::Table,
                    columns,
                    filters: vec![
                        FilterSpec { key: "q".into(), label: "Search".into(), ty: FilterType::Search },
                        FilterSpec { key: "parent".into(), label: "Location".into(), ty: FilterType::LocationId },
                        FilterSpec { key: "tag".into(), label: "Label".into(), ty: FilterType::LabelId },
                    ],
                },
                ResourceSpec {
                    id: "locations".into(), label: "Locations".into(), view: View::Tree,
                    columns: vec![
                        field("name", "Name", FieldType::Text, Tier::Cheap),
                        field("description", "Description", FieldType::Text, Tier::Cheap),
                        field("itemCount", "Items", FieldType::Number, Tier::Cheap),
                        field("location_url", "Homebox URL", FieldType::Text, Tier::Derived),
                    ],
                    filters: vec![],
                },
            ],
            relationships: vec![RelationshipSpec {
                id: "location_children".into(), label: "Contents".into(), from: "locations".into(), to: "entities".into(),
            }],
        })
    }

    pub async fn browse(&self, conn: &Connection, egress: &Egress, key: &SigningKey, req: BrowseRequest) -> Result<BrowsePage, ConnectorError> {
        let b = base(conn)?;
        let page_size = req.page_size.unwrap_or(PAGE_DEFAULT).min(200);
        let filter_hash = hash_filters(&req);
        // page comes from the cursor (bound) or starts at 1.
        let page = match &req.cursor {
            Some(tok) => cursor::verify(key, tok, &CursorBinding {
                connector: "homebox", connection: &conn.id, resource: &req.resource, filter_hash: &filter_hash,
            })?.page,
            None => 1,
        };

        if req.resource == "locations" {
            // tree: a single page of the location hierarchy (flattened to display rows).
            let tree: serde_json::Value = egress.get_json(&b, "/api/v1/entities/tree", &[("withItems".into(), "false".into())], &conn.credential).await?;
            let rows = flatten_tree(&tree);
            return Ok(BrowsePage { rows, next_cursor: None, has_more: false, count: None });
        }

        // entities: q / tags / parentIds + page / pageSize.
        let mut query: Vec<(String, String)> = vec![
            ("page".into(), page.to_string()),
            ("pageSize".into(), page_size.to_string()),
        ];
        if let Some(q) = req.filters.get("q") { query.push(("q".into(), q.clone())); }
        if let Some(tag) = req.filters.get("tag") { query.push(("tags".into(), tag.clone())); }
        // parent comes from a drill-down (BrowseParent) or the `parent` filter.
        if let Some(p) = req.parent.as_ref() { query.push(("parentIds".into(), p.key.clone())); }
        else if let Some(p) = req.filters.get("parent") { query.push(("parentIds".into(), p.clone())); }

        let resp: EntityList = egress.get_json(&b, "/api/v1/entities", &query, &conn.credential).await?;
        let rows: Vec<DisplayRow> = resp.items.iter().map(summary_to_row).collect();
        let total = resp.total.unwrap_or(0);
        let has_more = (page as u64) * (page_size as u64) < total;
        let next_cursor = has_more.then(|| cursor::sign(key, &CursorClaims {
            connector: "homebox".into(), connection: conn.id.clone(), resource: req.resource.clone(),
            filter_hash, page: page + 1, page_size,
        }));
        Ok(BrowsePage { rows, next_cursor, has_more, count: Some(total) })
    }

    pub async fn materialize(&self, conn: &Connection, egress: &Egress, req: MaterializeRequest) -> Result<Vec<LabelRow>, ConnectorError> {
        if req.rows.len() > MATERIALIZE_CAP {
            return Err(ConnectorError::BudgetExceeded);
        }
        let b = base(conn)?;
        let mut out = Vec::with_capacity(req.rows.len());
        for r in &req.rows {
            let detail: serde_json::Value = egress.get_json(&b, &format!("/api/v1/entities/{}", r.key), &[], &conn.credential).await?;
            let mut data = BTreeMap::new();
            for f in &req.fields {
                data.insert(f.clone(), extract_field(&detail, f, &conn.base_url, &r.key));
            }
            out.push(LabelRow { source: r.clone(), data });
        }
        Ok(out)
    }
}

#[derive(serde::Deserialize)]
struct EntityList { items: Vec<EntitySummary>, total: Option<u64> }

#[derive(serde::Deserialize)]
struct EntitySummary {
    id: String,
    name: Option<String>,
    #[serde(default)] description: Option<String>,
    #[serde(default, rename = "assetId")] asset_id: Option<String>,
    #[serde(default)] quantity: Option<f64>,
    #[serde(default, rename = "entityType")] entity_type: Option<serde_json::Value>,
    #[serde(default)] parent: Option<serde_json::Value>,
}

fn field(key: &str, label: &str, ty: FieldType, tier: Tier) -> FieldSpec {
    FieldSpec { key: key.into(), label: label.into(), ty, tier }
}

fn summary_to_row(e: &EntitySummary) -> DisplayRow {
    let mut cells = BTreeMap::new();
    cells.insert("name".into(), CellValue::Text(e.name.clone().unwrap_or_default()));
    cells.insert("description".into(), CellValue::Text(e.description.clone().unwrap_or_default()));
    cells.insert("assetId".into(), CellValue::Text(e.asset_id.clone().unwrap_or_default()));
    if let Some(q) = e.quantity { cells.insert("quantity".into(), CellValue::Number(q)); }
    cells.insert("entityType".into(), CellValue::Text(type_name(&e.entity_type)));
    cells.insert("location".into(), CellValue::Text(json_name(&e.parent)));
    DisplayRow { id: RowRef { resource: "entities".into(), key: e.id.clone() }, cells }
}

fn type_name(v: &Option<serde_json::Value>) -> String {
    v.as_ref().and_then(|t| t.get("name")).and_then(|n| n.as_str()).unwrap_or("").to_string()
}
fn json_name(v: &Option<serde_json::Value>) -> String {
    v.as_ref().and_then(|t| t.get("name")).and_then(|n| n.as_str()).unwrap_or("").to_string()
}

fn extract_field(detail: &serde_json::Value, key: &str, base_url: &str, id: &str) -> String {
    match key {
        "item_url" | "location_url" => format!("{}/entity/{}", base_url.trim_end_matches('/'), id),
        "location" => json_name(&detail.get("parent").cloned()),
        "entityType" => type_name(&detail.get("entityType").cloned()),
        k if k.starts_with("custom:") => {
            let want = &k["custom:".len()..];
            detail.get("fields").and_then(|f| f.as_array()).and_then(|arr| {
                arr.iter().find(|f| f.get("name").and_then(|n| n.as_str()) == Some(want))
                    .and_then(|f| f.get("textValue").or_else(|| f.get("value")))
                    .and_then(|v| v.as_str()).map(|s| s.to_string())
            }).unwrap_or_default()
        }
        _ => match detail.get(key) {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(serde_json::Value::Number(n)) => n.to_string(),
            _ => String::new(),
        },
    }
}

fn flatten_tree(tree: &serde_json::Value) -> Vec<DisplayRow> {
    fn walk(node: &serde_json::Value, out: &mut Vec<DisplayRow>) {
        if let (Some(id), name) = (node.get("id").and_then(|v| v.as_str()), node.get("name").and_then(|v| v.as_str())) {
            let mut cells = BTreeMap::new();
            cells.insert("name".into(), CellValue::Text(name.unwrap_or("").to_string()));
            out.push(DisplayRow { id: RowRef { resource: "locations".into(), key: id.to_string() }, cells });
        }
        if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
            for ch in children { walk(ch, out); }
        }
    }
    let mut out = Vec::new();
    if let Some(arr) = tree.as_array() { for n in arr { walk(n, &mut out); } }
    out
}

fn hash_filters(req: &BrowseRequest) -> String {
    let parent = req.parent.as_ref().map(|p| p.key.as_str()).unwrap_or("");
    let mut parts: Vec<String> = req.filters.iter().map(|(k, v)| format!("{k}={v}")).collect();
    parts.sort();
    crate::auth::sha256_hex(&format!("{}|{}|{}", req.resource, parent, parts.join("&")))
}
```
The `{ items, total }` envelope and the `/v1/entities/fields` string-array shape are VERIFIED against the swagger (see Global Constraints), so `EntityList` and the `Vec<String>` custom-field parse are correct as written. `#[allow(...)]` is NOT permitted: the `EntitySummary` struct above already uses snake_case Rust fields with `#[serde(rename = ...)]` for `assetId`/`entityType` to avoid a `non_snake_case` clippy warning, while the emitted cell keys stay `"assetId"`/`"entityType"` to match the schema column keys.

- [ ] **Step 4: Run tests** — `cargo test --lib connector::homebox` (4 tests pass). `cargo clippy` clean.

- [ ] **Step 5: Commit**
```bash
git add src/connector/homebox.rs
git commit -m "feat(connector): Homebox connector (entities browse + tree + custom-field schema + materialize)"
```

---

### Task 5: Connections CRUD endpoints (`src/api.rs`)

**Files:** Modify `src/api.rs` (handlers + routes + `AppState` fields), `src/lib.rs` (tests).

**Interfaces:**
- Consumes: `store` connection methods; `Egress`, `ConnectorRegistry` on `AppState`.
- Produces: routes `/connections` (GET list, POST create) and `/connections/{id}` (GET, PUT, DELETE). Response shape `{ id, connector, name, base_url, enabled, has_credential }` (credential REDACTED, never returned).

- [ ] **Step 1: Add `egress` + `connectors` + cursor key to `AppState`**
In `AppState`, add fields `egress: std::sync::Arc<crate::egress::Egress>`, `connectors: crate::connector::ConnectorRegistry`, `cursor_key: crate::connector::cursor::SigningKey`, and build them in `new()`:
```rust
egress: std::sync::Arc::new(crate::egress::Egress::new()),
connectors: crate::connector::ConnectorRegistry::default(),
cursor_key: crate::connector::cursor::SigningKey::random(),
```
Add getters `pub fn egress(&self) -> &crate::egress::Egress`, `pub fn connectors(&self) -> &crate::connector::ConnectorRegistry`, `pub fn cursor_key(&self) -> &crate::connector::cursor::SigningKey`. Also add a test-only builder so Task 6's endpoint happy-path tests can reach a wiremock server on 127.0.0.1:
```rust
#[cfg(test)]
pub fn with_loopback_egress(mut self) -> Self {
    self.egress = std::sync::Arc::new(crate::egress::Egress::with_loopback());
    self
}
```
(Call it as `Arc::new(AppState::new(...).with_loopback_egress())` before wrapping in `Arc`.)

- [ ] **Step 2: Write failing tests** (auth'd; credential never returned)
Add to `src/lib.rs`: authenticated `POST /api/connections {connector:"homebox",name,base_url,credential}` → 201 with body that has `has_credential:true` and NO `credential` field; `GET /api/connections` lists it (no credential); `PUT` updates; `DELETE` → 204. (Use the cookie/login or seeded-token helper.)

- [ ] **Step 3: Implement handlers + routes**
```rust
#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct ConnectionInput { pub connector: String, pub name: String, pub base_url: String, pub credential: Option<String>, #[serde(default = "default_true")] pub enabled: bool }
fn default_true() -> bool { true }

fn connection_view(c: &crate::store::Connection) -> serde_json::Value {
    serde_json::json!({ "id": c.id, "connector": c.connector, "name": c.name, "base_url": c.base_url, "enabled": c.enabled, "has_credential": !c.credential.is_empty() })
}

pub async fn list_connections(State(state): State<Arc<AppState>>) -> Result<Response, AppError> {
    let cs = state.store().list_connections().await.map_err(AppError::from)?;
    Ok(Json(cs.iter().map(connection_view).collect::<Vec<_>>()).into_response())
}
pub async fn create_connection(State(state): State<Arc<AppState>>, Json(body): Json<ConnectionInput>) -> Result<Response, AppError> {
    if state.connectors().get(&body.connector).is_none() { return Err(AppError::invalid_request("unknown connector")); }
    let cred = body.credential.unwrap_or_default();
    if cred.is_empty() { return Err(AppError::invalid_request("credential required")); }
    url::Url::parse(&body.base_url).map_err(|_| AppError::invalid_request("invalid base_url"))?;
    let _g = state.write_lock.lock().await;
    let c = state.store().create_connection(&body.connector, &body.name, &body.base_url, &cred).await.map_err(AppError::from)?;
    Ok((StatusCode::CREATED, Json(connection_view(&c))).into_response())
}
pub async fn get_connection_h(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Response, AppError> {
    let c = state.store().get_connection(&id).await.map_err(AppError::from)?.ok_or_else(|| AppError::not_found(&id))?;
    Ok(Json(connection_view(&c)).into_response())
}
pub async fn update_connection_h(State(state): State<Arc<AppState>>, Path(id): Path<String>, Json(body): Json<ConnectionInput>) -> Result<Response, AppError> {
    let _g = state.write_lock.lock().await;
    // credential: Some(non-empty) updates; None/empty keeps the existing one.
    let cred = body.credential.filter(|c| !c.is_empty());
    let ok = state.store().update_connection(&id, &body.name, &body.base_url, cred.as_deref(), body.enabled).await.map_err(AppError::from)?;
    if !ok { return Err(AppError::not_found(&id)); }
    let c = state.store().get_connection(&id).await.map_err(AppError::from)?.unwrap();
    Ok(Json(connection_view(&c)).into_response())
}
pub async fn delete_connection_h(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Response, AppError> {
    let _g = state.write_lock.lock().await;
    if !state.store().delete_connection(&id).await.map_err(AppError::from)? { return Err(AppError::not_found(&id)); }
    Ok(StatusCode::NO_CONTENT.into_response())
}
```
Routes in `api_router()`:
```rust
.route("/connections", get(list_connections).post(create_connection))
.route("/connections/{id}", get(get_connection_h).put(update_connection_h).delete(delete_connection_h))
```
(Fully-qualified `url::Url::parse` is used above; no extra `use` needed.)

- [ ] **Step 4: Run tests** — `cargo test --lib` (all pass; credential never appears in responses).

- [ ] **Step 5: Commit**
```bash
git add src/api.rs src/lib.rs
git commit -m "feat(connector): connections CRUD endpoints (credential redacted)"
```

---

### Task 6: Browse endpoints (`src/api.rs`)

**Files:** Modify `src/api.rs`, `src/lib.rs`.

**Interfaces:**
- Produces routes `/connections/{id}/schema` (GET), `/connections/{id}/browse` (POST), `/connections/{id}/materialize` (POST). They load the connection, pick the connector, and call schema/browse/materialize with the shared egress + cursor key. `ConnectorError` maps to HTTP via a helper.

- [ ] **Step 1: Write failing tests** (full HTTP path; the app is built with `with_loopback_egress()` so the real egress can reach a wiremock Homebox on 127.0.0.1). Build the app, seed a connection row pointing at the wiremock base, then drive the endpoints with `oneshot` + the seeded-token auth (`with_auth`). Cover BOTH happy and error paths:

```rust
#[tokio::test]
async fn browse_endpoint_returns_rows_e2e() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let hb = MockServer::start().await;
    Mock::given(method("GET")).and(path("/api/v1/entities"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [{"id":"e1","name":"Drill","entityType":{"name":"item"}}], "total": 1
        })))
        .mount(&hb).await;

    let state = std::sync::Arc::new(
        crate::api::AppState::new(/* ...same args other tests use... */).with_loopback_egress(),
    );
    // seed a connection row directly via the store, then exercise the HTTP endpoint:
    let c = state.store().create_connection("homebox", "h", &hb.uri(), "hb_key").await.unwrap();
    let app = crate::api::app(state.clone());

    let resp = with_auth(&state, Request::builder()
        .method("POST")
        .uri(format!("/api/connections/{}/browse", c.id))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"resource":"entities"}"#)).unwrap()).await;
    let resp = app.oneshot(resp).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = json_body(resp).await;
    assert_eq!(body["rows"][0]["id"]["key"], "e1");
}

#[tokio::test]
async fn schema_endpoint_e2e() {
    // wiremock returns the custom-fields array; assert /schema 200 lists the `entities` resource.
    // (mock GET /api/v1/entities/fields -> ["SKU"]; build app with with_loopback_egress; seed conn; GET /api/connections/{id}/schema)
}

#[tokio::test]
async fn materialize_endpoint_e2e() {
    // mock GET /api/v1/entities/e1 -> {id,name,manufacturer}; POST /api/connections/{id}/materialize
    // {"rows":[{"resource":"entities","key":"e1"}],"fields":["name","manufacturer"],"expansion":"as_listed"} -> 200, data.manufacturer present.
}

#[tokio::test]
async fn browse_requires_auth() {
    // POST /api/connections/{id}/browse with NO auth header -> 401 (require_auth gate).
}

#[tokio::test]
async fn browse_unknown_connection_404() {
    // POST /api/connections/does-not-exist/browse (authed) -> 404.
}

#[tokio::test]
async fn browse_foreign_cursor_400() {
    // authed browse with a `cursor` string that fails HMAC verify -> 400 InvalidFilter.
}
```
Flesh out the three sketched `e2e` bodies in the same shape as `browse_endpoint_returns_rows_e2e` (mock the relevant Homebox path, build the app with `with_loopback_egress`, seed the connection, assert status + body). Match the existing test helpers in `src/lib.rs` (`with_auth`, the `AppState::new(...)` argument list, and however the suite reads a JSON body. reuse that helper, named `json_body` here as a placeholder).

- [ ] **Step 2: Run to verify fail.**

- [ ] **Step 3: Implement**
```rust
fn connector_status(e: &crate::connector::ConnectorError) -> (StatusCode, &'static str, String) {
    use crate::connector::ConnectorError::*;
    match e {
        AuthFailed => (StatusCode::BAD_GATEWAY, "ConnectorAuthFailed", "upstream authentication failed".into()),
        Forbidden => (StatusCode::BAD_GATEWAY, "ConnectorForbidden", "upstream forbidden".into()),
        ConnectionFailed(m) => (StatusCode::BAD_GATEWAY, "ConnectorUnreachable", m.clone()),
        InvalidFilter(m) => (StatusCode::BAD_REQUEST, "InvalidFilter", m.clone()),
        UpstreamSchemaMismatch(m) => (StatusCode::BAD_GATEWAY, "UpstreamSchemaMismatch", m.clone()),
        RateLimited => (StatusCode::TOO_MANY_REQUESTS, "RateLimited", "upstream rate limited".into()),
        BudgetExceeded => (StatusCode::BAD_REQUEST, "BudgetExceeded", "too many rows requested".into()),
        Upstream(m) => (StatusCode::BAD_GATEWAY, "Upstream", m.clone()),
    }
}
fn connector_err(e: crate::connector::ConnectorError) -> AppError {
    let (status, code, msg) = connector_status(&e);
    AppError::with_status(status, code, msg) // add this constructor to errors.rs if missing
}

async fn load_conn_and_connector<'a>(state: &'a AppState, id: &str) -> Result<(crate::store::Connection, &'a crate::connector::Connectors), AppError> {
    let conn = state.store().get_connection(id).await.map_err(AppError::from)?.ok_or_else(|| AppError::not_found(id))?;
    let c = state.connectors().get(&conn.connector).ok_or_else(|| AppError::invalid_request("unknown connector"))?;
    Ok((conn, c))
}

pub async fn connection_schema(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Response, AppError> {
    let (conn, c) = load_conn_and_connector(&state, &id).await?;
    let schema = c.schema(&conn, state.egress()).await.map_err(connector_err)?;
    Ok(Json(schema).into_response())
}
pub async fn connection_browse(State(state): State<Arc<AppState>>, Path(id): Path<String>, Json(req): Json<crate::connector::BrowseRequest>) -> Result<Response, AppError> {
    let (conn, c) = load_conn_and_connector(&state, &id).await?;
    let page = c.browse(&conn, state.egress(), state.cursor_key(), req).await.map_err(connector_err)?;
    Ok(Json(page).into_response())
}
pub async fn connection_materialize(State(state): State<Arc<AppState>>, Path(id): Path<String>, Json(req): Json<crate::connector::MaterializeRequest>) -> Result<Response, AppError> {
    let (conn, c) = load_conn_and_connector(&state, &id).await?;
    let rows = c.materialize(&conn, state.egress(), req).await.map_err(connector_err)?;
    Ok(Json(rows).into_response())
}
```
Routes:
```rust
.route("/connections/{id}/schema", get(connection_schema))
.route("/connections/{id}/browse", post(connection_browse))
.route("/connections/{id}/materialize", post(connection_materialize))
```
Add `AppError::with_status(status, code, msg)` to `src/errors.rs` if not present.

- [ ] **Step 4: Run tests** — `cargo test --lib` (all pass).

- [ ] **Step 5: Commit**
```bash
git add src/api.rs src/lib.rs src/errors.rs
git commit -m "feat(connector): schema/browse/materialize endpoints"
```

---

### Task 7: OpenAPI + docs + review + integrate

**Files:** `src/openapi.rs`, `docs/adr/0018-api-integration-spine.md`, `docs/adr/README.md`, `docs/SPEC.md`, `docs/superpowers/plans/2026-06-16-homebox-backend.md` (mark done).

- [ ] **Step 1: OpenAPI** — register the connection + browse routes and their request/response schemas in `src/openapi.rs`: `ConnectionInput`, `ConnectorSchema` (+ `ResourceSpec`, `FieldSpec`, `FilterSpec`, `RelationshipSpec`, `View`, `FieldType`, `FilterType`, `Tier`), `BrowseRequest` (+ `BrowseParent`), `BrowsePage` (+ `DisplayRow`, `RowRef`, `CellValue`), `MaterializeRequest` (+ `ExpansionPolicy`), `LabelRow`. These already derive `utoipa::ToSchema` (added in Tasks 3 and 5), so registration is just listing them in `components(schemas(...))`. The connection VIEW is an ad-hoc `serde_json::json!` object, so document it inline on the path responses rather than as a named schema. Add `#[utoipa::path]` on the handlers (path WITHOUT `/api`). Confirm `/api/openapi.json` lists `/connections`, `/connections/{id}/schema|browse|materialize`.

- [ ] **Step 2: ADR-0018 "API integration spine"** — record: the hardened-egress decision (block loopback/link-local/unspecified/multicast, allow private LAN, no env flag) AND its accepted residual DNS-rebind TOCTOU (single-tenant authed LAN threat model, custom resolver out of scope); streaming response-size cap; the `Connector` enum-dispatch registry (no async-trait/dyn for one connector); signed process-lifetime browse cursors; connections store with redacted API-key credential; Homebox via unified `/v1/entities` with bearer key. Add the index row.

- [ ] **Step 3: SPEC** — add an "Integrations (connectors)" section (the three endpoints + the connections CRUD + the browse model summary + the egress policy) and a changelog entry. This is an API addition.

- [ ] **Step 4: Adversarial review loop** — dispatch a reviewer against `git diff main...homebox-backend`: egress IP policy correctness (IPv6 link-local mask, no bypass), credential never returned by any endpoint, cursor HMAC verify rejects tampering + binding mismatch, `ConnectorError` -> HTTP mapping, all new routes auth-gated, no secret logged, materialize cap enforced. Fix every meaningful finding; re-review until clean.

- [ ] **Step 5: Gate + integrate**
```bash
(cd ui && npm ci && npm run lint && npm run test && npm run build)
cargo fmt && cargo clippy --all-targets --all-features && cargo test
git checkout main && git merge homebox-backend && git push
```
(Plan A adds no UI; the UI gate just confirms nothing broke. No issue is closed yet. #35 closes after Plan B ships the UI; reference `#35` in commits, do not `Fixes` it here.)

---

## Self-Review

**1. Spec coverage:** hardened egress (allow private LAN, block loopback/link-local) -> Task 1; connections store + CRUD + redaction -> Tasks 2/5; Connector trait + browse model + registry -> Task 3; signed cursors -> Task 3; Homebox connector (bearer key, unified `/v1/entities`, tree, custom-field schema, tier split, derived URL, Direct drill, AsListed, materialize cap) -> Task 4; schema/browse/materialize endpoints + error mapping -> Task 6; OpenAPI + ADR + SPEC -> Task 7. Frontend (browse UI, mapping -> LabelGrid) is Plan B, out of scope here.

**2. Placeholder scan:** no TBD/TODO; the stub in Task 3 returns a real `Err` (not `todo!()`) and is replaced in Task 4. All Homebox API shapes (`/v1/entities` `{items,total}` envelope, `tags`/`parentIds` repeated bare-key params, `/v1/entities/fields` string array, `EntitySummary` fields) are VERIFIED against `/tmp/hb-swagger.json` and pinned by the connector test fixtures. The three `e2e` test bodies in Task 6 are sketched, not stubbed: each names the exact mock + assertion and points at `browse_endpoint_returns_rows_e2e` as the complete template.

**3. Consistency:** `Egress::get_json<T>` (generic) + `Egress::new`/`with_loopback`, `ip_allowed(ip, allow_loopback)`, the `ConnectorError` variants + `From<EgressError>` mapping, the browse-model type names, `ConnectorRegistry::get` -> `Connectors` enum, `cursor::{sign,verify,SigningKey,CursorClaims,CursorBinding}`, and the `Connection` store struct are used identically across Tasks 1-6. `AppState` gains `egress`/`connectors`/`cursor_key` (+ `#[cfg(test)] with_loopback_egress`) with getters used by Tasks 5/6.

**4. Egress test reachability:** production `Egress::new()` blocks loopback; every wiremock-backed test (Task 1 success, all Task 4, Task 6 endpoint e2e) uses `Egress::with_loopback()` (directly, or via `AppState::with_loopback_egress`), while `blocks_loopback_host` asserts the production blocking. This resolves what would otherwise be a contradiction between the loopback-block rule and the wiremock tests.

**Accepted residual risk (documented in ADR-0018):** resolve-then-connect leaves a sub-millisecond DNS-rebind TOCTOU because reqwest re-resolves on connect. For this single-tenant authed LAN tool the SSRF surface is small (the operator enters their own Homebox base URL); tightening it to a custom `reqwest::dns::Resolve` resolver is deliberately out of scope.
