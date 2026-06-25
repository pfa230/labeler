//! Current-time interpolation support (issue #76): strftime validation/formatting and the
//! `{datetime.*}` namespace resolver. Formats come from the `datetime_formats` app setting; the
//! captured `now` is server-local (`chrono::Local`).

use crate::errors::AppError;
use chrono::format::{Item, StrftimeItems};
use chrono::{DateTime, Local};
use std::collections::BTreeMap;

/// Format used by the bare `{datetime}` token. ISO 8601 date; always resolvable.
pub const BARE_DATETIME_FORMAT: &str = "%Y-%m-%d";

/// Validate a strftime pattern. `Err(msg)` if it contains an invalid specifier.
pub fn validate_pattern(pattern: &str) -> Result<(), String> {
    for item in StrftimeItems::new(pattern) {
        if matches!(item, Item::Error) {
            return Err(format!("invalid strftime pattern: {pattern:?}"));
        }
    }
    Ok(())
}

/// Format `now` with `pattern`. Uses lenient parsing so a stray bad specifier renders best-effort
/// instead of panicking (patterns are validated before storage, so this is defense in depth).
pub fn format_now(pattern: &str, now: DateTime<Local>) -> String {
    now.format_with_items(StrftimeItems::new_lenient(pattern))
        .to_string()
}

/// Resolves the `datetime` interpolation namespace. Holds the configured formats and a single
/// captured instant so every token in one render shares the same `now`.
pub struct DateTimeResolver<'a> {
    pub formats: &'a BTreeMap<String, String>,
    pub now: DateTime<Local>,
}

impl DateTimeResolver<'_> {
    /// `Some(Ok)` for a resolved datetime token, `Some(Err)` for an unknown named format, `None`
    /// if `token` is not in the datetime namespace (so the caller falls through to vars/data).
    pub fn resolve(&self, token: &str) -> Option<Result<String, AppError>> {
        if token == "datetime" {
            return Some(Ok(format_now(BARE_DATETIME_FORMAT, self.now)));
        }
        let name = token.strip_prefix("datetime.")?;
        Some(match self.formats.get(name) {
            Some(pattern) => Ok(format_now(pattern, self.now)),
            None => Err(AppError::missing_field(&format!("datetime.{name}"))),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    // A fixed LOCAL wall-clock instant: formatting uses the components we supply, so output is
    // deterministic regardless of the machine timezone. 2026-06-25 14:30:00 is not a DST edge.
    fn fixed_now() -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 6, 25, 14, 30, 0)
            .single()
            .unwrap()
    }

    fn formats() -> BTreeMap<String, String> {
        BTreeMap::from([
            ("short_date".to_string(), "%m/%d/%Y".to_string()),
            ("time".to_string(), "%H:%M".to_string()),
        ])
    }

    #[test]
    fn validate_accepts_good_and_rejects_bad() {
        assert!(validate_pattern("%Y-%m-%d %H:%M").is_ok());
        assert!(validate_pattern("%B %-d, %Y").is_ok());
        assert!(validate_pattern("%!").is_err()); // %! is not a valid specifier in chrono 0.4
    }

    #[test]
    fn format_now_is_deterministic() {
        assert_eq!(format_now("%Y-%m-%d", fixed_now()), "2026-06-25");
        assert_eq!(format_now("%m/%d/%Y", fixed_now()), "06/25/2026");
        assert_eq!(format_now("%H:%M", fixed_now()), "14:30");
    }

    #[test]
    fn resolve_bare_datetime_is_iso_date() {
        let r = DateTimeResolver {
            formats: &formats(),
            now: fixed_now(),
        };
        assert_eq!(r.resolve("datetime").unwrap().unwrap(), "2026-06-25");
    }

    #[test]
    fn resolve_named_format() {
        let r = DateTimeResolver {
            formats: &formats(),
            now: fixed_now(),
        };
        assert_eq!(
            r.resolve("datetime.short_date").unwrap().unwrap(),
            "06/25/2026"
        );
    }

    #[test]
    fn resolve_unknown_named_format_errors() {
        let r = DateTimeResolver {
            formats: &formats(),
            now: fixed_now(),
        };
        assert!(r.resolve("datetime.nope").unwrap().is_err());
    }

    #[test]
    fn resolve_non_datetime_token_is_none() {
        let r = DateTimeResolver {
            formats: &formats(),
            now: fixed_now(),
        };
        assert!(r.resolve("vars.x").is_none());
        assert!(r.resolve("title").is_none());
        assert!(r.resolve("datetimefoo").is_none()); // no dot, not the bare token
    }
}
