//! The tabs of the Settings screen (spec 23). Pure, so other screens can link to a tab and the
//! tab logic is tested.

/// The Settings screen's tabs, one per kind of setting. Adding one is a table entry here plus
/// its panel in `pages::settings::Settings`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    /// Appearance and how time is counted.
    General,
    /// When something counts as stale, overloaded or at risk.
    Thresholds,
    /// The status report template.
    Reports,
    /// Where the data is, Google Drive and backups.
    Data,
    /// Encryption.
    Security,
    /// Demo data. Debug builds only.
    Developer,
}

impl Tab {
    pub const ALL: [Tab; 6] = [
        Tab::General,
        Tab::Thresholds,
        Tab::Reports,
        Tab::Data,
        Tab::Security,
        Tab::Developer,
    ];

    /// Whether this build shows the tab: Developer only in debug builds (`cargo tauri dev`; the
    /// release `trunk build` drops it, and the backend refuses its command there too).
    pub fn is_shown(self) -> bool {
        self != Tab::Developer || cfg!(debug_assertions)
    }

    /// The tabs this build shows, in order.
    pub fn visible() -> Vec<Tab> {
        Tab::ALL.into_iter().filter(|t| t.is_shown()).collect()
    }

    /// The value of `?tab=` in the address.
    pub fn id(self) -> &'static str {
        match self {
            Tab::General => "general",
            Tab::Thresholds => "thresholds",
            Tab::Reports => "reports",
            Tab::Data => "data",
            Tab::Security => "security",
            Tab::Developer => "developer",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Tab::General => "General",
            Tab::Thresholds => "Thresholds",
            Tab::Reports => "Reports",
            Tab::Data => "Data & backup",
            Tab::Security => "Security",
            Tab::Developer => "Developer",
        }
    }

    /// The tab for an address value; anything unknown, hidden in this build, or missing is
    /// General.
    pub fn from_id(id: &str) -> Tab {
        Tab::ALL
            .into_iter()
            .find(|t| t.id() == id && t.is_shown())
            .unwrap_or(Tab::General)
    }

    /// The neighbouring visible tab, wrapping round at both ends.
    pub fn step(self, delta: isize) -> Tab {
        let tabs = Tab::visible();
        let n = tabs.len() as isize;
        let at = tabs.iter().position(|t| *t == self).unwrap_or(0) as isize;
        tabs[(at + delta).rem_euclid(n) as usize]
    }

    /// The address of this tab, for links from other screens.
    pub fn path(self) -> String {
        match self {
            Tab::General => "/settings".to_owned(),
            other => format!("/settings?tab={}", other.id()),
        }
    }

    pub fn button_id(self) -> String {
        format!("settings-tab-{}", self.id())
    }

    pub fn panel_id(self) -> String {
        format!("settings-panel-{}", self.id())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tab_has_its_own_address_and_round_trips() {
        let mut ids: Vec<&str> = Tab::ALL.iter().map(|t| t.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), Tab::ALL.len());
        for t in Tab::visible() {
            assert_eq!(Tab::from_id(t.id()), t);
        }
        assert_eq!(Tab::General.path(), "/settings");
        assert_eq!(Tab::Data.path(), "/settings?tab=data");
    }

    #[test]
    fn an_unknown_address_value_means_general() {
        assert_eq!(Tab::from_id(""), Tab::General);
        assert_eq!(Tab::from_id("nope"), Tab::General);
    }

    #[test]
    fn arrow_keys_wrap_round_both_ends() {
        let tabs = Tab::visible();
        let first = tabs[0];
        let last = tabs[tabs.len() - 1];
        assert_eq!(first.step(-1), last);
        assert_eq!(last.step(1), first);
        assert_eq!(first.step(1), tabs[1]);
        assert_eq!(first.step(tabs.len() as isize), first);
    }

    #[test]
    fn every_kind_of_setting_has_a_home() {
        let labels: Vec<&str> = Tab::visible().iter().map(|t| t.label()).collect();
        let mut expected = vec![
            "General",
            "Thresholds",
            "Reports",
            "Data & backup",
            "Security",
        ];
        // Tests run as a debug build, which shows the Developer tab.
        if cfg!(debug_assertions) {
            expected.push("Developer");
        }
        assert_eq!(labels, expected);
    }

    #[test]
    fn the_developer_tab_is_a_debug_build_thing() {
        assert_eq!(Tab::Developer.is_shown(), cfg!(debug_assertions));
        assert!(Tab::General.is_shown());
        // Where it is hidden, its address opens General instead.
        if !cfg!(debug_assertions) {
            assert_eq!(Tab::from_id("developer"), Tab::General);
        }
    }
}
