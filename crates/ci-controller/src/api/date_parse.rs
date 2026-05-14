use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};

use super::error::ApiError;

/// Parse a Chola date expression (relative or absolute).
///
/// Accepted forms (tried in priority order):
/// 1. Relative: `now`, `now-1h`, `now+30m`, `now-7d` etc.
///    Units: `s` seconds, `m` minutes, `h` hours, `d` days,
///           `w` weeks, `M` months (30 d), `y` years (365 d).
///    Note: `m` = minutes, `M` = months (case-sensitive on unit, like Kibana).
///    The `now` keyword itself is case-insensitive.
/// 2. Full RFC3339: `2026-04-23T15:30:00Z`, `2026-04-23T15:30:00+05:30`
/// 3. ISO datetime-local without seconds: `2026-04-23T15:30`
/// 4. ISO datetime-local with seconds: `2026-04-23T15:30:00`
/// 5. Date-only: `2026-04-23` — start-of-day UTC when `end_of_day=false`,
///    end-of-day (23:59:59 UTC) when `end_of_day=true`.
///
/// Anything else returns 400 with a descriptive message.
pub fn parse_flexible_datetime(
    field: &str,
    value: &str,
    end_of_day: bool,
) -> Result<DateTime<Utc>, ApiError> {
    let trimmed = value.trim();

    if let Some(dt) = try_parse_relative(trimmed)? {
        return Ok(dt);
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return Ok(dt.with_timezone(&Utc));
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M") {
        return Ok(Utc.from_utc_datetime(&naive));
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S") {
        return Ok(Utc.from_utc_datetime(&naive));
    }
    if let Ok(date) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        let naive = if end_of_day {
            date.and_hms_opt(23, 59, 59)
                .expect("23:59:59 is always valid")
        } else {
            date.and_hms_opt(0, 0, 0).expect("00:00:00 is always valid")
        };
        return Ok(Utc.from_utc_datetime(&naive));
    }
    Err(ApiError::BadRequest(format!(
        "Invalid {field} (expected now/now\u{b1}<n><unit>, RFC3339, ISO datetime, or YYYY-MM-DD): {value:?}"
    )))
}

/// Try to parse `now` or `now[+-]<n><unit>`.
/// Returns `Ok(None)` when the value does not look like a relative expression at all.
/// Returns `Ok(Some(dt))` on success.
/// Returns `Err` when the value starts with `now` but has an invalid suffix.
fn try_parse_relative(s: &str) -> Result<Option<DateTime<Utc>>, ApiError> {
    // Case-insensitive "now" check.
    let lower = s.to_ascii_lowercase();
    if lower == "now" {
        return Ok(Some(Utc::now()));
    }
    if !lower.starts_with("now") {
        return Ok(None);
    }

    // Expect `now` followed by optional whitespace, then `+` or `-`, then digits, then unit.
    let rest = s[3..].trim_start();
    let sign = match rest.chars().next() {
        Some('+') => 1i64,
        Some('-') => -1i64,
        _ => {
            return Err(ApiError::BadRequest(format!(
                "Invalid relative date {s:?}: expected `now`, `now+<n><unit>`, or `now-<n><unit>`"
            )))
        }
    };
    let after_sign = rest[1..].trim_start();

    // Split into digits and unit character.
    let digit_end = after_sign
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(after_sign.len());
    if digit_end == 0 {
        return Err(ApiError::BadRequest(format!(
            "Invalid relative date {s:?}: missing numeric amount"
        )));
    }
    let n: i64 = after_sign[..digit_end]
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("Invalid amount in {s:?}")))?;

    let unit_str = after_sign[digit_end..].trim_start();
    if unit_str.is_empty() {
        return Err(ApiError::BadRequest(format!(
            "Invalid relative date {s:?}: missing unit (s/m/h/d/w/M/y)"
        )));
    }
    // Only accept a single unit character; reject trailing garbage.
    let unit_char = unit_str.chars().next().expect("non-empty checked above");
    if unit_str.chars().count() > 1 {
        return Err(ApiError::BadRequest(format!(
            "Invalid relative date {s:?}: unknown unit {unit_str:?} — use one of s/m/h/d/w/M/y"
        )));
    }

    let duration = unit_to_duration(unit_char, n, s)?;
    let dt = Utc::now() + Duration::seconds(sign * duration);
    Ok(Some(dt))
}

/// Convert a unit character + amount to a total number of seconds.
fn unit_to_duration(unit: char, n: i64, original: &str) -> Result<i64, ApiError> {
    match unit {
        's' => Ok(n),
        'm' => Ok(n * 60),
        'h' => Ok(n * 3600),
        'd' => Ok(n * 86_400),
        'w' => Ok(n * 7 * 86_400),
        'M' => Ok(n * 30 * 86_400),
        'y' => Ok(n * 365 * 86_400),
        other => Err(ApiError::BadRequest(format!(
            "Invalid relative date {original:?}: unknown unit {other:?} — use one of s/m/h/d/w/M/y"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Relative expressions ──────────────────────────────────────────────────

    #[test]
    fn now_returns_current() {
        let before = Utc::now();
        let r = parse_flexible_datetime("from", "now", false).unwrap();
        let after = Utc::now();
        assert!(
            r >= before && r <= after,
            "now() should be in [before, after]"
        );
    }

    #[test]
    fn now_minus_1h() {
        let before = Utc::now() - Duration::hours(1);
        let r = parse_flexible_datetime("from", "now-1h", false).unwrap();
        let after = Utc::now() - Duration::hours(1);
        // Allow a 2-second window for test execution time.
        assert!(
            r >= before - Duration::seconds(2) && r <= after + Duration::seconds(2),
            "now-1h out of expected window"
        );
    }

    #[test]
    fn now_minus_30m() {
        let before = Utc::now() - Duration::minutes(30);
        let r = parse_flexible_datetime("from", "now-30m", false).unwrap();
        let after = Utc::now() - Duration::minutes(30);
        assert!(
            r >= before - Duration::seconds(2) && r <= after + Duration::seconds(2),
            "now-30m out of expected window"
        );
    }

    #[test]
    fn now_minus_7d() {
        let before = Utc::now() - Duration::days(7);
        let r = parse_flexible_datetime("from", "now-7d", false).unwrap();
        let after = Utc::now() - Duration::days(7);
        assert!(
            r >= before - Duration::seconds(2) && r <= after + Duration::seconds(2),
            "now-7d out of expected window"
        );
    }

    #[test]
    fn now_minus_1m_is_30_days() {
        let expected = Utc::now() - Duration::days(30);
        let r = parse_flexible_datetime("from", "now-1M", false).unwrap();
        let delta = (r - expected).num_seconds().unsigned_abs();
        assert!(delta <= 2, "now-1M should be ~30 days ago, delta={delta}s");
    }

    #[test]
    fn now_plus_offset() {
        let before = Utc::now() + Duration::hours(2);
        let r = parse_flexible_datetime("to", "now+2h", false).unwrap();
        let after = Utc::now() + Duration::hours(2);
        assert!(
            r >= before - Duration::seconds(2) && r <= after + Duration::seconds(2),
            "now+2h out of expected window"
        );
    }

    #[test]
    fn case_insensitive_now() {
        let r_upper = parse_flexible_datetime("from", "NOW", false);
        let r_mixed = parse_flexible_datetime("from", "Now", false);
        assert!(r_upper.is_ok(), "NOW should parse");
        assert!(r_mixed.is_ok(), "Now should parse");
    }

    #[test]
    fn whitespace_tolerant() {
        // Leading/trailing whitespace should be trimmed.
        let r = parse_flexible_datetime("from", "  now-1h  ", false);
        assert!(r.is_ok(), "whitespace-padded now-1h should parse");
    }

    #[test]
    fn invalid_unit_returns_error() {
        let err = parse_flexible_datetime("from", "now-1x", false).unwrap_err();
        match err {
            ApiError::BadRequest(msg) => {
                assert!(msg.contains("unknown unit"), "msg={msg}");
            }
            _ => panic!("expected BadRequest"),
        }
    }

    #[test]
    fn invalid_format_returns_error() {
        let err = parse_flexible_datetime("from", "now-abc", false).unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    // ── Absolute forms (existing tests kept) ─────────────────────────────────

    #[test]
    fn rfc3339_with_z() {
        let r = parse_flexible_datetime("from", "2026-04-23T15:30:00Z", false).unwrap();
        assert_eq!(r.to_rfc3339(), "2026-04-23T15:30:00+00:00");
    }

    #[test]
    fn rfc3339_with_offset() {
        let r = parse_flexible_datetime("from", "2026-04-23T15:30:00+05:30", false).unwrap();
        assert_eq!(r.to_rfc3339(), "2026-04-23T10:00:00+00:00");
    }

    #[test]
    fn datetime_local_no_seconds() {
        let r = parse_flexible_datetime("from", "2026-04-23T15:30", false).unwrap();
        assert_eq!(r.to_rfc3339(), "2026-04-23T15:30:00+00:00");
    }

    #[test]
    fn datetime_local_with_seconds() {
        let r = parse_flexible_datetime("from", "2026-04-23T15:30:45", false).unwrap();
        assert_eq!(r.to_rfc3339(), "2026-04-23T15:30:45+00:00");
    }

    #[test]
    fn date_only_start_of_day() {
        let r = parse_flexible_datetime("from", "2026-04-23", false).unwrap();
        assert_eq!(r.to_rfc3339(), "2026-04-23T00:00:00+00:00");
    }

    #[test]
    fn date_only_end_of_day() {
        let r = parse_flexible_datetime("to", "2026-04-23", true).unwrap();
        assert_eq!(r.to_rfc3339(), "2026-04-23T23:59:59+00:00");
    }

    #[test]
    fn garbage_returns_bad_request() {
        let err = parse_flexible_datetime("from", "not-a-date", false).unwrap_err();
        match err {
            ApiError::BadRequest(msg) => {
                assert!(msg.contains("Invalid from"));
                assert!(msg.contains("not-a-date"));
            }
            _ => panic!("expected BadRequest"),
        }
    }

    #[test]
    fn empty_string_returns_bad_request() {
        let err = parse_flexible_datetime("from", "", false).unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)));
    }
}
