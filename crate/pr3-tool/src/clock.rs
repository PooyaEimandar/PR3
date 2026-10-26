//! Date and time from a timestamp and a UTC offset, with no timezone database: the host
//! supplies the device's current offset.

use pr3_core::Timestamp;

const WEEKDAYS: [&str; 7] = [
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
];

/// e.g. "Tuesday 2026-10-20 19:45 (UTC+03:30)".
pub fn describe(now: Timestamp, utc_offset_minutes: i32) -> String {
    let (days, secs_of_day) = local(now, utc_offset_minutes);
    let (year, month, day) = civil_from_days(days);
    let weekday = WEEKDAYS[days.rem_euclid(7) as usize];
    let sign = if utc_offset_minutes < 0 { '-' } else { '+' };
    let offset = utc_offset_minutes.unsigned_abs();
    format!(
        "{weekday} {year:04}-{month:02}-{day:02} {:02}:{:02} (UTC{sign}{:02}:{:02})",
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60,
        offset / 60,
        offset % 60,
    )
}

/// The local date, e.g. "2026-10-20".
pub fn date(at: Timestamp, utc_offset_minutes: i32) -> String {
    let (year, month, day) = civil_from_days(local(at, utc_offset_minutes).0);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Local days since 1970-01-01 and seconds into that day.
fn local(at: Timestamp, utc_offset_minutes: i32) -> (i64, i64) {
    let secs = (at.0 / 1000) as i64 + i64::from(utc_offset_minutes) * 60;
    (secs.div_euclid(86_400), secs.rem_euclid(86_400))
}

/// Days since 1970-01-01 to a proleptic Gregorian (year, month, day). Howard Hinnant's
/// `civil_from_days` algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_dates() {
        assert_eq!(
            describe(Timestamp(0), 0),
            "Thursday 1970-01-01 00:00 (UTC+00:00)"
        );
        // 2026-10-20 16:15:00 UTC is 19:45 in Tehran (UTC+03:30).
        let t = Timestamp(1_792_512_900_000);
        assert_eq!(describe(t, 210), "Tuesday 2026-10-20 19:45 (UTC+03:30)");
        assert_eq!(describe(t, -420), "Tuesday 2026-10-20 09:15 (UTC-07:00)");
        assert_eq!(date(t, 210), "2026-10-20");
        assert_eq!(date(t, -1000), "2026-10-19");
    }

    #[test]
    fn leap_day() {
        // 2024-02-29 12:00 UTC
        assert_eq!(
            describe(Timestamp(1_709_208_000_000), 0),
            "Thursday 2024-02-29 12:00 (UTC+00:00)"
        );
    }
}
