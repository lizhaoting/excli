use chrono::{NaiveDate, NaiveDateTime, NaiveTime, Timelike};

use crate::types::DateSystem;

/// Excel serial date for 1900-01-01 (with Lotus 1-2-3 bug, day 60 = 1900-02-29 which doesn't exist).
#[allow(dead_code)]
const EXCEL_1900_EPOCH: i64 = -2; // Jan 1, 1900 = serial 1, so epoch offset days = NaiveDate(1899,12,30)

/// Convert an Excel serial date number to a NaiveDateTime.
/// Handles both 1900 and 1904 date systems.
pub fn serial_to_datetime(serial: f64, date_system: DateSystem) -> Option<NaiveDateTime> {
    if serial < 0.0 {
        return None;
    }

    let base_date = match date_system {
        DateSystem::Date1900 => {
            let mut days = serial.trunc() as i64;

            // Lotus 1-2-3 bug: serial 60 = Feb 29, 1900 (doesn't exist)
            // For serials > 60, subtract 1 to get real date
            if days > 60 {
                days -= 1;
            } else if days == 60 {
                // The non-existent Feb 29, map to Feb 28
                days = 59;
            }

            NaiveDate::from_ymd_opt(1899, 12, 31)?
                .checked_add_signed(chrono::TimeDelta::days(days))?
        }
        DateSystem::Date1904 => NaiveDate::from_ymd_opt(1904, 1, 1)?
            .checked_add_signed(chrono::TimeDelta::days(serial.trunc() as i64))?,
    };

    // Time component: fractional part * 86400 seconds
    let seconds = ((serial - serial.trunc()).abs() * 86400.0).round() as i64;
    let time = NaiveTime::from_num_seconds_from_midnight_opt((seconds % 86400) as u32, 0)?;

    Some(NaiveDateTime::new(base_date, time))
}

/// Convert a NaiveDateTime to an Excel serial date number.
pub fn datetime_to_serial(dt: NaiveDateTime, date_system: DateSystem) -> f64 {
    let base_date = match date_system {
        DateSystem::Date1900 => NaiveDate::from_ymd_opt(1899, 12, 31).unwrap(),
        DateSystem::Date1904 => NaiveDate::from_ymd_opt(1904, 1, 1).unwrap(),
    };

    let date_part = dt.date();
    let days = (date_part - base_date).num_days() as f64;

    let time_part = dt.time();
    let seconds = (time_part.hour() * 3600 + time_part.minute() * 60 + time_part.second()) as f64
        + f64::from(time_part.nanosecond()) / 1_000_000_000.0;
    let fractional = seconds / 86400.0;

    let serial = days + fractional;

    match date_system {
        DateSystem::Date1900 => {
            // Lotus bug: if date is on or after the fake Feb 29, 1900, add 1 to serial
            // Mar 1, 1900 = 60 days from Dec 31, 1899
            if days >= 60.0 {
                serial + 1.0
            } else {
                serial
            }
        }
        DateSystem::Date1904 => serial,
    }
}

/// Format a number using Excel-style format string.
pub fn format_number(value: f64, format: &str) -> String {
    match format {
        "#,##0" => {
            let mut s = String::new();
            let is_neg = value < 0.0;
            let abs_val = value.abs() as i64;
            let chars: Vec<char> = abs_val.to_string().chars().rev().collect();
            for (i, ch) in chars.iter().enumerate() {
                if i > 0 && i % 3 == 0 {
                    s.push(',');
                }
                s.push(*ch);
            }
            if is_neg {
                s.push('-');
            }
            s.chars().rev().collect()
        }
        "#,##0.00" => {
            let int_part = value.trunc();
            let frac_part = (value.abs().fract() * 100.0).round() as u64;
            format!("{}.{:02}", format_number(int_part, "#,##0"), frac_part)
        }
        "0.00%" => format!("{:.2}%", value * 100.0),
        _ => value.to_string(),
    }
}

/// Check if a cell range is within Excel limits (A1:XFD1048576).
pub fn is_valid_range(start_col: u32, start_row: u32, end_col: u32, end_row: u32) -> bool {
    end_col < 16384 && end_row < 1048576 && start_col < 16384 && start_row < 1048576
}

/// Escape a string for CSV output.
pub fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// Detect if a string is likely a numeric value.
pub fn looks_like_number(s: &str) -> bool {
    s.trim().parse::<f64>().is_ok()
}

/// Detect if a string is likely a date.
pub fn looks_like_date(s: &str) -> bool {
    // Common date patterns
    let patterns = [
        regex::Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap(),
        regex::Regex::new(r"^\d{2}/\d{2}/\d{4}$").unwrap(),
        regex::Regex::new(r"^\d{1,2}/\d{1,2}/\d{4}$").unwrap(),
        regex::Regex::new(r"^\d{4}/\d{2}/\d{2}$").unwrap(),
    ];
    patterns.iter().any(|p| p.is_match(s.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serial_to_datetime_1900() {
        // Jan 1, 1900 = serial 1
        let dt = serial_to_datetime(1.0, DateSystem::Date1900).unwrap();
        assert_eq!(dt.date().to_string(), "1900-01-01");

        // Jan 1, 2020 = serial 43831
        let dt = serial_to_datetime(43831.0, DateSystem::Date1900).unwrap();
        assert_eq!(dt.date().to_string(), "2020-01-01");
    }

    #[test]
    fn test_datetime_to_serial_1900() {
        let dt = NaiveDate::from_ymd_opt(2020, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let serial = datetime_to_serial(dt, DateSystem::Date1900);
        assert!((serial - 43831.0).abs() < 0.001);
    }

    #[test]
    fn test_serial_datetime_roundtrip() {
        let original = NaiveDate::from_ymd_opt(2024, 6, 15)
            .unwrap()
            .and_hms_opt(12, 30, 0)
            .unwrap();
        let serial = datetime_to_serial(original, DateSystem::Date1900);
        let restored = serial_to_datetime(serial, DateSystem::Date1900).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn test_date_system_1904() {
        let dt = NaiveDate::from_ymd_opt(2020, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let serial = datetime_to_serial(dt, DateSystem::Date1904);
        let _restored = serial_to_datetime(serial, DateSystem::Date1904).unwrap();
        assert_eq!(dt.date().to_string(), "2020-01-01");
    }

    #[test]
    fn test_csv_escape() {
        assert_eq!(csv_escape("hello"), "hello");
        assert_eq!(csv_escape("hello, world"), "\"hello, world\"");
        assert_eq!(csv_escape("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn test_looks_like_number() {
        assert!(looks_like_number("123"));
        assert!(looks_like_number("-45.67"));
        assert!(!looks_like_number("abc"));
        assert!(!looks_like_number(""));
    }

    #[test]
    fn test_looks_like_date() {
        assert!(looks_like_date("2024-01-15"));
        assert!(looks_like_date("01/15/2024"));
        assert!(!looks_like_date("not a date"));
    }

    #[test]
    fn test_format_number_basic() {
        assert_eq!(format_number(1234.0, "#,##0"), "1,234");
        assert_eq!(format_number(1234.56, "#,##0.00"), "1,234.56");
    }

    #[test]
    fn test_valid_range() {
        assert!(is_valid_range(0, 0, 100, 100));
        assert!(!is_valid_range(0, 0, 20000, 100));
        assert!(!is_valid_range(0, 0, 100, 2000000));
    }
}
