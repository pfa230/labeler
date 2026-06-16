use rusqlite::Connection;
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
    conn: Mutex<Connection>,
}

fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(
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
    )])
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let mut conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        migrations().to_latest(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        let mut conn = Connection::open_in_memory()?;
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

    pub async fn get_setting(&self, key: &str) -> Result<Option<String>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query_map([key], |row| row.get::<_, String>(0))?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub async fn set_setting(&self, key: &str, value: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = ?2",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }

    pub async fn all_settings(
        &self,
    ) -> Result<std::collections::BTreeMap<String, String>, StoreError> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare("SELECT key, value FROM settings ORDER BY key")?;
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
    async fn settings_and_jobs() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.get_setting("k").await.unwrap().is_none());
        store.set_setting("k", "v").await.unwrap();
        assert_eq!(store.get_setting("k").await.unwrap().as_deref(), Some("v"));
        store.set_setting("k", "v2").await.unwrap();
        assert_eq!(store.get_setting("k").await.unwrap().as_deref(), Some("v2"));

        let all = store.all_settings().await.unwrap();
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
}
