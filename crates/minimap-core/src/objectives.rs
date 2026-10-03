//! Objectives list rules: ordering and grouping by calendar quarter of the target date.

use std::cmp::Ordering;

use minimap_types::{ObjectiveGroup, ObjectiveGrouping, ObjectiveRow};
use time::Date;

/// Calendar quarter (1-4) of a date.
pub fn quarter_of(date: Date) -> (i32, u8) {
    (date.year(), (u8::from(date.month()) - 1) / 3 + 1)
}

pub fn quarter_label((year, quarter): (i32, u8)) -> String {
    format!("Q{quarter} {year}")
}

/// Priority 1 is the highest. Within a priority: earlier target date first, undated last,
/// then title, then id so the order is stable.
fn compare(a: &ObjectiveRow, b: &ObjectiveRow) -> Ordering {
    let (a, b) = (&a.objective, &b.objective);
    a.priority
        .cmp(&b.priority)
        .then_with(|| match (a.target_date, b.target_date) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        })
        .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        .then_with(|| a.id.cmp(&b.id))
}

/// Sorts the rows and arranges them into groups: one unlabelled group for a flat list, or
/// one group per quarter in date order with a final "No date" group.
pub fn arrange(mut rows: Vec<ObjectiveRow>, grouping: ObjectiveGrouping) -> Vec<ObjectiveGroup> {
    rows.sort_by(compare);
    if rows.is_empty() {
        return Vec::new();
    }
    match grouping {
        ObjectiveGrouping::None => vec![ObjectiveGroup { label: None, rows }],
        ObjectiveGrouping::Quarter => {
            let mut dated: std::collections::BTreeMap<(i32, u8), Vec<ObjectiveRow>> =
                Default::default();
            let mut undated = Vec::new();
            for row in rows {
                match row.objective.target_date {
                    Some(d) => dated.entry(quarter_of(d)).or_default().push(row),
                    None => undated.push(row),
                }
            }
            let mut groups: Vec<ObjectiveGroup> = dated
                .into_iter()
                .map(|(q, rows)| ObjectiveGroup {
                    label: Some(quarter_label(q)),
                    rows,
                })
                .collect();
            if !undated.is_empty() {
                groups.push(ObjectiveGroup {
                    label: Some("No date".to_owned()),
                    rows: undated,
                });
            }
            groups
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{Objective, ObjectiveStatus, Uuid};
    use time::{macros::date, OffsetDateTime};

    fn row(n: u128, title: &str, priority: u8, target: Option<Date>) -> ObjectiveRow {
        ObjectiveRow {
            objective: Objective {
                id: Uuid::from_u128(n),
                title: title.into(),
                description: String::new(),
                target_date: target,
                status: ObjectiveStatus::OnTrack,
                priority,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                archived_at: None,
            },
            contribution_count: 0,
        }
    }

    fn titles(groups: &[ObjectiveGroup]) -> Vec<Vec<&str>> {
        groups
            .iter()
            .map(|g| g.rows.iter().map(|r| r.objective.title.as_str()).collect())
            .collect()
    }

    #[test]
    fn quarter_boundaries() {
        assert_eq!(quarter_of(date!(2027 - 01 - 01)), (2027, 1));
        assert_eq!(quarter_of(date!(2027 - 03 - 31)), (2027, 1));
        assert_eq!(quarter_of(date!(2027 - 04 - 01)), (2027, 2));
        assert_eq!(quarter_of(date!(2027 - 09 - 30)), (2027, 3));
        assert_eq!(quarter_of(date!(2027 - 12 - 31)), (2027, 4));
        assert_eq!(quarter_label((2027, 1)), "Q1 2027");
    }

    #[test]
    fn flat_list_sorts_by_priority_then_date_with_undated_last() {
        let rows = vec![
            row(1, "low", 5, Some(date!(2027 - 01 - 01))),
            row(2, "p1 late", 1, Some(date!(2027 - 06 - 01))),
            row(3, "p1 undated", 1, None),
            row(4, "p1 early", 1, Some(date!(2027 - 02 - 01))),
            row(5, "p3", 3, None),
        ];
        let groups = arrange(rows, ObjectiveGrouping::None);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].label, None);
        assert_eq!(
            titles(&groups)[0],
            vec!["p1 early", "p1 late", "p1 undated", "p3", "low"]
        );
    }

    #[test]
    fn quarter_groups_are_chronological_with_no_date_last() {
        let rows = vec![
            row(1, "q3", 1, Some(date!(2027 - 08 - 15))),
            row(2, "undated", 1, None),
            row(3, "q1 p2", 2, Some(date!(2027 - 01 - 10))),
            row(4, "q1 p1", 1, Some(date!(2027 - 03 - 31))),
            row(5, "next year", 1, Some(date!(2028 - 01 - 02))),
        ];
        let groups = arrange(rows, ObjectiveGrouping::Quarter);
        let labels: Vec<_> = groups.iter().map(|g| g.label.as_deref().unwrap()).collect();
        assert_eq!(labels, vec!["Q1 2027", "Q3 2027", "Q1 2028", "No date"]);
        assert_eq!(
            titles(&groups),
            vec![
                vec!["q1 p1", "q1 p2"],
                vec!["q3"],
                vec!["next year"],
                vec!["undated"]
            ]
        );
    }

    #[test]
    fn empty_input_has_no_groups() {
        assert!(arrange(Vec::new(), ObjectiveGrouping::Quarter).is_empty());
        assert!(arrange(Vec::new(), ObjectiveGrouping::None).is_empty());
    }

    #[test]
    fn ties_are_stable_by_title_then_id() {
        let rows = vec![
            row(2, "b", 3, None),
            row(1, "B", 3, None),
            row(3, "a", 3, None),
        ];
        let groups = arrange(rows, ObjectiveGrouping::None);
        let ids: Vec<u128> = groups[0]
            .rows
            .iter()
            .map(|r| r.objective.id.as_u128())
            .collect();
        assert_eq!(ids, vec![3, 1, 2]);
    }
}
