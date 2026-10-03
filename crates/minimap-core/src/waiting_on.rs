//! Waiting-on rules: age, stale, snoozed, and the list order.

use minimap_types::{WaitingOn, WaitingOnFilter, WaitingOnItem, WaitingOnRow};
use time::Date;

/// Whole days since it was asked (0 if the date is in the future).
pub fn age_days(w: &WaitingOn, today: Date) -> i64 {
    (today - w.asked_on).whole_days().max(0)
}

pub fn is_open(w: &WaitingOn) -> bool {
    w.resolved_on.is_none()
}

/// Open and hidden until a future follow-up date; on that date it resurfaces.
pub fn is_snoozed(w: &WaitingOn, today: Date) -> bool {
    is_open(w) && w.follow_up_on.is_some_and(|d| d > today)
}

/// Open, not snoozed, and past its expected date.
pub fn is_overdue(w: &WaitingOn, today: Date) -> bool {
    is_open(w) && !is_snoozed(w, today) && w.expected_by.is_some_and(|d| d < today)
}

/// Open, not snoozed, and either asked more than `stale_days` days ago or past its expected date.
/// A snoozed item is not stale until it resurfaces.
pub fn is_stale(w: &WaitingOn, today: Date, stale_days: u32) -> bool {
    is_open(w)
        && !is_snoozed(w, today)
        && (age_days(w, today) > i64::from(stale_days) || is_overdue(w, today))
}

/// Filters, computes the flags and orders the list: oldest first (then by description, id).
pub fn arrange(
    items: Vec<WaitingOnItem>,
    filter: &WaitingOnFilter,
    today: Date,
    stale_days: u32,
) -> Vec<WaitingOnRow> {
    let mut rows: Vec<WaitingOnRow> = items
        .into_iter()
        .filter(|i| filter.include_resolved || is_open(&i.waiting))
        .filter(|i| filter.include_snoozed || !is_snoozed(&i.waiting, today))
        .filter(|i| filter.person_id.is_none_or(|p| i.waiting.person_id == p))
        .map(|i| WaitingOnRow {
            age_days: age_days(&i.waiting, today),
            stale: is_stale(&i.waiting, today, stale_days),
            snoozed: is_snoozed(&i.waiting, today),
            overdue: is_overdue(&i.waiting, today),
            waiting: i.waiting,
            person: i.person,
            about: i.about,
        })
        .collect();
    rows.sort_by(|a, b| {
        a.waiting
            .asked_on
            .cmp(&b.waiting.asked_on)
            .then_with(|| {
                a.waiting
                    .description
                    .to_lowercase()
                    .cmp(&b.waiting.description.to_lowercase())
            })
            .then_with(|| a.waiting.id.cmp(&b.waiting.id))
    });
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{NodeRef, NodeSummary, NodeType, Uuid};
    use proptest::prelude::*;
    use time::{macros::date, Duration, OffsetDateTime};

    const TODAY: Date = date!(2027 - 03 - 15);

    fn waiting(n: u128, asked_days_ago: i64) -> WaitingOn {
        WaitingOn {
            id: Uuid::from_u128(n),
            description: format!("item {n}"),
            person_id: Uuid::from_u128(900),
            asked_on: TODAY - Duration::days(asked_days_ago),
            expected_by: None,
            follow_up_on: None,
            resolved_on: None,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            archived_at: None,
        }
    }

    fn item(w: WaitingOn) -> WaitingOnItem {
        WaitingOnItem {
            person: NodeSummary {
                node: NodeRef::new(NodeType::Person, w.person_id),
                label: "Raj".into(),
                archived: false,
            },
            about: None,
            waiting: w,
        }
    }

    #[test]
    fn staleness_is_strictly_older_than_the_threshold() {
        assert!(
            !is_stale(&waiting(1, 7), TODAY, 7),
            "exactly 7 days is not stale"
        );
        assert!(is_stale(&waiting(1, 8), TODAY, 7));
        assert!(
            !is_stale(&waiting(1, 8), TODAY, 14),
            "the threshold is a setting"
        );
        assert!(is_stale(&waiting(1, 15), TODAY, 14));
        assert_eq!(age_days(&waiting(1, 0), TODAY), 0);
        // Asked "in the future" never has a negative age.
        assert_eq!(age_days(&waiting(1, -3), TODAY), 0);
    }

    #[test]
    fn past_expected_date_is_stale_even_when_young() {
        let mut w = waiting(1, 1);
        w.expected_by = Some(TODAY - Duration::days(1));
        assert!(is_stale(&w, TODAY, 7) && is_overdue(&w, TODAY));
        w.expected_by = Some(TODAY); // due today is not yet late
        assert!(!is_stale(&w, TODAY, 7) && !is_overdue(&w, TODAY));
        w.expected_by = Some(TODAY + Duration::days(3));
        assert!(!is_stale(&w, TODAY, 7));
    }

    #[test]
    fn resolved_items_are_never_stale_or_snoozed() {
        let mut w = waiting(1, 30);
        w.resolved_on = Some(TODAY);
        w.follow_up_on = Some(TODAY + Duration::days(5));
        assert!(!is_stale(&w, TODAY, 7));
        assert!(!is_snoozed(&w, TODAY));
    }

    #[test]
    fn snoozed_until_a_future_date_then_resurfaces() {
        let mut w = waiting(1, 30);
        w.follow_up_on = Some(TODAY + Duration::days(2));
        assert!(is_snoozed(&w, TODAY));
        assert!(!is_stale(&w, TODAY, 7), "snoozed items are not stale");
        w.follow_up_on = Some(TODAY + Duration::days(1));
        assert!(is_snoozed(&w, TODAY));
        // On the follow-up date it is back, and stale again if it qualifies.
        w.follow_up_on = Some(TODAY);
        assert!(!is_snoozed(&w, TODAY));
        assert!(is_stale(&w, TODAY, 7));
        w.follow_up_on = Some(TODAY - Duration::days(4));
        assert!(!is_snoozed(&w, TODAY));
    }

    #[test]
    fn list_is_oldest_first_and_hides_snoozed_and_resolved_by_default() {
        let mut snoozed = waiting(2, 20);
        snoozed.follow_up_on = Some(TODAY + Duration::days(3));
        let mut resolved = waiting(3, 40);
        resolved.resolved_on = Some(TODAY);
        let items = vec![
            item(waiting(1, 3)),
            item(snoozed),
            item(resolved),
            item(waiting(4, 12)),
            item(waiting(5, 3)),
        ];
        let ids = |f: &WaitingOnFilter| -> Vec<u128> {
            arrange(items.clone(), f, TODAY, 7)
                .iter()
                .map(|r| r.waiting.id.as_u128())
                .collect()
        };
        assert_eq!(ids(&WaitingOnFilter::default()), vec![4, 1, 5]); // oldest first; ties by name
        assert_eq!(
            ids(&WaitingOnFilter {
                include_snoozed: true,
                ..Default::default()
            }),
            vec![2, 4, 1, 5]
        );
        assert_eq!(
            ids(&WaitingOnFilter {
                include_resolved: true,
                ..Default::default()
            }),
            vec![3, 4, 1, 5]
        );
        assert_eq!(
            ids(&WaitingOnFilter {
                include_snoozed: true,
                include_resolved: true,
                ..Default::default()
            }),
            vec![3, 2, 4, 1, 5]
        );
        let other = WaitingOnFilter {
            person_id: Some(Uuid::from_u128(1)),
            ..Default::default()
        };
        assert!(ids(&other).is_empty());
    }

    #[test]
    fn rows_carry_age_and_flags() {
        let mut snoozed = waiting(2, 20);
        snoozed.follow_up_on = Some(TODAY + Duration::days(3));
        let rows = arrange(
            vec![item(waiting(1, 10)), item(snoozed), item(waiting(3, 2))],
            &WaitingOnFilter {
                include_snoozed: true,
                ..Default::default()
            },
            TODAY,
            7,
        );
        let flags: Vec<(i64, bool, bool)> = rows
            .iter()
            .map(|r| (r.age_days, r.stale, r.snoozed))
            .collect();
        assert_eq!(
            flags,
            vec![(20, false, true), (10, true, false), (2, false, false)]
        );
    }

    proptest! {
        /// Whatever the dates, a row is never both stale and snoozed, and resolved rows are neither.
        #[test]
        fn flags_are_consistent(
            asked in -400i64..400,
            expected in proptest::option::of(-400i64..400),
            follow in proptest::option::of(-400i64..400),
            resolved in any::<bool>(),
            stale_days in 1u32..365,
        ) {
            let mut w = waiting(1, asked);
            w.expected_by = expected.map(|d| TODAY + Duration::days(d));
            w.follow_up_on = follow.map(|d| TODAY + Duration::days(d));
            w.resolved_on = resolved.then_some(TODAY);
            let (stale, snoozed) = (is_stale(&w, TODAY, stale_days), is_snoozed(&w, TODAY));
            prop_assert!(!(stale && snoozed));
            if resolved { prop_assert!(!stale && !snoozed && !is_overdue(&w, TODAY)); }
            // Overdue always counts as stale.
            if is_overdue(&w, TODAY) { prop_assert!(stale); }
            prop_assert!(age_days(&w, TODAY) >= 0);
        }
    }
}
