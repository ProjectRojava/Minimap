mod api;
mod app;
mod components;
mod keyboard;
mod nav;
mod pages;
mod state;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(app::App);
}
