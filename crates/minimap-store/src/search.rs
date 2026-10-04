//! Full-text search over the index kept by the triggers in migration 0007. Query planning
//! (tokens, typo tolerance) is `minimap-core::search`; this module only runs queries.

use minimap_types::{NodeRef, NodeType, SearchHit};
use rusqlite::{params_from_iter, types::Value, Connection};

use crate::{convert::*, error::Result, nodes};

/// Every indexed term (lowercased, accents folded), for typo-tolerant matching.
pub fn vocabulary(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT term FROM search_vocab")?;
    let rows = stmt.query_map([], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Runs an FTS5 `MATCH` expression (built by core), best match first. A snippet of the
/// body is attached; `types` empty means every type.
pub fn run(
    conn: &Connection,
    expression: &str,
    types: &[NodeType],
    include_archived: bool,
    limit: usize,
) -> Result<Vec<SearchHit>> {
    let mut sql = String::from(
        "SELECT d.node_type, d.node_id, snippet(search_index, 1, '', '', '…', 14)
         FROM search_index JOIN search_docs d ON d.rowid = search_index.rowid
         WHERE search_index MATCH ?1",
    );
    let mut args: Vec<Value> = vec![expression.to_owned().into()];
    if !include_archived {
        sql.push_str(" AND d.archived = 0");
    }
    if !types.is_empty() {
        let marks = vec!["?"; types.len()].join(",");
        sql.push_str(&format!(" AND d.node_type IN ({marks})"));
        args.extend(types.iter().map(|t| Value::from(t.as_str().to_owned())));
    }
    sql.push_str(" ORDER BY bm25(search_index, 8.0, 1.0), d.rowid LIMIT ?");
    args.push(Value::from(i64::try_from(limit).unwrap_or(i64::MAX)));

    let mut stmt = conn.prepare(&sql)?;
    let found = stmt
        .query_map(params_from_iter(args), |r| {
            let node_type: NodeType = col_enum(r, 0)?;
            let id = col_uuid(r, 1)?;
            let snippet: String = r.get(2)?;
            Ok((NodeRef::new(node_type, id), snippet))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    found
        .into_iter()
        .map(|(node, snippet)| {
            let summary = nodes::summary(conn, node)?;
            Ok(SearchHit {
                node,
                label: summary.label,
                archived: summary.archived,
                snippet: tidy(&snippet),
            })
        })
        .collect()
}

/// One line: newlines and runs of spaces collapsed.
fn tidy(snippet: &str) -> String {
    snippet.split_whitespace().collect::<Vec<_>>().join(" ")
}
