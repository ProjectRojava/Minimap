//! Colour themes: plain data mapped onto the app's colour tokens (see `docs/design.md`).
//!
//! A theme is 14 colours. Established palettes (Nord, Catppuccin, Gruvbox, ...) are adapted
//! to those tokens; where a palette's own text colours are too faint to read comfortably
//! (common for "comment" greys) or red on a dark background, `fg`, `muted` and `danger` are
//! nudged toward white (dark themes) or black (light themes) until they pass the contrast
//! tests below: text 4.5:1 on canvas, panel and the selected row (secondary text 3.5:1 there).
//! `accent`, `success` and `warning` come from each palette's own blue/green/yellow and are
//! held to 4.5:1 on canvas and panel and 3.5:1 on the selected row (they are used as text and
//! as the background of primary buttons, whose label is the canvas colour).
//! To add a theme: append an entry to [`ALL`] and run the tests.

use minimap_types::DEFAULT_THEME;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Dark,
    Light,
}

/// The colour tokens every theme provides.
#[derive(Clone, Copy, Debug)]
pub struct Tokens {
    /// Main content area and inputs.
    pub canvas: &'static str,
    /// Sidebar, detail pane, headers, dialogs.
    pub panel: &'static str,
    /// Hover and keyboard cursor.
    pub hover: &'static str,
    /// Selected row, current nav item.
    pub active: &'static str,
    /// Hairline borders.
    pub line: &'static str,
    /// Focused inputs, emphasised borders.
    pub line_strong: &'static str,
    /// Primary text.
    pub fg: &'static str,
    /// Secondary text.
    pub muted: &'static str,
    /// Hints that are also available elsewhere (shortcut labels, icons).
    pub faint: &'static str,
    /// Errors and destructive actions.
    pub danger: &'static str,
    /// The brand colour: the current screen, primary buttons, focus, today, the critical path.
    pub accent: &'static str,
    /// Good news: on track, absorbed, done.
    pub success: &'static str,
    /// Needs attention: amber health, stale, important.
    pub warning: &'static str,
    /// Dimming behind overlays (any CSS colour).
    pub scrim: &'static str,
}

impl Tokens {
    /// (CSS custom property, value) pairs, applied to the document root.
    pub fn css_vars(&self) -> [(&'static str, &'static str); 14] {
        [
            ("--canvas", self.canvas),
            ("--panel", self.panel),
            ("--hover", self.hover),
            ("--active", self.active),
            ("--line", self.line),
            ("--line-strong", self.line_strong),
            ("--fg", self.fg),
            ("--muted", self.muted),
            ("--faint", self.faint),
            ("--danger", self.danger),
            ("--accent", self.accent),
            ("--success", self.success),
            ("--warning", self.warning),
            ("--scrim", self.scrim),
        ]
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: Kind,
    pub tokens: Tokens,
}

/// Follow the OS: Minimap Light or Minimap Dark.
pub const SYSTEM_ID: &str = "system";

const DARK_SCRIM: &str = "rgb(0 0 0 / 0.35)";
const LIGHT_SCRIM: &str = "rgb(0 0 0 / 0.12)";

const fn dark(id: &'static str, name: &'static str, t: Tokens) -> Theme {
    Theme {
        id,
        name,
        kind: Kind::Dark,
        tokens: t,
    }
}

const fn light(id: &'static str, name: &'static str, t: Tokens) -> Theme {
    Theme {
        id,
        name,
        kind: Kind::Light,
        tokens: t,
    }
}

#[allow(clippy::too_many_arguments)]
const fn t(
    canvas: &'static str,
    panel: &'static str,
    hover: &'static str,
    active: &'static str,
    line: &'static str,
    line_strong: &'static str,
    fg: &'static str,
    muted: &'static str,
    faint: &'static str,
    danger: &'static str,
    accent: &'static str,
    success: &'static str,
    warning: &'static str,
    scrim: &'static str,
) -> Tokens {
    Tokens {
        canvas,
        panel,
        hover,
        active,
        line,
        line_strong,
        fg,
        muted,
        faint,
        danger,
        accent,
        success,
        warning,
        scrim,
    }
}

pub const ALL: [Theme; 17] = [
    dark(
        "minimap-dark",
        "Minimap Dark",
        t(
            "#1e1f21", "#19191b", "#26272a", "#2e2f33", "#2a2b2e", "#44464b", "#d4d4d4", "#9a9ca1",
            "#64666b", "#d9776f", "#6c9bd2", "#7fb685", "#d7ba7d", DARK_SCRIM,
        ),
    ),
    light(
        "minimap-light",
        "Minimap Light",
        t(
            "#fafafa",
            "#f3f3f3",
            "#ebebeb",
            "#e2e2e2",
            "#e0e0e0",
            "#c4c4c4",
            "#2b2b2b",
            "#636363",
            "#a0a0a0",
            "#b4423a",
            "#2f6fb0",
            "#2f7d4a",
            "#946300",
            LIGHT_SCRIM,
        ),
    ),
    dark(
        "one-dark",
        "One Dark",
        t(
            "#282c34", "#21252b", "#2c313a", "#3a3f4b", "#333842", "#4b5263", "#abb2bf", "#949dab",
            "#636d83", "#e17078", "#61afef", "#98c379", "#e5c07b", DARK_SCRIM,
        ),
    ),
    light(
        "one-light",
        "One Light",
        t(
            "#fafafa",
            "#f0f0f0",
            "#e8e8e9",
            "#e0e0e1",
            "#dbdbdc",
            "#c2c2c4",
            "#383a42",
            "#696c77",
            "#a0a1a7",
            "#ca1243",
            "#3766cd",
            "#3c7a3b",
            "#916401",
            LIGHT_SCRIM,
        ),
    ),
    dark(
        "dracula",
        "Dracula",
        t(
            "#282a36", "#21222c", "#2f3140", "#44475a", "#343746", "#515470", "#f8f8f2", "#a9b2d6",
            "#6272a4", "#ff5555", "#bd93f9", "#50fa7b", "#ffb86c", DARK_SCRIM,
        ),
    ),
    dark(
        "nord",
        "Nord",
        t(
            "#2e3440", "#2a2f3a", "#3b4252", "#434c5e", "#3b4252", "#4c566a", "#d8dee9", "#a7b1c4",
            "#6d7a93", "#d9838b", "#88c0d0", "#a3be8c", "#ebcb8b", DARK_SCRIM,
        ),
    ),
    dark(
        "solarized-dark",
        "Solarized Dark",
        t(
            "#002b36", "#073642", "#0b3f4c", "#12495a", "#0d4350", "#2a5f6d", "#a6b2b2", "#8ba0a3",
            "#586e75", "#f2706d", "#55a4dc", "#93a51d", "#be971d", DARK_SCRIM,
        ),
    ),
    light(
        "solarized-light",
        "Solarized Light",
        t(
            "#fdf6e3",
            "#eee8d5",
            "#e6dfc8",
            "#ddd6c1",
            "#e3dcc8",
            "#c9c2ad",
            "#4d6066",
            "#576c73",
            "#93a1a1",
            "#c4211f",
            "#1f6ca5",
            "#606e00",
            "#836400",
            LIGHT_SCRIM,
        ),
    ),
    dark(
        "gruvbox-dark",
        "Gruvbox Dark",
        t(
            "#282828", "#1d2021", "#32302f", "#3c3836", "#3c3836", "#504945", "#ebdbb2", "#a89984",
            "#7c6f64", "#fb533f", "#83a598", "#b8bb26", "#fabd2f", DARK_SCRIM,
        ),
    ),
    light(
        "gruvbox-light",
        "Gruvbox Light",
        t(
            "#fbf1c7",
            "#f2e5bc",
            "#ebdbb2",
            "#d5c4a1",
            "#ebdbb2",
            "#bdae93",
            "#3c3836",
            "#665c54",
            "#a89984",
            "#9d0006",
            "#076678",
            "#67630c",
            "#88580d",
            LIGHT_SCRIM,
        ),
    ),
    dark(
        "catppuccin-mocha",
        "Catppuccin Mocha",
        t(
            "#1e1e2e", "#181825", "#313244", "#45475a", "#313244", "#585b70", "#cdd6f4", "#a6adc8",
            "#6c7086", "#f38ba8", "#89b4fa", "#a6e3a1", "#f9e2af", DARK_SCRIM,
        ),
    ),
    light(
        "catppuccin-latte",
        "Catppuccin Latte",
        t(
            "#eff1f5",
            "#e6e9ef",
            "#dce0e8",
            "#ccd0da",
            "#ccd0da",
            "#acb0be",
            "#4c4f69",
            "#5c5f77",
            "#9ca0b0",
            "#d00f38",
            "#1c5ee2",
            "#2f7420",
            "#8e5b12",
            LIGHT_SCRIM,
        ),
    ),
    dark(
        "tokyo-night",
        "Tokyo Night",
        t(
            "#1a1b26", "#16161e", "#24283b", "#2f334d", "#292e42", "#3b4261", "#c0caf5", "#9aa5ce",
            "#565f89", "#f7768e", "#7aa2f7", "#9ece6a", "#e0af68", DARK_SCRIM,
        ),
    ),
    dark(
        "github-dark",
        "GitHub Dark",
        t(
            "#0d1117", "#161b22", "#1c2128", "#262c36", "#30363d", "#484f58", "#e6edf3", "#8b949e",
            "#6e7681", "#f85149", "#58a6ff", "#3fb950", "#d29922", DARK_SCRIM,
        ),
    ),
    dark(
        "rose-pine",
        "Rosé Pine",
        t(
            "#191724", "#1f1d2e", "#26233a", "#403d52", "#26233a", "#403d52", "#e0def4", "#9692af",
            "#6e6a86", "#eb6f92", "#c4a7e7", "#7fc4a0", "#f6c177", DARK_SCRIM,
        ),
    ),
    light(
        "rose-pine-dawn",
        "Rosé Pine Dawn",
        t(
            "#faf4ed",
            "#fffaf3",
            "#f2e9e1",
            "#dfdad9",
            "#f2e9e1",
            "#cecacd",
            "#575279",
            "#6e6a86",
            "#9893a5",
            "#a45a6f",
            "#7a6890",
            "#3b7a6a",
            "#966421",
            LIGHT_SCRIM,
        ),
    ),
    dark(
        "monokai",
        "Monokai",
        t(
            "#272822", "#1e1f1c", "#33342d", "#3e3d32", "#3a3b33", "#57584d", "#f8f8f2", "#a6a79d",
            "#75715e", "#fa4989", "#66d9ef", "#a6e22e", "#e6db74", DARK_SCRIM,
        ),
    ),
];

pub fn default_theme() -> &'static Theme {
    find(DEFAULT_THEME).unwrap_or(&ALL[0])
}

pub fn find(id: &str) -> Option<&'static Theme> {
    ALL.iter().find(|t| t.id == id)
}

/// The theme to draw for a stored id: `system` follows the OS, unknown ids fall back to the default.
pub fn resolve(id: &str, os_prefers_dark: bool) -> &'static Theme {
    if id == SYSTEM_ID {
        let wanted = if os_prefers_dark {
            "minimap-dark"
        } else {
            "minimap-light"
        };
        return find(wanted).unwrap_or(&ALL[0]);
    }
    find(id).unwrap_or_else(default_theme)
}

#[cfg(test)]
fn channel(hex: &str, start: usize) -> f64 {
    let v = u8::from_str_radix(hex.get(start..start + 2).unwrap_or("00"), 16).unwrap_or(0);
    let c = f64::from(v) / 255.0;
    if c <= 0.039_28 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
fn luminance(hex: &str) -> f64 {
    0.2126 * channel(hex, 1) + 0.7152 * channel(hex, 3) + 0.0722 * channel(hex, 5)
}

#[cfg(test)]
/// WCAG contrast ratio between two `#rrggbb` colours (1 to 21).
pub fn contrast(a: &str, b: &str) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_hex(c: &str) -> bool {
        c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit())
    }

    #[test]
    fn ids_are_unique_valid_and_the_default_exists() {
        let mut seen = std::collections::HashSet::new();
        for th in &ALL {
            assert!(seen.insert(th.id), "duplicate id {}", th.id);
            // Must pass the backend's id check (lowercase, digits, hyphens).
            assert!(th
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
            assert!(!th.name.is_empty());
        }
        assert_ne!(SYSTEM_ID, "", "reserved id");
        assert!(
            find(SYSTEM_ID).is_none(),
            "`system` is not a theme of its own"
        );
        assert_eq!(default_theme().id, DEFAULT_THEME);
        assert_eq!(default_theme().kind, Kind::Dark, "dark is the default");
    }

    #[test]
    fn every_colour_is_a_hex_value() {
        for th in &ALL {
            for (name, v) in th.tokens.css_vars() {
                if name == "--scrim" {
                    assert!(v.starts_with("rgb("), "{} {name}", th.id);
                } else {
                    assert!(is_hex(v), "{} {name} = {v}", th.id);
                }
            }
        }
    }

    #[test]
    fn text_is_readable_on_every_surface() {
        let mut failures = Vec::new();
        for th in &ALL {
            let k = &th.tokens;
            for (surface_name, surface) in [
                ("canvas", k.canvas),
                ("panel", k.panel),
                ("active", k.active),
            ] {
                for (text_name, text, min) in [
                    ("fg", k.fg, 4.5),
                    // Secondary text on a selected row only needs to stay legible.
                    (
                        "muted",
                        k.muted,
                        if surface_name == "active" { 3.5 } else { 4.5 },
                    ),
                ] {
                    let c = contrast(text, surface);
                    if c < min {
                        failures.push(format!(
                            "{} {text_name} on {surface_name}: {c:.2} < {min}",
                            th.id
                        ));
                    }
                }
            }
            for (surface_name, surface) in [("canvas", k.canvas), ("panel", k.panel)] {
                for (name, colour) in [
                    ("danger", k.danger),
                    ("accent", k.accent),
                    ("success", k.success),
                    ("warning", k.warning),
                ] {
                    let c = contrast(colour, surface);
                    if c < 4.5 {
                        failures.push(format!("{} {name} on {surface_name}: {c:.2} < 4.5", th.id));
                    }
                }
            }
            for (name, colour) in [
                ("accent", k.accent),
                ("success", k.success),
                ("warning", k.warning),
            ] {
                let c = contrast(colour, k.active);
                if c < 3.5 {
                    failures.push(format!("{} {name} on active: {c:.2} < 3.5", th.id));
                }
            }
        }
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }

    #[test]
    fn surfaces_and_text_are_distinguishable() {
        for th in &ALL {
            let k = &th.tokens;
            // Hover/active must be visible against the canvas and each other.
            assert_ne!(k.canvas, k.active, "{}", th.id);
            assert_ne!(k.hover, k.active, "{}", th.id);
            assert!(
                contrast(k.line_strong, k.canvas) > contrast(k.line, k.canvas),
                "{} line_strong is not stronger than line",
                th.id
            );
            // Dark themes have light text, light themes dark text.
            let text_is_light = luminance(k.fg) > luminance(k.canvas);
            assert_eq!(
                text_is_light,
                th.kind == Kind::Dark,
                "{} kind does not match its colours",
                th.id
            );
        }
    }

    #[test]
    fn the_accent_and_status_colours_are_distinct_from_each_other_and_from_text() {
        for th in &ALL {
            let k = &th.tokens;
            let all = [k.accent, k.success, k.warning, k.danger];
            for (i, a) in all.iter().enumerate() {
                for b in &all[i + 1..] {
                    assert_ne!(a, b, "{} reuses a colour", th.id);
                }
                assert_ne!(*a, k.fg, "{}", th.id);
                assert_ne!(*a, k.muted, "{}", th.id);
            }
        }
    }

    #[test]
    fn system_follows_the_os_and_unknown_ids_fall_back() {
        assert_eq!(resolve("system", true).id, "minimap-dark");
        assert_eq!(resolve("system", false).id, "minimap-light");
        assert_eq!(resolve("nord", false).id, "nord");
        assert_eq!(resolve("no-such-theme", false).id, DEFAULT_THEME);
        assert_eq!(resolve("", true).id, DEFAULT_THEME);
    }

    #[test]
    fn css_defaults_match_the_default_theme() {
        // style/input.css holds the default (dark) tokens so the first paint is already right.
        let css = include_str!("../style/input.css");
        for (name, value) in default_theme().tokens.css_vars() {
            assert!(
                css.contains(&format!("{name}: {value}")),
                "input.css is missing {name}: {value}"
            );
        }
    }

    #[test]
    fn contrast_helper_is_sane() {
        assert!((contrast("#000000", "#ffffff") - 21.0).abs() < 0.01);
        assert!((contrast("#777777", "#777777") - 1.0).abs() < 1e-9);
    }
}
