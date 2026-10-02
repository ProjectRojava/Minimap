//! Row <-> DTO conversion helpers.

use minimap_types::timefmt::{fmt_date, fmt_ts, parse_date, parse_ts};
use rusqlite::{types::Type, Row};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use crate::error::{Result, StoreError};

pub fn now() -> OffsetDateTime {
    let t = OffsetDateTime::now_utc();
    // Millisecond precision keeps stored timestamps fixed-width.
    t.replace_nanosecond(t.nanosecond() / 1_000_000 * 1_000_000)
        .unwrap_or(t)
}

pub fn today() -> Date {
    now().date()
}

fn conv_err<E: std::error::Error + Send + Sync + 'static>(i: usize, e: E) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(i, Type::Text, Box::new(e))
}

pub fn col_uuid(r: &Row, i: usize) -> rusqlite::Result<Uuid> {
    let s: String = r.get(i)?;
    Uuid::parse_str(&s).map_err(|e| conv_err(i, e))
}

pub fn col_uuid_opt(r: &Row, i: usize) -> rusqlite::Result<Option<Uuid>> {
    let s: Option<String> = r.get(i)?;
    s.map(|s| Uuid::parse_str(&s).map_err(|e| conv_err(i, e)))
        .transpose()
}

pub fn col_date(r: &Row, i: usize) -> rusqlite::Result<Date> {
    let s: String = r.get(i)?;
    parse_date(&s).map_err(|e| conv_err(i, e))
}

pub fn col_date_opt(r: &Row, i: usize) -> rusqlite::Result<Option<Date>> {
    let s: Option<String> = r.get(i)?;
    s.map(|s| parse_date(&s).map_err(|e| conv_err(i, e)))
        .transpose()
}

pub fn col_ts(r: &Row, i: usize) -> rusqlite::Result<OffsetDateTime> {
    let s: String = r.get(i)?;
    parse_ts(&s).map_err(|e| conv_err(i, e))
}

pub fn col_ts_opt(r: &Row, i: usize) -> rusqlite::Result<Option<OffsetDateTime>> {
    let s: Option<String> = r.get(i)?;
    s.map(|s| parse_ts(&s).map_err(|e| conv_err(i, e)))
        .transpose()
}

pub fn col_enum<T>(r: &Row, i: usize) -> rusqlite::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    let s: String = r.get(i)?;
    s.parse().map_err(|e| conv_err(i, e))
}

pub fn id_s(id: Uuid) -> String {
    id.to_string()
}

pub fn id_opt_s(id: Option<Uuid>) -> Option<String> {
    id.map(id_s)
}

pub fn date_s(d: Option<Date>) -> Option<String> {
    d.map(fmt_date)
}

pub fn ts_s(t: OffsetDateTime) -> String {
    fmt_ts(t)
}

pub fn ts_opt_s(t: Option<OffsetDateTime>) -> Option<String> {
    t.map(fmt_ts)
}

pub fn ensure_priority(p: u8) -> Result<()> {
    if (1..=5).contains(&p) {
        Ok(())
    } else {
        Err(StoreError::Invalid(format!(
            "priority must be 1-5, got {p}"
        )))
    }
}

pub fn ensure_not_blank(field: &str, v: &str) -> Result<()> {
    if v.trim().is_empty() {
        Err(StoreError::Invalid(format!("{field} must not be empty")))
    } else {
        Ok(())
    }
}
