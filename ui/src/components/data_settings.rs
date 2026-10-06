//! Settings -> Data: where the database lives (read-only) and a button to show the folder.
//! The folder can't be moved from here (spec 23): Backup and Google Drive carry data elsewhere.

use leptos::{prelude::*, task::spawn_local};
use minimap_types::DataInfo;

use crate::{
    api,
    components::{
        backup_settings::size_text,
        form::BUTTON,
        page::{Card, Tone},
    },
    state::{DataVersion, Toasts},
};

/// One line of the card: what it is, then the value in a selectable monospace box.
pub fn rows(info: &DataInfo) -> Vec<(&'static str, String)> {
    vec![
        ("Database", info.database_path.clone()),
        ("Folder", info.folder.clone()),
        ("Log", info.log_path.clone()),
        ("Size", size_text(info.database_bytes)),
        ("Schema version", info.schema_version.to_string()),
    ]
}

#[component]
pub fn DataLocation() -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let version = expect_context::<DataVersion>();
    let info = LocalResource::new(move || {
        version.track();
        api::get_data_info()
    });
    let show = move |_| {
        spawn_local(async move {
            if let Err(e) = api::show_data_folder().await {
                toasts.error(&e);
            }
        });
    };
    view! {
        <Card title="Data location"
              description="Everything Minimap knows is in one database file, on this computer.">
            {move || match info.get() {
                None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
                Some(Err(_)) => view! { <p class="text-muted">"Couldn't read where the data is."</p> }.into_any(),
                Some(Ok(i)) => {
                    let lines = rows(&i)
                        .into_iter()
                        .map(|(label, value)| view! {
                            <span class="text-[11px] text-muted">{label}</span>
                            <code class="select-text break-all font-mono text-[12px]">{value}</code>
                        })
                        .collect_view();
                    view! {
                        <div class="grid grid-cols-[8rem_1fr] items-baseline gap-x-3 gap-y-1">{lines}</div>
                        <div class="flex items-center gap-2 pt-1">
                            <button class=BUTTON on:click=show>"Show in folder"</button>
                            {if i.encrypted {
                                view! { <span class=Tone::Success.chip()>"encrypted"</span> }.into_any()
                            } else {
                                view! { <span class=Tone::Neutral.chip() title="Turn encryption on in Security">"not encrypted"</span> }.into_any()
                            }}
                        </div>
                    }.into_any()
                }
            }}
            <p class="text-[11px] text-muted">
                "The location is fixed so backups, the attachment cache and encryption keep working. "
                "To use your data somewhere else, make a backup and restore it there, or connect Google Drive. "
                "The log never contains your notes or other content."
            </p>
        </Card>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_lists_the_file_the_folder_and_the_size() {
        let info = DataInfo {
            database_path: "/data/minimap.db".into(),
            folder: "/data".into(),
            database_bytes: 2048,
            schema_version: 8,
            encrypted: false,
            log_path: "/data/minimap.log".into(),
        };
        let r = rows(&info);
        assert_eq!(r[0], ("Database", "/data/minimap.db".to_owned()));
        assert_eq!(r[1].1, "/data");
        assert!(r.contains(&("Size", "2.0 KB".to_owned())));
        assert!(r.contains(&("Schema version", "8".to_owned())));
    }
}
