//! Small form controls shared by the detail panels and "new" forms.

use leptos::prelude::*;
use minimap_types::{timefmt::parse_date, AppError, Date, Patch};

pub const INPUT: &str =
    "w-full rounded-sm border border-line bg-canvas px-2 py-1 text-[13px] text-fg \
    focus:outline-none focus:border-line-strong";
pub const BUTTON: &str = "rounded-sm border border-line px-2 py-0.5 text-[12px] text-fg \
    hover:bg-hover disabled:opacity-40";
/// Monochrome "primary": inverted foreground/background instead of a brand colour.
pub const BUTTON_PRIMARY: &str =
    "rounded-sm border border-fg bg-fg px-2 py-0.5 text-[12px] text-canvas \
    hover:opacity-85 disabled:opacity-40";
pub const BUTTON_DANGER: &str =
    "rounded-sm border border-line px-2 py-0.5 text-[12px] text-danger \
    hover:bg-hover";

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
            <span class="block mb-0.5 text-[11px] text-muted">{label}</span>
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
        "rounded-sm border border-line bg-canvas px-1 py-0.5 text-[12px] text-fg focus:outline-none focus:border-line-strong"
    } else {
        INPUT
    };
    view! {
        <label class="block">
            {(!label.is_empty()).then(|| view! { <span class="block mb-0.5 text-[11px] text-muted">{label}</span> })}
            <select class=class on:change=move |ev| on_change.run(event_target_value(&ev))>
                {options.into_iter().map(|(v, l)| {
                    let selected = v == current;
                    view! { <option value=v selected=selected>{l}</option> }
                }).collect_view()}
            </select>
        </label>
    }
}

/// A date input's text as an update: empty clears the date, otherwise it must be `YYYY-MM-DD`.
pub fn date_patch(text: &str) -> Result<Patch<Date>, AppError> {
    match text.trim() {
        "" => Ok(Patch::Clear),
        t => parse_date(t).map(Patch::Set).map_err(|_| AppError {
            code: "invalid".into(),
            message: "Dates must look like 2027-03-31".into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_patches() {
        assert_eq!(date_patch("  ").unwrap(), Patch::Clear);
        assert_eq!(
            date_patch("2027-03-31").unwrap(),
            Patch::Set(parse_date("2027-03-31").unwrap())
        );
        assert_eq!(date_patch("31/03/2027").unwrap_err().code, "invalid");
    }
}
