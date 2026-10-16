use crate::errors::GiteditError;
use chrono::{DateTime, Duration, FixedOffset, Local, NaiveDate, NaiveDateTime, Offset, TimeZone};
use nom::{
    branch::alt,
    character::complete::{char, digit1, space0},
    combinator::eof,
    IResult, Parser,
};

/// Nom parser for relative offset strings such as `+2h`, `-30m`, `+1d`, `+2w`, `+10s`.
fn nom_relative_offset(input: &str) -> IResult<&str, (char, i64, char)> {
    let (input, sign) = alt((char('+'), char('-'))).parse(input)?;
    let (input, digits) = digit1.parse(input)?;
    let (input, _) = space0.parse(input)?;
    let (input, unit) = alt((char('s'), char('m'), char('h'), char('d'), char('w'))).parse(input)?;
    let (input, _) = eof.parse(input)?;

    let val = digits.parse::<i64>().map_err(|_| {
        nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::MapRes))
    })?;

    Ok((input, (sign, val, unit)))
}

/// Resolves reference date or defaults to local current time with fixed offset.
pub fn resolve_reference_date(
    reference: Option<DateTime<FixedOffset>>,
) -> DateTime<FixedOffset> {
    reference.unwrap_or_else(|| {
        let local_now = Local::now();
        let offset = local_now.offset().fix();
        local_now.with_timezone(&offset)
    })
}

/// Try parsing as Unix epoch timestamp: `@<timestamp>`.
pub fn parse_unix_epoch(
    input: &str,
    ref_offset: &FixedOffset,
) -> Option<Result<DateTime<FixedOffset>, GiteditError>> {
    let rest = input.strip_prefix('@')?;
    match rest.parse::<i64>() {
        Ok(ts) => Some(
            DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.with_timezone(ref_offset))
                .ok_or_else(|| GiteditError::InvalidDate {
                    input: input.to_string(),
                    reason: "timestamp out of range".to_string(),
                }),
        ),
        Err(e) => Some(Err(GiteditError::InvalidDate {
            input: input.to_string(),
            reason: format!("failed to parse epoch integer: {}", e),
        })),
    }
}

/// Try parsing relative duration shifts such as `+2h`, `-30m`, `+1d`, `+2w`, `+10s`.
pub fn parse_relative_offset(
    input: &str,
    ref_date: DateTime<FixedOffset>,
) -> Option<Result<DateTime<FixedOffset>, GiteditError>> {
    let (_, (sign_char, val, unit)) = nom_relative_offset.parse(input).ok()?;
    let sign = if sign_char == '+' { 1 } else { -1 };

    let duration = match unit {
        's' => Duration::seconds(val * sign),
        'm' => Duration::minutes(val * sign),
        'h' => Duration::hours(val * sign),
        'd' => Duration::days(val * sign),
        'w' => Duration::weeks(val * sign),
        _ => unreachable!(),
    };

    Some(Ok(ref_date + duration))
}

/// Try parsing standard ISO 8601 / RFC 3339 formats with an explicit timezone offset.
pub fn parse_iso_with_timezone(input: &str) -> Option<DateTime<FixedOffset>> {
    const ISO_WITH_TZ_FORMATS: &[&str] = &[
        "%Y-%m-%d %H:%M:%S %z",
        "%Y-%m-%d %H:%M:%S%:z",
        "%Y-%m-%dT%H:%M:%S%z",
        "%Y-%m-%dT%H:%M:%S%:z",
    ];

    for fmt in ISO_WITH_TZ_FORMATS {
        if let Ok(dt) = DateTime::parse_from_str(input, fmt) {
            return Some(dt);
        }
    }
    None
}

/// Try parsing ISO 8601 without a timezone, adopting the reference offset.
pub fn parse_iso_without_timezone(
    input: &str,
    ref_offset: &FixedOffset,
) -> Option<Result<DateTime<FixedOffset>, GiteditError>> {
    const ISO_WITHOUT_TZ_FORMATS: &[&str] = &[
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
    ];

    for fmt in ISO_WITHOUT_TZ_FORMATS {
        if let Ok(naive) = NaiveDateTime::parse_from_str(input, fmt) {
            let res = ref_offset
                .from_local_datetime(&naive)
                .single()
                .ok_or_else(|| GiteditError::InvalidDate {
                    input: input.to_string(),
                    reason: "ambiguous or invalid local datetime".to_string(),
                });
            return Some(res);
        }
    }
    None
}

/// Try parsing a date-only string (`YYYY-MM-DD`), setting time to 00:00:00.
pub fn parse_date_only(
    input: &str,
    ref_offset: &FixedOffset,
) -> Option<Result<DateTime<FixedOffset>, GiteditError>> {
    if let Ok(naive_date) = NaiveDate::parse_from_str(input, "%Y-%m-%d") {
        if let Some(naive_dt) = naive_date.and_hms_opt(0, 0, 0) {
            let res = ref_offset
                .from_local_datetime(&naive_dt)
                .single()
                .ok_or_else(|| GiteditError::InvalidDate {
                    input: input.to_string(),
                    reason: "ambiguous or invalid local date".to_string(),
                });
            return Some(res);
        }
    }
    None
}

/// Fallback parser for natural language date expressions (e.g. "yesterday", "2 days ago", "now").
pub fn parse_natural_language(
    input: &str,
    ref_date: DateTime<FixedOffset>,
    ref_offset: &FixedOffset,
) -> Result<DateTime<FixedOffset>, GiteditError> {
    let dialect = interim::Dialect::Us;
    interim::parse_date_string(input, ref_date, dialect)
        .map(|parsed_dt| parsed_dt.with_timezone(ref_offset))
        .map_err(|e| GiteditError::InvalidDate {
            input: input.to_string(),
            reason: format!("{}", e),
        })
}

/// Parse date from a variety of formats:
/// 1. Unix epoch: `@<timestamp>`
/// 2. Relative shifts: `+2h`, `-30m`, `+1d`, `-2w` applied to reference date (or current time)
/// 3. ISO 8601 / RFC 3339 with timezone
/// 4. ISO 8601 without timezone
/// 5. Date only (`YYYY-MM-DD`)
/// 6. Natural language: `"yesterday 14:00"`, `"3 days ago"`, `"now"` via interim
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

    let ref_date = resolve_reference_date(reference);
    let ref_offset = *ref_date.offset();

    // 1. Unix epoch: @<timestamp>
    if let Some(result) = parse_unix_epoch(input, &ref_offset) {
        return result;
    }

    // 2. Relative offset: e.g. "+2h", "-30m", "+1d", "+2w", "+10s"
    if let Some(result) = parse_relative_offset(input, ref_date) {
        return result;
    }

    // 3. ISO with timezone
    if let Some(dt) = parse_iso_with_timezone(input) {
        return Ok(dt);
    }

    // 4. ISO without timezone
    if let Some(result) = parse_iso_without_timezone(input, &ref_offset) {
        return result;
    }

    // 5. Date only: "YYYY-MM-DD"
    if let Some(result) = parse_date_only(input, &ref_offset) {
        return result;
    }

    // 6. Natural language fallback
    parse_natural_language(input, ref_date, &ref_offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relative_offset_nom_parser() {
        let (_, (sign, val, unit)) = nom_relative_offset("+2h").unwrap();
        assert_eq!(sign, '+');
        assert_eq!(val, 2);
        assert_eq!(unit, 'h');

        let (_, (sign, val, unit)) = nom_relative_offset("-30m").unwrap();
        assert_eq!(sign, '-');
        assert_eq!(val, 30);
        assert_eq!(unit, 'm');

        assert!(nom_relative_offset("invalid").is_err());
    }

    #[test]
    fn test_empty_date_error() {
        let res = parse_date("   ", None);
        assert!(matches!(res, Err(GiteditError::InvalidDate { .. })));
    }
}
