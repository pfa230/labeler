use crate::models::{
    ActivityEntry, ActivityPage, JobRecord, Printer, PrinterConnection, RenderProfile,
};
use rusqlite::Connection as SqlConnection;
use rusqlite::OptionalExtension;
use rusqlite_migration::{Migrations, M};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

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
/// SQLite operations and mutex acquisition block the calling thread. This inline execution assumes
/// small local operator workloads and provides no latency isolation for concurrent requests.
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
        M::up("ALTER TABLE printers ADD COLUMN is_default INTEGER NOT NULL DEFAULT 0;"),
        M::up("ALTER TABLE jobs ADD COLUMN user_id TEXT NOT NULL DEFAULT '';"),
        M::up(
            "CREATE TABLE favorites (
            user_id     TEXT NOT NULL,
            template_id TEXT NOT NULL,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (user_id, template_id)
        );",
        ),
        M::up("ALTER TABLE printers DROP COLUMN enabled;"),
        M::up("ALTER TABLE connections ADD COLUMN public_url TEXT;"),
        M::up("ALTER TABLE connections ADD COLUMN transforms TEXT;"),
        M::up(
            "ALTER TABLE connections DROP COLUMN enabled;
        ALTER TABLE connections DROP COLUMN transforms;",
        ),
        // Flat printer record. `kind` is dropped unchecked: only `cups` ever existed outside tests.
        M::up(
            "CREATE TABLE printers_new (
            id         TEXT PRIMARY KEY,
            name       TEXT NOT NULL,
            uri        TEXT NOT NULL,
            username   TEXT,
            password   TEXT,
            ca_cert    TEXT,
            insecure   INTEGER NOT NULL DEFAULT 0,
            color_mode TEXT,
            resolution INTEGER,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        INSERT INTO printers_new
            SELECT id, name, json_extract(config, '$.uri'),
                   json_extract(config, '$.username'), json_extract(config, '$.password'),
                   json_extract(config, '$.ca_cert'), coalesce(json_extract(config, '$.insecure'), 0),
                   json_extract(config, '$.render.color_mode'),
                   json_extract(config, '$.render.resolution'), created_at
            FROM printers;
        INSERT OR REPLACE INTO app_settings (key, value)
            SELECT 'default_printer_id', id FROM printers WHERE is_default = 1 ORDER BY id LIMIT 1;
        DROP TABLE printers;
        ALTER TABLE printers_new RENAME TO printers;",
        ),
        // Tokens belong to a user. No token existed when owners were introduced, so none is kept.
        M::up(
            "DROP TABLE api_tokens;
        CREATE TABLE api_tokens (
            id           TEXT PRIMARY KEY,
            owner        TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            name         TEXT NOT NULL,
            token_hash   TEXT NOT NULL UNIQUE,
            last_used_at TEXT,
            created_at   TEXT NOT NULL DEFAULT (datetime('now'))
        );",
        ),
        // Inventory connections and the default-connection override are gone with the connector
        // subsystem (issue #452); nothing else in the store depends on them.
        M::up(
            "DROP TABLE connections;
        DELETE FROM app_settings WHERE key = 'default_connection_id';",
        ),
        M::up(
            "CREATE TABLE activity (
                id INTEGER PRIMARY KEY,
                time TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                actor_id TEXT NOT NULL,
                actor_name TEXT NOT NULL,
                action TEXT NOT NULL,
                target TEXT NOT NULL,
                outcome TEXT NOT NULL,
                error_code TEXT,
                error_reason TEXT
            );",
        ),
    ])
}

impl Store {
    fn connection(&self) -> MutexGuard<'_, SqlConnection> {
        self.conn.lock().expect("store lock")
    }

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

    pub fn insert_activity(
        &self,
        actor_id: &str,
        actor_name: &str,
        action: &str,
        target: &str,
        failure: Option<(&str, Option<&str>)>,
    ) -> Result<(), StoreError> {
        self.connection().execute(
            "INSERT INTO activity (actor_id, actor_name, action, target, outcome, error_code, error_reason)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                actor_id, actor_name, action, target,
                if failure.is_some() { "failure" } else { "success" },
                failure.map(|(code, _)| code),
                failure.and_then(|(_, reason)| reason),
            ],
        )?;
        Ok(())
    }

    pub fn list_activity(&self, before_id: Option<i64>) -> Result<ActivityPage, StoreError> {
        let conn = self.connection();
        let mut stmt = conn.prepare(
            "SELECT id, time, actor_id, actor_name, action, target, outcome, error_code, error_reason
             FROM activity WHERE ?1 IS NULL OR id < ?1 ORDER BY id DESC LIMIT 101",
        )?;
        let mut entries = stmt
            .query_map([before_id], |row| {
                Ok(ActivityEntry {
                    id: row.get(0)?,
                    time: row.get(1)?,
                    actor_id: row.get(2)?,
                    actor_name: row.get(3)?,
                    action: row.get(4)?,
                    target: row.get(5)?,
                    outcome: row.get(6)?,
                    error_code: row.get(7)?,
                    error_reason: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let next_before_id = if entries.len() > 100 {
            entries.truncate(100);
            entries.last().map(|entry| entry.id)
        } else {
            None
        };
        Ok(ActivityPage {
            entries,
            next_before_id,
        })
    }

    pub fn list_printers(&self) -> Result<Vec<Printer>, StoreError> {
        let conn = self.connection();
        let mut stmt = conn.prepare(&format!("{SELECT_PRINTER} ORDER BY id"))?;
        let rows = stmt.query_map([], row_to_printer)?;
        Ok(rows
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|(id, name, connection)| Printer::new(id, name, connection))
            .collect())
    }

    pub fn get_printer(&self, id: &str) -> Result<Option<Printer>, StoreError> {
        Ok(self
            .get_printer_record(id)?
            .map(|(id, name, connection)| Printer::new(id, name, connection)))
    }

    /// The stored connection, password included, for the driver and the `PUT` keep rule.
    pub fn get_printer_connection(
        &self,
        id: &str,
    ) -> Result<Option<PrinterConnection>, StoreError> {
        Ok(self
            .get_printer_record(id)?
            .map(|(_, _, connection)| connection))
    }

    fn get_printer_record(&self, id: &str) -> Result<Option<PrinterRecord>, StoreError> {
        let conn = self.connection();
        conn.query_row(
            &format!("{SELECT_PRINTER} WHERE id = ?1"),
            [id],
            row_to_printer,
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn insert_printer(
        &self,
        id: &str,
        name: &str,
        connection: &PrinterConnection,
    ) -> Result<(), StoreError> {
        let conn = self.connection();
        let render = connection.render.as_ref();
        conn.execute(
            "INSERT INTO printers (id, name, uri, username, password, ca_cert, insecure, color_mode, resolution)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                id,
                name,
                connection.uri,
                connection.username,
                connection.password,
                connection.ca_cert,
                connection.insecure,
                render.and_then(|r| r.color_mode.as_deref()),
                render.and_then(|r| r.resolution),
            ],
        )?;
        Ok(())
    }

    /// Replace every stored field of printer `id`. Returns `false` if there is no such printer.
    pub fn replace_printer(
        &self,
        id: &str,
        name: &str,
        connection: &PrinterConnection,
    ) -> Result<bool, StoreError> {
        let conn = self.connection();
        let render = connection.render.as_ref();
        let n = conn.execute(
            "UPDATE printers SET name = ?2, uri = ?3, username = ?4, password = ?5, ca_cert = ?6,
                 insecure = ?7, color_mode = ?8, resolution = ?9
             WHERE id = ?1",
            rusqlite::params![
                id,
                name,
                connection.uri,
                connection.username,
                connection.password,
                connection.ca_cert,
                connection.insecure,
                render.and_then(|r| r.color_mode.as_deref()),
                render.and_then(|r| r.resolution),
            ],
        )?;
        Ok(n > 0)
    }

    /// Delete a printer and, in the same transaction, the `default_printer_id` setting when it named
    /// that printer. There is deliberately no plain `delete_printer`: a delete that does not clear
    /// the setting would leave a default naming a printer that no longer exists, and `GET
    /// /api/settings` takes no lock, so a second statement is a window a reader can land in.
    pub fn delete_printer_and_default(&self, id: &str) -> Result<bool, StoreError> {
        let mut conn = self.connection();
        let tx = conn.transaction()?;
        let existed = tx.execute("DELETE FROM printers WHERE id = ?1", [id])? > 0;
        if existed {
            tx.execute(
                "DELETE FROM app_settings WHERE key = ?1 AND value = ?2",
                rusqlite::params![crate::settings::DEFAULT_PRINTER_ID, id],
            )?;
        }
        tx.commit()?;
        Ok(existed)
    }

    pub fn get_variable(&self, key: &str) -> Result<Option<String>, StoreError> {
        let conn = self.connection();
        let mut stmt = conn.prepare("SELECT value FROM variables WHERE key = ?1")?;
        let mut rows = stmt.query_map([key], |row| row.get::<_, String>(0))?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn set_variable(&self, key: &str, value: &str) -> Result<(), StoreError> {
        let conn = self.connection();
        conn.execute(
            "INSERT INTO variables (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = ?2",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }

    pub fn all_variables(&self) -> Result<std::collections::BTreeMap<String, String>, StoreError> {
        let conn = self.connection();
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

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, StoreError> {
        let conn = self.connection();
        let mut stmt = conn.prepare("SELECT value FROM app_settings WHERE key = ?1")?;
        let mut rows = stmt.query_map([key], |row| row.get::<_, String>(0))?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), StoreError> {
        let conn = self.connection();
        conn.execute(
            "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = ?2",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }

    pub fn delete_setting(&self, key: &str) -> Result<bool, StoreError> {
        let conn = self.connection();
        Ok(conn.execute("DELETE FROM app_settings WHERE key = ?1", [key])? > 0)
    }

    pub fn all_settings(&self) -> Result<std::collections::BTreeMap<String, String>, StoreError> {
        let conn = self.connection();
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

    pub fn record_job(
        &self,
        template: &str,
        printer: Option<&str>,
        status: &str,
        error: Option<&str>,
        user_id: &str,
    ) -> Result<(), StoreError> {
        let conn = self.connection();
        conn.execute(
            "INSERT INTO jobs (template, printer, status, error, user_id) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![template, printer, status, error, user_id],
        )?;
        Ok(())
    }

    /// Every recorded job, newest first, with its actor's current username when one exists.
    pub fn list_jobs(&self) -> Result<Vec<JobRecord>, StoreError> {
        let conn = self.connection();
        let mut stmt = conn.prepare(
            "SELECT j.id,
                    strftime('%Y-%m-%dT%H:%M:%SZ', j.ts),
                    j.template,
                    j.printer,
                    j.status,
                    j.error,
                    j.user_id,
                    u.username
             FROM jobs j
             LEFT JOIN users u ON u.id = j.user_id
             ORDER BY j.ts DESC, j.id DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(JobRecord {
                id: r.get(0)?,
                timestamp: r.get(1)?,
                template_id: r.get(2)?,
                printer_id: r.get(3)?,
                status: r.get(4)?,
                error: r.get(5)?,
                actor_id: r.get(6)?,
                actor_username: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Each logged job's status, oldest first.
    #[cfg(test)]
    pub fn job_statuses(&self) -> Result<Vec<String>, StoreError> {
        let conn = self.connection();
        let mut stmt = conn.prepare("SELECT status FROM jobs ORDER BY id")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn list_favorites(&self, user_id: &str) -> Result<Vec<String>, StoreError> {
        let conn = self.connection();
        let mut stmt = conn.prepare(
            "SELECT template_id FROM favorites WHERE user_id = ?1 ORDER BY created_at, template_id",
        )?;
        let rows = stmt.query_map([user_id], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn add_favorite(&self, user_id: &str, template_id: &str) -> Result<(), StoreError> {
        let conn = self.connection();
        conn.execute(
            "INSERT OR IGNORE INTO favorites (user_id, template_id) VALUES (?1, ?2)",
            rusqlite::params![user_id, template_id],
        )?;
        Ok(())
    }

    pub fn remove_favorite(&self, user_id: &str, template_id: &str) -> Result<(), StoreError> {
        let conn = self.connection();
        conn.execute(
            "DELETE FROM favorites WHERE user_id = ?1 AND template_id = ?2",
            rusqlite::params![user_id, template_id],
        )?;
        Ok(())
    }

    /// Drop every user's favorite row for `template_id`. Called when the template is deleted so a
    /// later template reusing the id does not inherit favorites pointing at the old one. Not scoped
    /// to one actor: the delete invalidates the row for everybody, not just the caller (#140).
    pub fn remove_favorites_for_template(&self, template_id: &str) -> Result<usize, StoreError> {
        let conn = self.connection();
        let removed = conn.execute(
            "DELETE FROM favorites WHERE template_id = ?1",
            rusqlite::params![template_id],
        )?;
        Ok(removed)
    }

    /// The 6 most recently printed distinct templates for this user (deterministic: MAX(id) tiebreak).
    pub fn recent_templates(&self, user_id: &str) -> Result<Vec<String>, StoreError> {
        let conn = self.connection();
        let mut stmt = conn.prepare(
            "SELECT template FROM jobs WHERE user_id = ?1
             GROUP BY template ORDER BY MAX(ts) DESC, MAX(id) DESC LIMIT 6",
        )?;
        let rows = stmt.query_map([user_id], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn count_users(&self) -> Result<i64, StoreError> {
        let conn = self.connection();
        Ok(conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?)
    }

    pub fn create_user(&self, username: &str, password_hash: &str) -> Result<User, StoreError> {
        let id = crate::auth::random_secret();
        let conn = self.connection();
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

    pub fn get_user_by_username(&self, username: &str) -> Result<Option<User>, StoreError> {
        let conn = self.connection();
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

    pub fn get_user_by_id(&self, id: &str) -> Result<Option<User>, StoreError> {
        let conn = self.connection();
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

    pub fn list_users(&self) -> Result<Vec<User>, StoreError> {
        let conn = self.connection();
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

    pub fn delete_user(&self, id: &str) -> Result<bool, StoreError> {
        let conn = self.connection();
        Ok(conn.execute("DELETE FROM users WHERE id = ?1", [id])? > 0)
    }

    pub fn set_user_password(&self, id: &str, password_hash: &str) -> Result<(), StoreError> {
        let conn = self.connection();
        conn.execute(
            "UPDATE users SET password_hash = ?1 WHERE id = ?2",
            rusqlite::params![password_hash, id],
        )?;
        Ok(())
    }

    // Sessions
    pub fn create_session(
        &self,
        id_hash: &str,
        user_id: &str,
        ttl_modifier: &str,
    ) -> Result<(), StoreError> {
        let conn = self.connection();
        conn.execute(
            "INSERT INTO sessions (id, user_id, expires_at) VALUES (?1, ?2, datetime('now', ?3))",
            rusqlite::params![id_hash, user_id, ttl_modifier],
        )?;
        Ok(())
    }

    /// Look up a live (non-expired) session and its user. Slides expiry + last_seen, but only when
    /// last_seen is older than 1 hour (throttle), to avoid a write per request.
    pub fn lookup_session(&self, id_hash: &str) -> Result<Option<User>, StoreError> {
        let conn = self.connection();
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

    pub fn delete_session(&self, id_hash: &str) -> Result<(), StoreError> {
        let conn = self.connection();
        conn.execute("DELETE FROM sessions WHERE id = ?1", [id_hash])?;
        Ok(())
    }

    pub fn delete_user_sessions_except(
        &self,
        user_id: &str,
        keep_id_hash: &str,
    ) -> Result<(), StoreError> {
        let conn = self.connection();
        conn.execute(
            "DELETE FROM sessions WHERE user_id = ?1 AND id <> ?2",
            rusqlite::params![user_id, keep_id_hash],
        )?;
        Ok(())
    }

    // Tokens
    pub fn create_token(
        &self,
        owner: &str,
        name: &str,
        token_hash: &str,
    ) -> Result<ApiToken, StoreError> {
        let id = crate::auth::random_secret();
        let conn = self.connection();
        conn.execute(
            "INSERT INTO api_tokens (id, owner, name, token_hash) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, owner, name, token_hash],
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

    /// Look up a token by its hash and return its owner; on hit, throttled-update last_used_at.
    pub fn lookup_token(&self, token_hash: &str) -> Result<Option<User>, StoreError> {
        let conn = self.connection();
        let found = conn
            .query_row(
                "SELECT t.id, u.id, u.username, u.password_hash
                 FROM api_tokens t JOIN users u ON u.id = t.owner
                 WHERE t.token_hash = ?1",
                [token_hash],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        User {
                            id: r.get(1)?,
                            username: r.get(2)?,
                            password_hash: r.get(3)?,
                        },
                    ))
                },
            )
            .optional()?;
        let Some((token_id, owner)) = found else {
            return Ok(None);
        };
        conn.execute(
            "UPDATE api_tokens SET last_used_at = datetime('now')
             WHERE id = ?1 AND (last_used_at IS NULL OR last_used_at < datetime('now', '-1 hour'))",
            [token_id],
        )?;
        Ok(Some(owner))
    }

    pub fn list_tokens(&self, owner: &str) -> Result<Vec<ApiToken>, StoreError> {
        let conn = self.connection();
        let mut stmt = conn.prepare(
            "SELECT id, name, last_used_at, created_at FROM api_tokens WHERE owner = ?1
             ORDER BY created_at",
        )?;
        let rows = stmt.query_map([owner], |r| {
            Ok(ApiToken {
                id: r.get(0)?,
                name: r.get(1)?,
                last_used_at: r.get(2)?,
                created_at: r.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn delete_token(&self, owner: &str, id: &str) -> Result<bool, StoreError> {
        let conn = self.connection();
        Ok(conn.execute(
            "DELETE FROM api_tokens WHERE id = ?1 AND owner = ?2",
            rusqlite::params![id, owner],
        )? > 0)
    }
}

const SELECT_PRINTER: &str =
    "SELECT id, name, uri, username, password, ca_cert, insecure, color_mode, resolution FROM printers";

/// A printer row: id, name and its connection, password included.
type PrinterRecord = (String, String, PrinterConnection);

fn row_to_printer(r: &rusqlite::Row<'_>) -> rusqlite::Result<PrinterRecord> {
    let color_mode: Option<String> = r.get(7)?;
    let resolution: Option<u32> = r.get(8)?;
    let render = (color_mode.is_some() || resolution.is_some()).then_some(RenderProfile {
        color_mode,
        resolution,
    });
    Ok((
        r.get(0)?,
        r.get(1)?,
        PrinterConnection {
            uri: r.get(2)?,
            username: r.get(3)?,
            password: r.get(4)?,
            ca_cert: r.get(5)?,
            insecure: r.get(6)?,
            render,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connection(uri: &str, password: Option<&str>) -> PrinterConnection {
        PrinterConnection {
            uri: uri.to_string(),
            username: Some("u".to_string()),
            password: password.map(str::to_string),
            ca_cert: None,
            insecure: true,
            render: Some(RenderProfile {
                color_mode: None,
                resolution: Some(300),
            }),
        }
    }

    // Fails on lost/pruned history, wrong order, overlapping cursors, or unbounded pages.
    #[test]
    fn activity_survives_restart_with_exclusive_bounded_pages() {
        let path = std::env::temp_dir().join(format!(
            "labeler-activity-{}.sqlite",
            crate::auth::random_secret()
        ));
        {
            let store = Store::open(&path).unwrap();
            for i in 1..=205 {
                store
                    .insert_activity("u1", "pat", "template.create", &i.to_string(), None)
                    .unwrap();
            }
        }
        let store = Store::open(&path).unwrap();
        let mut cursor = None;
        for (first, last, next) in [(205, 106, Some(106)), (105, 6, Some(6)), (5, 1, None)] {
            let page = store.list_activity(cursor).unwrap();
            assert_eq!(
                page.entries
                    .iter()
                    .map(|entry| entry.id)
                    .collect::<Vec<_>>(),
                (last..=first).rev().collect::<Vec<_>>()
            );
            assert_eq!(page.next_before_id, next);
            cursor = next;
        }
        drop(store);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn printer_crud_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.list_printers().unwrap().is_empty());

        store
            .insert_printer("p1", "P1", &connection("ipp://x", Some("s")))
            .unwrap();

        let got = store.get_printer("p1").unwrap().unwrap();
        assert_eq!((got.name.as_str(), got.uri.as_str()), ("P1", "ipp://x"));
        assert_eq!(got.username.as_deref(), Some("u"));
        assert!(got.insecure);
        assert_eq!(got.render.and_then(|r| r.resolution), Some(300));
        let stored = store.get_printer_connection("p1").unwrap().unwrap();
        assert_eq!(stored.password.as_deref(), Some("s"));
        assert_eq!(store.list_printers().unwrap().len(), 1);

        assert!(store
            .replace_printer("p1", "P1b", &connection("ipp://y", None))
            .unwrap());
        let got = store.get_printer_connection("p1").unwrap().unwrap();
        assert_eq!((got.uri.as_str(), got.password), ("ipp://y", None));
        assert_eq!(store.get_printer("p1").unwrap().unwrap().name, "P1b");
        assert!(!store
            .replace_printer("ghost", "G", &connection("ipp://y", None))
            .unwrap());

        assert!(store.delete_printer_and_default("p1").unwrap());
        assert!(store.get_printer("p1").unwrap().is_none());
        assert!(!store.delete_printer_and_default("p1").unwrap());
    }

    #[test]
    fn variables_and_jobs() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.get_variable("k").unwrap().is_none());
        store.set_variable("k", "v").unwrap();
        assert_eq!(store.get_variable("k").unwrap().as_deref(), Some("v"));
        store.set_variable("k", "v2").unwrap();
        assert_eq!(store.get_variable("k").unwrap().as_deref(), Some("v2"));

        let all = store.all_variables().unwrap();
        assert_eq!(all.get("k").map(String::as_str), Some("v2"));

        store
            .record_job("tpl", Some("p1"), "ok", None, "u1")
            .unwrap();
        store
            .record_job("tpl", None, "failed", Some("boom"), "u1")
            .unwrap();
    }

    #[test]
    fn favorites_add_idempotent_list_and_remove() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.list_favorites("u1").unwrap().is_empty());
        store.add_favorite("u1", "tpl_a").unwrap();
        store.add_favorite("u1", "tpl_a").unwrap(); // idempotent
        store.add_favorite("u1", "tpl_b").unwrap();
        // per-user isolation
        store.add_favorite("u2", "tpl_c").unwrap();
        assert_eq!(
            store.list_favorites("u1").unwrap(),
            vec!["tpl_a".to_string(), "tpl_b".to_string()]
        );
        assert_eq!(
            store.list_favorites("u2").unwrap(),
            vec!["tpl_c".to_string()]
        );
        store.remove_favorite("u1", "tpl_a").unwrap();
        store.remove_favorite("u1", "tpl_a").unwrap(); // idempotent
        assert_eq!(
            store.list_favorites("u1").unwrap(),
            vec!["tpl_b".to_string()]
        );
    }

    #[test]
    fn remove_favorites_for_template_drops_every_users_row() {
        let store = Store::open_in_memory().unwrap();
        store.add_favorite("u1", "t1").unwrap();
        store.add_favorite("u2", "t1").unwrap();
        store.add_favorite("u1", "t2").unwrap();

        // Favorites are keyed by actor, so deleting the template must clear it for everyone.
        assert_eq!(store.remove_favorites_for_template("t1").unwrap(), 2);
        assert_eq!(store.list_favorites("u1").unwrap(), vec!["t2".to_string()]);
        assert!(store.list_favorites("u2").unwrap().is_empty());
        assert_eq!(store.remove_favorites_for_template("t1").unwrap(), 0);
    }

    #[test]
    fn recent_templates_distinct_ordered_and_per_user() {
        let store = Store::open_in_memory().unwrap();
        // Same ts for a and b; MAX(id) tiebreak makes b (inserted later) rank first.
        store.record_job("a", None, "ok", None, "u1").unwrap();
        store.record_job("a", None, "ok", None, "u1").unwrap();
        store.record_job("b", None, "ok", None, "u1").unwrap();
        store.record_job("c", None, "ok", None, "u2").unwrap();
        let recents = store.recent_templates("u1").unwrap();
        assert_eq!(recents, vec!["b".to_string(), "a".to_string()]);
        assert_eq!(store.recent_templates("u2").unwrap(), vec!["c".to_string()]);
        assert!(store.recent_templates("nobody").unwrap().is_empty());
    }

    #[test]
    fn list_jobs_returns_every_actor_newest_first() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.list_jobs().unwrap().is_empty());

        let alice = store.create_user("alice", "hash").unwrap();
        let bob = store.create_user("bob", "hash").unwrap();
        let long_error = "printer rejected job: paper jam on tray 2\nsecond line of detail";
        store
            .record_job("ok_tpl", Some("p1"), "ok", None, &alice.id)
            .unwrap();
        store
            .record_job("failed_tpl", None, "failed", Some(long_error), &bob.id)
            .unwrap();
        store
            .record_job("local_tpl", None, "ok", None, "local")
            .unwrap();
        store
            .record_job("ghost_tpl", None, "ok", None, "deleted-user")
            .unwrap();
        // Force one recorded second so the id is the only remaining ordering key.
        store
            .connection()
            .execute("UPDATE jobs SET ts = '2024-01-02 03:04:05'", [])
            .unwrap();

        let jobs = store.list_jobs().unwrap();
        assert_eq!(
            jobs.iter()
                .map(|j| j.template_id.as_str())
                .collect::<Vec<_>>(),
            vec!["ghost_tpl", "local_tpl", "failed_tpl", "ok_tpl"]
        );
        assert!(jobs.iter().all(|j| j.timestamp == "2024-01-02T03:04:05Z"));
        assert_eq!(
            (jobs[0].actor_id.as_str(), jobs[0].actor_username.clone()),
            ("deleted-user", None)
        );
        assert_eq!(
            (jobs[1].actor_id.as_str(), jobs[1].actor_username.clone()),
            ("local", None)
        );
        assert_eq!(
            (jobs[2].printer_id.clone(), jobs[2].status.as_str()),
            (None, "failed")
        );
        assert_eq!(jobs[2].error.as_deref(), Some(long_error));
        assert_eq!(jobs[2].actor_username.as_deref(), Some("bob"));
        assert_eq!(jobs[3].printer_id.as_deref(), Some("p1"));
        assert_eq!(jobs[3].actor_username.as_deref(), Some("alice"));
    }

    #[test]
    fn app_settings_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        // absent key reads as None
        assert_eq!(store.get_setting("datetime_formats").unwrap(), None);
        // set then get
        store.set_setting("datetime_formats", "{}").unwrap();
        assert_eq!(
            store.get_setting("datetime_formats").unwrap(),
            Some("{}".to_string())
        );
        // upsert overwrites
        store
            .set_setting("datetime_formats", "{\"d\":\"%d\"}")
            .unwrap();
        assert_eq!(
            store.get_setting("datetime_formats").unwrap(),
            Some("{\"d\":\"%d\"}".to_string())
        );
        // all_settings lists the override row
        let all = store.all_settings().unwrap();
        assert_eq!(
            all.get("datetime_formats"),
            Some(&"{\"d\":\"%d\"}".to_string())
        );
        // delete returns true when a row existed, false when it did not
        assert!(store.delete_setting("datetime_formats").unwrap());
        assert!(!store.delete_setting("datetime_formats").unwrap());
        assert_eq!(store.get_setting("datetime_formats").unwrap(), None);
    }
}

#[cfg(test)]
mod migration_tests {
    use super::*;

    /// #126: an existing database still carrying the dropped column must upgrade cleanly, and a
    /// printer that was disabled must survive as an ordinary printer. It becomes printable — that is
    /// the documented behaviour change, not an accident. `cargo test` otherwise only ever builds
    /// fresh databases, so nothing else exercises the upgrade path a deployment actually takes.
    #[test]
    fn migration_drops_enabled_and_keeps_printers() {
        let mut conn = SqlConnection::open_in_memory().expect("open");
        let migrations = migrations();
        // Version 9 is before printers.enabled was dropped in migration 10
        migrations
            .to_version(&mut conn, 9)
            .expect("migrate to the version before the drop");

        conn.execute(
            "INSERT INTO printers (id, name, kind, config, enabled)
             VALUES ('old', 'Old', 'cups', '{\"uri\":\"ipp://x\"}', 0)",
            [],
        )
        .expect("seed a disabled printer");

        migrations.to_latest(&mut conn).expect("migrate to latest");

        let name: String = conn
            .query_row("SELECT name FROM printers WHERE id = 'old'", [], |r| {
                r.get(0)
            })
            .expect("the printer survives the migration");
        assert_eq!(name, "Old");

        let err = conn
            .prepare("SELECT enabled FROM printers")
            .expect_err("the enabled column must be gone");
        assert!(
            err.to_string().contains("enabled"),
            "expected a no-such-column error, got: {err}"
        );
    }

    #[test]
    fn migration_flattens_printers_and_moves_the_default() {
        let mut conn = SqlConnection::open_in_memory().expect("open");
        let migrations = migrations();
        // Version 13 still stores printers as kind + JSON config + is_default.
        migrations
            .to_version(&mut conn, 13)
            .expect("migrate to version before the flat printer record");
        conn.execute(
            "INSERT INTO printers (id, name, kind, config, is_default) VALUES
             ('full', 'Full', 'cups', '{\"uri\":\"ipps://h/q\",\"username\":\"u\",\"password\":\"p\",\"ca_cert\":\"-----BEGIN CERTIFICATE-----\",\"insecure\":true,\"render\":{\"color_mode\":\"bilevel\",\"resolution\":203}}', 1),
             ('bare', 'Bare', 'cups', '{\"uri\":\"ipp://h/b\"}', 0)",
            [],
        )
        .expect("seed printers");

        migrations.to_latest(&mut conn).expect("migrate to latest");

        type Row = (
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            i64,
            Option<String>,
            Option<i64>,
        );
        let read = |id: &str| -> Row {
            conn.query_row(
                "SELECT name, uri, username, password, ca_cert, insecure, color_mode, resolution
                 FROM printers WHERE id = ?1",
                [id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                    ))
                },
            )
            .expect("printer survives with flat columns")
        };
        assert_eq!(
            read("full"),
            (
                "Full".to_string(),
                "ipps://h/q".to_string(),
                Some("u".to_string()),
                Some("p".to_string()),
                Some("-----BEGIN CERTIFICATE-----".to_string()),
                1,
                Some("bilevel".to_string()),
                Some(203),
            )
        );
        assert_eq!(
            read("bare"),
            (
                "Bare".to_string(),
                "ipp://h/b".to_string(),
                None,
                None,
                None,
                0,
                None,
                None,
            )
        );
        let default: String = conn
            .query_row(
                "SELECT value FROM app_settings WHERE key = 'default_printer_id'",
                [],
                |r| r.get(0),
            )
            .expect("default moved into app_settings");
        assert_eq!(default, "full");
    }

    #[test]
    fn migration_gives_api_tokens_an_owner() {
        let mut conn = SqlConnection::open_in_memory().expect("open");
        let migrations = migrations();
        // Version 14 still has unowned tokens.
        migrations
            .to_version(&mut conn, 14)
            .expect("migrate to version before token owners");
        conn.execute(
            "INSERT INTO api_tokens (id, name, token_hash) VALUES ('t1', 'ci', 'h1')",
            [],
        )
        .expect("seed an unowned token");

        migrations.to_latest(&mut conn).expect("migrate to latest");

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM api_tokens", [], |r| r.get(0))
            .expect("count tokens");
        assert_eq!(count, 0, "unowned tokens cannot survive");
        conn.prepare("SELECT owner FROM api_tokens")
            .expect("api_tokens has an owner column");
    }

    /// Every column of every row, as text, so two dumps of a table compare without caring what
    /// types the columns are.
    fn dump(conn: &SqlConnection, sql: &str) -> Vec<Vec<String>> {
        let mut stmt = conn.prepare(sql).expect("prepare dump");
        let columns = stmt.column_count();
        let mut rows = stmt.query([]).expect("run dump");
        let mut out = Vec::new();
        while let Some(row) = rows.next().expect("next row") {
            let mut cells = Vec::new();
            for i in 0..columns {
                cells.push(match row.get_ref(i).expect("cell") {
                    rusqlite::types::ValueRef::Null => "<null>".to_string(),
                    rusqlite::types::ValueRef::Integer(n) => n.to_string(),
                    rusqlite::types::ValueRef::Real(f) => f.to_string(),
                    rusqlite::types::ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
                    rusqlite::types::ValueRef::Blob(b) => format!("{b:?}"),
                });
            }
            out.push(cells);
        }
        out
    }

    /// #452: an upgrade drops the inventory connections and the default-connection override and
    /// leaves every other stored value alone. A freshly built database never holds that state, so
    /// this seeds one at the version before the drop and reads it back afterwards.
    #[test]
    fn migration_drops_inventory_connections_and_preserves_other_state() {
        let mut conn = SqlConnection::open_in_memory().expect("open");
        let migrations = migrations();
        // Version 15 is the last one that still keeps the connections table.
        migrations
            .to_version(&mut conn, 15)
            .expect("migrate to the version before the drop");

        conn.execute(
            "INSERT INTO connections (id, connector, name, base_url, public_url, credential)
             VALUES ('c1', 'homebox', 'Home', 'http://hb.lan', 'https://pub.lan', 'hb_secret')",
            [],
        )
        .expect("seed a connection");
        for (key, value) in [
            ("default_connection_id", "c1"),
            ("default_printer_id", "p1"),
            ("datetime_formats", "{\"day\":\"%d\"}"),
        ] {
            conn.execute(
                "INSERT INTO app_settings (key, value) VALUES (?1, ?2)",
                rusqlite::params![key, value],
            )
            .expect("seed a setting");
        }
        conn.execute(
            "INSERT INTO printers (id, name, uri, username, password, insecure, color_mode, resolution)
             VALUES ('p1', 'P1', 'ipp://h/p', 'u', 'pw', 0, 'color', 203)",
            [],
        )
        .expect("seed a printer");
        conn.execute(
            "INSERT INTO variables (key, value) VALUES ('qr_base_url', 'https://qr.example')",
            [],
        )
        .expect("seed a variable");
        conn.execute(
            "INSERT INTO users (id, username, password_hash)
             VALUES ('u1', 'alice', 'phc-hash')",
            [],
        )
        .expect("seed a user");
        conn.execute(
            "INSERT INTO sessions (id, user_id, expires_at)
             VALUES ('s1', 'u1', '2030-01-01')",
            [],
        )
        .expect("seed a session");
        conn.execute(
            "INSERT INTO api_tokens (id, owner, name, token_hash)
             VALUES ('t1', 'u1', 'ci', 'h1')",
            [],
        )
        .expect("seed a token");
        conn.execute(
            "INSERT INTO favorites (user_id, template_id) VALUES ('u1', 'brother_18mm')",
            [],
        )
        .expect("seed a favorite");
        conn.execute(
            "INSERT INTO jobs (template, printer, status, error, user_id)
             VALUES ('brother_18mm', 'p1', 'ok', NULL, 'u1')",
            [],
        )
        .expect("seed a job");

        let tables = [
            ("printers", "SELECT * FROM printers ORDER BY id"),
            ("variables", "SELECT * FROM variables ORDER BY key"),
            ("app_settings", "SELECT * FROM app_settings ORDER BY key"),
            ("jobs", "SELECT * FROM jobs ORDER BY id"),
            ("users", "SELECT * FROM users ORDER BY id"),
            ("sessions", "SELECT * FROM sessions ORDER BY id"),
            ("api_tokens", "SELECT * FROM api_tokens ORDER BY id"),
            (
                "favorites",
                "SELECT * FROM favorites ORDER BY user_id, template_id",
            ),
        ];
        let before: Vec<Vec<Vec<String>>> =
            tables.iter().map(|(_, sql)| dump(&conn, sql)).collect();

        migrations.to_latest(&mut conn).expect("migrate to latest");

        conn.prepare("SELECT id FROM connections")
            .expect_err("the connections table must be gone");
        for ((name, sql), was) in tables.iter().zip(&before) {
            let now = dump(&conn, sql);
            let (was, now) = if *name == "app_settings" {
                let keep = |rows: &[Vec<String>]| -> Vec<Vec<String>> {
                    rows.iter()
                        .filter(|r| r[0] != "default_connection_id")
                        .cloned()
                        .collect()
                };
                assert!(
                    now.iter().all(|r| r[0] != "default_connection_id"),
                    "the default inventory connection override must be gone"
                );
                (keep(was), keep(&now))
            } else {
                (was.clone(), now)
            };
            assert_eq!(now, was, "{name} must survive the upgrade unchanged");
        }
    }
}

#[cfg(test)]
mod auth_tests {
    use super::*;

    fn store() -> Store {
        Store::open_in_memory().unwrap()
    }

    #[test]
    fn user_lifecycle_and_count() {
        let s = store();
        assert_eq!(s.count_users().unwrap(), 0);
        let u = s.create_user("alice", "phc-hash").unwrap();
        assert_eq!(u.username, "alice");
        assert_eq!(s.count_users().unwrap(), 1);
        assert!(s.get_user_by_username("alice").unwrap().is_some());
        assert!(s.create_user("alice", "h").is_err()); // unique username
        assert_eq!(s.list_users().unwrap().len(), 1);
    }

    #[test]
    fn session_create_lookup_delete_and_user_cascade() {
        let s = store();
        let u = s.create_user("bob", "h").unwrap();
        let raw = "raw-session-value";
        s.create_session(&crate::auth::sha256_hex(raw), &u.id, "+30 days")
            .unwrap();
        let found = s.lookup_session(&crate::auth::sha256_hex(raw)).unwrap();
        assert_eq!(found.unwrap().username, "bob");
        // delete cascades when the user is removed
        s.delete_user(&u.id).unwrap();
        assert!(s
            .lookup_session(&crate::auth::sha256_hex(raw))
            .unwrap()
            .is_none());
    }

    #[test]
    fn expired_session_not_returned() {
        let s = store();
        let u = s.create_user("carol", "h").unwrap();
        s.create_session("h1", &u.id, "-1 minute").unwrap();
        assert!(s.lookup_session("h1").unwrap().is_none());
    }

    #[test]
    fn token_create_lookup_revoke() {
        let s = store();
        let owner = s.create_user("dora", "h").unwrap();
        let t = s
            .create_token(&owner.id, "ci", &crate::auth::sha256_hex("secretval"))
            .unwrap();
        assert_eq!(t.name, "ci");
        assert_eq!(
            s.lookup_token(&crate::auth::sha256_hex("secretval"))
                .unwrap()
                .map(|u| u.id),
            Some(owner.id.clone())
        );
        assert_eq!(s.list_tokens(&owner.id).unwrap().len(), 1);
        assert!(s.list_tokens("someone-else").unwrap().is_empty());
        assert!(!s.delete_token("someone-else", &t.id).unwrap());
        assert!(s.delete_token(&owner.id, &t.id).unwrap());
        assert!(s
            .lookup_token(&crate::auth::sha256_hex("secretval"))
            .unwrap()
            .is_none());
    }
}
