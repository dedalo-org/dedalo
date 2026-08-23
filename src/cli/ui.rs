//! Terminal output helpers: small, dependency-free table and value rendering.

use std::io::IsTerminal;

/// ANSI styling, disabled when stdout is not a terminal or NO_COLOR is set.
pub fn color_enabled() -> bool {
    std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
}

pub fn bold(text: &str) -> String {
    if color_enabled() {
        format!("\x1b[1m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub fn dim(text: &str) -> String {
    if color_enabled() {
        format!("\x1b[2m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub fn green(text: &str) -> String {
    if color_enabled() {
        format!("\x1b[32m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub fn yellow(text: &str) -> String {
    if color_enabled() {
        format!("\x1b[33m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
}

/// A minimal column-aligned table.
pub struct Table {
    headers: Vec<String>,
    aligns: Vec<Align>,
    rows: Vec<Vec<String>>,
}

impl Table {
    pub fn new(headers: &[(&str, Align)]) -> Self {
        Self {
            headers: headers.iter().map(|(h, _)| h.to_string()).collect(),
            aligns: headers.iter().map(|(_, a)| *a).collect(),
            rows: Vec::new(),
        }
    }

    pub fn push(&mut self, row: Vec<String>) {
        self.rows.push(row);
    }

    pub fn render(&self) -> String {
        let mut widths: Vec<usize> = self.headers.iter().map(|h| display_width(h)).collect();
        for row in &self.rows {
            for (index, cell) in row.iter().enumerate() {
                if index < widths.len() {
                    widths[index] = widths[index].max(display_width(cell));
                }
            }
        }

        let mut out = String::new();
        let header: Vec<String> = self
            .headers
            .iter()
            .enumerate()
            .map(|(i, h)| pad(h, widths[i], self.aligns[i]))
            .collect();
        out.push_str(&dim(header.join("  ").trim_end()));
        out.push('\n');

        for row in &self.rows {
            let line: Vec<String> = row
                .iter()
                .enumerate()
                .map(|(i, cell)| pad(cell, widths[i], self.aligns[i]))
                .collect();
            out.push_str(line.join("  ").trim_end());
            out.push('\n');
        }
        out
    }
}

/// Width ignoring ANSI escapes, so styled cells still align.
fn display_width(text: &str) -> usize {
    let mut width = 0;
    let mut in_escape = false;
    for ch in text.chars() {
        if in_escape {
            if ch == 'm' {
                in_escape = false;
            }
            continue;
        }
        if ch == '\x1b' {
            in_escape = true;
            continue;
        }
        width += 1;
    }
    width
}

fn pad(text: &str, width: usize, align: Align) -> String {
    let padding = width.saturating_sub(display_width(text));
    match align {
        Align::Left => format!("{text}{}", " ".repeat(padding)),
        Align::Right => format!("{}{text}", " ".repeat(padding)),
    }
}

/// Render a unix timestamp as `YYYY-MM-DD HH:MM` in UTC.
///
/// Implemented here rather than pulled from a date crate: formatting one
/// timestamp is not worth a dependency in a tool that handles money, and a
/// smaller tree is a smaller supply chain to audit.
pub fn format_timestamp(unix: i64) -> String {
    let days = div_floor(unix, 86_400);
    let seconds_of_day = unix - days * 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}")
}

/// Division that rounds towards negative infinity, so timestamps before 1970
/// land on the right day instead of the next one.
fn div_floor(numerator: i64, denominator: i64) -> i64 {
    let quotient = numerator / denominator;
    if numerator % denominator != 0 && (numerator < 0) != (denominator < 0) {
        quotient - 1
    } else {
        quotient
    }
}

/// Days since the unix epoch to a proleptic Gregorian date.
///
/// Howard Hinnant's `civil_from_days`, which is exact for the whole range of
/// `i64` days and needs no lookup tables or leap-year special cases.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    // Shift the epoch to 0000-03-01 so leap days fall at the end of the cycle.
    let z = days + 719_468;
    let era = div_floor(z, 146_097);
    let day_of_era = z - era * 146_097; // [0, 146096]
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153; // [0, 11], March-based
    let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u32;
    let month = (month_prime + if month_prime < 10 { 3 } else { -9 }) as u32;
    (year + i64::from(month <= 2), month, day)
}

/// Render milli-points as a human score.
pub fn format_score(milli: u128) -> String {
    let whole = milli / 1_000;
    let frac = (milli % 1_000) / 100;
    if frac == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{frac}")
    }
}

/// Render basis points as a percentage.
pub fn format_bps(bps: u32) -> String {
    format!("{}.{:02}%", bps / 100, bps % 100)
}

pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{kept}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_aligns_columns() {
        let mut table = Table::new(&[("A", Align::Left), ("B", Align::Right)]);
        table.push(vec!["long-value".into(), "1".into()]);
        table.push(vec!["x".into(), "1000".into()]);
        let rendered = table.render();
        let lines: Vec<&str> = rendered.lines().collect();
        assert_eq!(lines[1], "long-value     1");
        assert_eq!(lines[2], "x           1000");
    }

    #[test]
    fn width_ignores_ansi_sequences() {
        assert_eq!(display_width("\x1b[1mabc\x1b[0m"), 3);
    }

    #[test]
    fn formats_timestamps_in_utc() {
        assert_eq!(format_timestamp(0), "1970-01-01 00:00");
        assert_eq!(format_timestamp(1_700_000_000), "2023-11-14 22:13");
        // Leap day, and the last second before the next one.
        assert_eq!(format_timestamp(1_709_164_800), "2024-02-29 00:00");
        assert_eq!(format_timestamp(1_709_251_199), "2024-02-29 23:59");
        // Century rule: 1900 was not a leap year, 2000 was.
        assert_eq!(format_timestamp(951_782_400), "2000-02-29 00:00");
        // Before the epoch, where naive division would slip a day.
        assert_eq!(format_timestamp(-1), "1969-12-31 23:59");
        assert_eq!(format_timestamp(-86_400), "1969-12-31 00:00");
    }

    #[test]
    fn formats_scores_and_percentages() {
        assert_eq!(format_score(1_500), "1.5");
        assert_eq!(format_score(2_000), "2");
        assert_eq!(format_bps(250), "2.50%");
    }
}
