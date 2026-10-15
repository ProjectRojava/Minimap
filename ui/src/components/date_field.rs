//! A date input that follows the theme: a text box for `YYYY-MM-DD` (no locale-dependent
//! `mm/dd/yyyy`) plus a calendar pop-up. The webview's own date picker is drawn by the OS
//! toolkit and ignores page colours.

use leptos::{ev, html, prelude::*};

use super::{
    form::INPUT,
    overlay::{anchor_of, place, viewport},
};
use crate::calendar::{format_ymd, month_grid, parse_ymd, shift_month, MONTHS, WEEKDAYS};

const CALENDAR_W: f64 = 252.0;
const CALENDAR_H: f64 = 318.0;

/// Where a day sits in the week (the grid starts on Monday): Saturday and Sunday are the last two
/// columns. Saturday is tinted amber and Sunday red, so a weekend reads at a glance and the two
/// days are told apart.
fn is_saturday(col: usize) -> bool {
    col % 7 == 5
}

fn is_sunday(col: usize) -> bool {
    col % 7 == 6
}

/// The heading of a weekday column.
fn weekday_class(col: usize) -> &'static str {
    if is_saturday(col) {
        "py-1 font-semibold text-warning"
    } else if is_sunday(col) {
        "py-1 font-semibold text-danger"
    } else {
        "py-1"
    }
}

/// One day of the month grid. The chosen day is filled with the accent, today is ringed in it,
/// and weekends keep their tint (fainter on the days of the neighbouring months). Days before
/// today are dulled, and come back to full strength under the pointer.
fn day_class(col: usize, in_month: bool, chosen: bool, today: bool, past: bool) -> String {
    let look = if chosen {
        "bg-accent font-semibold text-canvas shadow-sm"
    } else if is_saturday(col) {
        if in_month {
            "bg-warning/10 text-warning hover:bg-warning/25"
        } else {
            "bg-warning/5 text-warning/40 hover:bg-warning/20"
        }
    } else if is_sunday(col) {
        if in_month {
            "bg-danger/10 text-danger hover:bg-danger/25"
        } else {
            "bg-danger/5 text-danger/40 hover:bg-danger/20"
        }
    } else if in_month {
        "text-fg hover:bg-hover"
    } else {
        "text-faint hover:bg-hover"
    };
    let ring = if today && !chosen {
        "font-semibold ring-1 ring-inset ring-accent"
    } else {
        ""
    };
    let dull = if past && !chosen {
        "opacity-40 hover:opacity-100"
    } else {
        ""
    };
    format!("h-8 rounded-md text-center tabular-nums transition-colors {look} {ring} {dull}")
}

/// Today's date in the user's local time zone.
pub(crate) fn today_ymd() -> (i32, u32, u32) {
    let now = js_sys::Date::new_0();
    (
        now.get_full_year() as i32,
        now.get_month() + 1,
        now.get_date(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saturday_and_sunday_get_their_own_colours() {
        // The grid starts on Monday, so columns 5 and 6 are the weekend, in every row.
        for row in 0..6 {
            let (sat, sun) = (row * 7 + 5, row * 7 + 6);
            assert!(day_class(sat, true, false, false, false).contains("text-warning"));
            assert!(day_class(sun, true, false, false, false).contains("text-danger"));
            assert_eq!(weekday_class(sat), "py-1 font-semibold text-warning");
            assert_eq!(weekday_class(sun), "py-1 font-semibold text-danger");
            for weekday in 0..5 {
                let class = day_class(row * 7 + weekday, true, false, false, false);
                assert!(!class.contains("warning") && !class.contains("danger"));
            }
        }
    }

    #[test]
    fn the_chosen_day_and_today_stand_out_even_on_a_weekend() {
        let chosen = day_class(5, true, true, true, false);
        assert!(chosen.contains("bg-accent") && !chosen.contains("bg-warning"));
        let today = day_class(6, true, false, true, false);
        assert!(today.contains("ring-accent") && today.contains("text-danger"));
        assert!(!day_class(2, true, false, false, false).contains("ring-accent"));
    }

    #[test]
    fn days_before_today_are_dulled_but_not_the_chosen_one() {
        assert!(day_class(2, true, false, false, true).contains("opacity-40"));
        // A past weekend keeps its colour, only dimmer.
        let sat = day_class(5, true, false, false, true);
        assert!(sat.contains("opacity-40") && sat.contains("text-warning"));
        assert!(!day_class(2, true, false, false, false).contains("opacity-40"));
        assert!(!day_class(2, true, false, true, false).contains("opacity-40"));
        // Someone who picked a past date still sees it clearly.
        assert!(!day_class(2, true, true, false, true).contains("opacity-40"));
    }

    #[test]
    fn days_of_other_months_are_dimmer() {
        assert!(day_class(5, false, false, false, false).contains("text-warning/40"));
        assert!(day_class(2, false, false, false, false).contains("text-faint"));
    }
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
                    class="fixed z-[71] rounded-lg border border-line-strong bg-panel p-3 text-[12px] text-fg shadow-xl"
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
                    <div class="mb-2 flex items-center justify-between">
                        <button type="button"
                                class="flex h-7 w-7 items-center justify-center rounded-md text-[15px] text-muted hover:bg-hover hover:text-fg"
                                aria-label="Previous month"
                                on:click=move |_| month.update(|(y, m, _)| { let (ny, nm) = shift_month((*y, *m), -1); *y = ny; *m = nm; })>
                            "‹"
                        </button>
                        <span class="text-[13px] font-semibold tracking-wide">
                            {move || { let (y, m, _) = month.get(); format!("{} {y}", MONTHS[(m as usize - 1).min(11)]) }}
                        </span>
                        <button type="button"
                                class="flex h-7 w-7 items-center justify-center rounded-md text-[15px] text-muted hover:bg-hover hover:text-fg"
                                aria-label="Next month"
                                on:click=move |_| month.update(|(y, m, _)| { let (ny, nm) = shift_month((*y, *m), 1); *y = ny; *m = nm; })>
                            "›"
                        </button>
                    </div>
                    <div class="grid grid-cols-7 gap-0.5 border-b border-line pb-1 text-center text-[10px] uppercase tracking-wider text-muted">
                        {WEEKDAYS.iter().enumerate().map(|(i, d)| view! { <span class=weekday_class(i)>{*d}</span> }).collect_view()}
                    </div>
                    <div class="mt-1 grid grid-cols-7 gap-0.5">
                        {move || {
                            let (y, m, _) = month.get();
                            let chosen = parse_ymd(&text.get());
                            let today = today_ymd();
                            month_grid(y, m).into_iter().enumerate().map(|(i, c)| {
                                let key = (c.y, c.m, c.d);
                                // (year, month, day) sorts in calendar order.
                                let class = day_class(i, c.in_month, Some(key) == chosen, key == today, key < today);
                                view! {
                                    <button type="button" class=class on:click=move |_| pick(c.y, c.m, c.d)>{c.d}</button>
                                }
                            }).collect_view()
                        }}
                    </div>
                    <div class="mt-2 flex items-center justify-between border-t border-line pt-2">
                        <button type="button" class="rounded-md px-2 py-1 font-medium text-accent hover:bg-accent/10"
                                on:click=move |_| { let (y, m, d) = today_ymd(); pick(y, m, d); }>
                            "Today"
                        </button>
                        <span class="flex items-center gap-2 text-[10px] text-muted">
                            <span class="flex items-center gap-1"><span class="h-2 w-2 rounded-full bg-warning"></span>"Sat"</span>
                            <span class="flex items-center gap-1"><span class="h-2 w-2 rounded-full bg-danger"></span>"Sun"</span>
                        </span>
                        <button type="button" class="rounded-md px-2 py-1 text-muted hover:bg-hover hover:text-fg"
                                on:click=move |_| { open.set(false); commit(String::new()); }>
                            "Clear"
                        </button>
                    </div>
                </div>
            </Show>
        </div>
    }
}
