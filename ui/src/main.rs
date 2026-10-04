mod api;
mod app;
mod calendar;
mod components;
mod keyboard;
mod labels;
mod mentions;
mod nav;
mod pages;
mod state;
mod theme;
mod themes;
mod timeline;
mod window;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(app::App);
}
