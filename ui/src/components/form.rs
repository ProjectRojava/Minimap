//! Small form controls shared by the detail panels and "new" forms.

use leptos::prelude::*;

pub const INPUT: &str =
    "w-full rounded border border-zinc-300 dark:border-zinc-700 bg-transparent \
    px-2 py-1 text-[13px] focus:outline-none focus:ring-1 focus:ring-emerald-500";
pub const BUTTON: &str =
    "rounded border border-zinc-300 dark:border-zinc-700 px-2.5 py-1 text-[12px] \
    hover:bg-zinc-100 dark:hover:bg-zinc-800 disabled:opacity-40";
pub const BUTTON_PRIMARY: &str =
    "rounded bg-emerald-600 px-2.5 py-1 text-[12px] font-medium text-white \
    hover:bg-emerald-700 disabled:opacity-40";
pub const BUTTON_DANGER: &str =
    "rounded border border-red-300 dark:border-red-900 px-2.5 py-1 text-[12px] \
    text-red-700 dark:text-red-300 hover:bg-red-50 dark:hover:bg-red-950";

/// Text input that reports its value once, when it loses focus or Enter is pressed,
/// and only if it changed. The stored value is not pushed back into the box while typing.
#[component]
pub fn TextField(
    label: &'static str,
    value: String,
    #[prop(into)] on_commit: Callback<String>,
    #[prop(optional)] multiline: bool,
    #[prop(default = "text")] kind: &'static str,
    #[prop(optional)] placeholder: &'static str,
) -> impl IntoView {
    let last = StoredValue::new(value.clone());
    let commit = move |v: String| {
        if v != last.get_value() {
            last.set_value(v.clone());
            on_commit.run(v);
        }
    };
    view! {
        <label class="block mb-2">
            <span class="block mb-0.5 text-[11px] text-zinc-500">{label}</span>
            {if multiline {
                view! {
                    <textarea class=INPUT rows="4" placeholder=placeholder prop:value=value
                        on:change=move |ev| commit(event_target_value(&ev))></textarea>
                }.into_any()
            } else {
                view! {
                    <input class=INPUT type=kind placeholder=placeholder prop:value=value
                        on:change=move |ev| commit(event_target_value(&ev)) />
                }.into_any()
            }}
        </label>
    }
}

/// `<select>` over `(value, label)` options. Reports the chosen value.
#[component]
pub fn SelectField(
    options: Vec<(String, String)>,
    current: String,
    #[prop(into)] on_change: Callback<String>,
    #[prop(optional)] label: &'static str,
    #[prop(optional)] compact: bool,
) -> impl IntoView {
    let class = if compact {
        "rounded border border-zinc-300 dark:border-zinc-700 bg-transparent px-1 py-0.5 text-[12px]"
    } else {
        INPUT
    };
    view! {
        <label class="block">
            {(!label.is_empty()).then(|| view! { <span class="block mb-0.5 text-[11px] text-zinc-500">{label}</span> })}
            <select class=class on:change=move |ev| on_change.run(event_target_value(&ev))>
                {options.into_iter().map(|(v, l)| {
                    let selected = v == current;
                    view! { <option value=v selected=selected>{l}</option> }
                }).collect_view()}
            </select>
        </label>
    }
}
