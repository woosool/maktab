//! Date serial numbers: days since 1899-12-30 (so 1900-03-01 is 61, keeping Excel's 1900 leap
//! year), or since 1904-01-01 for workbooks with the 1904 date system.

pub fn is_leap(y: i64) -> bool { (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 }

pub fn days_in_month(y: i64, m: u32) -> u32 {
    match m { 1 | 3 | 5 | 7 | 8 | 10 | 12 => 31, 4 | 6 | 9 | 11 => 30, _ => if is_leap(y) { 29 } else { 28 } }
}

/// Days from 1970-01-01 to a civil date (proleptic Gregorian).
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

const EPOCH_1900: i64 = -25569; // 1899-12-30 relative to 1970-01-01
const EPOCH_1904: i64 = -24107; // 1904-01-01

/// The serial of a date; months and days past their range roll over as DATE does.
pub fn serial(y: i64, m: i64, d: i64, date1904: bool) -> f64 {
    let y = if (0..1900).contains(&y) { y + 1900 } else { y };
    let total = y * 12 + (m - 1);
    let (yy, mm) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
    let days = days_from_civil(yy, mm, 1) + d - 1;
    if date1904 { return (days - EPOCH_1904) as f64; }
    let s = days - EPOCH_1900;
    // Excel counts 1900-02-29, which did not exist: serials before March 1900 are one less.
    (if s < 61 { s - 1 } else { s }) as f64
}

/// (year, month, day) of a serial.
pub fn ymd(serial: f64, date1904: bool) -> (i64, u32, u32) {
    let s = serial.floor() as i64;
    if date1904 { return civil_from_days(s + EPOCH_1904); }
    if s == 60 { return (1900, 2, 29); }
    if s == 0 { return (1900, 1, 0); }
    civil_from_days(if s < 60 { s + 1 } else { s } + EPOCH_1900)
}

/// (hour, minute, second, millisecond) of a serial's fraction, rounded to the millisecond.
pub fn hms(serial: f64) -> (u32, u32, u32, u32) {
    let ms = ((serial - serial.floor()) * 86_400_000.0).round() as u64 % 86_400_000;
    ((ms / 3_600_000) as u32, (ms / 60_000 % 60) as u32, (ms / 1000 % 60) as u32, (ms % 1000) as u32)
}

/// 0 = Sunday.
pub fn weekday(serial: f64, date1904: bool) -> u32 {
    let (y, m, d) = ymd(serial, date1904);
    if serial.floor() < 61.0 && !date1904 { return ((serial.floor() as i64 + 6).rem_euclid(7)) as u32; }
    ((days_from_civil(y, m, d) + 4).rem_euclid(7)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn serials() {
        assert_eq!(serial(1900, 1, 1, false), 1.0);
        assert_eq!(serial(1900, 3, 1, false), 61.0);
        assert_eq!(serial(2024, 1, 15, false), 45306.0);
        assert_eq!(ymd(45306.0, false), (2024, 1, 15));
        assert_eq!(ymd(1.0, false), (1900, 1, 1));
        assert_eq!(serial(2024, 14, 1, false), serial(2025, 2, 1, false));
        assert_eq!(weekday(45306.0, false), 1);
        assert_eq!(serial(1904, 1, 1, true), 0.0);
    }
}
