use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};

use super::error::ApiError;

/// Parse a datetime query param accepting (in priority order):
/// 1. Full RFC3339: `2026-04-23T15:30:00Z`, `2026-04-23T15:30:00+05:30`
/// 2. ISO datetime-local without seconds: `2026-04-23T15:30`
/// 3. ISO datetime-local with seconds: `2026-04-23T15:30:00`
/// 4. Date-only: `2026-04-23` — interpreted as start-of-day UTC if `end_of_day=false`,
///    end-of-day (23:59:59 UTC) if `end_of_day=true`.
///
/// Anything else returns 400 with a descriptive message.
pub fn parse_flexible_datetime(
    field: &str,
    value: &str,
    end_of_day: bool,
) -> Result<DateTime<Utc>, ApiError> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return Ok(dt.with_timezone(&Utc));
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M") {
        return Ok(Utc.from_utc_datetime(&naive));
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S") {
        return Ok(Utc.from_utc_datetime(&naive));
    }
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        let naive = if end_of_day {
            date.and_hms_opt(23, 59, 59)
                .expect("23:59:59 is always valid")
        } else {
            date.and_hms_opt(0, 0, 0).expect("00:00:00 is always valid")
        };
        return Ok(Utc.from_utc_datetime(&naive));
    }
    Err(ApiError::BadRequest(format!(
        "Invalid {field} (expected RFC3339, ISO datetime, or YYYY-MM-DD): {value:?}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

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
