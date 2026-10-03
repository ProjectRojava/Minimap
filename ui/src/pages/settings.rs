use leptos::{prelude::*, task::spawn_local};
use minimap_types::{AppError, UpdateSettings};

use crate::{
    api,
    components::form::TextField,
    state::{finish, DataVersion, Toasts},
    theme::ThemeCtx,
    themes::{self, Kind, Theme},
};

/// Minimal for now: spec 23 adds the rest (backups, encryption, ...). Today: appearance and
/// hours per day.
#[component]
pub fn Settings() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let settings = LocalResource::new(api::get_settings);
    Effect::new(move |_| {
        if let Some(Err(e)) = settings.get() {
            toasts.error(&e);
        }
    });

    let save_hours = move |v: String| match v.trim().parse::<f64>() {
        Ok(h) => spawn_local(async move {
            finish(
                api::update_settings(UpdateSettings {
                    hours_per_day: Some(h),
                    ..Default::default()
                })
                .await,
                toasts,
                version,
            );
        }),
        Err(_) => {
            toasts.error(&AppError {
                code: "invalid".into(),
                message: "Hours per day must be a number".into(),
            });
            version.bump();
        }
    };

    let save_stale = move |v: String| match v.trim().parse::<u32>() {
        Ok(d) => spawn_local(async move {
            finish(
                api::update_settings(UpdateSettings {
                    stale_waiting_days: Some(d),
                    ..Default::default()
                })
                .await,
                toasts,
                version,
            );
        }),
        Err(_) => {
            toasts.error(&AppError {
                code: "invalid".into(),
                message: "Stale after must be a whole number of days".into(),
            });
            version.bump();
        }
    };

    view! {
        <div class="max-w-3xl p-6 space-y-8">
            <h1 class="text-[13px] font-semibold">"Settings"</h1>
            <Appearance />
            <section class="space-y-2">
                <h2 class="text-[11px] font-semibold uppercase tracking-wide text-muted">"Time"</h2>
                {move || match settings.get() {
                    None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
                    Some(Err(_)) => view! { <p class="text-muted">"Couldn't load settings."</p> }.into_any(),
                    Some(Ok(s)) => view! {
                        <div class="max-w-xs">
                            <TextField label="Hours per working day" kind="number" value=s.hours_per_day.to_string()
                                on_commit=save_hours />
                        </div>
                        <p class="text-[11px] text-muted">
                            "Used to turn estimates like 4h into days (4h = 0.5 days at 8). "
                            "Estimates you've already entered keep their value; only new ones use the new setting."
                        </p>
                    }.into_any(),
                }}
            </section>
            <section class="space-y-2">
                <h2 class="text-[11px] font-semibold uppercase tracking-wide text-muted">"Waiting on"</h2>
                {move || match settings.get() {
                    Some(Ok(s)) => view! {
                        <div class="max-w-xs">
                            <TextField label="Stale after (days)" kind="number" value=s.stale_waiting_days.to_string()
                                on_commit=save_stale />
                        </div>
                        <p class="text-[11px] text-muted">
                            "An open waiting-on is stale once it is older than this, or past its expected date. "
                            "Snoozed ones are not stale until they resurface."
                        </p>
                    }.into_any(),
                    _ => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
                }}
            </section>
            <p class="text-[11px] text-muted">"More settings (backups, encryption) arrive with the Settings feature."</p>
        </div>
    }
}

/// Colour theme picker: dark is the default; "System" follows the OS.
#[component]
fn Appearance() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let theme = expect_context::<ThemeCtx>();

    // Applies at once; saving to the database follows.
    let choose = move |id: &'static str| {
        theme.select(id);
        spawn_local(async move {
            let patch = UpdateSettings {
                theme: Some(id.to_owned()),
                ..Default::default()
            };
            finish(api::update_settings(patch).await, toasts, version);
        });
    };
    let group = move |title: &'static str, kind: Kind| {
        let cards = themes::ALL
            .iter()
            .filter(|t| t.kind == kind)
            .map(|t| view! { <ThemeCard theme=t selected=Signal::derive(move || theme.selected() == t.id) on_pick=move || choose(t.id) /> })
            .collect_view();
        view! {
            <div class="space-y-2">
                <h3 class="text-[11px] text-muted">{title}</h3>
                <div class="grid grid-cols-2 gap-2 sm:grid-cols-3">{cards}</div>
            </div>
        }
    };

    view! {
        <section class="space-y-3">
            <h2 class="text-[11px] font-semibold uppercase tracking-wide text-muted">"Appearance"</h2>
            <button
                class=move || format!(
                    "flex w-full items-center gap-3 rounded-sm border p-2 text-left hover:bg-hover {}",
                    if theme.selected() == themes::SYSTEM_ID { "border-line-strong bg-active" } else { "border-line" })
                aria-pressed=move || (theme.selected() == themes::SYSTEM_ID).to_string()
                on:click=move |_| choose(themes::SYSTEM_ID)
            >
                <span class="flex h-8 w-14 shrink-0 overflow-hidden rounded-sm border border-line">
                    <span class="h-full w-1/2" style=format!("background:{}", themes::resolve("system", true).tokens.canvas)></span>
                    <span class="h-full w-1/2" style=format!("background:{}", themes::resolve("system", false).tokens.canvas)></span>
                </span>
                <span>
                    <span class="block font-medium">"System"</span>
                    <span class="block text-[11px] text-muted">"Follow your OS: Minimap Dark or Minimap Light"</span>
                </span>
            </button>
            {group("Dark", Kind::Dark)}
            {group("Light", Kind::Light)}
            <p class="text-[11px] text-muted">
                "Palettes are adapted from the originals to fit the app's colour tokens, with text colours "
                "nudged where needed so everything stays readable."
            </p>
        </section>
    }
}

/// A theme as a button with a miniature of its colours.
#[component]
fn ThemeCard(
    theme: &'static Theme,
    selected: Signal<bool>,
    on_pick: impl Fn() + 'static,
) -> impl IntoView {
    let k = theme.tokens;
    view! {
        <button
            class=move || format!(
                "rounded-sm border p-2 text-left hover:bg-hover {}",
                if selected.get() { "border-line-strong bg-active" } else { "border-line" })
            aria-pressed=move || selected.get().to_string()
            on:click=move |_| on_pick()
        >
            <span class="flex h-12 w-full overflow-hidden rounded-sm"
                  style=format!("background:{}; border:1px solid {}", k.canvas, k.line)>
                <span class="flex h-full w-6 flex-col gap-1 p-1"
                      style=format!("background:{}; border-right:1px solid {}", k.panel, k.line)>
                    <span class="h-0.5 w-full" style=format!("background:{}", k.muted)></span>
                    <span class="h-0.5 w-full" style=format!("background:{}", k.muted)></span>
                    <span class="h-0.5 w-2/3" style=format!("background:{}", k.muted)></span>
                </span>
                <span class="flex flex-1 flex-col gap-1 p-1.5">
                    <span class="h-1 w-3/4" style=format!("background:{}", k.fg)></span>
                    <span class="h-1 w-1/2" style=format!("background:{}", k.muted)></span>
                    <span class="h-1 w-1/4" style=format!("background:{}", k.danger)></span>
                </span>
            </span>
            <span class="mt-1.5 block truncate">{theme.name}</span>
        </button>
    }
}
