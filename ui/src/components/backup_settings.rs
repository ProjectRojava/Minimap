//! Settings → Backup (spec 20): where backups go, the daily backup, "Back up now", the list of
//! backups and "Restore…". Restoring always shows what is in the file and asks first.

use leptos::{prelude::*, task::spawn_local, web_sys};
use minimap_types::{
    BackupCount, BackupEntry, BackupKind, BackupStatus, RestorePreview, Secret, UpdateSettings,
};

use crate::{
    api,
    components::{
        form::{BUTTON, BUTTON_DANGER, BUTTON_PRIMARY, INPUT},
        page::{Card, Tone},
    },
    state::{finish, DataVersion, Toasts},
};

// ------------------------------------------------------------------ pure text

/// "just now", "5 minutes ago", "3 hours ago", "2 days ago".
pub fn age_text(minutes: u64) -> String {
    let plural = |n: u64, unit: &str| format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" });
    match minutes {
        0 => "just now".to_owned(),
        1..=59 => plural(minutes, "minute"),
        60..=1439 => plural(minutes / 60, "hour"),
        _ => plural(minutes / 1440, "day"),
    }
}

/// "512 B", "3.4 KB", "12.0 MB".
pub fn size_text(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{:.1} MB", b / (KB * KB))
    }
}

pub fn kind_label(kind: BackupKind) -> &'static str {
    match kind {
        BackupKind::Manual => "manual",
        BackupKind::Auto => "daily",
        BackupKind::PreMigration => "before upgrade",
        BackupKind::PreRestore => "before restore",
        BackupKind::PreEncryption => "before encryption change",
    }
}

pub fn kind_tone(kind: BackupKind) -> Tone {
    match kind {
        BackupKind::Manual => Tone::Accent,
        BackupKind::Auto => Tone::Neutral,
        BackupKind::PreMigration | BackupKind::PreRestore | BackupKind::PreEncryption => {
            Tone::Warning
        }
    }
}

/// "Last backup: 2027-03-03 15:30 UTC (3 hours ago)" or a nudge when there is none.
pub fn last_text(last: Option<&BackupEntry>) -> String {
    match last {
        Some(b) => format!("Last backup: {} ({})", b.created, age_text(b.age_minutes)),
        None => "No backup yet. Use Back up now, or leave the daily backup on.".to_owned(),
    }
}

/// "41 tasks, 5 projects" from the non-empty counts, in the order given.
pub fn counts_text(counts: &[BackupCount]) -> String {
    let parts: Vec<String> = counts
        .iter()
        .filter(|c| c.count > 0)
        .map(|c| format!("{} {}", c.count, c.label.to_lowercase()))
        .collect();
    if parts.is_empty() {
        "no items".to_owned()
    } else {
        parts.join(", ")
    }
}

// ------------------------------------------------------------------ the card

#[component]
pub fn BackupSettings() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let status = LocalResource::new(move || {
        version.track();
        api::get_backup_status()
    });
    let restore = Restore::new();
    view! {
        <Card title="Backup"
              description="Copies of your data, made with SQLite's online backup so the app keeps working.">
            {move || match status.get() {
                Some(Ok(s)) => view! { <Body status=s restore=restore /> }.into_any(),
                Some(Err(e)) => view! { <p class="text-danger">{e.message}</p> }.into_any(),
                None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
            }}
            {move || restore.need_key.get().map(|p| view! { <KeyPrompt path=p restore=restore /> })}
            {move || restore.pending.get().map(|p| view! { <ConfirmRestore preview=p restore=restore /> })}
        </Card>
    }
}

fn save_settings(patch: UpdateSettings, toasts: Toasts, version: DataVersion) {
    spawn_local(async move {
        finish(api::update_settings(patch).await, toasts, version);
    });
}

#[component]
fn Body(status: BackupStatus, restore: Restore) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();

    let change_folder = move |_| {
        spawn_local(async move {
            match api::pick_folder().await {
                Ok(Some(folder)) => save_settings(
                    UpdateSettings {
                        backup_folder: Some(folder),
                        ..Default::default()
                    },
                    toasts,
                    version,
                ),
                Ok(None) => {}
                Err(e) => toasts.error(&e),
            }
        });
    };
    let default_folder = move |_| {
        save_settings(
            UpdateSettings {
                backup_folder: Some(String::new()),
                ..Default::default()
            },
            toasts,
            version,
        );
    };
    let toggle_auto = move |ev: leptos::ev::Event| {
        save_settings(
            UpdateSettings {
                auto_backup: Some(event_target_checked(&ev)),
                ..Default::default()
            },
            toasts,
            version,
        );
    };
    let back_up = move |_| {
        spawn_local(async move {
            match api::backup_now(None).await {
                Ok(b) => toasts.info(format!(
                    "Backed up as {} ({})",
                    b.file_name,
                    size_text(b.bytes)
                )),
                Err(e) => toasts.error(&e),
            }
            version.bump();
        });
    };
    let pick_file = move |_| {
        spawn_local(async move {
            match api::pick_backup_file().await {
                Ok(Some(path)) => check(path, None, restore, toasts),
                Ok(None) => {}
                Err(e) => toasts.error(&e),
            }
        });
    };

    let db_encrypted = status.database_encrypted;
    let last = last_text(status.last_backup.as_ref());
    let is_default = status.is_default_folder;
    let keep = status.keep_auto;
    let rows = status
        .backups
        .iter()
        .map(|b| view! { <BackupRow entry=b.clone() restore=restore flag_plain=db_encrypted /> })
        .collect_view();
    let none = status.backups.is_empty();
    view! {
        <div class="space-y-1">
            <div class="text-[11px] text-muted">"Backup folder"</div>
            <div class="flex flex-wrap items-center gap-2">
                <code class="min-w-0 truncate rounded-sm border border-line bg-canvas px-2 py-0.5 font-mono text-[12px]"
                      title=status.folder.clone()>{status.folder.clone()}</code>
                {is_default.then(|| view! { <span class=Tone::Neutral.chip()>"default"</span> })}
                <button class=BUTTON on:click=change_folder>"Change…"</button>
                <button class=BUTTON disabled=is_default on:click=default_folder>"Use default"</button>
            </div>
            <p class="text-[11px] text-muted">
                "Tip: a folder that Google Drive, Dropbox or OneDrive syncs on this computer works as an "
                "off-site copy. Keep the database itself out of synced folders."
            </p>
        </div>
        <label class="flex items-center gap-2">
            <input type="checkbox" prop:checked=status.auto_backup on:change=toggle_auto />
            {format!("Back up automatically (when the app starts and the last backup is over a day old, then daily; the newest {keep} are kept)")}
        </label>
        <p class="text-muted">{last}</p>
        <div class="flex flex-wrap items-center gap-2">
            <button class=BUTTON_PRIMARY on:click=back_up>"Back up now"</button>
            <button class=BUTTON on:click=pick_file>"Restore from file…"</button>
        </div>
        {(!none).then(|| view! {
            <div class="max-h-64 overflow-y-auto rounded-sm border border-line">{rows}</div>
        })}
    }
}

/// The signals of a restore in progress: the checked file, the key typed for it (when it was
/// encrypted with another key) and the file waiting for such a key.
#[derive(Clone, Copy)]
struct Restore {
    pending: RwSignal<Option<RestorePreview>>,
    secret: RwSignal<Option<String>>,
    need_key: RwSignal<Option<String>>,
}

impl Restore {
    fn new() -> Self {
        Self {
            pending: RwSignal::new(None),
            secret: RwSignal::new(None),
            need_key: RwSignal::new(None),
        }
    }

    fn clear(&self) {
        self.pending.set(None);
        self.secret.set(None);
        self.need_key.set(None);
    }
}

/// Checks a file and, when it can be restored, asks for confirmation. A backup encrypted with
/// another key asks for that key first.
fn check(path: String, secret: Option<String>, restore: Restore, toasts: Toasts) {
    spawn_local(async move {
        match api::preview_restore(path.clone(), secret.clone().map(Secret)).await {
            Ok(preview) => {
                restore.need_key.set(None);
                restore.secret.set(secret);
                restore.pending.set(Some(preview));
            }
            Err(e)
                if e.code == "backup_key_needed" || (e.code == "wrong_key" && secret.is_some()) =>
            {
                restore.pending.set(None);
                restore.need_key.set(Some(path));
                if e.code == "wrong_key" {
                    toasts.info("That passphrase or recovery key doesn't open this backup");
                }
            }
            Err(e) => toasts.error(&e),
        }
    });
}

#[component]
fn KeyPrompt(path: String, restore: Restore) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let typed = RwSignal::new(String::new());
    let go = {
        let path = path.clone();
        move || {
            let text = typed.get_untracked();
            if !text.trim().is_empty() {
                check(path.clone(), Some(text), restore, toasts);
            }
        }
    };
    let go_click = go.clone();
    view! {
        <div class="space-y-2 rounded-sm border border-line bg-canvas p-3">
            <p>"This backup is encrypted with a key Minimap doesn't have. Enter the passphrase or "
               "recovery key it was encrypted with (it may be from before you changed keys)."</p>
            <input type="password" autocomplete="off" class=INPUT prop:value=move || typed.get()
                   on:input=move |ev| typed.set(event_target_value(&ev))
                   on:keydown=move |ev| if ev.key() == "Enter" { go() } />
            <div class="flex items-center gap-2">
                <button class=BUTTON_PRIMARY on:click=move |_| go_click()>"Continue"</button>
                <button class=BUTTON on:click=move |_| restore.clear()>"Cancel"</button>
            </div>
        </div>
    }
}

#[component]
fn BackupRow(entry: BackupEntry, restore: Restore, flag_plain: bool) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let path = entry.path.clone();
    let plain = flag_plain && !entry.encrypted;
    view! {
        <div class="flex items-center gap-2 border-b border-line px-2 py-1 last:border-b-0">
            <span class=kind_tone(entry.kind).chip()>{kind_label(entry.kind)}</span>
            {plain.then(|| view! { <span class=Tone::Danger.chip() title="This backup is not encrypted">"unencrypted"</span> })}
            <span class="w-40 shrink-0 tabular-nums">{entry.created.clone()}</span>
            <span class="min-w-0 flex-1 truncate text-[11px] text-muted" title=entry.file_name.clone()>
                {format!("{} · {}", age_text(entry.age_minutes), entry.file_name)}
            </span>
            <span class="shrink-0 text-[11px] tabular-nums text-muted">{size_text(entry.bytes)}</span>
            <button class=BUTTON on:click=move |_| check(path.clone(), None, restore, toasts)>"Restore…"</button>
        </div>
    }
}

#[component]
fn ConfirmRestore(preview: RestorePreview, restore: Restore) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let path = preview.path.clone();
    let secret = restore.secret.get_untracked();
    let busy = RwSignal::new(false);
    let do_restore = move |_| {
        let (path, secret) = (path.clone(), secret.clone());
        busy.set(true);
        spawn_local(async move {
            match api::restore_backup(path, secret.map(Secret)).await {
                Ok(_) => {
                    // Start from a clean slate: every screen, the theme and the first-run check
                    // read the restored data.
                    if let Some(w) = web_sys::window() {
                        let _ = w.location().reload();
                    }
                }
                Err(e) => {
                    busy.set(false);
                    toasts.error(&e);
                }
            }
        });
    };
    let upgrade = preview.will_upgrade.then(|| {
        view! {
            <p class="text-[11px] text-muted">
                {format!(
                    "This backup is from an older version (data format {}); it is upgraded to {} after restoring.",
                    preview.schema_version, preview.current_schema_version
                )}
            </p>
        }
    });
    view! {
        <div class="space-y-2 rounded-sm border border-danger/40 bg-danger/5 p-3" role="alertdialog"
             aria-label="Confirm restore">
            <p class="font-medium">{format!("Replace your current data with {}?", preview.file_name)}</p>
            <p class="text-muted">{format!("It holds {} ({}).", counts_text(&preview.counts), size_text(preview.bytes))}</p>
            {upgrade}
            <p class="text-[11px] text-muted">
                "Everything you have now is saved first as a \"before restore\" backup in the backup folder, "
                "so you can come back to it. Your database keeps its current encryption setting. The app "
                "reloads afterwards."
            </p>
            <div class="flex items-center gap-2">
                <button class=BUTTON_DANGER disabled=move || busy.get() on:click=do_restore>"Restore"</button>
                <button class=BUTTON disabled=move || busy.get() on:click=move |_| restore.clear()>"Cancel"</button>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(minutes: u64) -> BackupEntry {
        BackupEntry {
            file_name: "minimap-20270303-153000.db".into(),
            path: "/b/minimap-20270303-153000.db".into(),
            kind: BackupKind::Manual,
            created: "2027-03-03 15:30 UTC".into(),
            age_minutes: minutes,
            bytes: 2048,
            encrypted: true,
        }
    }

    #[test]
    fn ages_read_naturally() {
        assert_eq!(age_text(0), "just now");
        assert_eq!(age_text(1), "1 minute ago");
        assert_eq!(age_text(59), "59 minutes ago");
        assert_eq!(age_text(60), "1 hour ago");
        assert_eq!(age_text(180), "3 hours ago");
        assert_eq!(age_text(1439), "23 hours ago");
        assert_eq!(age_text(1440), "1 day ago");
        assert_eq!(age_text(60 * 24 * 9), "9 days ago");
    }

    #[test]
    fn sizes_read_naturally() {
        assert_eq!(size_text(0), "0 B");
        assert_eq!(size_text(1023), "1023 B");
        assert_eq!(size_text(1024), "1.0 KB");
        assert_eq!(size_text(1536), "1.5 KB");
        assert_eq!(size_text(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn the_last_backup_line_says_when_or_asks_for_one() {
        assert_eq!(
            last_text(Some(&entry(180))),
            "Last backup: 2027-03-03 15:30 UTC (3 hours ago)"
        );
        assert!(last_text(None).starts_with("No backup yet"));
    }

    #[test]
    fn counts_skip_empty_kinds() {
        let c = |label: &str, count| BackupCount {
            label: label.into(),
            count,
        };
        assert_eq!(
            counts_text(&[c("Tasks", 41), c("Teams", 0), c("People", 1)]),
            "41 tasks, 1 people"
        );
        assert_eq!(counts_text(&[c("Tasks", 0)]), "no items");
    }

    #[test]
    fn every_kind_has_a_label_and_the_cautionary_ones_are_warnings() {
        for kind in [
            BackupKind::Manual,
            BackupKind::Auto,
            BackupKind::PreMigration,
            BackupKind::PreRestore,
            BackupKind::PreEncryption,
        ] {
            assert!(!kind_label(kind).is_empty());
        }
        assert_eq!(kind_tone(BackupKind::PreRestore), Tone::Warning);
        assert_eq!(kind_tone(BackupKind::PreMigration), Tone::Warning);
    }
}
