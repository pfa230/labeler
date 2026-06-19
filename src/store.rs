use rusqlite::Connection as SqlConnection;
use rusqlite::OptionalExtension;
use rusqlite_migration::{Migrations, M};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::path::Path;
use std::sync::Mutex;
use utoipa::ToSchema;

/// A configured printer (a "machine" instance, per ADR-0007). `config` is an opaque per-kind JSON blob
/// that the driver for `kind` parses.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Printer {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub config: JsonValue,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone)]
pub struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String,
}

#[derive(Debug, Clone)]
pub struct ApiToken {
    pub id: String,
    pub name: String,
    pub last_used_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct Connection {
    pub id: String,
    pub connector: String,
    pub name: String,
    pub base_url: String,
    pub credential: String,
    pub enabled: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("migration error: {0}")]
    Migration(#[from] rusqlite_migration::Error),
    #[error("invalid stored config: {0}")]
    Json(#[from] serde_json::Error),
}

/// App-state persistence (printers, settings, a minimal job log). The only SQL touchpoint in the app;
/// methods are async-shaped so the backing store can later move to an async driver (e.g. sqlx) without
/// changing call sites.
pub struct Store {
    conn: Mutex<SqlConnection>,
}

fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(
            "CREATE TABLE printers (
            id         TEXT PRIMARY KEY,
            name       TEXT NOT NULL,
            kind       TEXT NOT NULL,
            config     TEXT NOT NULL,
            enabled    INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE jobs (
            id       INTEGER PRIMARY KEY AUTOINCREMENT,
            ts       TEXT NOT NULL DEFAULT (datetime('now')),
            template TEXT NOT NULL,
            printer  TEXT,
            status   TEXT NOT NULL,
            error    TEXT
        );",
        ),
        M::up(
            "CREATE TABLE users (
            id            TEXT PRIMARY KEY,
            username      TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            created_at    TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE sessions (
            id         TEXT PRIMARY KEY,
            user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            expires_at TEXT NOT NULL,
            last_seen  TEXT NOT NULL DEFAULT (datetime('now')),
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE api_tokens (
            id           TEXT PRIMARY KEY,
            name         TEXT NOT NULL,
            token_hash   TEXT NOT NULL UNIQUE,
            last_used_at TEXT,
            created_at   TEXT NOT NULL DEFAULT (datetime('now'))
        );",
        ),
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
        M::up("CREATE INDEX idx_jobs_ts ON jobs(ts);"),
        M::up("ALTER TABLE settings RENAME TO variables;"),
        M::up("CREATE TABLE app_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);"),
    ])
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let mut conn = SqlConnection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "foreign_keys", true)?;
        migrations().to_latest(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        let mut conn = SqlConnection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", true)?;
        migrations().to_latest(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub async fn list_printers(&self) -> Result<Vec<Printer>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt =
            conn.prepare("SELECT id, name, kind, config, enabled FROM printers ORDER BY id")?;
        let rows = stmt.query_map([], row_to_printer_parts)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(printer_from_parts(row?)?);
        }
        Ok(out)
    }

    pub async fn get_printer(&self, id: &str) -> Result<Option<Printer>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt =
            conn.prepare("SELECT id, name, kind, config, enabled FROM printers WHERE id = ?1")?;
        let mut rows = stmt.query_map([id], row_to_printer_parts)?;
        match rows.next() {
            Some(row) => Ok(Some(printer_from_parts(row?)?)),
            None => Ok(None),
        }
    }

    pub async fn upsert_printer(&self, printer: &Printer) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO printers (id, name, kind, config, enabled) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET name = ?2, kind = ?3, config = ?4, enabled = ?5",
            rusqlite::params![
                printer.id,
                printer.name,
                printer.kind,
                serde_json::to_string(&printer.config)?,
                printer.enabled as i64,
            ],
        )?;
        Ok(())
    }

    pub async fn delete_printer(&self, id: &str) -> Result<bool, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        let affected = conn.execute("DELETE FROM printers WHERE id = ?1", [id])?;
        Ok(affected > 0)
    }

    pub async fn get_variable(&self, key: &str) -> Result<Option<String>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare("SELECT value FROM variables WHERE key = ?1")?;
        let mut rows = stmt.query_map([key], |row| row.get::<_, String>(0))?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub async fn set_variable(&self, key: &str, value: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO variables (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = ?2",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }

    pub async fn all_variables(
        &self,
    ) -> Result<std::collections::BTreeMap<String, String>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare("SELECT key, value FROM variables ORDER BY key")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut out = std::collections::BTreeMap::new();
        for row in rows {
            let (k, v) = row?;
            out.insert(k, v);
        }
        Ok(out)
    }

    pub async fn get_setting(&self, key: &str) -> Result<Option<String>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare("SELECT value FROM app_settings WHERE key = ?1")?;
        let mut rows = stmt.query_map([key], |row| row.get::<_, String>(0))?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub async fn set_setting(&self, key: &str, value: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = ?2",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }

    pub async fn delete_setting(&self, key: &str) -> Result<bool, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        Ok(conn.execute("DELETE FROM app_settings WHERE key = ?1", [key])? > 0)
    }

    pub async fn all_settings(
        &self,
    ) -> Result<std::collections::BTreeMap<String, String>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare("SELECT key, value FROM app_settings ORDER BY key")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut out = std::collections::BTreeMap::new();
        for row in rows {
            let (k, v) = row?;
            out.insert(k, v);
        }
        Ok(out)
    }

    pub async fn record_job(
        &self,
        template: &str,
        printer: Option<&str>,
        status: &str,
        error: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO jobs (template, printer, status, error) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![template, printer, status, error],
        )?;
        Ok(())
    }

    /// Delete job-log rows older than `retention_days`. `0` disables (no-op). Returns rows deleted.
    /// `ts` is canonical `datetime('now')` UTC text, so the string compare against
    /// `datetime('now', '-<n> days')` is chronological. The modifier is bound as a full parameter
    /// (a `u32`, so it is always `-<digits> days`; no injection surface).
    pub async fn prune_jobs(&self, retention_days: u32) -> Result<usize, StoreError> {
        if retention_days == 0 {
            return Ok(0);
        }
        let conn = self.conn.lock().expect("store lock");
        let deleted = conn.execute(
            "DELETE FROM jobs WHERE ts < datetime('now', ?1)",
            rusqlite::params![format!("-{retention_days} days")],
        )?;
        Ok(deleted)
    }

    pub async fn count_users(&self) -> Result<i64, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        Ok(conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?)
    }

    pub async fn create_user(
        &self,
        username: &str,
        password_hash: &str,
    ) -> Result<User, StoreError> {
        let id = crate::auth::random_secret();
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO users (id, username, password_hash) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, username, password_hash],
        )?;
        Ok(User {
            id,
            username: username.to_string(),
            password_hash: password_hash.to_string(),
        })
    }

    pub async fn get_user_by_username(&self, username: &str) -> Result<Option<User>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        conn.query_row(
            "SELECT id, username, password_hash FROM users WHERE username = ?1",
            [username],
            |r| {
                Ok(User {
                    id: r.get(0)?,
                    username: r.get(1)?,
                    password_hash: r.get(2)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    pub async fn get_user_by_id(&self, id: &str) -> Result<Option<User>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        conn.query_row(
            "SELECT id, username, password_hash FROM users WHERE id = ?1",
            [id],
            |r| {
                Ok(User {
                    id: r.get(0)?,
                    username: r.get(1)?,
                    password_hash: r.get(2)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    pub async fn list_users(&self) -> Result<Vec<User>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt =
            conn.prepare("SELECT id, username, password_hash FROM users ORDER BY username")?;
        let rows = stmt.query_map([], |r| {
            Ok(User {
                id: r.get(0)?,
                username: r.get(1)?,
                password_hash: r.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub async fn delete_user(&self, id: &str) -> Result<bool, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        Ok(conn.execute("DELETE FROM users WHERE id = ?1", [id])? > 0)
    }

    pub async fn set_user_password(&self, id: &str, password_hash: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "UPDATE users SET password_hash = ?1 WHERE id = ?2",
            rusqlite::params![password_hash, id],
        )?;
        Ok(())
    }

    // Sessions
    pub async fn create_session(
        &self,
        id_hash: &str,
        user_id: &str,
        ttl_modifier: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO sessions (id, user_id, expires_at) VALUES (?1, ?2, datetime('now', ?3))",
            rusqlite::params![id_hash, user_id, ttl_modifier],
        )?;
        Ok(())
    }

    /// Look up a live (non-expired) session and its user. Slides expiry + last_seen, but only when
    /// last_seen is older than 1 hour (throttle), to avoid a write per request.
    pub async fn lookup_session(&self, id_hash: &str) -> Result<Option<User>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        let user = conn
            .query_row(
                "SELECT u.id, u.username, u.password_hash
                 FROM sessions s JOIN users u ON u.id = s.user_id
                 WHERE s.id = ?1 AND s.expires_at > datetime('now')",
                [id_hash],
                |r| {
                    Ok(User {
                        id: r.get(0)?,
                        username: r.get(1)?,
                        password_hash: r.get(2)?,
                    })
                },
            )
            .optional()?;
        if user.is_some() {
            conn.execute(
                "UPDATE sessions SET expires_at = datetime('now', '+30 days'), last_seen = datetime('now')
                 WHERE id = ?1 AND last_seen < datetime('now', '-1 hour')",
                [id_hash],
            )?;
        }
        Ok(user)
    }

    pub async fn delete_session(&self, id_hash: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute("DELETE FROM sessions WHERE id = ?1", [id_hash])?;
        Ok(())
    }

    pub async fn delete_user_sessions_except(
        &self,
        user_id: &str,
        keep_id_hash: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "DELETE FROM sessions WHERE user_id = ?1 AND id <> ?2",
            rusqlite::params![user_id, keep_id_hash],
        )?;
        Ok(())
    }

    // Tokens
    pub async fn create_token(&self, name: &str, token_hash: &str) -> Result<ApiToken, StoreError> {
        let id = crate::auth::random_secret();
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO api_tokens (id, name, token_hash) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, name, token_hash],
        )?;
        conn.query_row(
            "SELECT id, name, last_used_at, created_at FROM api_tokens WHERE id = ?1",
            [&id],
            |r| {
                Ok(ApiToken {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    last_used_at: r.get(2)?,
                    created_at: r.get(3)?,
                })
            },
        )
        .map_err(Into::into)
    }

    /// Look up a token by its hash; on hit, throttled-update last_used_at. Returns the token id.
    pub async fn lookup_token(&self, token_hash: &str) -> Result<Option<String>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        let id: Option<String> = conn
            .query_row(
                "SELECT id FROM api_tokens WHERE token_hash = ?1",
                [token_hash],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(ref tid) = id {
            conn.execute(
                "UPDATE api_tokens SET last_used_at = datetime('now')
                 WHERE id = ?1 AND (last_used_at IS NULL OR last_used_at < datetime('now', '-1 hour'))",
                [tid],
            )?;
        }
        Ok(id)
    }

    pub async fn list_tokens(&self) -> Result<Vec<ApiToken>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, name, last_used_at, created_at FROM api_tokens ORDER BY created_at",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ApiToken {
                id: r.get(0)?,
                name: r.get(1)?,
                last_used_at: r.get(2)?,
                created_at: r.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub async fn delete_token(&self, id: &str) -> Result<bool, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        Ok(conn.execute("DELETE FROM api_tokens WHERE id = ?1", [id])? > 0)
    }

    // Connections
    pub async fn create_connection(
        &self,
        connector: &str,
        name: &str,
        base_url: &str,
        credential: &str,
    ) -> Result<Connection, StoreError> {
        let id = crate::auth::random_secret();
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO connections (id, connector, name, base_url, credential) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, connector, name, base_url, credential],
        )?;
        Ok(Connection {
            id,
            connector: connector.to_string(),
            name: name.to_string(),
            base_url: base_url.to_string(),
            credential: credential.to_string(),
            enabled: true,
        })
    }

    pub async fn get_connection(&self, id: &str) -> Result<Option<Connection>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        conn.query_row(
            "SELECT id, connector, name, base_url, credential, enabled FROM connections WHERE id = ?1",
            [id],
            row_to_connection,
        )
        .optional()
        .map_err(Into::into)
    }

    pub async fn list_connections(&self) -> Result<Vec<Connection>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, connector, name, base_url, credential, enabled FROM connections ORDER BY name",
        )?;
        let rows = stmt.query_map([], row_to_connection)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub async fn update_connection(
        &self,
        id: &str,
        name: &str,
        base_url: &str,
        credential: Option<&str>,
        enabled: bool,
    ) -> Result<bool, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        let n = match credential {
            Some(cred) => conn.execute(
                "UPDATE connections SET name = ?1, base_url = ?2, credential = ?3, enabled = ?4 WHERE id = ?5",
                rusqlite::params![name, base_url, cred, enabled as i64, id],
            )?,
            None => conn.execute(
                "UPDATE connections SET name = ?1, base_url = ?2, enabled = ?3 WHERE id = ?4",
                rusqlite::params![name, base_url, enabled as i64, id],
            )?,
        };
        Ok(n > 0)
    }

    pub async fn delete_connection(&self, id: &str) -> Result<bool, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        Ok(conn.execute("DELETE FROM connections WHERE id = ?1", [id])? > 0)
    }
}

fn row_to_connection(r: &rusqlite::Row<'_>) -> rusqlite::Result<Connection> {
    Ok(Connection {
        id: r.get(0)?,
        connector: r.get(1)?,
        name: r.get(2)?,
        base_url: r.get(3)?,
        credential: r.get(4)?,
        enabled: r.get::<_, i64>(5)? != 0,
    })
}

type PrinterParts = (String, String, String, String, i64);

fn row_to_printer_parts(row: &rusqlite::Row<'_>) -> rusqlite::Result<PrinterParts> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
    ))
}

fn printer_from_parts(parts: PrinterParts) -> Result<Printer, StoreError> {
    let (id, name, kind, config, enabled) = parts;
    Ok(Printer {
        id,
        name,
        kind,
        config: serde_json::from_str(&config)?,
        enabled: enabled != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn printer_crud_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.list_printers().await.unwrap().is_empty());

        let printer = Printer {
            id: "p1".to_string(),
            name: "P1".to_string(),
            kind: "cups".to_string(),
            config: json!({ "uri": "ipp://x" }),
            enabled: true,
        };
        store.upsert_printer(&printer).await.unwrap();

        let got = store.get_printer("p1").await.unwrap().unwrap();
        assert_eq!(got.name, "P1");
        assert_eq!(got.config, json!({ "uri": "ipp://x" }));
        assert_eq!(store.list_printers().await.unwrap().len(), 1);

        let updated = Printer {
            name: "P1b".to_string(),
            ..printer.clone()
        };
        store.upsert_printer(&updated).await.unwrap();
        assert_eq!(store.get_printer("p1").await.unwrap().unwrap().name, "P1b");

        assert!(store.delete_printer("p1").await.unwrap());
        assert!(store.get_printer("p1").await.unwrap().is_none());
        assert!(!store.delete_printer("p1").await.unwrap());
    }

    #[tokio::test]
    async fn variables_and_jobs() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.get_variable("k").await.unwrap().is_none());
        store.set_variable("k", "v").await.unwrap();
        assert_eq!(store.get_variable("k").await.unwrap().as_deref(), Some("v"));
        store.set_variable("k", "v2").await.unwrap();
        assert_eq!(
            store.get_variable("k").await.unwrap().as_deref(),
            Some("v2")
        );

        let all = store.all_variables().await.unwrap();
        assert_eq!(all.get("k").map(String::as_str), Some("v2"));

        store
            .record_job("tpl", Some("p1"), "ok", None)
            .await
            .unwrap();
        store
            .record_job("tpl", None, "failed", Some("boom"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn prune_jobs_deletes_old_keeps_recent() {
        let store = Store::open_in_memory().unwrap();
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO jobs (ts, template, status) VALUES (datetime('now','-200 days'), 'tpl', 'ok')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO jobs (ts, template, status) VALUES (datetime('now'), 'tpl', 'ok')",
                [],
            )
            .unwrap();
        }
        let deleted = store.prune_jobs(90).await.unwrap();
        assert_eq!(deleted, 1);
        let remaining: i64 = {
            let conn = store.conn.lock().unwrap();
            conn.query_row("SELECT COUNT(*) FROM jobs", [], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(remaining, 1);
    }

    #[tokio::test]
    async fn prune_jobs_zero_is_noop() {
        let store = Store::open_in_memory().unwrap();
        store.record_job("tpl", None, "ok", None).await.unwrap();
        assert_eq!(store.prune_jobs(0).await.unwrap(), 0);
        let remaining: i64 = {
            let conn = store.conn.lock().unwrap();
            conn.query_row("SELECT COUNT(*) FROM jobs", [], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(remaining, 1);
    }

    #[tokio::test]
    async fn prune_job_log_once_reads_live_override() {
        use crate::settings::{prune_job_log_once, JOB_LOG_RETENTION_DAYS};
        let store = Store::open_in_memory().unwrap();
        // one row aged 200 days
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO jobs (ts, template, status) VALUES (datetime('now','-200 days'), 'tpl', 'ok')",
                [],
            )
            .unwrap();
        }
        // override retention to 0 (disabled): the old row survives
        store
            .set_setting(JOB_LOG_RETENTION_DAYS, "0")
            .await
            .unwrap();
        assert_eq!(prune_job_log_once(&store).await.unwrap(), 0);
        // remove the override: default 90 now prunes the 200-day-old row
        store.delete_setting(JOB_LOG_RETENTION_DAYS).await.unwrap();
        assert_eq!(prune_job_log_once(&store).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn app_settings_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        // absent key reads as None
        assert_eq!(
            store.get_setting("job_log_retention_days").await.unwrap(),
            None
        );
        // set then get
        store
            .set_setting("job_log_retention_days", "30")
            .await
            .unwrap();
        assert_eq!(
            store.get_setting("job_log_retention_days").await.unwrap(),
            Some("30".to_string())
        );
        // upsert overwrites
        store
            .set_setting("job_log_retention_days", "45")
            .await
            .unwrap();
        assert_eq!(
            store.get_setting("job_log_retention_days").await.unwrap(),
            Some("45".to_string())
        );
        // all_settings lists the override row
        let all = store.all_settings().await.unwrap();
        assert_eq!(all.get("job_log_retention_days"), Some(&"45".to_string()));
        // delete returns true when a row existed, false when it did not
        assert!(store
            .delete_setting("job_log_retention_days")
            .await
            .unwrap());
        assert!(!store
            .delete_setting("job_log_retention_days")
            .await
            .unwrap());
        assert_eq!(
            store.get_setting("job_log_retention_days").await.unwrap(),
            None
        );
    }
}

#[cfg(test)]
mod connection_tests {
    use super::*;
    fn store() -> Store {
        Store::open_in_memory().unwrap()
    }

    #[tokio::test]
    async fn connection_crud() {
        let s = store();
        let c = s
            .create_connection("homebox", "home", "http://hb.lan:7745", "hb_secret")
            .await
            .unwrap();
        assert_eq!(c.connector, "homebox");
        assert_eq!(c.credential, "hb_secret");
        assert!(s.get_connection(&c.id).await.unwrap().is_some());
        assert_eq!(s.list_connections().await.unwrap().len(), 1);
        // update name + keep credential (None = unchanged)
        assert!(s
            .update_connection(&c.id, "renamed", "http://hb.lan:7745", None, true)
            .await
            .unwrap());
        let g = s.get_connection(&c.id).await.unwrap().unwrap();
        assert_eq!(g.name, "renamed");
        assert_eq!(g.credential, "hb_secret"); // unchanged
                                               // update credential
        assert!(s
            .update_connection(&c.id, "renamed", "http://hb.lan:7745", Some("hb_new"), true)
            .await
            .unwrap());
        assert_eq!(
            s.get_connection(&c.id).await.unwrap().unwrap().credential,
            "hb_new"
        );
        assert!(s.delete_connection(&c.id).await.unwrap());
        assert!(s.get_connection(&c.id).await.unwrap().is_none());
    }
}

#[cfg(test)]
mod auth_tests {
    use super::*;

    fn store() -> Store {
        Store::open_in_memory().unwrap()
    }

    #[tokio::test]
    async fn user_lifecycle_and_count() {
        let s = store();
        assert_eq!(s.count_users().await.unwrap(), 0);
        let u = s.create_user("alice", "phc-hash").await.unwrap();
        assert_eq!(u.username, "alice");
        assert_eq!(s.count_users().await.unwrap(), 1);
        assert!(s.get_user_by_username("alice").await.unwrap().is_some());
        assert!(s.create_user("alice", "h").await.is_err()); // unique username
        assert_eq!(s.list_users().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn session_create_lookup_delete_and_user_cascade() {
        let s = store();
        let u = s.create_user("bob", "h").await.unwrap();
        let raw = "raw-session-value";
        s.create_session(&crate::auth::sha256_hex(raw), &u.id, "+30 days")
            .await
            .unwrap();
        let found = s
            .lookup_session(&crate::auth::sha256_hex(raw))
            .await
            .unwrap();
        assert_eq!(found.unwrap().username, "bob");
        // delete cascades when the user is removed
        s.delete_user(&u.id).await.unwrap();
        assert!(s
            .lookup_session(&crate::auth::sha256_hex(raw))
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn expired_session_not_returned() {
        let s = store();
        let u = s.create_user("carol", "h").await.unwrap();
        s.create_session("h1", &u.id, "-1 minute").await.unwrap();
        assert!(s.lookup_session("h1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn token_create_lookup_revoke() {
        let s = store();
        let t = s
            .create_token("ci", &crate::auth::sha256_hex("secretval"))
            .await
            .unwrap();
        assert_eq!(t.name, "ci");
        assert!(s
            .lookup_token(&crate::auth::sha256_hex("secretval"))
            .await
            .unwrap()
            .is_some());
        s.delete_token(&t.id).await.unwrap();
        assert!(s
            .lookup_token(&crate::auth::sha256_hex("secretval"))
            .await
            .unwrap()
            .is_none());
    }
}
