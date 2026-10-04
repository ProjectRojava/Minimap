//! Decision list rules: filtering and the newest-first order.

use std::cmp::Ordering;

use minimap_types::{Decision, DecisionFilter, DecisionItem, DecisionRow};
use time::Date;

use crate::notes::excerpt;

/// The date a decision sorts and filters by: when it was decided, else when it was written down.
pub fn effective_date(d: &Decision) -> Date {
    d.decided_on.unwrap_or_else(|| d.created_at.date())
}

/// Filters decisions and orders them newest first (decision date, then creation).
pub fn arrange(items: Vec<DecisionItem>, filter: &DecisionFilter) -> Vec<DecisionRow> {
    let terms: Vec<String> = filter
        .text
        .as_deref()
        .unwrap_or("")
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();
    let ranged = filter.date_from.is_some() || filter.date_to.is_some();
    let mut kept: Vec<DecisionItem> = items
        .into_iter()
        .filter(|i| filter.status.is_none_or(|s| i.decision.status == s))
        .filter(|i| {
            filter
                .affects_id
                .is_none_or(|id| i.affects.iter().any(|a| a.node.id == id))
        })
        .filter(|i| {
            if !ranged {
                return true;
            }
            let Some(on) = i.decision.decided_on else {
                return false;
            };
            filter.date_from.is_none_or(|d| on >= d) && filter.date_to.is_none_or(|d| on <= d)
        })
        .filter(|i| {
            if terms.is_empty() {
                return true;
            }
            let d = &i.decision;
            let hay =
                format!("{} {} {} {}", d.title, d.context, d.decision, d.rationale).to_lowercase();
            terms.iter().all(|t| hay.contains(t))
        })
        .collect();
    kept.sort_by(|a, b| -> Ordering {
        effective_date(&b.decision)
            .cmp(&effective_date(&a.decision))
            .then_with(|| b.decision.created_at.cmp(&a.decision.created_at))
            .then_with(|| b.decision.id.cmp(&a.decision.id))
    });
    kept.into_iter()
        .map(|i| {
            let d = i.decision;
            let text = if d.decision.trim().is_empty() {
                &d.context
            } else {
                &d.decision
            };
            DecisionRow {
                excerpt: excerpt(text, 120),
                id: d.id,
                title: d.title,
                status: d.status,
                decided_on: d.decided_on,
                affects: i.affects,
                superseded_by: i.superseded_by,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{DecisionStatus, NodeRef, NodeSummary, NodeType, Uuid};
    use time::{macros::date, OffsetDateTime};

    fn item(
        n: u128,
        title: &str,
        decided_on: Option<Date>,
        status: DecisionStatus,
    ) -> DecisionItem {
        DecisionItem {
            decision: Decision {
                id: Uuid::from_u128(n),
                title: title.into(),
                context: String::new(),
                decision: format!("{title} it is"),
                rationale: String::new(),
                decided_on,
                status,
                created_at: OffsetDateTime::UNIX_EPOCH + time::Duration::days(n as i64),
                updated_at: OffsetDateTime::UNIX_EPOCH,
                archived_at: None,
            },
            affects: vec![],
            superseded_by: None,
        }
    }

    fn titles(rows: &[DecisionRow]) -> Vec<&str> {
        rows.iter().map(|r| r.title.as_str()).collect()
    }

    #[test]
    fn newest_first_by_decision_date() {
        let items = vec![
            item(
                1,
                "old",
                Some(date!(2027 - 01 - 05)),
                DecisionStatus::Decided,
            ),
            item(
                2,
                "new",
                Some(date!(2027 - 03 - 01)),
                DecisionStatus::Decided,
            ),
            item(
                3,
                "mid",
                Some(date!(2027 - 02 - 01)),
                DecisionStatus::Decided,
            ),
        ];
        let rows = arrange(items, &DecisionFilter::default());
        assert_eq!(titles(&rows), ["new", "mid", "old"]);
    }

    #[test]
    fn undated_proposals_sort_by_when_they_were_written() {
        // Created 1970-01-xx, so both sort below any real decision date, newest creation first.
        let items = vec![
            item(2, "first draft", None, DecisionStatus::Proposed),
            item(9, "later draft", None, DecisionStatus::Proposed),
            item(
                1,
                "decided",
                Some(date!(2027 - 01 - 05)),
                DecisionStatus::Decided,
            ),
        ];
        let rows = arrange(items, &DecisionFilter::default());
        assert_eq!(titles(&rows), ["decided", "later draft", "first draft"]);
    }

    #[test]
    fn filters_by_status_text_and_what_they_affect() {
        let mut a = item(
            1,
            "Postgres",
            Some(date!(2027 - 01 - 05)),
            DecisionStatus::Decided,
        );
        let project = Uuid::from_u128(100);
        a.affects.push(NodeSummary {
            node: NodeRef::new(NodeType::Project, project),
            label: "API".into(),
            archived: false,
        });
        let b = item(2, "Mongo", None, DecisionStatus::Proposed);
        let items = vec![a, b];

        let by_status = DecisionFilter {
            status: Some(DecisionStatus::Proposed),
            ..Default::default()
        };
        assert_eq!(titles(&arrange(items.clone(), &by_status)), ["Mongo"]);

        let by_text = DecisionFilter {
            text: Some("POSTGRES it".into()),
            ..Default::default()
        };
        assert_eq!(titles(&arrange(items.clone(), &by_text)), ["Postgres"]);

        let by_node = DecisionFilter {
            affects_id: Some(project),
            ..Default::default()
        };
        assert_eq!(titles(&arrange(items, &by_node)), ["Postgres"]);
    }

    #[test]
    fn date_range_is_inclusive_and_skips_undated() {
        let items = vec![
            item(1, "a", Some(date!(2027 - 03 - 01)), DecisionStatus::Decided),
            item(2, "b", Some(date!(2027 - 03 - 07)), DecisionStatus::Decided),
            item(3, "c", Some(date!(2027 - 03 - 08)), DecisionStatus::Decided),
            item(4, "undated", None, DecisionStatus::Proposed),
        ];
        let week = DecisionFilter {
            date_from: Some(date!(2027 - 03 - 01)),
            date_to: Some(date!(2027 - 03 - 07)),
            ..Default::default()
        };
        assert_eq!(titles(&arrange(items, &week)), ["b", "a"]);
    }

    #[test]
    fn excerpt_prefers_the_decision_then_the_context() {
        let mut i = item(1, "x", None, DecisionStatus::Proposed);
        i.decision.decision = String::new();
        i.decision.context = "Why we are here\nmore".into();
        let rows = arrange(vec![i], &DecisionFilter::default());
        assert_eq!(rows[0].excerpt, "Why we are here");
    }
}
