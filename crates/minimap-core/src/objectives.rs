//! Objectives list rules: ordering and grouping by calendar quarter of the target date.

use std::cmp::Ordering;

use minimap_types::{
    NodeRef, NodeSummary, NodeType, Objective, ObjectiveGroup, ObjectiveGrouping, ObjectiveRow,
    ReviewDue,
};
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

/// The ongoing objectives (spec 30) whose review is overdue, or falls on or before `until`,
/// most overdue first. Goals and ongoing objectives with no review rhythm are never listed.
pub fn reviews_due(objectives: &[Objective], today: Date, until: Date) -> Vec<ReviewDue> {
    let mut out: Vec<ReviewDue> = objectives
        .iter()
        .filter(|o| o.archived_at.is_none())
        .filter_map(|o| {
            let due = o.review_due()?;
            (due <= until).then(|| ReviewDue {
                objective: NodeSummary {
                    node: NodeRef::new(NodeType::Objective, o.id),
                    label: o.title.clone(),
                    archived: false,
                },
                due,
                overdue_days: o.review_overdue_days(today),
            })
        })
        .collect();
    out.sort_by(|a, b| {
        a.due
            .cmp(&b.due)
            .then_with(|| a.objective.label.cmp(&b.objective.label))
    });
    out
}

/// Sorts the rows and arranges them into groups: one unlabelled group for a flat list, or
/// one group per quarter in date order with a "No date" group. Ongoing objectives (spec 30)
/// come last, in a group of their own, in either layout.
pub fn arrange(rows: Vec<ObjectiveRow>, grouping: ObjectiveGrouping) -> Vec<ObjectiveGroup> {
    let (mut ongoing, mut rows): (Vec<_>, Vec<_>) =
        rows.into_iter().partition(|r| r.objective.ongoing);
    rows.sort_by(compare);
    ongoing.sort_by(compare);
    let mut groups = arrange_goals(rows, grouping);
    if !ongoing.is_empty() {
        groups.push(ObjectiveGroup {
            label: Some("Ongoing".to_owned()),
            rows: ongoing,
        });
    }
    groups
}

fn arrange_goals(rows: Vec<ObjectiveRow>, grouping: ObjectiveGrouping) -> Vec<ObjectiveGroup> {
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
                ongoing: false,
                review_every_days: None,
                last_reviewed_on: None,
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

    fn ongoing(mut r: ObjectiveRow, every: Option<u32>, last: Option<Date>) -> ObjectiveRow {
        r.objective.ongoing = true;
        r.objective.review_every_days = every;
        r.objective.last_reviewed_on = last;
        r
    }

    #[test]
    fn ongoing_objectives_are_their_own_group_last_in_either_layout() {
        let rows = vec![
            ongoing(row(1, "Maintenance", 1, None), None, None),
            row(2, "Launch", 2, Some(date!(2027 - 03 - 31))),
            row(3, "Someday", 3, None),
        ];
        let flat = arrange(rows.clone(), ObjectiveGrouping::None);
        assert_eq!(
            flat.iter().map(|g| g.label.as_deref()).collect::<Vec<_>>(),
            [None, Some("Ongoing")]
        );
        assert_eq!(
            titles(&flat),
            [vec!["Launch", "Someday"], vec!["Maintenance"]]
        );
        let by_quarter = arrange(rows, ObjectiveGrouping::Quarter);
        assert_eq!(
            by_quarter
                .iter()
                .map(|g| g.label.as_deref().unwrap())
                .collect::<Vec<_>>(),
            ["Q1 2027", "No date", "Ongoing"]
        );
    }

    #[test]
    fn a_review_is_due_a_rhythm_after_the_last_one_or_after_creation() {
        let reviewed =
            ongoing(row(1, "A", 3, None), Some(30), Some(date!(2027 - 01 - 01))).objective;
        assert_eq!(reviewed.review_due(), Some(date!(2027 - 01 - 31)));
        // Never reviewed: counted from the day it was made (1970-01-01 in these tests).
        let fresh = ongoing(row(2, "B", 3, None), Some(7), None).objective;
        assert_eq!(fresh.review_due(), Some(date!(1970 - 01 - 08)));
        // No rhythm, or not ongoing: nothing is ever due.
        assert_eq!(
            ongoing(row(3, "C", 3, None), None, None)
                .objective
                .review_due(),
            None
        );
        let mut goal = row(4, "D", 3, None).objective;
        goal.review_every_days = Some(30);
        assert_eq!(goal.review_due(), None);
        // Overdue counts days past, and is `None` on the day and before.
        let today = date!(2027 - 02 - 04);
        assert_eq!(reviewed.review_overdue_days(today), Some(4));
        assert_eq!(reviewed.review_overdue_days(date!(2027 - 01 - 31)), None);
    }

    #[test]
    fn the_reviews_listed_are_overdue_or_due_by_the_end_of_the_week() {
        let today = date!(2027 - 03 - 03);
        let sunday = date!(2027 - 03 - 07);
        let mk = |n: u128, name: &str, last: Date| {
            ongoing(row(n, name, 3, None), Some(30), Some(last)).objective
        };
        let list = vec![
            mk(1, "Overdue", date!(2027 - 01 - 20)),   // due 02-19
            mk(2, "This week", date!(2027 - 02 - 05)), // due 03-07
            mk(3, "Later", date!(2027 - 02 - 20)),     // due 03-22
            row(4, "Goal", 3, None).objective,
        ];
        let due = reviews_due(&list, today, sunday);
        let names: Vec<&str> = due.iter().map(|r| r.objective.label.as_str()).collect();
        assert_eq!(names, ["Overdue", "This week"]);
        assert_eq!(due[0].overdue_days, Some(12));
        assert_eq!(due[1].overdue_days, None);
    }
}
