//! Settings -> Data & backup -> Export (spec 26): everything in open formats, so nobody is
//! locked in. JSON for all of it, and optionally Markdown to read it.

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{ExportAllResult, ExportFormat};

use crate::{
    api,
    components::{backup_settings::size_text, form::BUTTON_PRIMARY, page::Card},
    state::Toasts,
};

/// "11 files, 212.0 KB".
pub fn files_text(r: &ExportAllResult) -> String {
    format!(
        "{} file{}, {}",
        r.files,
        if r.files == 1 { "" } else { "s" },
        size_text(r.bytes)
    )
}

/// What the export holds: "2 objectives, 3 projects, 40 tasks, ..." (the kinds of item, then
/// links and history).
pub fn contents_text(r: &ExportAllResult) -> String {
    const KINDS: [(&str, &str, &str); 11] = [
        ("objectives", "objective", "objectives"),
        ("projects", "project", "projects"),
        ("tasks", "task", "tasks"),
        ("people", "person", "people"),
        ("teams", "team", "teams"),
        ("notes", "note", "notes"),
        ("decisions", "decision", "decisions"),
        ("waiting_on", "waiting-on", "waiting-ons"),
        ("edges", "link", "links"),
        ("attachments", "attachment", "attachments"),
        ("activity", "history entry", "history entries"),
    ];
    KINDS
        .iter()
        .filter_map(|(key, one, many)| {
            let n = *r.counts.get(*key)?;
            (n > 0).then(|| format!("{n} {}", if n == 1 { one } else { many }))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// A line about attached files, when there is something to say.
pub fn attachments_text(r: &ExportAllResult) -> Option<String> {
    let plural = |n: u32| if n == 1 { "file" } else { "files" };
    match (r.attachment_files, r.attachment_files_missing) {
        (0, 0) => None,
        (n, 0) => Some(format!("{n} attached {} included.", plural(n))),
        (0, m) => Some(format!(
            "{m} attached {} only on Google Drive, not on this computer, so not included (they are listed in attachments.json).",
            if m == 1 { "file is" } else { "files are" }
        )),
        (n, m) => Some(format!(
            "{n} attached {} included; {m} only on Google Drive, not on this computer, so not included (they are listed in attachments.json).",
            plural(n)
        )),
    }
}

#[component]
pub fn ExportSettings() -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let markdown = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let result = RwSignal::new(None::<ExportAllResult>);

    let export = move |_| {
        spawn_local(async move {
            let folder = match api::pick_folder("Choose where to put the export").await {
                Ok(Some(folder)) => folder,
                Ok(None) => return,
                Err(e) => return toasts.error(&e),
            };
            let format = if markdown.get_untracked() {
                ExportFormat::JsonAndMarkdown
            } else {
                ExportFormat::Json
            };
            busy.set(true);
            result.set(None);
            match api::export_all(folder, format).await {
                Ok(done) => {
                    toasts.info(format!("Exported to {}", done.folder));
                    result.set(Some(done));
                }
                Err(e) => toasts.error(&e),
            }
            busy.set(false);
        });
    };

    view! {
        <Card title="Export your data"
              description="Everything in open formats, so you are never locked in.">
            <p class="text-[12px] text-muted">
                "Writes a new folder with one JSON file per kind of item (tasks, projects, people, notes, ...), "
                "every link, the full history, archived items, and the files attached to items that are on "
                "this computer. Nothing in the folder you pick is changed or overwritten."
            </p>
            <label class="flex items-center gap-2">
                <input type="checkbox" prop:checked=move || markdown.get()
                    on:change=move |ev| markdown.set(event_target_checked(&ev)) />
                "Also write Markdown: one file per project with its tasks, the inbox, and each note as a .md file"
            </label>
            <div class="flex items-center gap-2">
                <button class=BUTTON_PRIMARY disabled=move || busy.get() on:click=export>
                    {move || if busy.get() { "Exporting…" } else { "Export…" }}
                </button>
            </div>
            {move || result.get().map(|r| view! {
                <div class="space-y-1 rounded-sm border border-line bg-canvas p-3 text-[12px]">
                    <p>
                        "Exported to "
                        <code class="select-text break-all font-mono">{r.folder.clone()}</code>
                    </p>
                    <p class="text-muted">{format!("{} · {}", files_text(&r), contents_text(&r))}</p>
                    {attachments_text(&r).map(|t| view! { <p class="text-muted">{t}</p> })}
                </div>
            })}
            <p class="text-[11px] text-muted">
                "The export is plain text: it is not encrypted, even if your Minimap data is, so keep it somewhere you trust. "
                "Importing it back isn't built yet; the files are ordinary JSON and Markdown that other tools can read."
            </p>
        </Card>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn result(counts: &[(&str, u32)], present: u32, missing: u32) -> ExportAllResult {
        ExportAllResult {
            folder: "/x/minimap-export-1".into(),
            files: 14,
            bytes: 2048,
            counts: counts
                .iter()
                .map(|(k, v)| ((*k).to_owned(), *v))
                .collect::<BTreeMap<_, _>>(),
            markdown_files: 0,
            attachment_files: present,
            attachment_files_missing: missing,
        }
    }

    #[test]
    fn the_summary_names_files_size_and_contents() {
        let r = result(
            &[
                ("tasks", 40),
                ("people", 1),
                ("projects", 3),
                ("teams", 0),
                ("edges", 99),
                ("waiting_on", 3),
            ],
            0,
            0,
        );
        assert_eq!(files_text(&r), "14 files, 2.0 KB");
        assert_eq!(
            contents_text(&r),
            "3 projects, 40 tasks, 1 person, 3 waiting-ons, 99 links"
        );
        assert_eq!(contents_text(&result(&[], 0, 0)), "");
    }

    #[test]
    fn attached_files_are_accounted_for_in_words() {
        assert_eq!(attachments_text(&result(&[], 0, 0)), None);
        assert_eq!(
            attachments_text(&result(&[], 1, 0)).as_deref(),
            Some("1 attached file included.")
        );
        let some = attachments_text(&result(&[], 2, 3)).unwrap();
        assert!(
            some.starts_with("2 attached files included; 3 only on Google Drive"),
            "{some}"
        );
        let none = attachments_text(&result(&[], 0, 1)).unwrap();
        assert!(
            none.starts_with("1 attached file is only on Google Drive"),
            "{none}"
        );
    }
}
