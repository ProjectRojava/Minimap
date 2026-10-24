//! The order of work (spec 60, ADR-0024): writing a person's steps.

use std::collections::HashMap;

use minimap_types::{Date, Patch, UpdateTask, Uuid};
use rusqlite::{Connection, Transaction};

use crate::{edges, people, projects, settings, tasks, Result, StoreError};

/// Makes `steps` (top to bottom; tasks in one step side by side) `person`'s order: their tasks
/// are renumbered 1..n, and their other open tasks leave the order. One transaction, so one undo
/// step. Refused, with the reason in words, when a task can't be in the order or the order goes
/// against a `blocks` link.
pub fn set(conn: &mut Connection, person: Uuid, steps: Vec<Vec<Uuid>>) -> Result<()> {
    let tx = conn.transaction()?;
    set_in_tx(&tx, person, steps)?;
    tx.commit()?;
    Ok(())
}

/// Gives `task` to `person` (their only assignee) and makes `steps` their order, which may hold
/// the task or not (spec 61: dragged to someone's column on *My team*). One transaction: a
/// refused order leaves the task with the person it had.
pub fn give(conn: &mut Connection, task: Uuid, person: Uuid, steps: Vec<Vec<Uuid>>) -> Result<()> {
    let tx = conn.transaction()?;
    people::get(&tx, person)?;
    tasks::set_assignee_in_tx(&tx, task, Some(person))?;
    set_in_tx(&tx, person, steps)?;
    tx.commit()?;
    Ok(())
}

fn set_in_tx(tx: &Transaction, person: Uuid, steps: Vec<Vec<Uuid>>) -> Result<()> {
    let who = people::get(tx, person)?;
    let steps = minimap_core::order::normalise(steps);
    let all = tasks::list(tx, false)?;
    let links = edges::list_active(tx)?;
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
                tx,
                t.id,
                UpdateTask {
                    order_step: step.map_or(Patch::Clear, Patch::Set),
                    ..Default::default()
                },
            )?;
        }
    }
    Ok(())
}

/// Writes the dates the order gives (`minimap_core::order::dates`): each task that comes after
/// another in someone's order starts the working day after the one above finishes (a slip moves
/// the dates below), and an ordered task with an estimate and no due date gets its last working
/// day as the due date. Returns how many tasks it changed. One transaction; the caller's undo
/// step takes in what it wrote. A schedule that can't be made (a loop in `blocks`) writes
/// nothing.
pub fn sync_dates(conn: &mut Connection, today: Date) -> Result<usize> {
    let tx = conn.transaction()?;
    let all = tasks::list_planned(&tx)?;
    let links = edges::list_active(&tx)?;
    let projects = projects::list(&tx, false)?;
    let week = settings::get(&tx)?.work_week;
    let Ok(dates) = minimap_core::order::dates(&all, &links, &projects, today, week) else {
        return Ok(0);
    };
    for d in &dates {
        tasks::update_in_tx(
            &tx,
            d.id,
            UpdateTask {
                start_date: d.start_date.map_or(Patch::Keep, Patch::Set),
                due_date: d.due_date.map_or(Patch::Keep, Patch::Set),
                ..Default::default()
            },
        )?;
    }
    tx.commit()?;
    Ok(dates.len())
}
