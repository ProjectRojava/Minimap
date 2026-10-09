//! Settings -> General -> Task types (spec 32): add, rename, recolour and archive the kinds of
//! work a task can be. A type is never deleted (tasks keep pointing at it); archiving stops
//! offering it.

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{TaskType, UpdateSettings, MAX_TASK_TYPES, MAX_TASK_TYPE_NAME, TASK_TYPE_HUES};

use crate::{
    api,
    components::{
        form::{BUTTON, BUTTON_PRIMARY, INPUT},
        page::Card,
    },
    state::{DataVersion, Toasts},
};

/// The colour a new type starts with: the first of the palette no live type wears, else the
/// palette in turn.
pub fn next_hue(list: &[TaskType]) -> u16 {
    TASK_TYPE_HUES
        .iter()
        .copied()
        .find(|h| !list.iter().any(|t| !t.archived && t.hue == *h))
        .unwrap_or(TASK_TYPE_HUES[list.len() % TASK_TYPE_HUES.len()])
}

/// The list with the type `id` changed by `change`.
pub fn changed(list: &[TaskType], id: &str, change: impl FnOnce(&mut TaskType)) -> Vec<TaskType> {
    let mut next = list.to_vec();
    if let Some(t) = next.iter_mut().find(|t| t.id == id) {
        change(t);
    }
    next
}

#[component]
pub fn TaskTypeSettings() -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let version = expect_context::<DataVersion>();
    let list = RwSignal::new(Vec::<TaskType>::new());
    let loaded = RwSignal::new(false);
    spawn_local(async move {
        match api::get_settings().await {
            Ok(s) => {
                list.set(s.task_types);
                loaded.set(true);
            }
            Err(e) => toasts.error(&e),
        }
    });

    // Saves the whole list; the answer (with ids for new entries) replaces what is shown, and a
    // refusal puts the stored list back.
    let save = move |next: Vec<TaskType>| {
        spawn_local(async move {
            let patch = UpdateSettings {
                task_types: Some(next),
                ..Default::default()
            };
            match api::update_settings(patch).await {
                Ok(saved) => {
                    list.set(saved.task_types);
                    version.bump();
                }
                Err(e) => {
                    toasts.error(&e);
                    list.set(list.get_untracked());
                }
            }
        });
    };

    let new_name = RwSignal::new(String::new());
    let add = move || {
        let name = new_name.get_untracked();
        if name.trim().is_empty() {
            return;
        }
        let mut next = list.get_untracked();
        let hue = next_hue(&next);
        next.push(TaskType {
            id: String::new(),
            name,
            hue,
            archived: false,
        });
        new_name.set(String::new());
        save(next);
    };

    let cannot_add = move || new_name.get().trim().is_empty() || list.get().len() >= MAX_TASK_TYPES;
    let rows = move || {
        list.get()
            .into_iter()
            .map(|t| view! { <TypeRow id=t.id list=list save=Callback::new(save) /> })
            .collect_view()
    };

    view! {
        <Card title="Task types" description="The kinds of work a task can be. A task has one type, or none.">
            {move || (!loaded.get()).then(|| view! { <p class="text-muted">"Loading…"</p> })}
            <div class="space-y-1.5">{rows}</div>
            <div class="flex items-end gap-2 pt-1">
                <label class="block max-w-xs flex-1">
                    <span class="mb-0.5 block text-[11px] text-muted">"New type"</span>
                    <input class=INPUT type="text" autocomplete="off" maxlength=MAX_TASK_TYPE_NAME
                        placeholder="Legal review"
                        prop:value=move || new_name.get()
                        on:input=move |ev| new_name.set(event_target_value(&ev))
                        on:keydown=move |ev| if ev.key() == "Enter" { ev.prevent_default(); add(); } />
                </label>
                <button class=BUTTON_PRIMARY
                    disabled=cannot_add
                    on:click=move |_| add()>"Add type"</button>
            </div>
            <p class="text-[11px] text-muted">
                "Rename a type and every task that has it follows. A type can't be deleted, only archived: "
                "tasks that have it keep it, and it is no longer offered for new ones."
            </p>
        </Card>
    }
}

/// One type: its colour, its name and Archive / Restore.
#[component]
fn TypeRow(
    id: String,
    list: RwSignal<Vec<TaskType>>,
    #[prop(into)] save: Callback<Vec<TaskType>>,
) -> impl IntoView {
    let this = {
        let id = id.clone();
        move || list.with(|l| l.iter().find(|t| t.id == id).cloned())
    };
    let rename = {
        let (id, this) = (id.clone(), this.clone());
        move |name: String| {
            let Some(now) = this() else { return };
            if name.trim() == now.name {
                // Put the box back to the stored text (it may hold stray spaces).
                list.set(list.get_untracked());
                return;
            }
            save.run(changed(&list.get_untracked(), &id, |t| t.name = name));
        }
    };
    let swatches = TASK_TYPE_HUES
        .iter()
        .map(|hue| {
            let hue = *hue;
            let (id, this) = (id.clone(), this.clone());
            let pressed = {
                let this = this.clone();
                move || this().is_some_and(|t| t.hue == hue)
            };
            view! {
                <button type="button" title="Colour"
                    class={
                        let pressed = pressed.clone();
                        move || format!(
                            "obj-chip !px-1 {}",
                            if pressed() { "ring-1 ring-fg" } else { "opacity-70 hover:opacity-100" })
                    }
                    style=format!("--obj-h: {hue}")
                    aria-pressed=move || pressed().to_string()
                    on:click=move |_| save.run(changed(&list.get_untracked(), &id, |t| t.hue = hue))>
                    <span class="obj-dot" />
                </button>
            }
        })
        .collect_view();
    let archive = {
        let (id, this) = (id.clone(), this.clone());
        move |_| {
            let archived = this().is_some_and(|t| t.archived);
            save.run(changed(&list.get_untracked(), &id, |t| {
                t.archived = !archived
            }));
        }
    };
    let archived = {
        let this = this.clone();
        move || this().is_some_and(|t| t.archived)
    };
    let archived_class = archived.clone();
    let is_meeting = id == minimap_types::MEETING_TYPE;
    view! {
        <div class=move || format!("flex flex-wrap items-center gap-2 {}", if archived_class() { "opacity-60" } else { "" })>
            <input class=format!("{INPUT} max-w-[14rem]") type="text" autocomplete="off"
                maxlength=MAX_TASK_TYPE_NAME aria-label="Type name"
                prop:value=move || this().map(|t| t.name).unwrap_or_default()
                on:change=move |ev| rename(event_target_value(&ev)) />
            <span class="flex items-center gap-1">{swatches}</span>
            {if is_meeting {
                view! {
                    <span class="text-[11px] text-muted"
                          title="Meetings have a day and a time, and start and end on their own">
                        "Built in: rename or recolour it, it can't be archived"
                    </span>
                }.into_any()
            } else {
                view! {
                    <button class=BUTTON on:click=archive>
                        {move || if archived() { "Restore" } else { "Archive" }}
                    </button>
                }.into_any()
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::default_task_types;

    #[test]
    fn a_new_type_gets_a_colour_nobody_live_has() {
        // Without the built-in meeting type the defaults use seven of the eight colours; the
        // eighth is the one left.
        let mut list = default_task_types();
        list.retain(|t| t.id != minimap_types::MEETING_TYPE);
        let hue = next_hue(&list);
        assert!(TASK_TYPE_HUES.contains(&hue));
        assert!(!list.iter().any(|t| t.hue == hue));
        // An archived type frees its colour.
        list[0].archived = true;
        list.push(TaskType {
            id: "x".into(),
            name: "X".into(),
            hue,
            archived: false,
        });
        assert_eq!(next_hue(&list), list[0].hue);
        // Every colour taken: it starts over from the palette.
        let all: Vec<TaskType> = TASK_TYPE_HUES
            .iter()
            .enumerate()
            .map(|(i, h)| TaskType {
                id: i.to_string(),
                name: i.to_string(),
                hue: *h,
                archived: false,
            })
            .collect();
        assert!(TASK_TYPE_HUES.contains(&next_hue(&all)));
    }

    #[test]
    fn changing_one_type_leaves_the_rest_and_the_order() {
        let list = default_task_types();
        let next = changed(&list, "build", |t| t.name = "Implement".into());
        assert_eq!(next.len(), list.len());
        assert_eq!(next[1].name, "Implement");
        assert_eq!(next[1].id, "build");
        assert_eq!(next[0], list[0]);
        assert_eq!(changed(&list, "nope", |t| t.archived = true), list);
    }
}
