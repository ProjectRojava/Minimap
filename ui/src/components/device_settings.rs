//! Settings → General → "Window and tray": what closing the window does, the tray, starting with
//! the computer, reminders and locking. All of it is about this computer only (it is kept in a
//! file next to the database, never synced), so another computer can have other answers.

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{
    AppError, DeviceSettings, DeviceView, OnClose, MAX_LOCK_AFTER_MINUTES, MAX_MEETING_LEAD_MINUTES,
};

use crate::{
    api,
    components::{
        form::{BUTTON, INPUT},
        page::Card,
    },
    state::Toasts,
};

/// The value of the "when I close the window" box for a setting.
pub fn on_close_value(on_close: OnClose) -> &'static str {
    match on_close {
        OnClose::Ask => "ask",
        OnClose::Tray => "tray",
        OnClose::Quit => "quit",
    }
}

/// The setting for a value of the box (anything unknown asks).
pub fn parse_on_close(value: &str) -> OnClose {
    match value {
        "tray" => OnClose::Tray,
        "quit" => OnClose::Quit,
        _ => OnClose::Ask,
    }
}

/// A whole number of minutes typed in a box.
pub fn parse_minutes(text: &str) -> Result<u32, String> {
    text.trim()
        .parse::<u32>()
        .map_err(|_| "Enter a whole number of minutes".to_owned())
}

type Current = RwSignal<Option<DeviceView>>;

fn show(toasts: Toasts, current: Current, result: Result<DeviceView, AppError>) {
    match result {
        Ok(view) => current.set(Some(view)),
        Err(e) => {
            toasts.error(&e);
            // Show what is really stored, so the box never says something that isn't saved.
            spawn_local(async move {
                if let Ok(view) = api::get_device_settings().await {
                    current.set(Some(view));
                }
            });
        }
    }
}

/// Changes one thing in the stored settings and saves.
fn change(toasts: Toasts, current: Current, edit: impl FnOnce(&mut DeviceSettings) + 'static) {
    let Some(view) = current.get_untracked() else {
        return;
    };
    let mut settings = view.settings;
    edit(&mut settings);
    spawn_local(async move {
        show(toasts, current, api::update_device_settings(settings).await);
    });
}

#[component]
pub fn DeviceSettingsCard() -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let current: Current = RwSignal::new(None);
    spawn_local(async move {
        match api::get_device_settings().await {
            Ok(view) => current.set(Some(view)),
            Err(e) => toasts.error(&e),
        }
    });
    view! {
        <Card title="Window and tray"
              description="What closing the window does, starting with the computer, reminders and locking. These are about this computer only.">
            {move || match current.get() {
                Some(view) => body(view, toasts, current).into_any(),
                None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
            }}
        </Card>
    }
}

fn check(
    label: &'static str,
    note: &'static str,
    on: bool,
    disabled: bool,
    toasts: Toasts,
    current: Current,
    edit: impl Fn(&mut DeviceSettings, bool) + Copy + 'static,
) -> impl IntoView {
    view! {
        <label class="flex items-start gap-2">
            <input type="checkbox" class="mt-0.5" prop:checked=on prop:disabled=disabled
                on:change=move |ev| {
                    let on = event_target_checked(&ev);
                    change(toasts, current, move |s| edit(s, on));
                } />
            <span>
                <span class="block font-medium">{label}</span>
                <span class="block text-[11px] text-muted">{note}</span>
            </span>
        </label>
    }
}

fn minutes_box(
    label: &'static str,
    value: u32,
    max: u32,
    disabled: bool,
    toasts: Toasts,
    current: Current,
    edit: impl Fn(&mut DeviceSettings, u32) + Copy + 'static,
) -> impl IntoView {
    view! {
        <label class="block w-40">
            <span class="mb-0.5 block text-[11px] text-muted">{label}</span>
            <input class=INPUT type="number" min="0" max=max.to_string() prop:value=value.to_string()
                prop:disabled=disabled
                on:change=move |ev| match parse_minutes(&event_target_value(&ev)) {
                    Ok(minutes) => change(toasts, current, move |s| edit(s, minutes)),
                    Err(message) => {
                        toasts.error(&AppError { code: "invalid".into(), message });
                        // Put the stored number back.
                        spawn_local(async move {
                            if let Ok(view) = api::get_device_settings().await {
                                current.set(Some(view));
                            }
                        });
                    }
                } />
        </label>
    }
}

fn body(view: DeviceView, toasts: Toasts, current: Current) -> impl IntoView {
    let s = view.settings;
    let quit = move |_| {
        spawn_local(async move {
            if let Err(e) = api::quit_app().await {
                toasts.error(&e);
            }
        });
    };
    let no_tray = (!view.tray_available).then(|| {
        view! {
            <p class="rounded-sm border border-line bg-hover px-2 py-1.5 text-[12px] text-muted">
                "This computer has no system tray that Minimap can use, so closing the window closes Minimap. (On Linux a tray needs an AppIndicator extension.)"
            </p>
        }
    });
    let tray = view.tray_available;
    view! {
        {no_tray}
        <label class="block w-64">
            <span class="mb-0.5 block text-[11px] text-muted">"When I close the window"</span>
            <select class=INPUT prop:disabled=!tray
                on:change=move |ev| {
                    let value = parse_on_close(&event_target_value(&ev));
                    change(toasts, current, move |s| s.on_close = value);
                }>
                <option value=on_close_value(OnClose::Ask) selected=s.on_close == OnClose::Ask>"Ask me"</option>
                <option value=on_close_value(OnClose::Tray) selected=s.on_close == OnClose::Tray>"Keep running in the tray"</option>
                <option value=on_close_value(OnClose::Quit) selected=s.on_close == OnClose::Quit>"Quit Minimap"</option>
            </select>
        </label>
        {check("Minimising hides the window to the tray",
            "The minimise button then takes Minimap off the task bar too. Click the tray icon to bring it back.",
            s.minimise_to_tray, !tray, toasts, current, |s, on| s.minimise_to_tray = on)}
        {check("Start Minimap when this computer starts",
            "Only an installed Minimap does this; a development build leaves your start-up list alone.",
            s.start_at_login, false, toasts, current, |s, on| s.start_at_login = on)}
        {check("…and stay in the tray",
            "When the computer starts it, show just the tray icon. Start it by hand and the window opens.",
            s.start_hidden, !s.start_at_login || !tray, toasts, current, |s, on| s.start_hidden = on)}
        {check("Remind me before a meeting",
            "A notification shortly before a meeting starts, even when the window is hidden.",
            s.notify_meetings, false, toasts, current, |s, on| s.notify_meetings = on)}
        {minutes_box("Minutes before", s.meeting_lead_minutes, MAX_MEETING_LEAD_MINUTES,
            !s.notify_meetings, toasts, current, |s, m| s.meeting_lead_minutes = m)}
        {check("Tell me when pings are waiting",
            "One notification a day, the first time a ping has come due while the window is hidden.",
            s.notify_pings, false, toasts, current, |s, on| s.notify_pings = on)}
        {minutes_box("Lock after hidden for (minutes, 0 = never)", s.lock_after_minutes,
            MAX_LOCK_AFTER_MINUTES, !view.can_lock, toasts, current,
            |s, m| s.lock_after_minutes = m)}
        <p class="text-[11px] text-muted">
            {if view.can_lock {
                "When the window has been hidden in the tray this long, Minimap locks and asks for your passphrase again."
            } else {
                "Locking needs a database that is encrypted with a passphrase (Settings → Security). With the keychain, Minimap would open again without asking, so there is nothing to lock."
            }}
        </p>
        <div>
            <button type="button" class=BUTTON on:click=quit>"Quit Minimap"</button>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_close_box_round_trips_and_an_unknown_value_asks() {
        for on_close in [OnClose::Ask, OnClose::Tray, OnClose::Quit] {
            assert_eq!(parse_on_close(on_close_value(on_close)), on_close);
        }
        assert_eq!(parse_on_close("sideways"), OnClose::Ask);
    }

    #[test]
    fn minutes_are_whole_numbers() {
        assert_eq!(parse_minutes(" 15 "), Ok(15));
        assert_eq!(parse_minutes("0"), Ok(0));
        assert!(parse_minutes("-1").is_err());
        assert!(parse_minutes("ten").is_err());
        assert!(parse_minutes("").is_err());
    }
}
