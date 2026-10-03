//! Projects list and board: filtering, ordering and grouping.

use std::cmp::Ordering;

use minimap_types::{NodeSummary, ProjectFilter, ProjectGroup, ProjectRow, ProjectStatus};

/// Board columns, left to right (workflow order).
pub const BOARD_ORDER: [ProjectStatus; 5] = [
    ProjectStatus::Planned,
    ProjectStatus::Active,
    ProjectStatus::Paused,
    ProjectStatus::Done,
    ProjectStatus::Cancelled,
];

pub fn status_title(s: ProjectStatus) -> &'static str {
    match s {
        ProjectStatus::Planned => "Planned",
        ProjectStatus::Active => "Active",
        ProjectStatus::Paused => "Paused",
        ProjectStatus::Done => "Done",
        ProjectStatus::Cancelled => "Cancelled",
    }
}

/// Order within a list section: live work first, finished work last.
fn status_rank(s: ProjectStatus) -> u8 {
    match s {
        ProjectStatus::Active => 0,
        ProjectStatus::Planned => 1,
        ProjectStatus::Paused => 2,
        ProjectStatus::Done => 3,
        ProjectStatus::Cancelled => 4,
    }
}

pub fn filter(rows: Vec<ProjectRow>, f: &ProjectFilter) -> Vec<ProjectRow> {
    rows.into_iter()
        .filter(|r| f.status.is_none_or(|s| r.project.status == s))
        .filter(|r| {
            f.owner_person_id
                .is_none_or(|o| r.project.owner_person_id == Some(o))
        })
        .filter(|r| {
            f.objective_id
                .is_none_or(|o| r.objectives.iter().any(|x| x.node.id == o))
        })
        .collect()
}

/// Priority 1 first, then earlier target date (undated last), then title, then id.
fn by_priority(a: &ProjectRow, b: &ProjectRow) -> Ordering {
    let (a, b) = (&a.project, &b.project);
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

/// One section per objective (by name), then "No objective". A project that contributes to
/// several objectives appears under each. Inside a section: status (active first), priority,
/// target date.
pub fn by_objective(rows: Vec<ProjectRow>) -> Vec<ProjectGroup> {
    let mut sections: Vec<(Option<NodeSummary>, Vec<ProjectRow>)> = Vec::new();
    for row in rows {
        let targets: Vec<Option<NodeSummary>> = if row.objectives.is_empty() {
            vec![None]
        } else {
            row.objectives.iter().cloned().map(Some).collect()
        };
        for target in targets {
            match sections
                .iter_mut()
                .find(|(o, _)| o.as_ref().map(|o| o.node.id) == target.as_ref().map(|o| o.node.id))
            {
                Some((_, rows)) => rows.push(row.clone()),
                None => sections.push((target, vec![row.clone()])),
            }
        }
    }
    // Named objectives alphabetically, the "No objective" bucket last.
    sections.sort_by(|(a, _), (b, _)| match (a, b) {
        (Some(a), Some(b)) => a.label.to_lowercase().cmp(&b.label.to_lowercase()),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    });
    sections
        .into_iter()
        .map(|(objective, mut rows)| {
            rows.sort_by(|a, b| {
                status_rank(a.project.status)
                    .cmp(&status_rank(b.project.status))
                    .then_with(|| by_priority(a, b))
            });
            ProjectGroup {
                label: objective
                    .as_ref()
                    .map_or_else(|| "No objective".to_owned(), |o| o.label.clone()),
                objective,
                status: None,
                rows,
            }
        })
        .collect()
}

/// Every status gets a column, even when empty, so cards can be dropped anywhere.
pub fn by_status(rows: Vec<ProjectRow>) -> Vec<ProjectGroup> {
    BOARD_ORDER
        .iter()
        .map(|&status| {
            let mut rows: Vec<ProjectRow> = rows
                .iter()
                .filter(|r| r.project.status == status)
                .cloned()
                .collect();
            rows.sort_by(by_priority);
            ProjectGroup {
                label: status_title(status).to_owned(),
                objective: None,
                status: Some(status),
                rows,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{NodeRef, NodeType, Project, Uuid};
    use time::{macros::date, Date, OffsetDateTime};

    fn objective(n: u128, label: &str) -> NodeSummary {
        NodeSummary {
            node: NodeRef::new(NodeType::Objective, Uuid::from_u128(n)),
            label: label.into(),
            archived: false,
        }
    }

    fn row(
        n: u128,
        title: &str,
        status: ProjectStatus,
        priority: u8,
        target: Option<Date>,
        objectives: Vec<NodeSummary>,
    ) -> ProjectRow {
        ProjectRow {
            project: Project {
                id: Uuid::from_u128(n),
                title: title.into(),
                slug: title.to_lowercase(),
                description: String::new(),
                owner_person_id: n.is_multiple_of(2).then(|| Uuid::from_u128(900)),
                start_date: None,
                target_date: target,
                status,
                priority,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                archived_at: None,
            },
            owner: None,
            objectives,
            task_count: 0,
            done_task_count: 0,
        }
    }

    fn titles(g: &ProjectGroup) -> Vec<&str> {
        g.rows.iter().map(|r| r.project.title.as_str()).collect()
    }

    #[test]
    fn list_groups_by_objective_with_no_objective_last() {
        let (eu, growth) = (objective(1, "Launch EU"), objective(2, "Grow ARR"));
        let rows = vec![
            row(1, "loose", ProjectStatus::Active, 1, None, vec![]),
            row(
                2,
                "shared",
                ProjectStatus::Planned,
                2,
                None,
                vec![eu.clone(), growth.clone()],
            ),
            row(
                3,
                "eu only",
                ProjectStatus::Active,
                3,
                None,
                vec![eu.clone()],
            ),
        ];
        let groups = by_objective(rows);
        let labels: Vec<_> = groups.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(labels, vec!["Grow ARR", "Launch EU", "No objective"]);
        // A project with two objectives shows under both.
        assert_eq!(titles(&groups[0]), vec!["shared"]);
        assert_eq!(titles(&groups[1]), vec!["eu only", "shared"]); // active before planned
        assert_eq!(titles(&groups[2]), vec!["loose"]);
        assert!(groups.iter().all(|g| g.status.is_none()));
    }

    #[test]
    fn list_orders_by_status_then_priority_then_date() {
        let o = objective(1, "O");
        let rows = vec![
            row(1, "done p1", ProjectStatus::Done, 1, None, vec![o.clone()]),
            row(
                2,
                "active p3",
                ProjectStatus::Active,
                3,
                None,
                vec![o.clone()],
            ),
            row(
                3,
                "active p1 late",
                ProjectStatus::Active,
                1,
                Some(date!(2027 - 06 - 01)),
                vec![o.clone()],
            ),
            row(
                4,
                "active p1 early",
                ProjectStatus::Active,
                1,
                Some(date!(2027 - 01 - 01)),
                vec![o.clone()],
            ),
            row(5, "paused", ProjectStatus::Paused, 1, None, vec![o.clone()]),
            row(
                6,
                "planned",
                ProjectStatus::Planned,
                1,
                None,
                vec![o.clone()],
            ),
        ];
        let g = by_objective(rows);
        assert_eq!(
            titles(&g[0]),
            vec![
                "active p1 early",
                "active p1 late",
                "active p3",
                "planned",
                "paused",
                "done p1"
            ]
        );
    }

    #[test]
    fn board_has_a_column_per_status_even_when_empty() {
        let rows = vec![
            row(1, "b", ProjectStatus::Active, 2, None, vec![]),
            row(2, "a", ProjectStatus::Active, 2, None, vec![]),
            row(3, "c", ProjectStatus::Done, 1, None, vec![]),
        ];
        let cols = by_status(rows);
        let labels: Vec<_> = cols.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["Planned", "Active", "Paused", "Done", "Cancelled"]
        );
        assert_eq!(
            cols.iter().map(|c| c.status.unwrap()).collect::<Vec<_>>(),
            BOARD_ORDER.to_vec()
        );
        assert!(cols[0].rows.is_empty() && cols[2].rows.is_empty() && cols[4].rows.is_empty());
        assert_eq!(titles(&cols[1]), vec!["a", "b"]);
        assert_eq!(titles(&cols[3]), vec!["c"]);
        // A project is in exactly one column.
        assert_eq!(cols.iter().map(|c| c.rows.len()).sum::<usize>(), 3);
    }

    #[test]
    fn filters_combine() {
        let eu = objective(1, "EU");
        let rows = vec![
            row(1, "a", ProjectStatus::Active, 1, None, vec![eu.clone()]),
            row(2, "b", ProjectStatus::Active, 1, None, vec![]),
            row(4, "c", ProjectStatus::Planned, 1, None, vec![eu.clone()]),
        ];
        let names = |f: &ProjectFilter| -> Vec<String> {
            filter(rows.clone(), f)
                .into_iter()
                .map(|r| r.project.title)
                .collect()
        };
        assert_eq!(names(&ProjectFilter::default()), vec!["a", "b", "c"]);
        let active = ProjectFilter {
            status: Some(ProjectStatus::Active),
            ..Default::default()
        };
        assert_eq!(names(&active), vec!["a", "b"]);
        let in_eu = ProjectFilter {
            objective_id: Some(eu.node.id),
            ..Default::default()
        };
        assert_eq!(names(&in_eu), vec!["a", "c"]);
        let both = ProjectFilter {
            status: Some(ProjectStatus::Planned),
            objective_id: Some(eu.node.id),
            ..Default::default()
        };
        assert_eq!(names(&both), vec!["c"]);
        // Owner: rows with an even id are owned by person 900.
        let owned = ProjectFilter {
            owner_person_id: Some(Uuid::from_u128(900)),
            ..Default::default()
        };
        assert_eq!(names(&owned), vec!["b", "c"]);
    }
}
