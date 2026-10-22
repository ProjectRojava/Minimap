//! The order of work (spec 60, ADR-0024): writing a person's steps.

use std::collections::HashMap;

use minimap_types::{Patch, UpdateTask, Uuid};
use rusqlite::Connection;

use crate::{edges, people, tasks, Result, StoreError};

/// Makes `steps` (top to bottom; tasks in one step side by side) `person`'s order: their tasks
/// are renumbered 1..n, and their other open tasks leave the order. One transaction, so one undo
/// step. Refused, with the reason in words, when a task can't be in the order or the order goes
/// against a `blocks` link.
pub fn set(conn: &mut Connection, person: Uuid, steps: Vec<Vec<Uuid>>) -> Result<()> {
    let tx = conn.transaction()?;
    let who = people::get(&tx, person)?;
    let steps = minimap_core::order::normalise(steps);
    let all = tasks::list(&tx, false)?;
    let links = edges::list_active(&tx)?;
    minimap_core::order::check(&all, &links, person, &who.name, &steps)
        .map_err(StoreError::Invalid)?;
    let want: HashMap<Uuid, u32> = steps
        .iter()
        .enumerate()
        .flat_map(|(i, ids)| ids.iter().map(move |id| (*id, i as u32 + 1)))
        .collect();
    for t in minimap_core::order::tasks_of(&all, &links, person) {
        let step = want.get(&t.id).copied();
        if t.order_step != step {
            tasks::update_in_tx(
                &tx,
                t.id,
                UpdateTask {
                    order_step: step.map_or(Patch::Clear, Patch::Set),
                    ..Default::default()
                },
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}
