//! Typed application configuration (distinct from template `variables`). Defaults live here once and
//! are resolved on read; only operator overrides are stored (see ADR-0024). Never interpolated.

use crate::store::{Store, StoreError};

pub const JOB_LOG_RETENTION_DAYS: &str = "job_log_retention_days";
const DEFAULT_RETENTION_DAYS: u32 = 90;

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
    key == JOB_LOG_RETENTION_DAYS
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
}
