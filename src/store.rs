use crate::models::{Printer, PrinterConnection, RenderProfile};
use rusqlite::Connection as SqlConnection;
use rusqlite::OptionalExtension;
use rusqlite_migration::{Migrations, M};
use std::path::Path;
use std::sync::Mutex;

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
    pub public_url: Option<String>,
    pub credential: String,
}

#[derive(Debug, Clone)]
pub struct NewConnection<'a> {
    pub connector: &'a str,
    pub name: &'a str,
    pub base_url: &'a str,
    pub public_url: Option<&'a str>,
    pub credential: &'a str,
}

#[derive(Debug, Clone)]
pub struct UpdateConnection<'a> {
    pub name: &'a str,
    pub base_url: &'a str,
    pub public_url: Option<&'a str>,
    pub credential: Option<&'a str>,
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
        let mut stmt = conn.prepare(&format!("{SELECT_PRINTER} ORDER BY id"))?;
        let rows = stmt.query_map([], row_to_printer)?;
        Ok(rows
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|(id, name, connection)| Printer::new(id, name, connection))
            .collect())
    }

    pub async fn get_printer(&self, id: &str) -> Result<Option<Printer>, StoreError> {
        Ok(self
            .get_printer_record(id)?
            .map(|(id, name, connection)| Printer::new(id, name, connection)))
    }

    /// The stored connection, password included, for the driver and the `PUT` keep rule.
    pub async fn get_printer_connection(
        &self,
        id: &str,
    ) -> Result<Option<PrinterConnection>, StoreError> {
        Ok(self
            .get_printer_record(id)?
            .map(|(_, _, connection)| connection))
    }

    fn get_printer_record(&self, id: &str) -> Result<Option<PrinterRecord>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        conn.query_row(
            &format!("{SELECT_PRINTER} WHERE id = ?1"),
            [id],
            row_to_printer,
        )
        .optional()
        .map_err(Into::into)
    }

    pub async fn insert_printer(
        &self,
        id: &str,
        name: &str,
        connection: &PrinterConnection,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("store lock");
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
    pub async fn replace_printer(
        &self,
        id: &str,
        name: &str,
        connection: &PrinterConnection,
    ) -> Result<bool, StoreError> {
        let conn = self.conn.lock().expect("store lock");
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
    /// that printer; see `delete_connection_and_default` for why there is no plain delete.
    pub async fn delete_printer_and_default(&self, id: &str) -> Result<bool, StoreError> {
        let mut conn = self.conn.lock().expect("store lock");
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
        user_id: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO jobs (template, printer, status, error, user_id) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![template, printer, status, error, user_id],
        )?;
        Ok(())
    }

    pub async fn list_favorites(&self, user_id: &str) -> Result<Vec<String>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
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

    pub async fn add_favorite(&self, user_id: &str, template_id: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT OR IGNORE INTO favorites (user_id, template_id) VALUES (?1, ?2)",
            rusqlite::params![user_id, template_id],
        )?;
        Ok(())
    }

    pub async fn remove_favorite(
        &self,
        user_id: &str,
        template_id: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "DELETE FROM favorites WHERE user_id = ?1 AND template_id = ?2",
            rusqlite::params![user_id, template_id],
        )?;
        Ok(())
    }

    /// Drop every user's favorite row for `template_id`. Called when the template is deleted so a
    /// later template reusing the id does not inherit favorites pointing at the old one. Not scoped
    /// to one actor: the delete invalidates the row for everybody, not just the caller (#140).
    pub async fn remove_favorites_for_template(
        &self,
        template_id: &str,
    ) -> Result<usize, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        let removed = conn.execute(
            "DELETE FROM favorites WHERE template_id = ?1",
            rusqlite::params![template_id],
        )?;
        Ok(removed)
    }

    /// The 6 most recently printed distinct templates for this user (deterministic: MAX(id) tiebreak).
    pub async fn recent_templates(&self, user_id: &str) -> Result<Vec<String>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
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
    pub async fn create_token(
        &self,
        owner: &str,
        name: &str,
        token_hash: &str,
    ) -> Result<ApiToken, StoreError> {
        let id = crate::auth::random_secret();
        let conn = self.conn.lock().expect("store lock");
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
    pub async fn lookup_token(&self, token_hash: &str) -> Result<Option<User>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
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

    pub async fn list_tokens(&self, owner: &str) -> Result<Vec<ApiToken>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
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

    pub async fn delete_token(&self, owner: &str, id: &str) -> Result<bool, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        Ok(conn.execute(
            "DELETE FROM api_tokens WHERE id = ?1 AND owner = ?2",
            rusqlite::params![id, owner],
        )? > 0)
    }

    // Connections
    pub async fn create_connection(
        &self,
        new: NewConnection<'_>,
    ) -> Result<Connection, StoreError> {
        let id = crate::auth::random_secret();
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO connections (id, connector, name, base_url, public_url, credential) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![id, new.connector, new.name, new.base_url, new.public_url, new.credential],
        )?;
        Ok(Connection {
            id,
            connector: new.connector.to_string(),
            name: new.name.to_string(),
            base_url: new.base_url.to_string(),
            public_url: new.public_url.map(ToString::to_string),
            credential: new.credential.to_string(),
        })
    }

    pub async fn get_connection(&self, id: &str) -> Result<Option<Connection>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        conn.query_row(
            "SELECT id, connector, name, base_url, credential, public_url FROM connections WHERE id = ?1",
            [id],
            row_to_connection,
        )
        .optional()
        .map_err(Into::into)
    }

    pub async fn list_connections(&self) -> Result<Vec<Connection>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, connector, name, base_url, credential, public_url FROM connections ORDER BY name, id",
        )?;
        let rows = stmt.query_map([], row_to_connection)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub async fn update_connection(
        &self,
        id: &str,
        update: UpdateConnection<'_>,
    ) -> Result<bool, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        let n = match update.credential {
            Some(credential) => conn.execute(
                "UPDATE connections SET name = ?1, base_url = ?2, public_url = ?3, credential = ?4 WHERE id = ?5",
                rusqlite::params![update.name, update.base_url, update.public_url, credential, id],
            )?,
            None => conn.execute(
                "UPDATE connections SET name = ?1, base_url = ?2, public_url = ?3 WHERE id = ?4",
                rusqlite::params![update.name, update.base_url, update.public_url, id],
            )?,
        };
        Ok(n > 0)
    }

    /// Delete a connection and, in the same transaction, the `default_connection_id` setting when it
    /// named that connection. There is deliberately no plain `delete_connection`: a delete that does
    /// not clear the setting would leave a default naming a connection that no longer exists, and
    /// `GET /api/settings` takes no lock, so a second statement is a window a reader can land in.
    pub async fn delete_connection_and_default(&self, id: &str) -> Result<bool, StoreError> {
        let mut conn = self.conn.lock().expect("store lock");
        let tx = conn.transaction()?;
        let existed = tx.execute("DELETE FROM connections WHERE id = ?1", [id])? > 0;
        if existed {
            tx.execute(
                "DELETE FROM app_settings WHERE key = ?1 AND value = ?2",
                rusqlite::params![crate::settings::DEFAULT_CONNECTION_ID, id],
            )?;
        }
        tx.commit()?;
        Ok(existed)
    }
}

fn row_to_connection(r: &rusqlite::Row<'_>) -> rusqlite::Result<Connection> {
    Ok(Connection {
        id: r.get(0)?,
        connector: r.get(1)?,
        name: r.get(2)?,
        base_url: r.get(3)?,
        credential: r.get(4)?,
        public_url: r.get(5)?,
    })
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

    #[tokio::test]
    async fn printer_crud_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.list_printers().await.unwrap().is_empty());

        store
            .insert_printer("p1", "P1", &connection("ipp://x", Some("s")))
            .await
            .unwrap();

        let got = store.get_printer("p1").await.unwrap().unwrap();
        assert_eq!((got.name.as_str(), got.uri.as_str()), ("P1", "ipp://x"));
        assert_eq!(got.username.as_deref(), Some("u"));
        assert!(got.insecure);
        assert_eq!(got.render.and_then(|r| r.resolution), Some(300));
        let stored = store.get_printer_connection("p1").await.unwrap().unwrap();
        assert_eq!(stored.password.as_deref(), Some("s"));
        assert_eq!(store.list_printers().await.unwrap().len(), 1);

        assert!(store
            .replace_printer("p1", "P1b", &connection("ipp://y", None))
            .await
            .unwrap());
        let got = store.get_printer_connection("p1").await.unwrap().unwrap();
        assert_eq!((got.uri.as_str(), got.password), ("ipp://y", None));
        assert_eq!(store.get_printer("p1").await.unwrap().unwrap().name, "P1b");
        assert!(!store
            .replace_printer("ghost", "G", &connection("ipp://y", None))
            .await
            .unwrap());

        assert!(store.delete_printer_and_default("p1").await.unwrap());
        assert!(store.get_printer("p1").await.unwrap().is_none());
        assert!(!store.delete_printer_and_default("p1").await.unwrap());
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
            .record_job("tpl", Some("p1"), "ok", None, "u1")
            .await
            .unwrap();
        store
            .record_job("tpl", None, "failed", Some("boom"), "u1")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn favorites_add_idempotent_list_and_remove() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.list_favorites("u1").await.unwrap().is_empty());
        store.add_favorite("u1", "tpl_a").await.unwrap();
        store.add_favorite("u1", "tpl_a").await.unwrap(); // idempotent
        store.add_favorite("u1", "tpl_b").await.unwrap();
        // per-user isolation
        store.add_favorite("u2", "tpl_c").await.unwrap();
        assert_eq!(
            store.list_favorites("u1").await.unwrap(),
            vec!["tpl_a".to_string(), "tpl_b".to_string()]
        );
        assert_eq!(
            store.list_favorites("u2").await.unwrap(),
            vec!["tpl_c".to_string()]
        );
        store.remove_favorite("u1", "tpl_a").await.unwrap();
        store.remove_favorite("u1", "tpl_a").await.unwrap(); // idempotent
        assert_eq!(
            store.list_favorites("u1").await.unwrap(),
            vec!["tpl_b".to_string()]
        );
    }

    #[tokio::test]
    async fn remove_favorites_for_template_drops_every_users_row() {
        let store = Store::open_in_memory().unwrap();
        store.add_favorite("u1", "t1").await.unwrap();
        store.add_favorite("u2", "t1").await.unwrap();
        store.add_favorite("u1", "t2").await.unwrap();

        // Favorites are keyed by actor, so deleting the template must clear it for everyone.
        assert_eq!(store.remove_favorites_for_template("t1").await.unwrap(), 2);
        assert_eq!(
            store.list_favorites("u1").await.unwrap(),
            vec!["t2".to_string()]
        );
        assert!(store.list_favorites("u2").await.unwrap().is_empty());
        assert_eq!(store.remove_favorites_for_template("t1").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn recent_templates_distinct_ordered_and_per_user() {
        let store = Store::open_in_memory().unwrap();
        // Same ts for a and b; MAX(id) tiebreak makes b (inserted later) rank first.
        store.record_job("a", None, "ok", None, "u1").await.unwrap();
        store.record_job("a", None, "ok", None, "u1").await.unwrap();
        store.record_job("b", None, "ok", None, "u1").await.unwrap();
        store.record_job("c", None, "ok", None, "u2").await.unwrap();
        let recents = store.recent_templates("u1").await.unwrap();
        assert_eq!(recents, vec!["b".to_string(), "a".to_string()]);
        assert_eq!(
            store.recent_templates("u2").await.unwrap(),
            vec!["c".to_string()]
        );
        assert!(store.recent_templates("nobody").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn app_settings_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        // absent key reads as None
        assert_eq!(store.get_setting("datetime_formats").await.unwrap(), None);
        // set then get
        store.set_setting("datetime_formats", "{}").await.unwrap();
        assert_eq!(
            store.get_setting("datetime_formats").await.unwrap(),
            Some("{}".to_string())
        );
        // upsert overwrites
        store
            .set_setting("datetime_formats", "{\"d\":\"%d\"}")
            .await
            .unwrap();
        assert_eq!(
            store.get_setting("datetime_formats").await.unwrap(),
            Some("{\"d\":\"%d\"}".to_string())
        );
        // all_settings lists the override row
        let all = store.all_settings().await.unwrap();
        assert_eq!(
            all.get("datetime_formats"),
            Some(&"{\"d\":\"%d\"}".to_string())
        );
        // delete returns true when a row existed, false when it did not
        assert!(store.delete_setting("datetime_formats").await.unwrap());
        assert!(!store.delete_setting("datetime_formats").await.unwrap());
        assert_eq!(store.get_setting("datetime_formats").await.unwrap(), None);
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
    fn migration_adds_public_url_to_connections() {
        let mut conn = SqlConnection::open_in_memory().expect("open");
        let migrations = migrations();
        // Version 10 is before public_url was added in migration 11
        migrations
            .to_version(&mut conn, 10)
            .expect("migrate to version before public_url");

        conn.execute(
            "INSERT INTO connections (id, connector, name, base_url, credential, enabled)
             VALUES ('c1', 'homebox', 'Home', 'http://hb.lan', 'secret', 1)",
            [],
        )
        .expect("seed connection");

        migrations.to_latest(&mut conn).expect("migrate to latest");

        let (name, public_url): (String, Option<String>) = conn
            .query_row(
                "SELECT name, public_url FROM connections WHERE id = 'c1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("connection survives with public_url as NULL");
        assert_eq!(name, "Home");
        assert_eq!(public_url, None);
    }

    /// #416: the connection record has no `enabled` and no `transforms`. An existing database drops
    /// both columns, and its connections survive with every field the record keeps.
    #[test]
    fn migration_drops_enabled_and_transforms_from_connections() {
        let mut conn = SqlConnection::open_in_memory().expect("open");
        let migrations = migrations();
        // Version 12 is before both columns were dropped in migration 13
        migrations
            .to_version(&mut conn, 12)
            .expect("migrate to the version before the drop");

        conn.execute(
            "INSERT INTO connections (id, connector, name, base_url, public_url, credential, enabled, transforms)
             VALUES ('c1', 'homebox', 'Home', 'http://hb.lan', 'https://pub.lan', 'secret', 0, '[]')",
            [],
        )
        .expect("seed a disabled connection with a transform rule");

        migrations.to_latest(&mut conn).expect("migrate to latest");

        let row: (String, String, String, String, Option<String>, String) = conn
            .query_row(
                "SELECT id, connector, name, base_url, public_url, credential FROM connections",
                [],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                },
            )
            .expect("the connection survives the migration");
        assert_eq!(
            row,
            (
                "c1".to_string(),
                "homebox".to_string(),
                "Home".to_string(),
                "http://hb.lan".to_string(),
                Some("https://pub.lan".to_string()),
                "secret".to_string()
            )
        );

        for column in ["enabled", "transforms"] {
            let err = conn
                .prepare(&format!("SELECT {column} FROM connections"))
                .expect_err("the column must be gone");
            assert!(
                err.to_string().contains(column),
                "expected a no-such-column error, got: {err}"
            );
        }
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
            .create_connection(NewConnection {
                connector: "homebox",
                name: "home",
                base_url: "http://hb.lan:7745",
                public_url: None,
                credential: "hb_secret",
            })
            .await
            .unwrap();
        assert_eq!(c.connector, "homebox");
        assert_eq!(c.credential, "hb_secret");
        assert_eq!(c.public_url, None);
        assert!(s.get_connection(&c.id).await.unwrap().is_some());
        assert_eq!(s.list_connections().await.unwrap().len(), 1);
        // update name + keep credential (None = unchanged)
        assert!(s
            .update_connection(
                &c.id,
                UpdateConnection {
                    name: "renamed",
                    base_url: "http://hb.lan:7745",
                    public_url: None,
                    credential: None,
                },
            )
            .await
            .unwrap());
        let g = s.get_connection(&c.id).await.unwrap().unwrap();
        assert_eq!(g.name, "renamed");
        assert_eq!(g.credential, "hb_secret"); // unchanged
        assert_eq!(g.public_url, None);
        // update credential
        assert!(s
            .update_connection(
                &c.id,
                UpdateConnection {
                    name: "renamed",
                    base_url: "http://hb.lan:7745",
                    public_url: None,
                    credential: Some("hb_new"),
                },
            )
            .await
            .unwrap());
        assert_eq!(
            s.get_connection(&c.id).await.unwrap().unwrap().credential,
            "hb_new"
        );
        assert!(s.delete_connection_and_default(&c.id).await.unwrap());
        assert!(s.get_connection(&c.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn connection_crud_with_public_url_and_update_fields() {
        let store = Store::open_in_memory().unwrap();
        let c = store
            .create_connection(NewConnection {
                connector: "homebox",
                name: "Home",
                base_url: "http://homebox.lan:7745",
                public_url: Some("https://homebox.example.com"),
                credential: "token123",
            })
            .await
            .unwrap();
        assert_eq!(c.public_url.as_deref(), Some("https://homebox.example.com"));

        let fetched = store.get_connection(&c.id).await.unwrap().unwrap();
        assert_eq!(
            fetched.public_url.as_deref(),
            Some("https://homebox.example.com")
        );

        // An update replaces the record: no public_url clears it
        store
            .update_connection(
                &c.id,
                UpdateConnection {
                    name: "Home Updated",
                    base_url: "http://homebox.lan:7745",
                    public_url: None,
                    credential: None,
                },
            )
            .await
            .unwrap();
        let cleared = store.get_connection(&c.id).await.unwrap().unwrap();
        assert_eq!(cleared.name, "Home Updated");
        assert_eq!(cleared.public_url, None);
        assert_eq!(cleared.credential, "token123");

        store
            .update_connection(
                &c.id,
                UpdateConnection {
                    name: "Home Updated",
                    base_url: "http://homebox.lan:7745",
                    public_url: Some("https://new.example.com"),
                    credential: None,
                },
            )
            .await
            .unwrap();
        let set_again = store.get_connection(&c.id).await.unwrap().unwrap();
        assert_eq!(
            set_again.public_url.as_deref(),
            Some("https://new.example.com")
        );
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
        let owner = s.create_user("dora", "h").await.unwrap();
        let t = s
            .create_token(&owner.id, "ci", &crate::auth::sha256_hex("secretval"))
            .await
            .unwrap();
        assert_eq!(t.name, "ci");
        assert_eq!(
            s.lookup_token(&crate::auth::sha256_hex("secretval"))
                .await
                .unwrap()
                .map(|u| u.id),
            Some(owner.id.clone())
        );
        assert_eq!(s.list_tokens(&owner.id).await.unwrap().len(), 1);
        assert!(s.list_tokens("someone-else").await.unwrap().is_empty());
        assert!(!s.delete_token("someone-else", &t.id).await.unwrap());
        assert!(s.delete_token(&owner.id, &t.id).await.unwrap());
        assert!(s
            .lookup_token(&crate::auth::sha256_hex("secretval"))
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn list_connections_orders_by_name_then_id() {
        let s = store();
        {
            let conn = s.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO connections (id, connector, name, base_url, credential) VALUES ('b', 'homebox', 'Homebox', 'http://b.lan', 'sec')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO connections (id, connector, name, base_url, credential) VALUES ('a', 'homebox', 'Homebox', 'http://a.lan', 'sec')",
                [],
            )
            .unwrap();
        }
        let list = s.list_connections().await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "a");
        assert_eq!(list[1].id, "b");
    }

    #[tokio::test]
    async fn delete_connection_and_default_cascade() {
        let s = store();
        let c1 = s
            .create_connection(NewConnection {
                connector: "homebox",
                name: "c1",
                base_url: "http://c1.lan",
                public_url: None,
                credential: "sec",
            })
            .await
            .unwrap();
        let c2 = s
            .create_connection(NewConnection {
                connector: "homebox",
                name: "c2",
                base_url: "http://c2.lan",
                public_url: None,
                credential: "sec",
            })
            .await
            .unwrap();

        s.set_setting("default_connection_id", &c1.id)
            .await
            .unwrap();
        assert_eq!(
            s.get_setting("default_connection_id").await.unwrap(),
            Some(c1.id.clone())
        );

        // Deleting unknown id returns false and clears nothing
        assert!(!s.delete_connection_and_default("unknown-id").await.unwrap());
        assert_eq!(
            s.get_setting("default_connection_id").await.unwrap(),
            Some(c1.id.clone())
        );

        // Deleting non-default connection c2 returns true and leaves default_connection_id intact
        assert!(s.delete_connection_and_default(&c2.id).await.unwrap());
        assert_eq!(
            s.get_setting("default_connection_id").await.unwrap(),
            Some(c1.id.clone())
        );

        // Deleting default connection c1 returns true and clears default_connection_id
        assert!(s.delete_connection_and_default(&c1.id).await.unwrap());
        assert_eq!(s.get_setting("default_connection_id").await.unwrap(), None);
    }
}
