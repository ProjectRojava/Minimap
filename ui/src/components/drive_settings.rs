//! Settings → Google Drive (spec 22): connect and disconnect, the recovery key (shown once), what
//! is saved and when, the other computers, what the last merge did, checkpoints to recover from,
//! and the OAuth client. While Drive isn't connected this card says plainly that the data lives
//! on this computer only.

use leptos::{prelude::*, task::spawn_local, web_sys};
use minimap_types::{
    CheckpointInfo, ConnectOutcome, MergeSummary, Secret, SyncState, SyncStatus, UpdateSyncSettings,
};

use crate::{
    api,
    components::{
        backup_settings::size_text,
        form::{TextField, BUTTON, BUTTON_DANGER, BUTTON_PRIMARY, INPUT},
        page::{Card, Tone},
        sync_status::{state_hint, state_tone, SyncCtx},
    },
    state::{DataVersion, Toasts},
};

// ------------------------------------------------------------------ pure text

/// "From Desktop at 2027-03-03 15:30 UTC: 3 items added, 2 updated, 1 deleted".
pub fn merge_text(m: &MergeSummary) -> String {
    let mut parts = Vec::new();
    if m.added > 0 {
        parts.push(format!("{} added", m.added));
    }
    if m.updated > 0 {
        parts.push(format!("{} updated", m.updated));
    }
    if m.deleted > 0 {
        parts.push(format!("{} deleted", m.deleted));
    }
    let what = if parts.is_empty() {
        "nothing changed".to_owned()
    } else {
        parts.join(", ")
    };
    format!("From {} at {}: {what}", m.from_device, m.at)
}

/// How much Drive space Minimap uses, and how much attachment space this computer uses.
pub fn usage_text(s: &SyncStatus) -> String {
    let mut text = match s.drive_bytes {
        Some(b) => format!("Minimap uses {} on your Google Drive. ", size_text(b)),
        None => String::new(),
    };
    text.push_str(&format!(
        "Attachments on this computer: {} of {} allowed.",
        size_text(s.media_cache_bytes),
        size_text(s.media_cache_limit_bytes)
    ));
    if s.media_waiting > 0 {
        text.push_str(&format!(" {} waiting to be uploaded.", s.media_waiting));
    }
    text
}

/// "Google Drive already holds Minimap data from 2 computers."
pub fn existing_text(account: &str, devices: u32) -> String {
    let computers = if devices == 1 {
        "1 computer".to_owned()
    } else {
        format!("{devices} computers")
    };
    format!("Google Drive ({account}) already holds Minimap data from {computers}.")
}

/// What is missing before the Connect button can work.
pub fn connect_blocker(s: &SyncStatus) -> Option<&'static str> {
    (!s.client_configured)
        .then_some("Add a Google OAuth client first (see Advanced below), then connect.")
}

/// Why a Drive state calls for a coloured box in the card.
pub fn needs_box(state: SyncState) -> bool {
    matches!(state, SyncState::Error | SyncState::NeedsAttention)
}

// ------------------------------------------------------------------ the card

#[derive(Clone, PartialEq)]
enum Phase {
    Idle,
    /// Waiting for the browser sign-in.
    Waiting,
    /// Signed in; Drive already holds data, so the recovery key is needed.
    NeedKey {
        account: String,
        devices: u32,
    },
    /// Connected as the first device: the recovery key to be saved.
    ShowKey(String),
}

#[component]
pub fn DriveSettings() -> impl IntoView {
    let ctx = expect_context::<SyncCtx>();
    let phase = RwSignal::new(Phase::Idle);
    // Only the *kind* of view switches here, so typing into a field isn't thrown away each time
    // the status is read again.
    let connected = Memo::new(move |_| ctx.status.get().map(|s| s.connected));
    view! {
        <Card title="Google Drive"
              description="Saves your data and attachments to your own Google Drive as you work, and keeps every computer you use in step. Everything is encrypted before it leaves this computer.">
            {move || match connected.get() {
                None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
                Some(false) => view! { <Disconnected phase=phase /> }.into_any(),
                Some(true) => view! { <Connected /> }.into_any(),
            }}
            {move || match phase.get() {
                Phase::ShowKey(key) => view! { <RecoveryKey key=key phase=phase /> }.into_any(),
                _ => ().into_any(),
            }}
        </Card>
    }
}

// ------------------------------------------------------------------ not connected

#[component]
fn Disconnected(phase: RwSignal<Phase>) -> impl IntoView {
    let ctx = expect_context::<SyncCtx>();
    let toasts = expect_context::<Toasts>();
    let version = expect_context::<DataVersion>();

    let connect = move |_| {
        phase.set(Phase::Waiting);
        spawn_local(async move {
            match api::connect_drive().await {
                Ok(ConnectOutcome::Started { recovery_key }) => {
                    phase.set(Phase::ShowKey(recovery_key.0));
                }
                Ok(ConnectOutcome::Joined { adopted, devices }) => {
                    phase.set(Phase::Idle);
                    toasts.info(format!(
                        "Connected to Google Drive ({devices} computer{} already there)",
                        if devices == 1 { "" } else { "s" }
                    ));
                    if adopted {
                        // The data on this computer was replaced by what is on Drive.
                        if let Some(w) = web_sys::window() {
                            let _ = w.location().reload();
                        }
                    }
                    version.bump();
                }
                Ok(ConnectOutcome::NeedsRecoveryKey { account, devices }) => {
                    phase.set(Phase::NeedKey { account, devices });
                }
                Err(e) if e.code == "cancelled" => phase.set(Phase::Idle),
                Err(e) => {
                    phase.set(Phase::Idle);
                    toasts.error(&e);
                }
            }
            ctx.refresh.run(());
        });
    };
    let cancel = move |_| {
        spawn_local(async move {
            let _ = api::cancel_drive_connect().await;
        });
        phase.set(Phase::Idle);
    };
    let blocker = move || ctx.status.get().and_then(|s| connect_blocker(&s));
    view! {
        <div class="space-y-2 rounded-sm border border-warning/40 bg-warning/10 p-3" role="note">
            <p class="font-medium text-warning">"Your data is stored on this computer only"</p>
            <p>
                "Without Google Drive there is no copy anywhere else. If this computer is lost, damaged or "
                "reset, your data is lost with it. The backups below protect against mistakes; they help "
                "against losing the computer only if you keep them somewhere else."
            </p>
        </div>
        <ul class="list-disc space-y-0.5 pl-5 text-muted">
            <li>"Every change is saved to your Drive within seconds, and it keeps working offline."</li>
            <li>"Your other computers pick changes up automatically; nothing to merge by hand."</li>
            <li>"Attachments (images, PDFs, Word, Excel, PowerPoint, Markdown) are stored there too."</li>
            <li>"Minimap can only see files it made itself, and everything is encrypted with a key only you hold."</li>
        </ul>
        {move || match phase.get() {
            Phase::Waiting => view! {
                <div class="space-y-2 rounded-sm border border-line bg-canvas p-3">
                    <p>"Waiting for you to sign in to Google in your browser…"</p>
                    <button class=BUTTON on:click=cancel>"Cancel"</button>
                </div>
            }.into_any(),
            Phase::NeedKey { account, devices } => view! {
                <NeedKey account=account devices=devices phase=phase />
            }.into_any(),
            _ => view! {
                <div class="flex flex-wrap items-center gap-2">
                    <button class=BUTTON_PRIMARY disabled=move || blocker().is_some() on:click=connect>
                        "Connect Google Drive"
                    </button>
                    {move || blocker().map(|b| view! { <span class="text-[11px] text-muted">{b}</span> })}
                </div>
            }.into_any(),
        }}
        <ClientSettings />
    }
}

#[component]
fn NeedKey(account: String, devices: u32, phase: RwSignal<Phase>) -> impl IntoView {
    let ctx = expect_context::<SyncCtx>();
    let toasts = expect_context::<Toasts>();
    let version = expect_context::<DataVersion>();
    let typed = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let go = move || {
        let key = typed.get_untracked();
        if key.trim().is_empty() || busy.get_untracked() {
            return;
        }
        busy.set(true);
        spawn_local(async move {
            match api::finish_drive_connect(Secret(key)).await {
                Ok(outcome) => {
                    phase.set(Phase::Idle);
                    if let ConnectOutcome::Joined { adopted, .. } = outcome {
                        toasts.info("Connected to Google Drive");
                        if adopted {
                            if let Some(w) = web_sys::window() {
                                let _ = w.location().reload();
                            }
                        }
                    }
                    version.bump();
                }
                Err(e) => {
                    busy.set(false);
                    toasts.error(&e);
                }
            }
            ctx.refresh.run(());
        });
    };
    let go_key = go;
    let cancel = move |_| {
        spawn_local(async move {
            let _ = api::cancel_drive_connect().await;
        });
        phase.set(Phase::Idle);
    };
    view! {
        <div class="space-y-2 rounded-sm border border-line bg-canvas p-3">
            <p>{existing_text(&account, devices)}</p>
            <p class="text-muted">
                "Enter the recovery key that was shown when Google Drive was first connected. "
                "If this computer already has data of its own, both sets are kept and merged; "
                "a new computer simply takes what is on Drive."
            </p>
            <input class=INPUT placeholder="Recovery key (groups of four letters and digits)"
                   autocomplete="off" spellcheck="false" prop:value=move || typed.get()
                   on:input=move |ev| typed.set(event_target_value(&ev))
                   on:keydown=move |ev| if ev.key() == "Enter" { go_key() } />
            <div class="flex items-center gap-2">
                <button class=BUTTON_PRIMARY disabled=move || busy.get() on:click=move |_| go()>"Connect"</button>
                <button class=BUTTON on:click=cancel>"Cancel"</button>
            </div>
        </div>
    }
}

/// The recovery key, shown once, until the user says it is saved.
#[component]
fn RecoveryKey(key: String, phase: RwSignal<Phase>) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let saved = RwSignal::new(false);
    let shown = key.clone();
    let copy = move |_| {
        let key = key.clone();
        spawn_local(async move {
            match api::copy_text(&key).await {
                Ok(()) => toasts.info("Recovery key copied"),
                Err(e) => toasts.error(&e),
            }
        });
    };
    view! {
        <div class="space-y-2 rounded-sm border border-warning/40 bg-warning/10 p-3" role="alertdialog"
             aria-label="Save your recovery key">
            <p class="font-medium text-warning">"Save your recovery key"</p>
            <p>
                "Everything on Google Drive is encrypted. This key is the only way to open it on a new "
                "computer or after a reset. Minimap can't recover it for you, and Google can't read your data "
                "without it."
            </p>
            <code class="block select-all break-all rounded-sm border border-line bg-canvas px-2 py-1.5 font-mono text-[13px]">
                {shown}
            </code>
            <div class="flex flex-wrap items-center gap-2">
                <button class=BUTTON on:click=copy>"Copy"</button>
            </div>
            <label class="flex items-center gap-2">
                <input type="checkbox" prop:checked=move || saved.get()
                       on:change=move |ev| saved.set(event_target_checked(&ev)) />
                "I saved this key somewhere safe (a password manager, or on paper)"
            </label>
            <button class=BUTTON_PRIMARY disabled=move || !saved.get()
                    on:click=move |_| phase.set(Phase::Idle)>"Done"</button>
        </div>
    }
}

/// The Google OAuth client Minimap signs in with.
#[component]
fn ClientSettings() -> impl IntoView {
    let ctx = expect_context::<SyncCtx>();
    let toasts = expect_context::<Toasts>();
    let id = RwSignal::new(String::new());
    let secret = RwSignal::new(String::new());
    let configured = move || ctx.status.get().is_some_and(|s| s.client_configured);
    let built_in = move || ctx.status.get().is_some_and(|s| s.client_built_in);
    let save = move |_| {
        let (i, s) = (id.get_untracked(), secret.get_untracked());
        spawn_local(async move {
            let result = api::update_sync_settings(UpdateSyncSettings {
                client_id: Some(i),
                client_secret: Some(Secret(s)),
                ..Default::default()
            })
            .await;
            match result {
                Ok(_) => {
                    id.set(String::new());
                    secret.set(String::new());
                    toasts.info("Saved");
                }
                Err(e) => toasts.error(&e),
            }
            ctx.refresh.run(());
        });
    };
    view! {
        <details class="rounded-sm border border-line" open=move || !configured()>
            <summary class="cursor-pointer px-3 py-1.5 text-[12px] text-muted">
                "Advanced: the Google OAuth client"
                {move || configured().then(|| view! {
                    <span class=format!("ml-2 {}", Tone::Success.chip())>
                        {if built_in() { "built in" } else { "yours" }}
                    </span>
                })}
            </summary>
            <div class="space-y-2 p-3">
                <p class="text-muted">
                    "Google needs an OAuth client to let Minimap ask for access. Create one in the Google Cloud "
                    "Console: make a project, turn on the Google Drive API, then create an OAuth client ID of "
                    "type \"Desktop app\" and paste its ID and secret here. Minimap only ever asks for the "
                    "drive.file permission, so it can see only the files it made. (Google treats a desktop "
                    "app's secret as non-confidential.)"
                </p>
                <label class="block">
                    <span class="block mb-0.5 text-[11px] text-muted">"Client ID"</span>
                    <input class=INPUT autocomplete="off" spellcheck="false" prop:value=move || id.get()
                           on:input=move |ev| id.set(event_target_value(&ev)) />
                </label>
                <label class="block">
                    <span class="block mb-0.5 text-[11px] text-muted">"Client secret"</span>
                    <input class=INPUT type="password" autocomplete="off" prop:value=move || secret.get()
                           on:input=move |ev| secret.set(event_target_value(&ev)) />
                </label>
                <p class="text-[11px] text-muted">"Leave both empty and save to remove your own client."</p>
                <button class=BUTTON on:click=save>"Save client"</button>
            </div>
        </details>
    }
}

// ------------------------------------------------------------------ connected

#[component]
fn Connected() -> impl IntoView {
    let ctx = expect_context::<SyncCtx>();
    let toasts = expect_context::<Toasts>();
    let version = expect_context::<DataVersion>();
    let syncing = RwSignal::new(false);
    let confirm_disconnect = RwSignal::new(false);
    let recover = RwSignal::new(None::<CheckpointInfo>);

    let status = move || ctx.status.get();
    let initial_name = status().map(|s| s.device_name).unwrap_or_default();
    let rename = move |name: String| {
        spawn_local(async move {
            match api::update_sync_settings(UpdateSyncSettings {
                device_name: Some(name),
                ..Default::default()
            })
            .await
            {
                Ok(_) => {}
                Err(e) => toasts.error(&e),
            }
            ctx.refresh.run(());
        });
    };
    let sync = move |_| {
        syncing.set(true);
        spawn_local(async move {
            if let Err(e) = api::sync_now().await {
                toasts.error(&e);
            }
            syncing.set(false);
            ctx.refresh.run(());
            version.bump();
        });
    };
    let disconnect = move |_| {
        spawn_local(async move {
            match api::disconnect_drive().await {
                Ok(_) => {
                    toasts.info("Disconnected from Google Drive. Your data stays on this computer.")
                }
                Err(e) => toasts.error(&e),
            }
            confirm_disconnect.set(false);
            ctx.refresh.run(());
        });
    };
    let refresh_checkpoints = move |_| {
        spawn_local(async move {
            if let Err(e) = api::list_drive_checkpoints().await {
                toasts.error(&e);
            }
            ctx.refresh.run(());
        });
    };

    view! {
        <div class="flex flex-wrap items-center gap-2">
            {move || status().map(|s| view! {
                <span class=state_tone(s.state).chip() title=state_hint(&s)>{s.summary.clone()}</span>
                {s.account.clone().map(|a| view! { <span class="text-muted">{format!("Signed in as {a}")}</span> })}
            })}
            <button class=BUTTON_PRIMARY disabled=move || syncing.get() on:click=sync>
                {move || if syncing.get() { "Syncing…" } else { "Sync now" }}
            </button>
        </div>
        {move || status().and_then(|s| needs_box(s.state).then(|| view! {
            <div class="rounded-sm border border-danger/40 bg-danger/10 p-3 text-danger" role="alert">
                {state_hint(&s)}
            </div>
        }))}
        {move || status().and_then(|s| s.warning.map(|w| view! {
            <div class="rounded-sm border border-warning/40 bg-warning/10 p-3 text-warning" role="note">{w}</div>
        }))}
        {move || status().map(|s| view! {
            <p class="text-muted">
                {match &s.last_saved {
                    Some(t) => format!("Last saved to Drive: {t}."),
                    None => "Nothing saved to Drive yet.".to_owned(),
                }}
                {s.unsaved_changes.then_some(" Changes made since are waiting to be saved.")}
            </p>
            <p class="text-[11px] text-muted">{usage_text(&s)}</p>
        })}
        <TextField label="This computer is called" value=initial_name on_commit=rename />

        <div class="space-y-1">
            <div class="text-[11px] text-muted">"Computers using this Google Drive"</div>
            <div class="rounded-sm border border-line">
                {move || status().map(|s| s.devices.into_iter().map(|d| view! {
                    <div class="flex items-center gap-2 border-b border-line px-2 py-1 last:border-b-0">
                        <span class="min-w-0 flex-1 truncate">{d.name.clone()}</span>
                        {d.is_this_device.then(|| view! { <span class=Tone::Accent.chip()>"this computer"</span> })}
                        <span class="shrink-0 text-[11px] tabular-nums text-muted">
                            {if d.last_saved.is_empty() { "not saved yet".to_owned() } else { format!("saved {}", d.last_saved) }}
                        </span>
                    </div>
                }).collect_view())}
            </div>
        </div>

        {move || status().and_then(|s| s.last_merge.map(|m| view! {
            <div class="space-y-1">
                <div class="text-[11px] text-muted">"Last time another computer's changes were merged in"</div>
                <p>{merge_text(&m)}</p>
                {(!m.repairs.is_empty()).then(|| view! {
                    <div class="space-y-1 rounded-sm border border-warning/40 bg-warning/10 p-2">
                        <p class="text-[11px] text-warning">
                            "Two computers changed things in ways that couldn't both stand, so Minimap fixed these:"
                        </p>
                        <ul class="list-disc space-y-0.5 pl-5">
                            {m.repairs.iter().map(|r| view! { <li>{r.clone()}</li> }).collect_view()}
                        </ul>
                    </div>
                })}
            </div>
        }))}

        <div class="space-y-1">
            <div class="flex items-center gap-2">
                <span class="text-[11px] text-muted">"Checkpoints on Drive (hourly and daily copies you can bring items back from)"</span>
                <button class=BUTTON on:click=refresh_checkpoints>"Refresh"</button>
            </div>
            <div class="max-h-56 overflow-y-auto rounded-sm border border-line">
                {move || {
                    let list = status().map(|s| s.checkpoints).unwrap_or_default();
                    if list.is_empty() {
                        view! { <p class="px-2 py-1 text-muted">"None yet. The first is made within an hour of saving."</p> }.into_any()
                    } else {
                        list.into_iter().map(|c| {
                            let picked = c.clone();
                            view! {
                                <div class="flex items-center gap-2 border-b border-line px-2 py-1 last:border-b-0">
                                    <span class="w-40 shrink-0 tabular-nums">{c.created.clone()}</span>
                                    <span class="min-w-0 flex-1 truncate text-[11px] text-muted">{format!("from {}", c.device_name)}</span>
                                    <span class="shrink-0 text-[11px] tabular-nums text-muted">{size_text(c.bytes)}</span>
                                    <button class=BUTTON on:click=move |_| recover.set(Some(picked.clone()))>"Recover…"</button>
                                </div>
                            }
                        }).collect_view().into_any()
                    }
                }}
            </div>
        </div>
        {move || recover.get().map(|c| view! { <ConfirmRecover checkpoint=c recover=recover /> })}

        <div class="border-t border-line pt-2">
            {move || if confirm_disconnect.get() {
                view! {
                    <div class="space-y-2 rounded-sm border border-danger/40 bg-danger/5 p-3" role="alertdialog">
                        <p class="font-medium">"Disconnect Google Drive?"</p>
                        <p class="text-muted">
                            "Minimap stops saving to Drive and stops syncing with your other computers. "
                            "Everything stays on this computer and on Drive, and you can connect again any time."
                        </p>
                        <div class="flex items-center gap-2">
                            <button class=BUTTON_DANGER on:click=disconnect>"Disconnect"</button>
                            <button class=BUTTON on:click=move |_| confirm_disconnect.set(false)>"Cancel"</button>
                        </div>
                    </div>
                }.into_any()
            } else {
                view! { <button class=BUTTON on:click=move |_| confirm_disconnect.set(true)>"Disconnect…"</button> }.into_any()
            }}
        </div>
    }
}

#[component]
fn ConfirmRecover(
    checkpoint: CheckpointInfo,
    recover: RwSignal<Option<CheckpointInfo>>,
) -> impl IntoView {
    let ctx = expect_context::<SyncCtx>();
    let toasts = expect_context::<Toasts>();
    let version = expect_context::<DataVersion>();
    let busy = RwSignal::new(false);
    let name = checkpoint.name.clone();
    let go = move |_| {
        busy.set(true);
        let name = name.clone();
        spawn_local(async move {
            match api::recover_checkpoint(name).await {
                Ok(r) => toasts.info(format!(
                    "{} item{} brought back",
                    r.restored,
                    if r.restored == 1 { "" } else { "s" }
                )),
                Err(e) => toasts.error(&e),
            }
            recover.set(None);
            ctx.refresh.run(());
            version.bump();
        });
    };
    view! {
        <div class="space-y-2 rounded-sm border border-warning/40 bg-warning/10 p-3" role="alertdialog"
             aria-label="Confirm recovery">
            <p class="font-medium">{format!("Bring items back from {}?", checkpoint.created)}</p>
            <p class="text-muted">
                "Items that were changed or deleted since then come back as they were at that time. They count "
                "as new edits, so your other computers get them too. Items made since then stay as they are. "
                "A backup of your current data is made first."
            </p>
            <div class="flex items-center gap-2">
                <button class=BUTTON_PRIMARY disabled=move || busy.get() on:click=go>"Recover"</button>
                <button class=BUTTON disabled=move || busy.get() on:click=move |_| recover.set(None)>"Cancel"</button>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn merge(added: u32, updated: u32, deleted: u32) -> MergeSummary {
        MergeSummary {
            from_device: "Desktop".into(),
            at: "2027-03-03 15:30 UTC".into(),
            added,
            updated,
            deleted,
            repairs: Vec::new(),
        }
    }

    #[test]
    fn a_merge_is_described_by_what_it_did() {
        assert_eq!(
            merge_text(&merge(3, 2, 1)),
            "From Desktop at 2027-03-03 15:30 UTC: 3 added, 2 updated, 1 deleted"
        );
        assert_eq!(
            merge_text(&merge(0, 4, 0)),
            "From Desktop at 2027-03-03 15:30 UTC: 4 updated"
        );
        assert!(merge_text(&merge(0, 0, 0)).ends_with("nothing changed"));
    }

    #[test]
    fn existing_data_is_described_with_the_account_and_a_count() {
        assert_eq!(
            existing_text("me@example.com", 1),
            "Google Drive (me@example.com) already holds Minimap data from 1 computer."
        );
        assert!(existing_text("me@example.com", 3).ends_with("from 3 computers."));
    }

    fn status(client: bool) -> SyncStatus {
        SyncStatus {
            state: SyncState::LocalOnly,
            summary: String::new(),
            connected: false,
            account: None,
            client_configured: client,
            client_built_in: false,
            device_id: String::new(),
            device_name: String::new(),
            unsaved_changes: false,
            last_saved: None,
            last_saved_seconds_ago: None,
            last_merge: None,
            last_error: None,
            warning: None,
            devices: Vec::new(),
            checkpoints: Vec::new(),
            drive_bytes: Some(3 * 1024 * 1024),
            media_waiting: 2,
            data_revision: 0,
            banner_hidden_until: None,
            media_cache_bytes: 1024 * 1024,
            media_cache_limit_bytes: 2 * 1024 * 1024 * 1024,
        }
    }

    #[test]
    fn connecting_needs_an_oauth_client_and_says_so() {
        assert!(connect_blocker(&status(false))
            .unwrap()
            .contains("OAuth client"));
        assert!(connect_blocker(&status(true)).is_none());
    }

    #[test]
    fn usage_names_drive_space_cache_space_and_uploads_waiting() {
        let text = usage_text(&status(true));
        assert!(text.contains("3.0 MB on your Google Drive"), "{text}");
        assert!(text.contains("1.0 MB of 2.0 GB"), "{text}");
        assert!(text.contains("2 waiting to be uploaded"), "{text}");
        let mut local = status(true);
        local.drive_bytes = None;
        local.media_waiting = 0;
        assert!(!usage_text(&local).contains("Google Drive"));
    }

    #[test]
    fn only_trouble_gets_a_red_box() {
        assert!(needs_box(SyncState::Error));
        assert!(needs_box(SyncState::NeedsAttention));
        assert!(!needs_box(SyncState::Saved));
        assert!(!needs_box(SyncState::Offline));
    }
}
