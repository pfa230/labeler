//! Current-time interpolation support (issue #76): strftime validation/formatting and the
//! `{datetime.*}` namespace resolver. Formats come from the `datetime_formats` app setting; the
//! captured `now` is server-local (`chrono::Local`).

use crate::errors::AppError;
use chrono::format::{Item, StrftimeItems};
use chrono::{DateTime, Local};
use std::collections::BTreeMap;

use chrono::{LocalResult, NaiveDate, NaiveDateTime, TimeZone};

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

/// Parse a datetime override string in a given timezone.
/// Accepts:
/// - `YYYY-MM-DD` (midnight in tz)
/// - `YYYY-MM-DDTHH:MM:SS` (wall-clock in tz)
/// - `YYYY-MM-DDTHH:MM` (wall-clock in tz)
/// - RFC 3339 timestamp with offset or Z, converted to tz
pub fn parse_datetime_in_tz<Tz: TimeZone>(s: &str, tz: &Tz) -> Result<DateTime<Tz>, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("datetime string is empty".to_string());
    }

    // 1. Date only: YYYY-MM-DD -> local midnight
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        if let Some(ndt) = date.and_hms_opt(0, 0, 0) {
            return match tz.from_local_datetime(&ndt) {
                LocalResult::Single(dt) => Ok(dt),
                LocalResult::Ambiguous(earlier, _later) => Ok(earlier),
                LocalResult::None => Err(
                    "local date/time does not exist due to daylight saving transition".to_string(),
                ),
            };
        }
    }

    // 2. Local date-and-time with seconds: YYYY-MM-DDTHH:MM:SS
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return match tz.from_local_datetime(&ndt) {
            LocalResult::Single(dt) => Ok(dt),
            LocalResult::Ambiguous(earlier, _later) => Ok(earlier),
            LocalResult::None => {
                Err("local date/time does not exist due to daylight saving transition".to_string())
            }
        };
    }

    // 3. Local date-and-time without seconds: YYYY-MM-DDTHH:MM
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M") {
        return match tz.from_local_datetime(&ndt) {
            LocalResult::Single(dt) => Ok(dt),
            LocalResult::Ambiguous(earlier, _later) => Ok(earlier),
            LocalResult::None => {
                Err("local date/time does not exist due to daylight saving transition".to_string())
            }
        };
    }

    // 4. RFC 3339 timestamp with offset or Z
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(tz));
    }

    Err(format!("cannot parse datetime override '{s}'"))
}

/// Parse a datetime override string in the server-local timezone (`chrono::Local`).
pub fn parse_datetime_override(s: &str) -> Result<DateTime<Local>, String> {
    parse_datetime_in_tz(s, &Local)
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

    /// Resolve a token against the template's declared datetime parameters.
    /// Returns `Some(Ok(str))` if resolved, `Some(Err(AppError))` if format name is unknown,
    /// or `None` if token head does not name a datetime parameter.
    pub fn resolve_param(
        &self,
        token: &str,
        instants: &BTreeMap<String, DateTime<Local>>,
    ) -> Option<Result<String, AppError>> {
        let (head, tail) = match token.split_once('.') {
            Some((h, t)) => (h, Some(t)),
            None => (token, None),
        };
        let instant = instants.get(head)?;
        Some(match tail {
            None => Ok(format_now(BARE_DATETIME_FORMAT, *instant)),
            Some(fmt_name) => match self.formats.get(fmt_name) {
                Some(pattern) => Ok(format_now(pattern, *instant)),
                None => Err(AppError::missing_field(&format!("{head}.{fmt_name}"))),
            },
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
            ("long_date".to_string(), "%B %-d, %Y".to_string()),
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

    #[test]
    fn resolve_param_bare_and_dotted() {
        let r = DateTimeResolver {
            formats: &formats(),
            now: fixed_now(),
        };
        let instant = Local
            .with_ymd_and_hms(2026, 8, 19, 14, 30, 0)
            .single()
            .unwrap();
        let mut instants = BTreeMap::new();
        instants.insert("printed_on".to_string(), instant);

        assert_eq!(
            r.resolve_param("printed_on", &instants).unwrap().unwrap(),
            "2026-08-19"
        );
        assert_eq!(
            r.resolve_param("printed_on.long_date", &instants)
                .unwrap()
                .unwrap(),
            "August 19, 2026"
        );
        assert_eq!(
            r.resolve_param("printed_on.time", &instants)
                .unwrap()
                .unwrap(),
            "14:30"
        );
        assert!(r
            .resolve_param("printed_on.no_such_fmt", &instants)
            .unwrap()
            .is_err());
        assert!(r.resolve_param("other_param", &instants).is_none());
        assert!(r.resolve_param("other_param.foo", &instants).is_none());
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct MockDstTz;

    impl TimeZone for MockDstTz {
        type Offset = chrono::FixedOffset;

        fn from_offset(_offset: &Self::Offset) -> Self {
            MockDstTz
        }

        fn offset_from_local_date(&self, _local: &chrono::NaiveDate) -> LocalResult<Self::Offset> {
            LocalResult::Single(chrono::FixedOffset::east_opt(0).unwrap())
        }

        fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> LocalResult<Self::Offset> {
            use chrono::Timelike;
            if local.date() == NaiveDate::from_ymd_opt(2026, 3, 29).unwrap()
                && local.time().hour() == 2
            {
                LocalResult::None
            } else if local.date() == NaiveDate::from_ymd_opt(2026, 10, 25).unwrap()
                && local.time().hour() == 2
            {
                LocalResult::Ambiguous(
                    chrono::FixedOffset::east_opt(7200).unwrap(), // earlier (+2h)
                    chrono::FixedOffset::east_opt(3600).unwrap(), // later (+1h)
                )
            } else {
                LocalResult::Single(chrono::FixedOffset::east_opt(3600).unwrap())
            }
        }

        fn offset_from_utc_date(&self, _utc: &chrono::NaiveDate) -> Self::Offset {
            chrono::FixedOffset::east_opt(3600).unwrap()
        }

        fn offset_from_utc_datetime(&self, _utc: &NaiveDateTime) -> Self::Offset {
            chrono::FixedOffset::east_opt(3600).unwrap()
        }
    }

    #[test]
    fn parse_datetime_override_accepted_forms_and_trim() {
        let parsed = parse_datetime_override(" 2026-08-19 ").unwrap();
        assert_eq!(parsed.format("%Y-%m-%d").to_string(), "2026-08-19");

        let parsed = parse_datetime_override("2026-08-19T14:30").unwrap();
        assert_eq!(
            parsed.format("%Y-%m-%dT%H:%M").to_string(),
            "2026-08-19T14:30"
        );

        let parsed = parse_datetime_override("2026-08-19T14:30:45").unwrap();
        assert_eq!(
            parsed.format("%Y-%m-%dT%H:%M:%S").to_string(),
            "2026-08-19T14:30:45"
        );

        let parsed = parse_datetime_override("2026-08-19T14:30:00Z").unwrap();
        assert_eq!(parsed.format("%Y-%m-%d").to_string(), "2026-08-19");

        assert!(parse_datetime_override("yesterday").is_err());
        assert!(parse_datetime_override("").is_err());
        assert!(parse_datetime_override("   ").is_err());
    }

    #[test]
    fn parse_datetime_in_tz_dst_gap_and_ambiguity() {
        let tz = MockDstTz;

        // Gap: 2026-03-29 02:30 does not exist
        let gap_err = parse_datetime_in_tz("2026-03-29T02:30", &tz);
        assert!(gap_err.is_err(), "DST gap must be rejected");

        // Ambiguity: 2026-10-25 02:30 resolves to earlier instant (offset +2h = 7200)
        let amb_res = parse_datetime_in_tz("2026-10-25T02:30", &tz).unwrap();
        assert_eq!(amb_res.offset().local_minus_utc(), 7200);
    }
}
