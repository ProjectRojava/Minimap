//! Shared page chrome: the header, key hints, filter bar, column headings, group labels, empty
//! states and chips every list screen uses, so the screens look and behave alike.
//! Flat, theme tokens only (see `docs/design.md`).

use leptos::prelude::*;

use crate::icons;

/// A 16px line icon in the current text colour (path data in `icons.rs`).
#[component]
pub fn Icon(
    name: &'static str,
    /// Tailwind size classes.
    #[prop(default = "h-4 w-4")]
    size: &'static str,
) -> impl IntoView {
    let paths = icons::paths(name).unwrap_or(&[]);
    view! {
        <svg class=format!("{size} shrink-0") viewBox="0 0 16 16" fill="none"
             stroke="currentColor" stroke-width="1.4" stroke-linecap="round"
             stroke-linejoin="round" aria-hidden="true">
            {paths.iter().map(|d| view! { <path d=*d /> }).collect_view()}
        </svg>
    }
}

/// What a pill or status says, in colour: where something stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// Nothing special: planned, general, cancelled.
    Neutral,
    /// Under way or notable: active, in progress, proposed, 1:1.
    Accent,
    /// Good: on track, done, decided, resolved.
    Success,
    /// Needs attention: at risk, blocked, paused, stale.
    Warning,
    /// Bad: off track, overdue.
    Danger,
}

impl Tone {
    /// A tinted pill: border, soft background and text in the tone's colour.
    pub fn chip(self) -> &'static str {
        match self {
            Tone::Neutral => CHIP,
            Tone::Accent => "inline-block rounded-sm border border-accent/40 bg-accent/10 px-1.5 text-[10px] leading-4 uppercase tracking-wide text-accent",
            Tone::Success => "inline-block rounded-sm border border-success/40 bg-success/10 px-1.5 text-[10px] leading-4 uppercase tracking-wide text-success",
            Tone::Warning => "inline-block rounded-sm border border-warning/40 bg-warning/10 px-1.5 text-[10px] leading-4 uppercase tracking-wide text-warning",
            Tone::Danger => "inline-block rounded-sm border border-danger/40 bg-danger/10 px-1.5 text-[10px] leading-4 uppercase tracking-wide text-danger",
        }
    }

    /// A dropdown button or box that wears the tone: tinted background, border and text. The
    /// neutral tone is the plain look of a text input.
    pub fn field(self) -> &'static str {
        match self {
            Tone::Neutral => "border-line bg-canvas text-fg",
            Tone::Accent => "border-accent/40 bg-accent/10 text-accent",
            Tone::Success => "border-success/40 bg-success/10 text-success",
            Tone::Warning => "border-warning/40 bg-warning/10 text-warning",
            Tone::Danger => "border-danger/40 bg-danger/10 text-danger",
        }
    }

    /// A small filled dot in the tone (a heading's marker).
    pub fn dot(self) -> &'static str {
        match self {
            Tone::Neutral => "bg-faint",
            Tone::Accent => "bg-accent",
            Tone::Success => "bg-success",
            Tone::Warning => "bg-warning",
            Tone::Danger => "bg-danger",
        }
    }

    /// Just the text colour (for dropdown buttons and inline words).
    pub fn text(self) -> &'static str {
        match self {
            Tone::Neutral => "text-fg",
            Tone::Accent => "text-accent",
            Tone::Success => "text-success",
            Tone::Warning => "text-warning",
            Tone::Danger => "text-danger",
        }
    }
}

/// Small label used for statuses and kinds in lists (the neutral pill).
pub const CHIP: &str = "inline-block rounded-sm border border-line px-1.5 text-[10px] leading-4 \
                        uppercase tracking-wide text-muted";

/// A pill for what matters most or needs attention: the warning tone.
pub const CHIP_STRONG: &str =
    "inline-block rounded-sm border border-warning/40 bg-warning/10 px-1.5 \
                               text-[10px] leading-4 uppercase tracking-wide text-warning";

/// The bar of filters under a page header.
pub const FILTER_BAR: &str =
    "flex flex-wrap items-center gap-2 px-4 py-2 shrink-0 border-b border-line";

/// The "new item" form strip under the header or filters.
pub const FORM_BAR: &str = "flex items-end gap-2 px-4 py-2 shrink-0 border-b border-line bg-panel";

/// Class for a row of column headings above a grid list; `cols` is the list's grid class.
pub fn column_head(cols: &str) -> String {
    format!(
        "{cols} px-4 h-7 shrink-0 border-b border-line bg-panel \
         text-[10px] font-semibold uppercase tracking-wider text-muted"
    )
}

/// Page title bar: an icon tile, the title and a one-line description, then the page's own
/// controls (pass buttons as children; give one `ml-auto` to push it right).
#[component]
pub fn PageHeader(
    icon: &'static str,
    title: &'static str,
    #[prop(optional)] subtitle: &'static str,
    children: Children,
) -> impl IntoView {
    view! {
        <header class="flex h-12 shrink-0 items-center gap-3 border-b border-line bg-panel px-4">
            <span class="flex h-7 w-7 shrink-0 items-center justify-center rounded-sm border border-accent/30 bg-accent/10 text-accent">
                <Icon name=icon />
            </span>
            <div class="min-w-0 shrink-0">
                <h1 class="text-[14px] font-semibold leading-4">{title}</h1>
                {(!subtitle.is_empty()).then(|| view! {
                    <p class="hidden truncate text-[11px] leading-4 text-muted lg:block">{subtitle}</p>
                })}
            </div>
            <div class="flex min-w-0 flex-1 items-center gap-2 pl-2">{children()}</div>
        </header>
    }
}

/// Keyboard hints as key chips: `[("n", "new"), ("j/k", "move")]`. Hidden on narrow windows.
#[component]
pub fn Hints(keys: &'static [(&'static str, &'static str)]) -> impl IntoView {
    view! {
        <span class="ml-auto hidden shrink-0 items-center gap-3 text-[11px] text-muted xl:flex">
            {keys.iter().map(|(k, what)| view! {
                <span class="flex items-center gap-1">
                    <kbd class="rounded-sm border border-line px-1 font-mono text-[10px] leading-4 text-faint">{*k}</kbd>
                    {*what}
                </span>
            }).collect_view()}
        </span>
    }
}

/// "Quarter 1 2027  3": a group heading inside a list.
#[component]
pub fn GroupLabel(
    #[prop(into)] label: String,
    count: usize,
    /// Shows the objective as its coloured chip instead of the plain label.
    #[prop(optional)]
    objective: Option<minimap_types::NodeSummary>,
) -> impl IntoView {
    view! {
        <div class="flex items-center gap-2 px-4 pt-4 pb-1 text-[10px] font-semibold uppercase tracking-wider text-muted">
            {match objective {
                Some(o) => view! { <crate::components::objective_colour::ObjectiveChip objective=o /> }.into_any(),
                None => label.into_any(),
            }}
            <span class="rounded-sm border border-line px-1 font-normal leading-4 text-faint">{count}</span>
        </div>
    }
}

/// Nothing to show: an icon, what is missing and what to do about it.
#[component]
pub fn EmptyState(
    icon: &'static str,
    title: &'static str,
    #[prop(into)] hint: String,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    view! {
        <div class="mx-auto flex max-w-sm flex-col items-center gap-2 px-6 py-14 text-center">
            <span class="flex h-10 w-10 items-center justify-center rounded-sm border border-line bg-panel text-muted">
                <Icon name=icon size="h-5 w-5" />
            </span>
            <p class="text-[13px] font-medium">{title}</p>
            <p class="text-muted">{hint}</p>
            {children.map(|c| view! { <div class="mt-1">{c()}</div> })}
        </div>
    }
}

/// A settings-style block: a title, what it does, then the controls.
#[component]
pub fn Card(
    title: &'static str,
    #[prop(optional)] description: &'static str,
    children: Children,
) -> impl IntoView {
    view! {
        <section class="rounded-sm border border-line bg-panel">
            <div class="border-b border-line px-4 py-2.5">
                <h2 class="text-[13px] font-semibold">{title}</h2>
                {(!description.is_empty()).then(|| view! { <p class="text-[12px] text-muted">{description}</p> })}
            </div>
            <div class="space-y-2 p-4">{children()}</div>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_headings_reuse_the_lists_grid() {
        let c = column_head("grid grid-cols-[1fr_2fr] gap-3");
        assert!(c.starts_with("grid grid-cols-[1fr_2fr] gap-3 "));
        assert!(c.contains("uppercase") && c.contains("px-4"));
    }

    #[test]
    fn every_tone_has_a_pill_and_a_text_colour_from_the_theme() {
        let tones = [
            Tone::Neutral,
            Tone::Accent,
            Tone::Success,
            Tone::Warning,
            Tone::Danger,
        ];
        let pills: std::collections::HashSet<_> = tones.iter().map(|t| t.chip()).collect();
        assert_eq!(pills.len(), 5, "pills must be distinct");
        for t in tones {
            assert!(
                t.chip().contains("rounded-sm") && t.chip().contains("text-"),
                "{t:?}"
            );
            assert!(t.text().starts_with("text-"));
        }
        assert!(Tone::Accent.chip().contains("accent") && Tone::Danger.chip().contains("danger"));
    }

    #[test]
    fn chip_and_bars_use_only_theme_tokens() {
        for class in [
            CHIP,
            CHIP_STRONG,
            Tone::Success.chip(),
            Tone::Warning.chip(),
            FILTER_BAR,
            FORM_BAR,
        ] {
            for word in class.split_whitespace() {
                assert!(
                    !["zinc", "gray", "slate", "red-", "emerald", "dark:", "shadow"]
                        .iter()
                        .any(|bad| word.contains(bad)),
                    "{word}"
                );
            }
        }
    }
}
