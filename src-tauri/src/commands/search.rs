use minimap_core::search::{fuzzy_expression, strict_expression, tokens};
use minimap_store::Connection;
use minimap_types::{AppError, SearchFilter, SearchHit};
use tauri::State;

use crate::{error::store_error, state::AppState};

const DEFAULT_LIMIT: u32 = 20;
const MAX_LIMIT: u32 = 100;

/// Finds nodes by text: every word is a prefix, best match first. When nothing matches, words
/// are retried with typo tolerance. Archived nodes only when asked.
#[tauri::command]
pub async fn search(
    state: State<'_, AppState>,
    query: String,
    filter_by: SearchFilter,
) -> Result<Vec<SearchHit>, AppError> {
    state
        .run(move |conn| search_impl(conn, &query, &filter_by))
        .await
}

pub(crate) fn search_impl(
    conn: &Connection,
    query: &str,
    filter: &SearchFilter,
) -> Result<Vec<SearchHit>, AppError> {
    let words = tokens(query);
    let Some(strict) = strict_expression(&words) else {
        return Ok(Vec::new());
    };
    let limit = filter.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT) as usize;
    let run = |expression: &str| {
        minimap_store::search::run(
            conn,
            expression,
            &filter.types,
            filter.include_archived,
            limit,
        )
        .map_err(store_error)
    };
    let hits = run(&strict)?;
    if !hits.is_empty() {
        return Ok(hits);
    }
    // Nothing matched as typed: look for indexed words a typo away. This reads the vocabulary,
    // so it only happens on a miss.
    let vocabulary = minimap_store::search::vocabulary(conn).map_err(store_error)?;
    match fuzzy_expression(&words, &vocabulary) {
        Some(expression) => run(&expression),
        None => Ok(hits),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{
        CreateNote, CreateObjective, CreatePerson, CreateProject, CreateTask, NodeType, UpdateNote,
        Uuid,
    };

    fn labels(hits: &[SearchHit]) -> Vec<&str> {
        hits.iter().map(|h| h.label.as_str()).collect()
    }

    fn find(conn: &Connection, q: &str) -> Vec<SearchHit> {
        search_impl(conn, q, &SearchFilter::default()).unwrap()
    }

    fn task(conn: &mut Connection, title: &str, description: &str) -> Uuid {
        minimap_store::tasks::create(
            conn,
            CreateTask {
                links: Vec::new(),
                title: title.into(),
                description: description.into(),
                project_id: None,
                status: None,
                estimate_days: None,
                start_date: None,
                due_date: None,
                priority: None,
                assignee: minimap_types::AssigneeChoice::Nobody,
                recurrence: None,
            },
        )
        .unwrap()
        .id
    }

    #[test]
    fn every_word_is_a_prefix() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        task(&mut conn, "Fix login timeout", "");
        task(&mut conn, "Fix invoice layout", "");
        assert_eq!(labels(&find(&conn, "fix log")), ["Fix login timeout"]);
        assert_eq!(find(&conn, "fix").len(), 2);
        assert!(find(&conn, "").is_empty());
        assert!(find(&conn, " ?! ").is_empty());
    }

    #[test]
    fn typos_are_found_only_when_nothing_matches_as_typed() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        task(&mut conn, "Fix login timeout", "");
        task(&mut conn, "Budget review", "");
        assert_eq!(labels(&find(&conn, "loign")), ["Fix login timeout"]);
        assert_eq!(
            labels(&find(&conn, "fix loign timeot")),
            ["Fix login timeout"]
        );
        assert_eq!(labels(&find(&conn, "budgte")), ["Budget review"]);
        // Too short to guess, and nothing close: no results rather than wrong ones.
        assert!(find(&conn, "fox").is_empty());
        assert!(find(&conn, "zzzzzz").is_empty());
    }

    #[test]
    fn titles_rank_above_body_text() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        task(
            &mut conn,
            "Quarterly planning",
            "mention of budget somewhere",
        );
        task(&mut conn, "Budget", "");
        let hits = find(&conn, "budget");
        assert_eq!(labels(&hits), ["Budget", "Quarterly planning"]);
        // The snippet is a piece of the body.
        assert!(hits[1].snippet.contains("budget"));
    }

    #[test]
    fn note_bodies_are_searchable_and_mentions_match_by_name_not_id() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let priya = minimap_store::people::create(
            &mut conn,
            CreatePerson {
                name: "Priya".into(),
                role_title: "Staff engineer".into(),
                email: None,
                weekly_capacity_hours: None,
                is_self: false,
                notes: String::new(),
            },
        )
        .unwrap();
        let note = minimap_store::notes::create(
            &mut conn,
            CreateNote {
                title: "Weekly sync".into(),
                body: format!(
                    "Agreed to migrate the warehouse. {}",
                    minimap_types::mention_token("Priya", priya.id)
                ),
                note_date: None,
                kind: None,
                recurrence: None,
            },
        )
        .unwrap();

        let hits = find(&conn, "warehouse");
        assert_eq!(labels(&hits), ["Weekly sync"]);
        assert_eq!(hits[0].node.node_type, NodeType::Note);
        assert!(hits[0].snippet.contains("warehouse"));
        // The mention token's id and "node" scaffolding never reach the index.
        let id_part = priya.id.to_string()[..8].to_owned();
        assert!(find(&conn, &id_part).is_empty());
        assert!(find(&conn, "node").is_empty());
        // The person's name finds both the person and the note that mentions them.
        let names = find(&conn, "priya");
        assert_eq!(names.len(), 2);

        // Edits are reindexed.
        minimap_store::notes::update(
            &mut conn,
            note.id,
            UpdateNote {
                body: Some("Now about the datacenter".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(find(&conn, "warehouse").is_empty());
        // A typo in one word still finds it.
        assert_eq!(labels(&find(&conn, "datacentre")), ["Weekly sync"]);
        assert_eq!(labels(&find(&conn, "datacenter")), ["Weekly sync"]);
    }

    #[test]
    fn archived_nodes_are_hidden_unless_asked_and_deleted_ones_are_gone() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let t = task(&mut conn, "Old rollout", "");
        let node = minimap_types::NodeRef::new(NodeType::Task, t);
        minimap_store::nodes::archive(&mut conn, node).unwrap();
        assert!(find(&conn, "rollout").is_empty());
        let with_archived = SearchFilter {
            include_archived: true,
            ..Default::default()
        };
        let hits = search_impl(&conn, "rollout", &with_archived).unwrap();
        assert_eq!(labels(&hits), ["Old rollout"]);
        assert!(hits[0].archived);

        minimap_store::nodes::unarchive(&mut conn, node).unwrap();
        assert_eq!(find(&conn, "rollout").len(), 1);

        minimap_store::nodes::archive(&mut conn, node).unwrap();
        minimap_store::nodes::delete(&mut conn, node).unwrap();
        assert!(search_impl(&conn, "rollout", &with_archived)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn the_type_filter_and_limit_apply() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        for i in 0..5 {
            task(&mut conn, &format!("Launch task {i}"), "");
        }
        minimap_store::objectives::create(
            &mut conn,
            CreateObjective {
                title: "Launch EU".into(),
                description: String::new(),
                target_date: None,
                status: None,
                priority: None,
            },
        )
        .unwrap();
        minimap_store::projects::create(
            &mut conn,
            CreateProject {
                title: "Launch API".into(),
                slug: None,
                description: String::new(),
                owner_person_id: None,
                start_date: None,
                target_date: None,
                status: None,
                priority: None,
            },
        )
        .unwrap();
        assert_eq!(find(&conn, "launch").len(), 7);
        let only = |types: Vec<NodeType>, limit| SearchFilter {
            types,
            limit,
            ..Default::default()
        };
        let hits = search_impl(&conn, "launch", &only(vec![NodeType::Objective], None)).unwrap();
        assert_eq!(labels(&hits), ["Launch EU"]);
        let hits = search_impl(
            &conn,
            "launch",
            &only(vec![NodeType::Objective, NodeType::Project], None),
        )
        .unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(
            search_impl(&conn, "launch", &only(vec![], Some(3)))
                .unwrap()
                .len(),
            3
        );
        // The project's handle is searchable too.
        assert!(find(&conn, "launch-api")
            .iter()
            .any(|h| h.node.node_type == NodeType::Project));
    }

    #[test]
    fn accents_and_case_are_ignored() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        task(&mut conn, "Café MENU refresh", "");
        assert_eq!(find(&conn, "cafe menu").len(), 1);
        assert_eq!(find(&conn, "CAFÉ").len(), 1);
    }

    /// Timing on a deliberately large database (spec 11: under 100 ms). Slow to build, so it
    /// only runs on request: `cargo test -p minimap --release search_speed -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn search_speed() {
        use std::time::Instant;
        let mut conn = minimap_store::open_in_memory().unwrap();
        // ~20k distinct terms: syllable combinations.
        let syl = [
            "ka", "lo", "mi", "ne", "ru", "ta", "vo", "xi", "pe", "su", "do", "fa", "ge", "hi",
            "jo", "bu",
        ];
        let word = |n: usize| {
            format!(
                "{}{}{}{}",
                syl[n % 16],
                syl[(n / 16) % 16],
                syl[(n / 256) % 16],
                syl[(n / 4096) % 16]
            )
        };
        for i in 0..2500usize {
            task(
                &mut conn,
                &format!("{} {} {}", word(i), word(i * 7 + 1), word(i * 13 + 5)),
                &format!("{} {}", word(i * 3), word(i * 11 + 2)),
            );
        }
        for i in 0..400usize {
            let body: Vec<String> = (0..400).map(|k| word(i * 31 + k * 17)).collect();
            minimap_store::notes::create(
                &mut conn,
                CreateNote {
                    title: format!("Note {}", word(i)),
                    body: body.join(" "),
                    note_date: None,
                    kind: None,
                    recurrence: None,
                },
            )
            .unwrap();
        }
        let terms = minimap_store::search::vocabulary(&conn).unwrap().len();
        let time = |label: &str, q: &str| {
            let t = Instant::now();
            let hits = find(&conn, q);
            println!(
                "{label:<24} {:>8.2?}  ({} hits, {terms} terms)",
                t.elapsed(),
                hits.len()
            );
        };
        time("prefix", "kalo");
        time("two words", "kalomi netaru");
        time("miss -> typo retry", "kaloxx");
        time("typo that corrects", "kalmi");
    }
}
