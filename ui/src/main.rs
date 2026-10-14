mod api;
mod app;
mod calendar;
mod components;
mod graph_view;
mod help;
mod icons;
mod keyboard;
mod labels;
mod md_edit;
mod mentions;
mod nav;
mod pages;
mod radar_geom;
mod settings_tab;
mod splash;
mod state;
mod theme;
mod themes;
mod timeline;
mod window;
mod zoom;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(app::App);
    // The splash in index.html has done its job once the app is on the page.
    splash::dismiss();
}
