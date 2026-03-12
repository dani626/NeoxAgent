/// Lightweight time utilities to replace the heavy `chrono` crate.
/// Uses only `std::time::SystemTime` — no timezone databases needed.

use std::time::SystemTime;

/// Returns the current UTC time formatted as RFC 3339 (ISO 8601).
/// Example: "2026-03-12T15:30:00Z"
pub fn now_rfc3339() -> String {
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();

    let secs = duration.as_secs();
    format_timestamp_rfc3339(secs as i64, 0)
}

/// Returns the current UTC timestamp as a struct for formatting.
pub struct Timestamp {
    pub secs: i64,
}

impl Timestamp {
    /// Get the current UTC timestamp.
    pub fn now() -> Self {
        let duration = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        Self {
            secs: duration.as_secs() as i64,
        }
    }

    /// Format as RFC 3339.
    pub fn to_rfc3339(&self) -> String {
        format_timestamp_rfc3339(self.secs, 0)
    }

    /// Format as a backup-friendly filename timestamp: YYYYMMDD_HHMMSS
    pub fn to_filename(&self) -> String {
        let (y, m, d, h, min, s) = decompose_timestamp(self.secs);
        format!("{:04}{:02}{:02}_{:02}{:02}{:02}", y, m, d, h, min, s)
    }
}

/// Convert a Unix timestamp (seconds + nanos) to RFC 3339 string.
pub fn format_timestamp_rfc3339(unix_secs: i64, _nanos: u32) -> String {
    let (y, m, d, h, min, s) = decompose_timestamp(unix_secs);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y, m, d, h, min, s
    )
}

/// Convert a `SystemTime` to RFC 3339.
pub fn system_time_to_rfc3339(time: SystemTime) -> String {
    let duration = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    format_timestamp_rfc3339(duration.as_secs() as i64, duration.subsec_nanos())
}

/// Decompose a Unix timestamp into (year, month, day, hour, minute, second).
fn decompose_timestamp(unix_secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    // Days since Unix epoch
    let secs_per_day: i64 = 86400;
    let mut days = unix_secs / secs_per_day;
    let day_secs = (unix_secs % secs_per_day) as u32;

    let h = day_secs / 3600;
    let min = (day_secs % 3600) / 60;
    let s = day_secs % 60;

    // Civil date from days since 1970-01-01 (algorithm from Howard Hinnant)
    days += 719468; // shift to 0000-03-01
    let era = if days >= 0 { days } else { days - 146096 } / 146097;
    let doe = (days - era * 146097) as u32; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // year of era
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    (y, m, d, h, min, s)
}
