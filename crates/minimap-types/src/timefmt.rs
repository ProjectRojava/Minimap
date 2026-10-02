//! Canonical text forms for SQLite: dates `YYYY-MM-DD`, timestamps
//! `YYYY-MM-DDTHH:MM:SS.mmmZ` (UTC, fixed width, so they sort as text).

use time::{
    format_description::well_known::Rfc3339, macros::format_description, Date, OffsetDateTime,
    UtcOffset,
};

pub fn fmt_date(d: Date) -> String {
    format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day())
}

pub fn parse_date(s: &str) -> Result<Date, time::error::Parse> {
    Date::parse(s, format_description!("[year]-[month]-[day]"))
}

pub fn fmt_ts(t: OffsetDateTime) -> String {
    let t = t.to_offset(UtcOffset::UTC);
    format!(
        "{}T{:02}:{:02}:{:02}.{:03}Z",
        fmt_date(t.date()),
        t.hour(),
        t.minute(),
        t.second(),
        t.millisecond()
    )
}

pub fn parse_ts(s: &str) -> Result<OffsetDateTime, time::error::Parse> {
    OffsetDateTime::parse(s, &Rfc3339)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let d = parse_date("2027-03-31").unwrap();
        assert_eq!(fmt_date(d), "2027-03-31");
        let t = parse_ts("2026-10-02T13:00:00.123Z").unwrap();
        assert_eq!(fmt_ts(t), "2026-10-02T13:00:00.123Z");
    }
}
