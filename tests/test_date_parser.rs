use gitedit::date_parser::parse_date;
use chrono::{FixedOffset, TimeZone};

#[test]
fn test_parse_iso_with_tz() {
    let dt = parse_date("2026-10-16 14:30:00 +03:00", None).unwrap();
    assert_eq!(dt.to_rfc3339(), "2026-10-16T14:30:00+03:00");
}

#[test]
fn test_parse_epoch() {
    let dt = parse_date("@1700000000", None).unwrap();
    assert_eq!(dt.timestamp(), 1700000000);
}

#[test]
fn test_parse_relative_shift() {
    let offset = FixedOffset::east_opt(3 * 3600).unwrap();
    let base = offset.with_ymd_and_hms(2026, 10, 16, 12, 0, 0).unwrap();

    let dt = parse_date("+2h", Some(base)).unwrap();
    assert_eq!(dt.format("%H:%M:%S").to_string(), "14:00:00");

    let dt2 = parse_date("-1d", Some(base)).unwrap();
    assert_eq!(dt2.format("%Y-%m-%d").to_string(), "2026-10-15");
}

#[test]
fn test_parse_natural_language() {
    let offset = FixedOffset::east_opt(0).unwrap();
    let base = offset.with_ymd_and_hms(2026, 10, 16, 12, 0, 0).unwrap();

    let dt = parse_date("yesterday", Some(base)).unwrap();
    assert_eq!(dt.format("%Y-%m-%d").to_string(), "2026-10-15");
}
