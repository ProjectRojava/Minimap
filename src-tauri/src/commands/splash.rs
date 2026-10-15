//! The splash screen's window (docs/product-design/13-splash-screen.md). It is a small window of
//! its own, centred, with no frame, opened the moment the shell starts. The main window stays
//! hidden while the app downloads and starts, and is shown (and the splash closed) once the app
//! says it is ready, but not before the splash has been up for five seconds. If the app never says
//! so, the main window is shown anyway after thirty, so a failed start shows its own error.
//!
//! The splash has a dark look and a light look and shows the one that matches **the app's
//! theme**. The theme is only known to the app (it is in the database, which is not open yet), so
//! the app tells the shell which kind it is drawing, every time it applies a theme
//! (`remember_splash_scheme`), and the shell keeps that in a small file next to the database and
//! reads it first thing at the next start.

use std::{
    path::Path,
    time::{Duration, Instant},
};

use minimap_types::AppError;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::{error::app_error, state::AppState};

/// The splash stays at least this long, so a fast start still shows the mark and the version.
pub const MIN_SHOW: Duration = Duration::from_secs(5);
/// After this long the main window is shown whether or not the app has said it is ready.
pub const GIVE_UP: Duration = Duration::from_secs(30);
/// The splash window's size, in logical pixels: the mark, the name, a line and the version.
pub const SIZE: (f64, f64) = (440.0, 340.0);
/// The file (in the app's data folder) that holds the kind of theme the app last drew.
pub const SCHEME_FILE: &str = "splash-scheme";

/// Which look the splash has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    Dark,
    Light,
    /// No word yet (the first start), or the app follows the system: the system decides.
    System,
}

impl Scheme {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "dark" => Some(Self::Dark),
            "light" => Some(Self::Light),
            "system" => Some(Self::System),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::System => "system",
        }
    }

    /// The page, opened so that it shows this look (`splash.html` itself follows the system).
    pub fn page(self) -> &'static str {
        match self {
            Self::Dark => "splash.html#force-dark",
            Self::Light => "splash.html#force-light",
            Self::System => "splash.html",
        }
    }
}

/// The scheme the app last said it was drawing (the system's when it never has).
pub fn read_scheme(dir: &Path) -> Scheme {
    std::fs::read_to_string(dir.join(SCHEME_FILE))
        .ok()
        .and_then(|t| Scheme::parse(&t))
        .unwrap_or(Scheme::System)
}

/// Keeps the scheme for the next start.
pub fn write_scheme(dir: &Path, scheme: Scheme) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(SCHEME_FILE), scheme.as_str())
}

/// When the shell started, for the minimum time.
pub struct SplashClock(pub Instant);

/// How much longer to wait before showing the main window.
pub fn wait_before_reveal(elapsed: Duration, min: Duration) -> Duration {
    min.saturating_sub(elapsed)
}

/// Opens the splash window in the look for `scheme`. If that fails the main window is shown at
/// once: a window with no splash is better than none.
pub fn open(app: &AppHandle, scheme: Scheme) {
    // The window's own colour is the page's, so nothing flashes before it paints: night navy for
    // a dark look, paper for a light one (the system's, for the system look).
    let dark = match scheme {
        Scheme::Dark => true,
        Scheme::Light => false,
        Scheme::System => {
            app.get_webview_window("main")
                .and_then(|w| w.theme().ok())
                != Some(tauri::Theme::Light)
        }
    };
    let background = if dark {
        tauri::window::Color(11, 21, 36, 255)
    } else {
        tauri::window::Color(251, 248, 240, 255)
    };
    let built = WebviewWindowBuilder::new(app, "splash", WebviewUrl::App(scheme.page().into()))
        .title("Minimap")
        .inner_size(SIZE.0, SIZE.1)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .shadow(true)
        .center()
        .always_on_top(true)
        .skip_taskbar(true)
        .background_color(background)
        .build();
    if let Err(e) = built {
        tracing::warn!(error = %e, "could not open the splash window");
        reveal(app);
    }
}

/// Shows the main window and closes the splash. Safe to do twice.
pub fn reveal(app: &AppHandle) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
        let _ = main.set_focus();
    }
    if let Some(splash) = app.get_webview_window("splash") {
        let _ = splash.close();
    }
}

/// The failsafe: shows the main window after [`GIVE_UP`] if nothing else has.
pub fn spawn_failsafe(app: AppHandle, started: Instant) {
    let _ = std::thread::Builder::new()
        .name("splash-failsafe".into())
        .spawn(move || {
            std::thread::sleep(wait_before_reveal(started.elapsed(), GIVE_UP));
            reveal(&app);
        });
}

/// The app is on the page: show it, once the splash has had its time.
#[tauri::command]
pub fn app_ready(app: AppHandle, clock: State<'_, SplashClock>) {
    let wait = wait_before_reveal(clock.0.elapsed(), MIN_SHOW);
    let _ = std::thread::Builder::new()
        .name("splash-reveal".into())
        .spawn(move || {
            std::thread::sleep(wait);
            reveal(&app);
        });
}

/// The app says which kind of theme it is drawing (`dark`, `light`, or `system` when it follows
/// the system), for the splash of the next start.
#[tauri::command]
pub fn remember_splash_scheme(
    state: State<'_, AppState>,
    scheme: String,
) -> Result<(), AppError> {
    let scheme = Scheme::parse(&scheme)
        .ok_or_else(|| app_error("invalid", "the splash look is dark, light or system"))?;
    // Only written when it changed: the app says it at every start.
    if read_scheme(&state.data_dir) == scheme
        && state.data_dir.join(SCHEME_FILE).exists()
    {
        return Ok(());
    }
    write_scheme(&state.data_dir, scheme)
        .map_err(|e| app_error("io", format!("could not keep the splash look: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fast_start_waits_for_the_rest_of_the_five_seconds() {
        assert_eq!(
            wait_before_reveal(Duration::from_millis(1200), MIN_SHOW),
            Duration::from_millis(3800)
        );
        assert_eq!(wait_before_reveal(Duration::ZERO, MIN_SHOW), MIN_SHOW);
    }

    #[test]
    fn a_slow_start_shows_the_app_at_once() {
        assert_eq!(
            wait_before_reveal(Duration::from_secs(8), MIN_SHOW),
            Duration::ZERO
        );
        assert_eq!(
            wait_before_reveal(Duration::from_secs(60), MIN_SHOW),
            Duration::ZERO
        );
    }

    #[test]
    fn the_failsafe_comes_after_the_splash_and_after_the_slow_note() {
        // "Still starting" shows at 9 s; giving up comes well after, and the splash window is
        // small enough not to cover anything.
        assert!(GIVE_UP > Duration::from_secs(9) && GIVE_UP > MIN_SHOW);
        assert!(SIZE.0 <= 600.0 && SIZE.1 <= 500.0);
    }

    #[test]
    fn the_shell_and_the_page_agree_on_the_five_seconds() {
        let page_side = include_str!("../../../ui/src/splash.rs");
        assert!(page_side.contains(&format!(
            "MIN_SHOW_MS: f64 = {}.0",
            MIN_SHOW.as_millis()
        )));
    }

    #[test]
    fn the_splash_page_is_shipped_and_the_main_window_starts_hidden() {
        let conf = include_str!("../../tauri.conf.json");
        assert!(conf.contains("\"visible\": false"), "the main window opens hidden");
        let index = include_str!("../../../ui/index.html");
        assert!(index.contains("rel=\"copy-file\" href=\"splash.html\""));
        assert!(include_str!("../../../ui/splash.html").contains("id=\"splash\""));
    }

    #[test]
    fn each_kind_of_theme_opens_its_own_look_and_the_system_follows_itself() {
        assert_eq!(Scheme::Dark.page(), "splash.html#force-dark");
        assert_eq!(Scheme::Light.page(), "splash.html#force-light");
        assert_eq!(Scheme::System.page(), "splash.html");
        // The page has the targets those addresses name.
        let page = include_str!("../../../ui/splash.html");
        assert!(page.contains("id=\"force-dark\"") && page.contains("id=\"force-light\""));
        for s in [Scheme::Dark, Scheme::Light, Scheme::System] {
            assert_eq!(Scheme::parse(s.as_str()), Some(s));
        }
        assert_eq!(Scheme::parse("sepia"), None);
        assert_eq!(Scheme::parse(" light\n"), Some(Scheme::Light));
    }

    #[test]
    fn the_scheme_is_kept_in_a_file_and_the_system_look_is_the_fallback() {
        let dir = std::env::temp_dir().join(format!("minimap-splash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // Nothing yet (the first start): the system decides.
        assert_eq!(read_scheme(&dir), Scheme::System);
        write_scheme(&dir, Scheme::Light).unwrap();
        assert_eq!(read_scheme(&dir), Scheme::Light);
        write_scheme(&dir, Scheme::Dark).unwrap();
        assert_eq!(read_scheme(&dir), Scheme::Dark);
        // A damaged file is not a theme.
        std::fs::write(dir.join(SCHEME_FILE), "nonsense").unwrap();
        assert_eq!(read_scheme(&dir), Scheme::System);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
