//! Typed application configuration (distinct from template `variables`). Defaults live here once and
//! are resolved on read; only operator overrides are stored. Never interpolated.

use crate::store::{Store, StoreError};
use std::collections::BTreeMap;

/// Setting key for the named `{datetime.*}` strftime formats (issue #76).
pub const DATETIME_FORMATS: &str = "datetime_formats";

/// Setting key for the default connection id on the Connect page (issue #203).
pub const DEFAULT_CONNECTION_ID: &str = "default_connection_id";

/// Setting key for the printer the print form preselects.
pub const DEFAULT_PRINTER_ID: &str = "default_printer_id";

/// Every setting this build knows about, in `GET /settings` order.
pub const KNOWN: [&str; 3] = [DATETIME_FORMATS, DEFAULT_CONNECTION_ID, DEFAULT_PRINTER_ID];

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
    KNOWN.contains(&key)
}

/// Validate a JSON value for `key`, returning the canonical text to store, or a client-facing message
/// for a `400`. Callers must check `is_known` first; an unknown key here is a programming error.
pub fn validate(key: &str, value: &serde_json::Value) -> Result<String, String> {
    match key {
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
            Ok(serde_json::to_string(&normalized)
                .map_err(|e| format!("serializing setting value: {e}"))?)
        }
        DEFAULT_CONNECTION_ID | DEFAULT_PRINTER_ID => {
            let s = value
                .as_str()
                .ok_or_else(|| format!("'{key}' must be a string"))?;
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return Err(format!("'{key}' must be non-empty"));
            }
            Ok(trimmed.to_string())
        }
        _ => Err(format!("unknown setting '{key}'")),
    }
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

/// Pure resolution of a default-id setting (`DEFAULT_CONNECTION_ID`, `DEFAULT_PRINTER_ID`): `None`
/// when there is no override, else the stored id. Empty or whitespace-only stored text is corrupt.
pub fn resolve_default_id_from(
    key: &str,
    stored: Option<String>,
) -> Result<Option<String>, SettingError> {
    match stored {
        None => Ok(None),
        Some(s) => {
            if s.trim().is_empty() {
                Err(SettingError::Corrupt {
                    key: key.to_string(),
                    value: s,
                })
            } else {
                Ok(Some(s))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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

    #[test]
    fn validate_default_connection_id_accepts_non_empty_string_and_trims() {
        assert_eq!(
            validate(DEFAULT_CONNECTION_ID, &json!("  conn-1  ")).unwrap(),
            "conn-1"
        );
        assert_eq!(
            validate(DEFAULT_CONNECTION_ID, &json!("conn-1")).unwrap(),
            "conn-1"
        );
    }

    #[test]
    fn validate_default_connection_id_rejects_bad_values() {
        assert!(validate(DEFAULT_CONNECTION_ID, &json!("")).is_err());
        assert!(validate(DEFAULT_CONNECTION_ID, &json!("   ")).is_err());
        assert!(validate(DEFAULT_CONNECTION_ID, &json!(null)).is_err());
        assert!(validate(DEFAULT_CONNECTION_ID, &json!(123)).is_err());
        assert!(validate(DEFAULT_CONNECTION_ID, &json!({})).is_err());
        assert!(validate(DEFAULT_CONNECTION_ID, &json!([])).is_err());
    }

    #[test]
    fn resolve_default_connection_id_defaults_when_absent() {
        assert_eq!(
            resolve_default_id_from(DEFAULT_CONNECTION_ID, None).unwrap(),
            None
        );
    }

    #[test]
    fn resolve_default_connection_id_uses_override_including_dangling() {
        assert_eq!(
            resolve_default_id_from(DEFAULT_CONNECTION_ID, Some("dangling-id".to_string()))
                .unwrap(),
            Some("dangling-id".to_string())
        );
    }

    #[test]
    fn resolve_default_connection_id_rejects_corrupt() {
        assert!(resolve_default_id_from(DEFAULT_CONNECTION_ID, Some("".to_string())).is_err());
        assert!(resolve_default_id_from(DEFAULT_CONNECTION_ID, Some("   ".to_string())).is_err());
    }
}
