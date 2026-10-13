//! Pings (docs/product-design/05-pings.md, ADR-0020): the rules for a marker on an item with a
//! day to come back to it. Pure. The wording of a ping ("pinged Mon", "for 3 days") is the UI's.

use minimap_types::{Ping, PingRow, PingsView, MAX_PING_NOTE};
use thiserror::Error;
use time::Date;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PingError {
    #[error("A ping's note can be at most {MAX_PING_NOTE} characters")]
    NoteTooLong,
    #[error("Pick today or later")]
    InThePast,
}

/// The note as it is kept (trimmed, on one line) and the day, checked.
///
/// A past day is refused ("Pick today or later") except when it is the day the ping already has
/// (`previous_on_date`), so changing only the note of an old ping works.
pub fn validate(
    note: &str,
    on_date: Date,
    today: Date,
    previous_on_date: Option<Date>,
) -> Result<String, PingError> {
    let note = note
        .split(['\n', '\r'])
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if note.chars().count() > MAX_PING_NOTE {
        return Err(PingError::NoteTooLong);
    }
    if on_date < today && previous_on_date != Some(on_date) {
        return Err(PingError::InThePast);
    }
    Ok(note)
}

/// Live and its day has come.
pub fn is_due(ping: &Ping, today: Date) -> bool {
    ping.is_live() && ping.on_date <= today
}

/// Splits the live pings of active items into those whose day has come (oldest day first, then
/// the order they were made in) and those still to come (soonest first). Cleared, removed and
/// archived-target pings are left out.
pub fn arrange(rows: Vec<PingRow>, today: Date) -> PingsView {
    let (mut due, mut upcoming): (Vec<PingRow>, Vec<PingRow>) = rows
        .into_iter()
        .filter(|r| r.ping.is_live() && !r.target.archived)
        .partition(|r| is_due(&r.ping, today));
    let order = |a: &PingRow, b: &PingRow| {
        a.ping
            .on_date
            .cmp(&b.ping.on_date)
            .then_with(|| a.ping.created_at.cmp(&b.ping.created_at))
            .then_with(|| a.ping.id.cmp(&b.ping.id))
    };
    due.sort_by(order);
    upcoming.sort_by(order);
    PingsView { due, upcoming }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{NodeRef, NodeSummary, NodeType, Uuid};
    use proptest::prelude::*;
    use time::{macros::date, Duration, OffsetDateTime};

    const TODAY: Date = date!(2027 - 03 - 03);

    fn row(
        n: u128,
        on: Date,
        cleared: Option<Date>,
        archived: bool,
        target_archived: bool,
    ) -> PingRow {
        let target = NodeRef::new(NodeType::Project, Uuid::from_u128(1000 + n));
        let made = OffsetDateTime::UNIX_EPOCH + Duration::seconds(n as i64);
        PingRow {
            ping: Ping {
                id: Uuid::from_u128(n),
                target,
                note: String::new(),
                on_date: on,
                cleared_on: cleared,
                created_at: made,
                updated_at: made,
                archived_at: archived.then_some(made),
            },
            target: NodeSummary {
                node: target,
                label: format!("P{n}"),
                archived: target_archived,
            },
        }
    }

    #[test]
    fn the_note_is_trimmed_and_kept_on_one_line() {
        assert_eq!(
            validate("  check it  ", TODAY, TODAY, None).unwrap(),
            "check it"
        );
        assert_eq!(validate("a\n b \r\n", TODAY, TODAY, None).unwrap(), "a b");
        assert_eq!(validate("   ", TODAY, TODAY, None).unwrap(), "");
    }

    #[test]
    fn a_note_can_be_200_characters_but_not_201() {
        let ok = "é".repeat(MAX_PING_NOTE);
        assert!(validate(&ok, TODAY, TODAY, None).is_ok());
        let long = "é".repeat(MAX_PING_NOTE + 1);
        assert_eq!(
            validate(&long, TODAY, TODAY, None),
            Err(PingError::NoteTooLong)
        );
    }

    #[test]
    fn a_past_day_is_refused_unless_it_is_unchanged() {
        let yesterday = TODAY - Duration::days(1);
        assert_eq!(
            validate("", yesterday, TODAY, None),
            Err(PingError::InThePast)
        );
        assert_eq!(
            validate("", yesterday, TODAY, Some(TODAY)),
            Err(PingError::InThePast)
        );
        assert!(validate("new words", yesterday, TODAY, Some(yesterday)).is_ok());
        assert!(validate("", TODAY, TODAY, None).is_ok());
        assert!(validate("", TODAY + Duration::days(30), TODAY, None).is_ok());
        assert_eq!(PingError::InThePast.to_string(), "Pick today or later");
    }

    #[test]
    fn due_is_live_and_on_or_before_today() {
        let tomorrow = TODAY + Duration::days(1);
        assert!(is_due(&row(1, TODAY, None, false, false).ping, TODAY));
        assert!(is_due(
            &row(1, TODAY - Duration::days(9), None, false, false).ping,
            TODAY
        ));
        assert!(!is_due(&row(1, tomorrow, None, false, false).ping, TODAY));
        assert!(!is_due(
            &row(1, TODAY, Some(TODAY), false, false).ping,
            TODAY
        ));
        assert!(!is_due(&row(1, TODAY, None, true, false).ping, TODAY));
    }

    #[test]
    fn arrange_orders_due_oldest_first_and_upcoming_soonest_first() {
        let d = |n: i64| TODAY + Duration::days(n);
        let view = arrange(
            vec![
                row(1, d(3), None, false, false),
                row(2, d(0), None, false, false),
                row(3, d(-4), None, false, false),
                row(4, d(1), None, false, false),
                row(5, d(-4), None, false, false),
            ],
            TODAY,
        );
        let ids = |rows: &[PingRow]| rows.iter().map(|r| r.ping.id.as_u128()).collect::<Vec<_>>();
        assert_eq!(ids(&view.due), [3, 5, 2]);
        assert_eq!(ids(&view.upcoming), [4, 1]);
    }

    #[test]
    fn cleared_removed_and_archived_target_pings_are_hidden() {
        let view = arrange(
            vec![
                row(1, TODAY, Some(TODAY), false, false),
                row(2, TODAY, None, true, false),
                row(3, TODAY, None, false, true),
                row(4, TODAY, None, false, false),
            ],
            TODAY,
        );
        assert_eq!(view.due.len(), 1);
        assert_eq!(view.due[0].ping.id.as_u128(), 4);
        assert!(view.upcoming.is_empty());
    }

    proptest! {
        /// The live pings of active targets are exactly the due ones plus the upcoming ones, with
        /// no duplicates, and each side holds the right ones.
        #[test]
        fn arrange_partitions_the_live_pings(
            specs in proptest::collection::vec((-20i64..20, any::<bool>(), any::<bool>(), any::<bool>()), 0..30)
        ) {
            let rows: Vec<PingRow> = specs
                .iter()
                .enumerate()
                .map(|(i, (offset, cleared, archived, target_archived))| {
                    row(
                        i as u128 + 1,
                        TODAY + Duration::days(*offset),
                        cleared.then_some(TODAY),
                        *archived,
                        *target_archived,
                    )
                })
                .collect();
            let live: std::collections::BTreeSet<u128> = rows
                .iter()
                .filter(|r| r.ping.is_live() && !r.target.archived)
                .map(|r| r.ping.id.as_u128())
                .collect();
            let view = arrange(rows, TODAY);
            let due: Vec<u128> = view.due.iter().map(|r| r.ping.id.as_u128()).collect();
            let up: Vec<u128> = view.upcoming.iter().map(|r| r.ping.id.as_u128()).collect();
            let mut all: Vec<u128> = due.iter().chain(up.iter()).copied().collect();
            all.sort_unstable();
            let unique: std::collections::BTreeSet<u128> = all.iter().copied().collect();
            prop_assert_eq!(all.len(), unique.len());
            prop_assert_eq!(unique, live);
            prop_assert!(view.due.iter().all(|r| r.ping.on_date <= TODAY));
            prop_assert!(view.upcoming.iter().all(|r| r.ping.on_date > TODAY));
            prop_assert!(view.due.windows(2).all(|w| w[0].ping.on_date <= w[1].ping.on_date));
            prop_assert!(view.upcoming.windows(2).all(|w| w[0].ping.on_date <= w[1].ping.on_date));
        }
    }
}
