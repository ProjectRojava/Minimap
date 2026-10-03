use leptos::{prelude::*, task::spawn_local};
use minimap_types::{AppError, UpdateSettings};

use crate::{
    api,
    components::form::TextField,
    state::{finish, DataVersion, Toasts},
};

/// Minimal for now: spec 23 adds the rest of the settings. Only the hours-per-day setting exists.
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

    view! {
        <div class="max-w-xl p-6 space-y-6">
            <h1 class="text-[13px] font-semibold">"Settings"</h1>
            <section class="space-y-2">
                <h2 class="text-[11px] font-semibold uppercase tracking-wide text-muted">"Time"</h2>
                {move || match settings.get() {
                    None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
                    Some(Err(_)) => view! { <p class="text-muted">"Couldn't load settings."</p> }.into_any(),
                    Some(Ok(s)) => view! {
                        <TextField label="Hours per working day" kind="number" value=s.hours_per_day.to_string()
                            on_commit=save_hours />
                        <p class="text-[11px] text-muted">
                            "Used to turn estimates like 4h into days (4h = 0.5 days at 8). "
                            "Estimates you've already entered keep their value; only new ones use the new setting."
                        </p>
                    }.into_any(),
                }}
            </section>
            <p class="text-[11px] text-muted">"More settings (theme, backups, encryption) arrive with the Settings feature."</p>
        </div>
    }
}
