//! Typed application configuration (distinct from template `variables`). Defaults live here once and
//! are resolved on read; only operator overrides are stored (see ADR-0024). Never interpolated.

use crate::store::{Store, StoreError};
use std::collections::BTreeMap;

pub const JOB_LOG_RETENTION_DAYS: &str = "job_log_retention_days";
const DEFAULT_RETENTION_DAYS: u32 = 90;

/// Setting key for the named `{datetime.*}` strftime formats (issue #76).
pub const DATETIME_FORMATS: &str = "datetime_formats";

/// Seeded default named formats. Overridable; nothing hardcoded in the renderer.
pub fn default_datetime_formats() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("iso_date".to_string(), "%Y-%m-%d".to_string()),
        ("iso_date_time".to_string(), "%Y-%m-%d %H:%M".to_string()),
        ("short_date".to_string(), "%m/%d/%Y".to_string()),
        ("long_date".to_string(), "%B %-d, %Y".to_string()),
        ("time".to_string(), "%H:%M".to_string()),
    ])
}

/// Errors resolving a setting: a store failure, or a stored override that no longer parses (corruption
/// or manual tampering, since `validate` gates every write).
#[derive(Debug)]
pub enum SettingError {
    Store(StoreError),
    Corrupt { key: String, value: String },
}

impl std::fmt::Display for SettingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SettingError::Store(e) => write!(f, "settings store error: {e}"),
            SettingError::Corrupt { key, value } => {
                write!(f, "stored value for setting '{key}' is invalid: {value:?}")
            }
        }
    }
}

impl std::error::Error for SettingError {}

impl From<StoreError> for SettingError {
    fn from(e: StoreError) -> Self {
        SettingError::Store(e)
    }
}

/// Whether `key` is a setting this build knows about.
pub fn is_known(key: &str) -> bool {
    key == JOB_LOG_RETENTION_DAYS || key == DATETIME_FORMATS
}

/// Validate a JSON value for `key`, returning the canonical text to store, or a client-facing message
/// for a `400`. Callers must check `is_known` first; an unknown key here is a programming error.
pub fn validate(key: &str, value: &serde_json::Value) -> Result<String, String> {
    match key {
        JOB_LOG_RETENTION_DAYS => {
            // as_u64 is Some only for a JSON integer >= 0; floats, strings, and negatives are None.
            let n = value
                .as_u64()
                .filter(|n| *n <= u32::MAX as u64)
                .ok_or_else(|| {
                    format!(
                        "'{JOB_LOG_RETENTION_DAYS}' must be an integer between 0 and {}",
                        u32::MAX
                    )
                })?;
            Ok(n.to_string())
        }
        DATETIME_FORMATS => {
            let obj = value.as_object().ok_or_else(|| {
                format!("'{DATETIME_FORMATS}' must be a JSON object of name -> strftime")
            })?;
            for (name, pattern) in obj {
                if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    return Err(format!(
                        "format name '{name}' must be non-empty and match [A-Za-z0-9_]"
                    ));
                }
                let pat = pattern
                    .as_str()
                    .ok_or_else(|| format!("format '{name}' must be a string"))?;
                crate::datetime_fmt::validate_pattern(pat)
                    .map_err(|e| format!("format '{name}': {e}"))?;
            }
            // Canonical stored text: normalize through a BTreeMap so the serialized order is
            // key-stable regardless of the incoming JSON's insertion order.
            let normalized: BTreeMap<String, serde_json::Value> =
                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            Ok(serde_json::to_string(&normalized).expect("serializing a validated map"))
        }
        _ => Err(format!("unknown setting '{key}'")),
    }
}

/// Pure resolution: in-code default when there is no override, else the parsed override.
pub fn resolve_retention_days_from(stored: Option<String>) -> Result<u32, SettingError> {
    match stored {
        None => Ok(DEFAULT_RETENTION_DAYS),
        Some(s) => s.parse::<u32>().map_err(|_| SettingError::Corrupt {
            key: JOB_LOG_RETENTION_DAYS.to_string(),
            value: s,
        }),
    }
}

/// Resolve the effective `job_log_retention_days` from the store.
pub async fn resolve_retention_days(store: &Store) -> Result<u32, SettingError> {
    let stored = store.get_setting(JOB_LOG_RETENTION_DAYS).await?;
    resolve_retention_days_from(stored)
}

/// Pure resolution: the seeded default map when no override, else the parsed override object.
pub fn resolve_datetime_formats_from(
    stored: Option<String>,
) -> Result<BTreeMap<String, String>, SettingError> {
    match stored {
        None => Ok(default_datetime_formats()),
        Some(s) => {
            let obj: BTreeMap<String, String> =
                serde_json::from_str(&s).map_err(|_| SettingError::Corrupt {
                    key: DATETIME_FORMATS.to_string(),
                    value: s.clone(),
                })?;
            Ok(obj)
        }
    }
}

/// Resolve the effective `datetime_formats` from the store.
pub async fn resolve_datetime_formats(
    store: &Store,
) -> Result<BTreeMap<String, String>, SettingError> {
    let stored = store.get_setting(DATETIME_FORMATS).await?;
    resolve_datetime_formats_from(stored)
}

/// Resolve the live retention and prune the job log once. `0` is a no-op (handled by `prune_jobs`).
pub async fn prune_job_log_once(store: &Store) -> Result<usize, SettingError> {
    let days = resolve_retention_days(store).await?;
    Ok(store.prune_jobs(days).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validate_retention_accepts_non_negative_integer() {
        assert_eq!(validate(JOB_LOG_RETENTION_DAYS, &json!(0)).unwrap(), "0");
        assert_eq!(validate(JOB_LOG_RETENTION_DAYS, &json!(90)).unwrap(), "90");
    }

    #[test]
    fn validate_retention_rejects_bad_values() {
        assert!(validate(JOB_LOG_RETENTION_DAYS, &json!(-1)).is_err());
        assert!(validate(JOB_LOG_RETENTION_DAYS, &json!(90.0)).is_err());
        assert!(validate(JOB_LOG_RETENTION_DAYS, &json!("90")).is_err());
        // above u32::MAX
        assert!(validate(JOB_LOG_RETENTION_DAYS, &json!(4_294_967_296u64)).is_err());
    }

    #[test]
    fn resolve_defaults_to_90_when_absent() {
        assert_eq!(resolve_retention_days_from(None).unwrap(), 90);
    }

    #[test]
    fn resolve_uses_override_when_present() {
        assert_eq!(resolve_retention_days_from(Some("30".into())).unwrap(), 30);
    }

    #[test]
    fn resolve_errors_on_corrupt_override() {
        assert!(resolve_retention_days_from(Some("not-a-number".into())).is_err());
    }

    #[test]
    fn validate_datetime_formats_accepts_valid_map() {
        let v = json!({ "iso_date": "%Y-%m-%d", "t": "%H:%M" });
        assert!(validate(DATETIME_FORMATS, &v).is_ok());
    }

    #[test]
    fn validate_datetime_formats_rejects_bad() {
        assert!(validate(DATETIME_FORMATS, &json!("not-an-object")).is_err());
        assert!(validate(DATETIME_FORMATS, &json!({ "bad name": "%Y" })).is_err());
        assert!(validate(DATETIME_FORMATS, &json!({ "x": "%!" })).is_err()); // invalid specifier (%q is VALID = quarter; use %!)
        assert!(validate(DATETIME_FORMATS, &json!({ "x": 5 })).is_err()); // non-string value
    }

    #[test]
    fn validate_datetime_formats_canonical_text_is_order_stable() {
        let a = validate(DATETIME_FORMATS, &json!({ "a": "%Y", "b": "%m" })).unwrap();
        let b = validate(DATETIME_FORMATS, &json!({ "b": "%m", "a": "%Y" })).unwrap();
        assert_eq!(a, b);
        assert_eq!(a, r#"{"a":"%Y","b":"%m"}"#);
    }

    #[test]
    fn resolve_datetime_formats_defaults_when_absent() {
        let m = resolve_datetime_formats_from(None).unwrap();
        assert_eq!(m.get("iso_date").map(String::as_str), Some("%Y-%m-%d"));
    }

    #[test]
    fn resolve_datetime_formats_uses_override() {
        let stored = Some(r#"{"only":"%Y"}"#.to_string());
        let m = resolve_datetime_formats_from(stored).unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m.get("only").map(String::as_str), Some("%Y"));
    }
}
