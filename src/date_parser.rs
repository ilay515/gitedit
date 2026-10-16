use crate::errors::GiteditError;
use chrono::{DateTime, Duration, FixedOffset, Local, NaiveDate, NaiveDateTime, Offset, TimeZone};
use regex::Regex;

/// Parse date from a variety of formats:
/// 1. ISO 8601 / RFC 3339 with timezone (e.g. "2026-10-16 14:30:00 +03:00", "2026-10-16T14:30:00+03:00")
/// 2. ISO 8601 without timezone (e.g. "2026-10-16 14:30:00") -> adopts reference timezone or local offset
/// 3. Date only ("2026-10-16") -> 00:00:00 with reference or local offset
/// 4. Relative shifts: "+2h", "-30m", "+1d", "-2w" applied to reference date (or current time)
/// 5. Epoch timestamp: "@1760613600"
/// 6. Natural language: "yesterday 14:00", "3 days ago", "now" via interim
pub fn parse_date(
    input: &str,
    reference: Option<DateTime<FixedOffset>>,
) -> Result<DateTime<FixedOffset>, GiteditError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(GiteditError::InvalidDate {
            input: input.to_string(),
            reason: "date string is empty".to_string(),
        });
    }

    // Reference time or current local time
    let local_now = Local::now();
    let ref_date = reference.unwrap_or_else(|| {
        let offset = local_now.offset().fix();
        local_now.with_timezone(&offset)
    });
    let ref_offset = *ref_date.offset();

    // 1. Unix epoch: @<timestamp>
    if let Some(rest) = input.strip_prefix('@') {
        if let Ok(ts) = rest.parse::<i64>() {
            return DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.with_timezone(&ref_offset))
                .ok_or_else(|| GiteditError::InvalidDate {
                    input: input.to_string(),
                    reason: "timestamp out of range".to_string(),
                });
        }
    }

    // 2. Relative offset: e.g. "+2h", "-30m", "+1d", "+2w", "+10s"
    let rel_regex = Regex::new(r"^([+-])(\d+)\s*([smhdw])$").unwrap();
    if let Some(caps) = rel_regex.captures(input) {
        let sign = if &caps[1] == "+" { 1 } else { -1 };
        let val: i64 = caps[2].parse().map_err(|e| GiteditError::InvalidDate {
            input: input.to_string(),
            reason: format!("failed to parse number: {}", e),
        })?;
        let unit = &caps[3];

        let duration = match unit {
            "s" => Duration::seconds(val * sign),
            "m" => Duration::minutes(val * sign),
            "h" => Duration::hours(val * sign),
            "d" => Duration::days(val * sign),
            "w" => Duration::weeks(val * sign),
            _ => unreachable!(),
        };

        return Ok(ref_date + duration);
    }

    // 3. ISO with timezone: "2026-10-16 14:30:00 +0300", "2026-10-16 14:30:00 +03:00", "2026-10-16T14:30:00+03:00", etc.
    let iso_with_tz_formats = [
        "%Y-%m-%d %H:%M:%S %z",
        "%Y-%m-%d %H:%M:%S%:z",
        "%Y-%m-%dT%H:%M:%S%z",
        "%Y-%m-%dT%H:%M:%S%:z",
    ];

    for fmt in &iso_with_tz_formats {
        if let Ok(dt) = DateTime::parse_from_str(input, fmt) {
            return Ok(dt);
        }
    }

    // 4. ISO without timezone: "2026-10-16 14:30:00" or "2026-10-16T14:30:00"
    for fmt in &["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(input, fmt) {
            return ref_offset
                .from_local_datetime(&naive)
                .single()
                .ok_or_else(|| GiteditError::InvalidDate {
                    input: input.to_string(),
                    reason: "ambiguous or invalid local datetime".to_string(),
                });
        }
    }

    // 5. Date only: "2026-10-16"
    if let Ok(naive_date) = NaiveDate::parse_from_str(input, "%Y-%m-%d") {
        if let Some(naive_dt) = naive_date.and_hms_opt(0, 0, 0) {
            return ref_offset
                .from_local_datetime(&naive_dt)
                .single()
                .ok_or_else(|| GiteditError::InvalidDate {
                    input: input.to_string(),
                    reason: "ambiguous or invalid local date".to_string(),
                });
        }
    }

    // 6. Natural language: e.g. "yesterday 14:00", "3 days ago", "now"
    let dialect = interim::Dialect::Us;
    match interim::parse_date_string(input, ref_date, dialect) {
        Ok(parsed_dt) => Ok(parsed_dt.with_timezone(&ref_offset)),
        Err(e) => Err(GiteditError::InvalidDate {
            input: input.to_string(),
            reason: format!("{}", e),
        }),
    }
}
