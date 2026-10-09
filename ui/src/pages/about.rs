//! About: who made Minimap, how to reach them, and which version this is.

use leptos::{prelude::*, task::spawn_local};

use crate::{
    api,
    components::page::{Card, PageHeader},
    state::Toasts,
};

/// The developer's name.
pub const DEVELOPER: &str = "Uditt Lamba";

/// Where to write to the developer.
pub const EMAIL: &str = "uditt.lamba@pm.me";

/// The version of this build: the workspace version, so a release bump is the only change needed.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The address that opens the user's mail program with the developer's address filled in.
pub fn mailto() -> String {
    format!("mailto:{EMAIL}")
}

/// One line of the card: what it is, then the value.
#[component]
fn Row(label: &'static str, children: Children) -> impl IntoView {
    view! {
        <div class="flex items-baseline gap-4">
            <span class="w-32 shrink-0 text-[11px] uppercase tracking-wider text-muted">{label}</span>
            <span class="min-w-0 break-words">{children()}</span>
        </div>
    }
}

#[component]
pub fn About() -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    // The mail program is opened by the app (http, https and mailto only), like any link.
    let write = move |_| {
        spawn_local(async move {
            if let Err(e) = api::open_link(mailto()).await {
                toasts.error(&e);
            }
        });
    };
    view! {
        <div class="flex h-full flex-col">
            <PageHeader icon="about" title="About" subtitle="Who made Minimap and which version this is"><span></span></PageHeader>
            <div class="min-h-0 flex-1 overflow-y-auto p-4">
                <div class="max-w-xl">
                    <Card title="Minimap" description="A private command center for the work you lead. Your data stays on your computer.">
                        <Row label="Developer">{DEVELOPER}</Row>
                        <Row label="Email">
                            <button class="text-accent underline underline-offset-2 hover:opacity-80"
                                    title="Write an email" on:click=write>{EMAIL}</button>
                        </Row>
                        <Row label="Latest version"><span class="tabular-nums">{VERSION}</span></Row>
                    </Card>
                </div>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_says_who_made_it_and_how_to_write_to_them() {
        assert_eq!(DEVELOPER, "Uditt Lamba");
        assert_eq!(EMAIL, "uditt.lamba@pm.me");
        assert_eq!(mailto(), "mailto:uditt.lamba@pm.me");
    }

    #[test]
    fn the_version_is_the_apps_own() {
        // Three numbers separated by dots (0.3.0); the workspace sets it for every crate.
        let parts: Vec<&str> = VERSION.split('.').collect();
        assert_eq!(parts.len(), 3, "{VERSION}");
        assert!(parts.iter().all(|p| p.parse::<u32>().is_ok()), "{VERSION}");
    }
}
