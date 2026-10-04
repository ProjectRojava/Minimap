//! A date input that follows the theme: a text box for `YYYY-MM-DD` (no locale-dependent
//! `mm/dd/yyyy`) plus a calendar pop-up. The webview's own date picker is drawn by the OS
//! toolkit and ignores page colours.

use leptos::{ev, html, prelude::*};

use super::{
    form::INPUT,
    overlay::{anchor_of, place, viewport},
};
use crate::calendar::{format_ymd, month_grid, parse_ymd, shift_month, MONTHS, WEEKDAYS};

const CALENDAR_W: f64 = 232.0;
const CALENDAR_H: f64 = 262.0;

/// Today's date in the user's local time zone.
fn today_ymd() -> (i32, u32, u32) {
    let now = js_sys::Date::new_0();
    (
        now.get_full_year() as i32,
        now.get_month() + 1,
        now.get_date(),
    )
}

/// `current` is the starting text (`YYYY-MM-DD`, or empty). `on_commit` gets the text when it
/// changes: after typing (on blur or Enter) or after choosing a day, Today or Clear. The
/// receiver validates what was typed (an invalid date is reported there).
#[component]
pub fn DateField(
    current: String,
    #[prop(into)] on_commit: Callback<String>,
    #[prop(optional)] label: &'static str,
    #[prop(optional)] compact: bool,
    #[prop(optional)] placeholder: &'static str,
    /// DOM id of the text box, so keyboard shortcuts can focus it.
    #[prop(optional, into)]
    id: Option<String>,
) -> impl IntoView {
    let text = RwSignal::new(current.clone());
    let last = StoredValue::new(current);
    let open = RwSignal::new(false);
    let month = RwSignal::new(today_ymd());
    let pos = RwSignal::new((0.0_f64, 0.0_f64));
    let wrapper = NodeRef::<html::Div>::new();

    let commit = move |value: String| {
        text.set(value.clone());
        if value != last.get_value() {
            last.set_value(value.clone());
            on_commit.run(value);
        }
    };

    let open_calendar = move || {
        let Some(el) = wrapper.get() else {
            return;
        };
        let a = anchor_of(&el);
        pos.set(place(&a, CALENDAR_W, CALENDAR_H, viewport()));
        // Start on the month of the date in the box, else this month.
        let (y, m, _) = parse_ymd(&text.get_untracked()).unwrap_or_else(today_ymd);
        month.set((y, m, 1));
        open.set(true);
    };

    let pick = move |y: i32, m: u32, d: u32| {
        open.set(false);
        commit(format_ymd(y, m, d));
    };

    let on_keydown = move |ev: ev::KeyboardEvent| match ev.key().as_str() {
        "Enter" => {
            ev.prevent_default();
            commit(text.get_untracked());
        }
        "Escape" if open.get_untracked() => {
            ev.prevent_default();
            ev.stop_propagation();
            open.set(false);
        }
        "ArrowDown" if !open.get_untracked() => {
            ev.prevent_default();
            open_calendar();
        }
        _ => {}
    };

    let input_class = if compact {
        "w-[6.75rem] rounded-sm border border-line bg-canvas px-1.5 py-0.5 text-[12px] tabular-nums text-fg \
         focus:outline-none focus:border-accent"
    } else {
        INPUT
    };
    let button_class = "shrink-0 rounded-sm border border-line bg-canvas px-1.5 text-[12px] text-muted \
                        hover:text-fg hover:border-line-strong focus:outline-none focus:border-accent";
    let block = if compact { "inline-block" } else { "block" };

    view! {
        <div class=block>
            {(!label.is_empty()).then(|| view! { <span class="block mb-0.5 text-[11px] text-muted">{label}</span> })}
            <div node_ref=wrapper class="flex items-stretch gap-1">
                <input
                    id=id
                    class=input_class
                    type="text"
                    inputmode="numeric"
                    placeholder=if placeholder.is_empty() { "YYYY-MM-DD" } else { placeholder }
                    prop:value=move || text.get()
                    on:input=move |ev| text.set(event_target_value(&ev))
                    on:change=move |ev| commit(event_target_value(&ev))
                    on:keydown=on_keydown
                />
                <button type="button" class=button_class aria-label="Open calendar" title="Calendar"
                        on:click=move |_| if open.get_untracked() { open.set(false) } else { open_calendar() }>
                    "▦"
                </button>
            </div>
            <Show when=move || open.get()>
                <div class="fixed inset-0 z-[70]" on:mousedown=move |_| open.set(false)></div>
                <div
                    class="fixed z-[71] rounded-sm border border-line bg-panel p-2 text-[12px] text-fg"
                    style=move || {
                        let (left, top) = pos.get();
                        format!("left:{left}px; top:{top}px; width:{CALENDAR_W}px")
                    }
                    on:keydown=move |ev: ev::KeyboardEvent| if ev.key() == "Escape" {
                        ev.prevent_default();
                        ev.stop_propagation();
                        open.set(false);
                    }
                >
                    <div class="mb-1 flex items-center justify-between">
                        <button type="button" class="px-2 text-muted hover:text-fg" aria-label="Previous month"
                                on:click=move |_| month.update(|(y, m, _)| { let (ny, nm) = shift_month((*y, *m), -1); *y = ny; *m = nm; })>
                            "‹"
                        </button>
                        <span class="font-medium">
                            {move || { let (y, m, _) = month.get(); format!("{} {y}", MONTHS[(m as usize - 1).min(11)]) }}
                        </span>
                        <button type="button" class="px-2 text-muted hover:text-fg" aria-label="Next month"
                                on:click=move |_| month.update(|(y, m, _)| { let (ny, nm) = shift_month((*y, *m), 1); *y = ny; *m = nm; })>
                            "›"
                        </button>
                    </div>
                    <div class="grid grid-cols-7 text-center text-[11px] text-muted">
                        {WEEKDAYS.iter().map(|d| view! { <span class="py-0.5">{*d}</span> }).collect_view()}
                    </div>
                    <div class="grid grid-cols-7 gap-px">
                        {move || {
                            let (y, m, _) = month.get();
                            let chosen = parse_ymd(&text.get());
                            let today = today_ymd();
                            month_grid(y, m).into_iter().map(|c| {
                                let key = (c.y, c.m, c.d);
                                let class = format!(
                                    "h-7 rounded-sm text-center tabular-nums {} {}",
                                    if Some(key) == chosen { "bg-accent text-canvas" }
                                    else if c.in_month { "text-fg hover:bg-hover" }
                                    else { "text-faint hover:bg-hover" },
                                    if key == today && Some(key) != chosen { "border border-line-strong" } else { "" });
                                view! {
                                    <button type="button" class=class on:click=move |_| pick(c.y, c.m, c.d)>{c.d}</button>
                                }
                            }).collect_view()
                        }}
                    </div>
                    <div class="mt-2 flex justify-between">
                        <button type="button" class="px-1 text-muted hover:text-fg"
                                on:click=move |_| { let (y, m, d) = today_ymd(); pick(y, m, d); }>
                            "Today"
                        </button>
                        <button type="button" class="px-1 text-muted hover:text-fg"
                                on:click=move |_| { open.set(false); commit(String::new()); }>
                            "Clear"
                        </button>
                    </div>
                </div>
            </Show>
        </div>
    }
}
