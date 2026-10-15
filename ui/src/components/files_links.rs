//! A task's *Files & links*: the web and Drive addresses (spec 28) and the attached files (spec 22)
//! in one section, since both are material from outside the task. It stays hidden while there is
//! none of either; the *Add* row (or the section's own *Attach file…*) starts one.

use leptos::prelude::*;
use minimap_types::{NodeRef, NodeType, Uuid};

use crate::components::{
    attachments::{pick_and_attach, use_attachments, AttachmentsBody},
    detail_pane::{Section, SECTION_ACTION},
    pane_reveal::shown,
    reference_links::{use_task, ReferenceLinksBody},
};

/// "· 3" (links and files together), nothing when there are none.
pub fn count_meta(links: usize, files: usize) -> String {
    match links + files {
        0 => String::new(),
        n => format!("· {n}"),
    }
}

#[component]
pub fn FilesAndLinks(task: Uuid) -> impl IntoView {
    let node = NodeRef::new(NodeType::Task, task);
    let loaded = use_task(task);
    let files = use_attachments(node);
    let counts = move || {
        let links = loaded
            .get()
            .and_then(|t| t.ok())
            .map_or(0, |t| t.links.len());
        let files = files.get().and_then(|f| f.ok()).map_or(0, |f| f.len());
        (links, files)
    };
    view! {
        <div class:hidden=move || !shown("files", { let (l, f) = counts(); l + f > 0 })>
            <Section title="Files & links"
                meta=move || { let (l, f) = counts(); count_meta(l, f) }
                actions=move || view! {
                    <button class=SECTION_ACTION on:click=move |_| pick_and_attach(node)>"Attach file…"</button>
                }>
                <p class="mb-1 text-[11px] text-muted">"Links"</p>
                <ReferenceLinksBody task=task loaded=loaded />
                <p class="mb-1 mt-3 text-[11px] text-muted">"Files"</p>
                <AttachmentsBody node=node list=files />
            </Section>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_count_is_links_and_files_together() {
        assert_eq!(count_meta(0, 0), "");
        assert_eq!(count_meta(2, 1), "· 3");
        assert_eq!(count_meta(0, 4), "· 4");
    }
}
