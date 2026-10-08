//! Attachments on an item (spec 22): the list in the detail pane with thumbnails, "Attach
//! file…", drag and drop, open, copy a link for a note, and remove. Files can also be pasted or
//! dropped into a note's text (see `note_panel.rs`), which uses [`attach_files`].

use leptos::{prelude::*, task::spawn_local, web_sys};
use minimap_types::{Attachment, AttachmentState, NodeRef};

use crate::{
    api,
    components::{
        backup_settings::size_text,
        detail_pane::{Section, SECTION_ACTION},
        form::BUTTON,
        page::{Icon, Tone},
    },
    state::{DataVersion, Toasts},
};

/// Where a file's bytes are, as a pill: its words, colour and tooltip.
pub fn state_label(state: AttachmentState) -> (&'static str, Tone, &'static str) {
    match state {
        AttachmentState::LocalOnly => (
            "this computer only",
            Tone::Warning,
            "Google Drive isn't connected, so this file exists only on this computer.",
        ),
        AttachmentState::Waiting => (
            "uploading",
            Tone::Accent,
            "On this computer, waiting to be uploaded to Google Drive.",
        ),
        AttachmentState::OnDrive => ("on drive", Tone::Success, "Saved to Google Drive."),
        AttachmentState::NotDownloaded => (
            "on drive",
            Tone::Neutral,
            "On Google Drive; downloaded when you open it.",
        ),
    }
}

/// The files of a drop or paste, as a list.
pub fn files_of(list: Option<web_sys::FileList>) -> Vec<web_sys::File> {
    let Some(list) = list else { return Vec::new() };
    (0..list.length()).filter_map(|i| list.get(i)).collect()
}

/// Attaches each file to `node`; failures become toasts. Returns what was attached.
pub async fn attach_files(
    node: NodeRef,
    files: Vec<web_sys::File>,
    toasts: Toasts,
) -> Vec<Attachment> {
    let mut done = Vec::new();
    for file in files {
        match api::add_attachment_file(node, &file).await {
            Ok(a) => done.push(a),
            Err(e) => toasts.error(&e),
        }
    }
    done
}

#[component]
pub fn Attachments(node: NodeRef) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let list = LocalResource::new(move || {
        version.track();
        api::list_attachments(node)
    });
    let over = RwSignal::new(false);

    let pick = move |_| {
        spawn_local(async move {
            match api::pick_attachment().await {
                Ok(Some(path)) => match api::add_attachment(node, path).await {
                    Ok(_) => {}
                    Err(e) => toasts.error(&e),
                },
                Ok(None) => {}
                Err(e) => toasts.error(&e),
            }
            version.bump();
        });
    };
    let on_drop = move |ev: leptos::ev::DragEvent| {
        ev.prevent_default();
        over.set(false);
        let files = files_of(ev.data_transfer().and_then(|d| d.files()));
        if files.is_empty() {
            return;
        }
        spawn_local(async move {
            attach_files(node, files, toasts).await;
            version.bump();
        });
    };

    view! {
        <Section title="Attachments"
            meta=move || view! {
                {move || list.get().and_then(|l| l.ok()).filter(|l| !l.is_empty())
                    .map(|l| format!("· {}", l.len()))}
            }
            actions=move || view! {
                <button class=SECTION_ACTION on:click=pick>"Attach file…"</button>
            }>
            <div class=move || format!(
                     "space-y-2 rounded-sm border border-dashed p-2 {}",
                     if over.get() { "border-accent bg-accent/10" } else { "border-line" })
                 on:dragover=move |ev| { ev.prevent_default(); over.set(true); }
                 on:dragleave=move |_| over.set(false)
                 on:drop=on_drop>
                {move || match list.get() {
                    None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
                    Some(Err(e)) => view! { <p class="text-danger">{e.message}</p> }.into_any(),
                    Some(Ok(items)) if items.is_empty() => view! {
                        <p class="text-muted">"Nothing attached. Drop a file here."</p>
                    }.into_any(),
                    Some(Ok(items)) => view! {
                        <ul class="space-y-1">
                            {items.into_iter().map(|a| view! { <AttachmentRow attachment=a /> }).collect_view()}
                        </ul>
                    }.into_any(),
                }}
                <p class="text-[11px] text-muted">
                    "Images, SVG, Markdown, PDF, Word, Excel, PowerPoint"
                </p>
            </div>
        </Section>
    }
}

#[component]
fn AttachmentRow(attachment: Attachment) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let id = attachment.id;
    let (state_text, state_tone, state_hint) = state_label(attachment.state);
    let markdown = attachment.markdown.clone();
    let open = move |_| {
        spawn_local(async move {
            if let Err(e) = api::open_attachment(id).await {
                toasts.error(&e);
            }
        });
    };
    let copy = move |_| {
        let text = markdown.clone();
        spawn_local(async move {
            match api::copy_text(&text).await {
                Ok(()) => toasts.info("Copied. Paste it into a note to show the file there."),
                Err(e) => toasts.error(&e),
            }
        });
    };
    let remove = move |_| {
        spawn_local(async move {
            if let Err(e) = api::remove_attachment(id).await {
                toasts.error(&e);
            }
            version.bump();
        });
    };
    let thumb = if attachment.kind.is_picture() {
        view! {
            <img class="h-9 w-9 shrink-0 rounded-sm border border-line bg-canvas object-cover"
                 src=api::attachment_url(id) alt="" loading="lazy" />
        }
        .into_any()
    } else {
        view! {
            <span class="flex h-9 w-9 shrink-0 items-center justify-center rounded-sm border border-line bg-canvas text-muted">
                <Icon name="attach" />
            </span>
        }
        .into_any()
    };
    view! {
        <li class="flex items-center gap-2">
            {thumb}
            <div class="min-w-0 flex-1">
                <button class="block max-w-full truncate text-left hover:text-accent hover:underline"
                        title="Open" on:click=open>{attachment.file_name.clone()}</button>
                <div class="flex flex-wrap items-center gap-1.5 text-[11px] text-muted">
                    <span>{attachment.kind.label()}</span>
                    <span>"·"</span>
                    <span class="tabular-nums">{size_text(attachment.size_bytes)}</span>
                    <span class=state_tone.chip() title=state_hint>{state_text}</span>
                </div>
            </div>
            <button class=BUTTON title="Copy Markdown to show this file in a note" on:click=copy>"Copy"</button>
            <button class=BUTTON on:click=remove>"Remove"</button>
        </li>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_only_files_are_flagged_like_the_rest_of_the_app_flags_local_only_data() {
        let (text, tone, hint) = state_label(AttachmentState::LocalOnly);
        assert_eq!(tone, Tone::Warning);
        assert!(text.contains("this computer"));
        assert!(hint.contains("Google Drive isn't connected"));
    }

    #[test]
    fn files_on_drive_are_green_when_here_and_neutral_when_not_yet_downloaded() {
        assert_eq!(state_label(AttachmentState::OnDrive).1, Tone::Success);
        assert_eq!(state_label(AttachmentState::NotDownloaded).1, Tone::Neutral);
        assert_eq!(state_label(AttachmentState::Waiting).1, Tone::Accent);
        assert!(state_label(AttachmentState::NotDownloaded)
            .2
            .contains("downloaded when you open"));
    }
}
