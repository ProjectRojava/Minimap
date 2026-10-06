use leptos::{ev, prelude::*, task::spawn_local};
use leptos_router::{hooks::query_signal_with_options, NavigateOptions};
use minimap_types::{
    AppError, HealthThresholds, Settings as AppSettings, UpdateSettings, WorkWeek,
    REPORT_PLACEHOLDERS, WEEKDAY_NAMES,
};
use wasm_bindgen::JsCast;

use crate::{
    api,
    components::form::{BUTTON, BUTTON_ON, BUTTON_PRIMARY, COMPACT_INPUT, INPUT},
    components::{
        backup_settings::BackupSettings,
        data_settings::DataLocation,
        demo_data_settings::DemoDataSettings,
        developer_settings::DeveloperSettings,
        drive_settings::DriveSettings,
        export_settings::ExportSettings,
        page::{Card, PageHeader},
        security_settings::SecuritySettings,
    },
    settings_tab::Tab,
    state::{finish, DataVersion, Toasts},
    theme::ThemeCtx,
    themes::{self, Kind, Theme},
};

fn tab_class(selected: bool) -> &'static str {
    if selected {
        "-mb-px border-b-2 border-accent px-3 py-2 text-[12px] font-medium text-fg"
    } else {
        "-mb-px border-b-2 border-transparent px-3 py-2 text-[12px] text-muted hover:text-fg"
    }
}

fn focus_tab(tab: Tab) {
    if let Some(el) = document()
        .get_element_by_id(&tab.button_id())
        .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = el.focus();
    }
}

/// The row of tabs. Arrow keys, Home and End move between them.
#[component]
fn TabBar(current: Signal<Tab>, #[prop(into)] on_select: Callback<Tab>) -> impl IntoView {
    let buttons = Tab::visible()
        .into_iter()
        .map(|tab| {
            let key = move |ev: ev::KeyboardEvent| {
                let next = match ev.key().as_str() {
                    "ArrowRight" => Some(tab.step(1)),
                    "ArrowLeft" => Some(tab.step(-1)),
                    "Home" => Tab::visible().first().copied(),
                    "End" => Tab::visible().last().copied(),
                    _ => None,
                };
                if let Some(next) = next {
                    ev.prevent_default();
                    on_select.run(next);
                    focus_tab(next);
                }
            };
            view! {
                <button role="tab" id=tab.button_id() aria-controls=tab.panel_id()
                    aria-selected=move || (current.get() == tab).to_string()
                    tabindex=move || if current.get() == tab { "0" } else { "-1" }
                    class=move || tab_class(current.get() == tab)
                    on:click=move |_| on_select.run(tab)
                    on:keydown=key>
                    {tab.label()}
                </button>
            }
        })
        .collect_view();
    view! {
        <div class="flex shrink-0 gap-1 border-b border-line bg-panel px-4" role="tablist" aria-label="Settings sections">
            {buttons}
        </div>
    }
}

type Loaded = LocalResource<Result<AppSettings, AppError>>;

/// What a card shows while the stored settings load (or can't be read), then `render`.
fn when_loaded(
    settings: Loaded,
    render: impl Fn(AppSettings) -> AnyView + 'static,
) -> impl Fn() -> AnyView {
    move || match settings.get() {
        Some(Ok(s)) => render(s),
        Some(Err(_)) => view! { <p class="text-muted">"Couldn't load settings."</p> }.into_any(),
        None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
    }
}

#[component]
pub fn Settings() -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let settings: Loaded = LocalResource::new(api::get_settings);
    Effect::new(move |_| {
        if let Some(Err(e)) = settings.get() {
            toasts.error(&e);
        }
    });

    // The tab lives in the address (`/settings?tab=data`) so other screens can link to it.
    let (tab_param, set_tab_param) = query_signal_with_options::<String>(
        "tab",
        NavigateOptions {
            replace: true,
            ..Default::default()
        },
    );
    let current = Signal::derive(move || Tab::from_id(tab_param.get().as_deref().unwrap_or("")));
    let select = move |tab: Tab| {
        set_tab_param.set((tab != Tab::General).then(|| tab.id().to_owned()));
    };

    // Every panel stays mounted (hidden when not shown), so a half-typed template survives a
    // trip to another tab.
    let scroller = NodeRef::<leptos::html::Div>::new();
    Effect::new(move |_| {
        current.track();
        if let Some(el) = scroller.get() {
            el.set_scroll_top(0);
        }
    });
    let panel = move |tab: Tab, content: AnyView| {
        view! {
            <div role="tabpanel" id=tab.panel_id() aria-labelledby=tab.button_id()
                 class="mx-auto max-w-3xl space-y-4 p-6" class:hidden=move || current.get() != tab>
                {content}
            </div>
        }
    };

    view! {
        <div class="flex h-full flex-col">
            <PageHeader icon="settings" title="Settings" subtitle="Appearance, time, thresholds, reports, data and security"><span></span></PageHeader>
            <TabBar current=current on_select=select />
            <div class="flex-1 overflow-y-auto" node_ref=scroller>
                {panel(Tab::General, view! {
                    <TimeSettings settings=settings />
                    <Appearance />
                }.into_any())}
                {panel(Tab::Thresholds, view! {
                    <WaitingSettings settings=settings />
                    <CapacitySettings settings=settings />
                    <HealthSettings />
                }.into_any())}
                {panel(Tab::Reports, view! { <ReportSettings /> }.into_any())}
                {panel(Tab::Data, view! {
                    <DemoDataSettings />
                    <DataLocation />
                    <DriveSettings />
                    <BackupSettings />
                    <ExportSettings />
                }.into_any())}
                {panel(Tab::Security, view! { <SecuritySettings /> }.into_any())}
                {Tab::Developer.is_shown().then(|| panel(Tab::Developer, view! { <DeveloperSettings /> }.into_any()))}
            </div>
        </div>
    }
}

/// A number box that saves when it loses focus. A number that can't be read or is refused is
/// reported and the stored one put back, so the box never shows something that isn't saved.
#[component]
fn NumberSetting(
    label: &'static str,
    /// The stored value.
    stored: f64,
    /// Whole numbers only (days, counts).
    whole: bool,
    /// Said when the text isn't a number.
    invalid: &'static str,
    /// The patch that sets the value.
    patch: fn(f64) -> UpdateSettings,
    /// The value in what the backend saved.
    read: fn(&AppSettings) -> f64,
) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let version = expect_context::<DataVersion>();
    let last = StoredValue::new(stored);
    let text = RwSignal::new(stored.to_string());
    let commit = move |raw: String| {
        let back = move || text.set(last.get_value().to_string());
        let n = match raw.trim().parse::<f64>() {
            Ok(n) if n.is_finite() && (!whole || (n >= 0.0 && n.fract() == 0.0)) => n,
            _ => {
                toasts.error(&AppError {
                    code: "invalid".into(),
                    message: invalid.into(),
                });
                back();
                return;
            }
        };
        if n == last.get_value() {
            text.set(n.to_string());
            return;
        }
        spawn_local(async move {
            match api::update_settings(patch(n)).await {
                Ok(saved) => {
                    let value = read(&saved);
                    last.set_value(value);
                    text.set(value.to_string());
                    version.bump();
                }
                Err(e) => {
                    toasts.error(&e);
                    back();
                }
            }
        });
    };
    view! {
        <label class="mb-2 block max-w-xs">
            <span class="mb-0.5 block text-[11px] text-muted">{label}</span>
            <input class=INPUT type="number" prop:value=move || text.get()
                on:input=move |ev| text.set(event_target_value(&ev))
                on:change=move |ev| commit(event_target_value(&ev)) />
        </label>
    }
}

/// How the work week reads in the picker's summary: `Mon-Fri · 5 working days a week`.
pub fn work_week_text(week: WorkWeek) -> String {
    let n = week.days_per_week();
    format!(
        "{} · {n} working day{} a week",
        week.summary(),
        if n == 1 { "" } else { "s" }
    )
}

/// Seven day buttons; each click saves at once. At least one day stays on.
#[component]
fn WorkWeekPicker(current: WorkWeek) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let version = expect_context::<DataVersion>();
    let week = RwSignal::new(current);
    let toggle = move |day: u8| {
        let before = week.get_untracked();
        let Some(next) = before.with(day, !before.contains(day)) else {
            toasts.error(&AppError {
                code: "invalid".into(),
                message: "Keep at least one working day".into(),
            });
            return;
        };
        week.set(next);
        spawn_local(async move {
            let patch = UpdateSettings {
                work_week: Some(next),
                ..Default::default()
            };
            match api::update_settings(patch).await {
                Ok(saved) => {
                    week.set(saved.work_week);
                    version.bump();
                }
                Err(e) => {
                    week.set(before);
                    toasts.error(&e);
                }
            }
        });
    };
    let buttons = (0..7u8)
        .map(|day| {
            let name = WEEKDAY_NAMES[day as usize];
            view! {
                <button
                    class=move || format!("{BUTTON} w-12 {}", if week.get().contains(day) { BUTTON_ON } else { "" })
                    aria-pressed=move || week.get().contains(day).to_string()
                    title=name
                    on:click=move |_| toggle(day)>
                    {&name[..3]}
                </button>
            }
        })
        .collect_view();
    view! {
        <div>
            <span class="mb-0.5 block text-[11px] text-muted">"Working days"</span>
            <div class="flex flex-wrap gap-1" role="group" aria-label="Working days">{buttons}</div>
            <p class="mt-1 text-[11px] text-muted">{move || work_week_text(week.get())}</p>
        </div>
    }
}

/// How time is counted: hours in a day, which days are worked, a new person's weekly hours.
#[component]
fn TimeSettings(settings: Loaded) -> impl IntoView {
    let body = when_loaded(settings, |s| {
        view! {
            <NumberSetting label="Hours per working day" stored=s.hours_per_day whole=false
                invalid="Hours per day must be a number"
                patch=|n| UpdateSettings { hours_per_day: Some(n), ..Default::default() }
                read=|s| s.hours_per_day />
            <p class="text-[11px] text-muted">
                "Used to turn estimates like 4h into days (4h = 0.5 days at 8). "
                "Estimates you've already entered keep their value; only new ones use the new setting."
            </p>
            <WorkWeekPicker current=s.work_week />
            <p class="text-[11px] text-muted">
                "The schedule, capacity and the weekly review count only these days; a task of three "
                "days takes three of them. Weeks still run Monday to Sunday on This week and the review."
            </p>
            <NumberSetting label="Default weekly capacity (hours)" stored=s.default_weekly_capacity_hours whole=false
                invalid="Weekly capacity must be a number of hours"
                patch=|n| UpdateSettings { default_weekly_capacity_hours: Some(n), ..Default::default() }
                read=|s| s.default_weekly_capacity_hours />
            <p class="text-[11px] text-muted">
                "What a new person gets when you don't say. Capacity is hours a week divided by hours "
                "per working day, so 32 hours is 4 days at 8. People you've already added keep their own."
            </p>
        }
        .into_any()
    });
    view! {
        <Card title="Time" description="How estimates, working days and capacity are measured.">{body}</Card>
    }
}

/// When an open waiting-on counts as stale.
#[component]
fn WaitingSettings(settings: Loaded) -> impl IntoView {
    let body = when_loaded(settings, |s| {
        view! {
            <NumberSetting label="Stale after (days)" stored=f64::from(s.stale_waiting_days) whole=true
                invalid="Stale after must be a whole number of days"
                patch=|n| UpdateSettings { stale_waiting_days: Some(n as u32), ..Default::default() }
                read=|s| f64::from(s.stale_waiting_days) />
            <p class="text-[11px] text-muted">
                "An open waiting-on is stale once it is older than this, or past its expected date. "
                "Snoozed ones are not stale until they resurface."
            </p>
        }
        .into_any()
    });
    view! { <Card title="Waiting on" description="When an open waiting-on counts as stale.">{body}</Card> }
}

/// When someone has too much on, whatever the estimates say.
#[component]
fn CapacitySettings(settings: Loaded) -> impl IntoView {
    let body = when_loaded(settings, |s| {
        view! {
            <NumberSetting label="Too many open tasks above" stored=f64::from(s.capacity_task_limit) whole=true
                invalid="The open-task limit must be a whole number"
                patch=|n| UpdateSettings { capacity_task_limit: Some(n as u32), ..Default::default() }
                read=|s| f64::from(s.capacity_task_limit) />
            <p class="text-[11px] text-muted">
                "A person with more open tasks than this is flagged on the Capacity screen and the Overview. "
                "It works even when estimates are missing. Weekly load uses each person's weekly hours and "
                "the hours per working day."
            </p>
        }
        .into_any()
    });
    view! { <Card title="Capacity" description="When someone has too much on, whatever the estimates say.">{body}</Card> }
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
        <Card title="Appearance" description="Colour theme. Dark is the default; System follows your OS.">
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
        </Card>
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
                    <span class="flex h-1 gap-0.5">
                        <span class="w-1/4" style=format!("background:{}", k.accent)></span>
                        <span class="w-1/4" style=format!("background:{}", k.success)></span>
                        <span class="w-1/4" style=format!("background:{}", k.warning)></span>
                        <span class="w-1/4" style=format!("background:{}", k.danger)></span>
                    </span>
                </span>
            </span>
            <span class="mt-1.5 block truncate">{theme.name}</span>
        </button>
    }
}

/// When a signal turns a project's health amber or red.
#[component]
fn HealthSettings() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let settings = LocalResource::new(move || {
        version.track();
        api::get_settings()
    });
    let save = move |new: HealthThresholds| {
        spawn_local(async move {
            finish(
                api::update_settings(UpdateSettings {
                    health: Some(new),
                    ..Default::default()
                })
                .await,
                toasts,
                version,
            );
        });
    };
    view! {
        <Card title="Project health" description="When a project turns amber or red on the Overview.">
            {move || match settings.get() {
                Some(Ok(s)) => view! { <HealthRows current=s.health on_save=save /> }.into_any(),
                Some(Err(_)) => view! { <p class="text-muted">"Couldn't load settings."</p> }.into_any(),
                None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
            }}
        </Card>
    }
}

#[component]
fn NumberCell(value: u32, #[prop(into)] on_commit: Callback<String>) -> impl IntoView {
    view! {
        <input type="number" min="1" class=format!("{COMPACT_INPUT} w-20")
               prop:value=value.to_string()
               on:change=move |ev| on_commit.run(event_target_value(&ev)) />
    }
}

#[component]
fn HealthRows(
    current: HealthThresholds,
    #[prop(into)] on_save: Callback<HealthThresholds>,
) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let version = expect_context::<DataVersion>();
    // One edited number: save it, or say it isn't a number and put the stored value back.
    let edit = move |apply: fn(&mut HealthThresholds, u32)| {
        move |v: String| match v.trim().parse::<u32>() {
            Ok(n) => {
                let mut t = current;
                apply(&mut t, n);
                on_save.run(t);
            }
            Err(_) => {
                toasts.error(&AppError {
                    code: "invalid".into(),
                    message: "Thresholds are whole numbers".into(),
                });
                version.bump();
            }
        }
    };
    let late_amber = edit(|t, n| t.late_amber_days = n);
    let late_red = edit(|t, n| t.late_red_days = n);
    let risky_amber = edit(|t, n| t.risky_amber_pct = n);
    let risky_red = edit(|t, n| t.risky_red_pct = n);
    let unest_amber = edit(|t, n| t.unestimated_amber_pct = n);
    let unest_red = edit(|t, n| t.unestimated_red_pct = n);
    let reset = move |_| on_save.run(HealthThresholds::default());
    view! {
        <div class="grid max-w-xl grid-cols-[1fr_5rem_5rem] items-center gap-x-3 gap-y-1">
            <span></span>
            <span class="text-[11px] text-muted">"Amber at"</span>
            <span class="text-[11px] text-muted">"Red at"</span>
            <span>"Projected late (working days past the target)"</span>
            <NumberCell value=current.late_amber_days on_commit=late_amber />
            <NumberCell value=current.late_red_days on_commit=late_red />
            <span>"Blocked or overdue (% of open tasks)"</span>
            <NumberCell value=current.risky_amber_pct on_commit=risky_amber />
            <NumberCell value=current.risky_red_pct on_commit=risky_red />
            <span>"Without an estimate (% of open tasks)"</span>
            <NumberCell value=current.unestimated_amber_pct on_commit=unest_amber />
            <NumberCell value=current.unestimated_red_pct on_commit=unest_red />
        </div>
        <p class="text-[11px] text-muted">
            "A project is as healthy as its worst signal. Amber can't be above red. "
            "Used by the Overview, the project and objective panels, and the risk ranking."
        </p>
        <button class=BUTTON on:click=reset>"Reset to defaults"</button>
    }
}

/// The Markdown template of the weekly status report.
#[component]
fn ReportSettings() -> impl IntoView {
    // Loaded once: other settings changing must not replace what is being typed here.
    let settings = LocalResource::new(api::get_settings);
    view! {
        <Card title="Status report"
              description="The template behind the weekly review's report. Write Markdown and place {{sections}} where they should go.">
            {move || match settings.get() {
                Some(Ok(s)) => view! { <TemplateEditor stored=s.report_template /> }.into_any(),
                Some(Err(_)) => view! { <p class="text-muted">"Couldn't load settings."</p> }.into_any(),
                None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
            }}
        </Card>
    }
}

#[component]
fn TemplateEditor(stored: String) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let saved = RwSignal::new(stored.clone());
    let text = RwSignal::new(stored);
    let dirty = move || text.get() != saved.get();
    // A refused template keeps what was typed so it can be fixed.
    let send = move |template: String| {
        spawn_local(async move {
            let patch = UpdateSettings {
                report_template: Some(template),
                ..Default::default()
            };
            match api::update_settings(patch).await {
                Ok(s) => {
                    saved.set(s.report_template.clone());
                    text.set(s.report_template);
                    toasts.info("Saved the report template");
                }
                Err(e) => toasts.error(&e),
            }
        });
    };
    let save = move |_| send(text.get_untracked());
    let reset = move |_| send(String::new());
    let names = REPORT_PLACEHOLDERS
        .iter()
        .map(|(name, what)| {
            view! {
                <code class="font-mono text-accent">{format!("{{{{{name}}}}}")}</code>
                <span class="text-muted">{*what}</span>
            }
        })
        .collect_view();
    view! {
        <textarea rows="20" spellcheck="false" aria-label="Status report template"
            class=format!("{COMPACT_INPUT} w-full resize-y p-2 font-mono leading-5")
            prop:value=move || text.get()
            on:input=move |ev| text.set(event_target_value(&ev)) ></textarea>
        <div class="flex items-center gap-2">
            <button class=BUTTON_PRIMARY disabled=move || !dirty() on:click=save>"Save template"</button>
            <button class=BUTTON title="Go back to the built-in template" on:click=reset>"Reset to default"</button>
        </div>
        <p class="text-[11px] text-muted">
            "Each section placeholder fills in that part of the report without a heading, so the "
            "template sets the headings, their order and which sections appear. The report is written "
            "for a board or executive audience."
        </p>
        <div class="grid max-w-xl grid-cols-[10rem_1fr] gap-x-3 gap-y-0.5 text-[11px]">{names}</div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_work_week_reads_as_days_and_a_count() {
        assert_eq!(
            work_week_text(WorkWeek::default()),
            "Mon-Fri · 5 working days a week"
        );
        assert_eq!(
            work_week_text(WorkWeek::from_days(&[2]).unwrap()),
            "Wednesday · 1 working day a week"
        );
        assert_eq!(
            work_week_text(WorkWeek::from_days(&[0, 1, 2, 3]).unwrap()),
            "Mon-Thu · 4 working days a week"
        );
    }
}
