//! Google Drive on screen (spec 22): the status the whole app polls, the status bar along the
//! bottom, and the banner that says, in plain words, when data lives on this computer only or
//! when Drive needs attention. The Settings card is in `drive_settings.rs`.

use std::time::Duration;

use leptos::{prelude::*, task::spawn_local};
use leptos_router::components::A;
use minimap_types::{SyncState, SyncStatus, UpdateSyncSettings};

use crate::{
    api,
    components::page::{Icon, Tone},
    state::DataVersion,
};

/// How often the status is read from the backend (a cheap, in-memory read).
const POLL: Duration = Duration::from_secs(4);

#[derive(Clone, Copy)]
pub struct SyncCtx {
    /// The latest status; `None` until the first answer.
    pub status: RwSignal<Option<SyncStatus>>,
    /// Reads it again now (after connecting, disconnecting, syncing).
    pub refresh: Callback<()>,
}

/// Starts polling the sync status and provides it to every screen. When another device's
/// changes were merged in, every list and panel reloads. Call once, inside the app shell.
pub fn use_sync_status() -> SyncCtx {
    let version = expect_context::<DataVersion>();
    let status = RwSignal::new(None::<SyncStatus>);
    let seen_revision = StoredValue::new(None::<u64>);
    let refresh = Callback::new(move |()| {
        spawn_local(async move {
            if let Ok(s) = api::get_sync_status().await {
                if seen_revision
                    .get_value()
                    .is_some_and(|r| r != s.data_revision)
                {
                    version.bump();
                }
                seen_revision.set_value(Some(s.data_revision));
                status.set(Some(s));
            }
        });
    });
    refresh.run(());
    let handle = set_interval_with_handle(move || refresh.run(()), POLL).ok();
    on_cleanup(move || {
        if let Some(h) = handle {
            h.clear();
        }
    });
    let ctx = SyncCtx { status, refresh };
    provide_context(ctx);
    ctx
}

/// The colour a state is shown in.
pub fn state_tone(state: SyncState) -> Tone {
    match state {
        SyncState::LocalOnly | SyncState::Offline => Tone::Warning,
        SyncState::Saved => Tone::Success,
        SyncState::Saving | SyncState::Syncing => Tone::Accent,
        SyncState::Error | SyncState::NeedsAttention => Tone::Danger,
    }
}

/// The longer explanation behind a status line, for its tooltip.
pub fn state_hint(status: &SyncStatus) -> String {
    match status.state {
        SyncState::LocalOnly => "Your data exists only on this computer. Connect Google Drive in Settings to back it up and keep your other computers in sync.".to_owned(),
        SyncState::Saved => match &status.account {
            Some(a) => format!("Everything is on Google Drive ({a}). Other computers receive changes within about 30 seconds."),
            None => "Everything is on Google Drive.".to_owned(),
        },
        SyncState::Saving => "Saving your latest changes to Google Drive.".to_owned(),
        SyncState::Syncing => "Merging changes made on another computer.".to_owned(),
        SyncState::Offline => "Google Drive can't be reached. Changes are kept and saved when the connection is back.".to_owned(),
        SyncState::Error => status
            .last_error
            .clone()
            .unwrap_or_else(|| "Something went wrong talking to Google Drive.".to_owned()),
        SyncState::NeedsAttention => status
            .last_error
            .clone()
            .unwrap_or_else(|| "Google Drive needs you: open Settings → Google Drive.".to_owned()),
    }
}

#[component]
pub fn StatusBar() -> impl IntoView {
    let ctx = expect_context::<SyncCtx>();
    view! {
        <footer class="flex h-6 shrink-0 items-center gap-3 border-t border-line bg-panel px-3 text-[11px] text-muted"
                aria-label="Google Drive status">
            {move || ctx.status.get().map(|s| {
                let tone = state_tone(s.state);
                let hint = state_hint(&s);
                let local_only = s.state == SyncState::LocalOnly;
                let warning = s.warning.clone();
                let device = s.device_name.clone();
                view! {
                    <A href="/settings" attr:class="flex items-center gap-1.5 hover:text-fg" attr:title=hint>
                        <Icon name="cloud" size="h-3.5 w-3.5" />
                        <span class=tone.chip()>{s.summary.clone()}</span>
                    </A>
                    {local_only.then(|| view! {
                        <span class="hidden sm:inline">
                            "Your data is stored on this device only. "
                            <A href="/settings" attr:class="text-accent hover:underline">"Connect Google Drive"</A>
                        </span>
                    })}
                    {warning.map(|w| {
                        let tip = w.clone();
                        view! { <span class="truncate text-warning" title=tip>{w}</span> }
                    })}
                    <span class="ml-auto shrink-0">{device}</span>
                }
            })}
        </footer>
    }
}

/// A note at the top of the landing screen when data is local only, or when Drive needs the
/// user. Local only can be put off for a week, never for good.
#[component]
pub fn SyncBanner() -> impl IntoView {
    let ctx = expect_context::<SyncCtx>();
    let hide = move |_| {
        spawn_local(async move {
            let _ = api::update_sync_settings(UpdateSyncSettings {
                hide_banner: true,
                ..Default::default()
            })
            .await;
            ctx.refresh.run(());
        });
    };
    view! {
        {move || ctx.status.get().and_then(|s| match s.state {
            SyncState::LocalOnly if s.banner_hidden_until.is_none() => Some(view! {
                <div class="mx-4 mt-3 space-y-2 rounded-sm border border-warning/40 bg-warning/10 p-3" role="note">
                    <p class="font-medium text-warning">"Your data is stored on this computer only"</p>
                    <p>
                        "If this computer is lost, damaged or reset, everything in Minimap goes with it. "
                        "Connect Google Drive and your work is saved automatically and kept in step on every computer you use."
                    </p>
                    <div class="flex flex-wrap items-center gap-2">
                        <A href="/settings" attr:class=crate::components::form::BUTTON_PRIMARY>"Connect Google Drive"</A>
                        <button class=crate::components::form::BUTTON on:click=hide>"Remind me in a week"</button>
                    </div>
                </div>
            }.into_any()),
            SyncState::NeedsAttention => Some(view! {
                <div class="mx-4 mt-3 space-y-1 rounded-sm border border-danger/40 bg-danger/10 p-3" role="alert">
                    <p class="font-medium text-danger">"Google Drive needs you"</p>
                    <p>{state_hint(&s)}</p>
                    <A href="/settings" attr:class=crate::components::form::BUTTON>"Open Google Drive settings"</A>
                </div>
            }.into_any()),
            _ => s.warning.map(|w| view! {
                <div class="mx-4 mt-3 rounded-sm border border-warning/40 bg-warning/10 p-3" role="note">
                    <p class="text-warning">{w}</p>
                </div>
            }.into_any()),
        })}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(state: SyncState) -> SyncStatus {
        SyncStatus {
            state,
            summary: String::new(),
            connected: state != SyncState::LocalOnly,
            account: Some("me@example.com".into()),
            client_configured: true,
            client_built_in: false,
            device_id: "d".into(),
            device_name: "Laptop".into(),
            unsaved_changes: false,
            last_saved: None,
            last_saved_seconds_ago: None,
            last_merge: None,
            last_error: None,
            warning: None,
            devices: Vec::new(),
            checkpoints: Vec::new(),
            drive_bytes: None,
            media_waiting: 0,
            data_revision: 0,
            banner_hidden_until: None,
            media_cache_bytes: 0,
            media_cache_limit_bytes: 0,
        }
    }

    #[test]
    fn every_state_has_a_colour_and_trouble_is_never_green() {
        assert_eq!(state_tone(SyncState::Saved), Tone::Success);
        assert_eq!(state_tone(SyncState::LocalOnly), Tone::Warning);
        assert_eq!(state_tone(SyncState::Offline), Tone::Warning);
        assert_eq!(state_tone(SyncState::Saving), Tone::Accent);
        assert_eq!(state_tone(SyncState::Error), Tone::Danger);
        assert_eq!(state_tone(SyncState::NeedsAttention), Tone::Danger);
    }

    #[test]
    fn the_local_only_hint_says_the_data_is_only_here() {
        let hint = state_hint(&status(SyncState::LocalOnly));
        assert!(hint.contains("only on this computer"), "{hint}");
        assert!(hint.contains("Connect Google Drive"));
    }

    #[test]
    fn errors_show_their_reason_and_saved_names_the_account() {
        let mut s = status(SyncState::Error);
        s.last_error = Some("Your Google Drive is full".into());
        assert_eq!(state_hint(&s), "Your Google Drive is full");
        s.last_error = None;
        assert!(state_hint(&s).contains("went wrong"));
        assert!(state_hint(&status(SyncState::Saved)).contains("me@example.com"));
        let needs = state_hint(&status(SyncState::NeedsAttention));
        assert!(needs.contains("Settings"), "{needs}");
    }
}
