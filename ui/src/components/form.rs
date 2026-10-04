//! Small form controls shared by the detail panels and "new" forms.

use leptos::prelude::*;
use minimap_types::{timefmt::parse_date, AppError, Date, Patch};

pub const INPUT: &str =
    "w-full rounded-sm border border-line bg-canvas px-2 py-1 text-[13px] text-fg \
    focus:outline-none focus:border-accent";
pub const COMPACT_INPUT: &str =
    "rounded-sm border border-line bg-canvas px-1 py-0.5 text-[12px] text-fg \
    focus:outline-none focus:border-accent";
/// The default button: outlined grey, warming to the accent on hover.
pub const BUTTON: &str = "rounded-sm border border-line px-2 py-0.5 text-[12px] text-fg \
    hover:border-accent/50 hover:bg-accent/10 hover:text-accent \
    disabled:opacity-40 disabled:hover:border-line disabled:hover:bg-transparent disabled:hover:text-fg";
/// The main action of a form or dialog: filled with the accent.
pub const BUTTON_PRIMARY: &str =
    "rounded-sm border border-accent bg-accent px-2 py-0.5 text-[12px] text-canvas \
    hover:opacity-85 disabled:opacity-40";
/// A page's "new" action: the accent as a soft tint.
pub const BUTTON_SOFT: &str =
    "rounded-sm border border-accent/40 bg-accent/10 px-2 py-0.5 text-[12px] text-accent \
    hover:bg-accent/20 disabled:opacity-40";
/// Completing something: resolve, done.
pub const BUTTON_SUCCESS: &str =
    "rounded-sm border border-success/40 bg-success/10 px-2 py-0.5 text-[12px] text-success \
    hover:bg-success/20 disabled:opacity-40";
/// Destructive: archive, delete.
pub const BUTTON_DANGER: &str =
    "rounded-sm border border-danger/40 bg-danger/10 px-2 py-0.5 text-[12px] text-danger \
    hover:bg-danger/20 disabled:opacity-40";
/// Extra classes for the selected button of a toggle group (add to `BUTTON`).
pub const BUTTON_ON: &str = "border-accent/50 bg-accent/15 text-accent";

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
    if kind == "date" {
        // The webview's own date control ignores the theme; use ours.
        return view! {
            <div class="mb-2"><DateField label=label current=value on_commit=on_commit placeholder=placeholder /></div>
        }
        .into_any();
    }
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
    .into_any()
}

pub use super::{date_field::DateField, select::SelectField};

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
