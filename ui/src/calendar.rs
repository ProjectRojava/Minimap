//! Calendar layout for the date picker. Pure, proleptic Gregorian, weeks start on Monday.
//! (Business date rules live in core; this is only how a month is drawn.)

pub const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
pub const WEEKDAYS: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

pub fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
    }
}

/// 0 = Monday ... 6 = Sunday.
pub fn weekday(y: i32, m: u32, d: u32) -> u32 {
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if m < 3 { y - 1 } else { y };
    let sunday0 =
        (y + y / 4 - y / 100 + y / 400 + T[(m as usize - 1).min(11)] + d as i32).rem_euclid(7);
    ((sunday0 + 6) % 7) as u32
}

/// The month `delta` months from `(year, month)`.
pub fn shift_month((y, m): (i32, u32), delta: i32) -> (i32, u32) {
    let index = y * 12 + (m as i32 - 1) + delta;
    (index.div_euclid(12), index.rem_euclid(12) as u32 + 1)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub y: i32,
    pub m: u32,
    pub d: u32,
    /// Belongs to the month being shown (the rest are neighbours that fill the weeks).
    pub in_month: bool,
}

/// Six full weeks covering the month, starting on the Monday on or before the 1st.
pub fn month_grid(y: i32, m: u32) -> Vec<Cell> {
    let lead = weekday(y, m, 1);
    let (py, pm) = shift_month((y, m), -1);
    let (ny, nm) = shift_month((y, m), 1);
    let prev_len = days_in_month(py, pm);
    let len = days_in_month(y, m);
    (0..42u32)
        .map(|i| {
            if i < lead {
                Cell {
                    y: py,
                    m: pm,
                    d: prev_len - lead + i + 1,
                    in_month: false,
                }
            } else if i < lead + len {
                Cell {
                    y,
                    m,
                    d: i - lead + 1,
                    in_month: true,
                }
            } else {
                Cell {
                    y: ny,
                    m: nm,
                    d: i - lead - len + 1,
                    in_month: false,
                }
            }
        })
        .collect()
}

pub fn format_ymd(y: i32, m: u32, d: u32) -> String {
    format!("{y:04}-{m:02}-{d:02}")
}

/// `YYYY-MM-DD` for a real calendar date.
pub fn parse_ymd(text: &str) -> Option<(i32, u32, u32)> {
    let mut parts = text.trim().split('-');
    let (y, m, d) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || y.len() != 4 || m.len() != 2 || d.len() != 2 {
        return None;
    }
    let (y, m, d): (i32, u32, u32) = (y.parse().ok()?, m.parse().ok()?, d.parse().ok()?);
    ((1..=12).contains(&m) && d >= 1 && d <= days_in_month(y, m)).then_some((y, m, d))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_lengths() {
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2027, 2), 28);
        assert_eq!(days_in_month(1900, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);
        assert_eq!(days_in_month(2026, 4), 30);
        assert_eq!(days_in_month(2026, 12), 31);
    }

    #[test]
    fn known_weekdays() {
        assert_eq!(weekday(2000, 1, 1), 5); // Saturday
        assert_eq!(weekday(2026, 10, 3), 5); // Saturday
        assert_eq!(weekday(2024, 2, 29), 3); // Thursday
        assert_eq!(weekday(2027, 3, 31), 2); // Wednesday
        assert_eq!(weekday(2025, 12, 28), 6); // Sunday
        assert_eq!(weekday(2025, 12, 29), 0); // Monday
    }

    #[test]
    fn months_shift_across_years() {
        assert_eq!(shift_month((2026, 12), 1), (2027, 1));
        assert_eq!(shift_month((2026, 1), -1), (2025, 12));
        assert_eq!(shift_month((2026, 6), 0), (2026, 6));
        assert_eq!(shift_month((2026, 3), -15), (2024, 12));
        assert_eq!(shift_month((2026, 3), 25), (2028, 4));
    }

    #[test]
    fn grids_are_six_whole_weeks_starting_on_monday() {
        for (y, m) in [
            (2026, 10),
            (2027, 2),
            (2024, 2),
            (2026, 6),
            (2025, 12),
            (2023, 1),
        ] {
            let g = month_grid(y, m);
            assert_eq!(g.len(), 42);
            assert_eq!(
                weekday(g[0].y, g[0].m, g[0].d),
                0,
                "{y}-{m} starts on a Monday"
            );
            let in_month: Vec<_> = g.iter().filter(|c| c.in_month).collect();
            assert_eq!(in_month.len() as u32, days_in_month(y, m));
            assert_eq!(in_month[0].d, 1);
            // Consecutive days: each cell is the day after the previous one.
            for w in g.windows(2) {
                let next = if w[0].d == days_in_month(w[0].y, w[0].m) {
                    (shift_month((w[0].y, w[0].m), 1), 1)
                } else {
                    ((w[0].y, w[0].m), w[0].d + 1)
                };
                assert_eq!(((w[1].y, w[1].m), w[1].d), next);
            }
        }
        // October 2026 begins on a Thursday: three leading days from September.
        let g = month_grid(2026, 10);
        assert_eq!((g[0].m, g[0].d), (9, 28));
        assert_eq!((g[3].m, g[3].d, g[3].in_month), (10, 1, true));
    }

    #[test]
    fn parsing_accepts_only_real_dates() {
        assert_eq!(parse_ymd("2027-03-31"), Some((2027, 3, 31)));
        assert_eq!(parse_ymd(" 2024-02-29 "), Some((2024, 2, 29)));
        for bad in [
            "2027-02-29",
            "2027-13-01",
            "2027-00-10",
            "2027-04-31",
            "27-03-31",
            "2027-3-1",
            "2027/03/31",
            "",
            "2027-03",
            "2027-03-31-1",
            "abcd-ef-gh",
        ] {
            assert_eq!(parse_ymd(bad), None, "{bad:?}");
        }
        assert_eq!(format_ymd(2027, 3, 5), "2027-03-05");
        assert_eq!(parse_ymd(&format_ymd(2026, 10, 3)), Some((2026, 10, 3)));
    }
}
