mod api;
mod app;
mod components;
mod keyboard;
mod labels;
mod nav;
mod pages;
mod state;
mod window;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(app::App);
}
