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
}

impl Tab {
    pub const ALL: [Tab; 5] = [
        Tab::General,
        Tab::Thresholds,
        Tab::Reports,
        Tab::Data,
        Tab::Security,
    ];

    /// The value of `?tab=` in the address.
    pub fn id(self) -> &'static str {
        match self {
            Tab::General => "general",
            Tab::Thresholds => "thresholds",
            Tab::Reports => "reports",
            Tab::Data => "data",
            Tab::Security => "security",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Tab::General => "General",
            Tab::Thresholds => "Thresholds",
            Tab::Reports => "Reports",
            Tab::Data => "Data & backup",
            Tab::Security => "Security",
        }
    }

    /// The tab for an address value; anything unknown (or no value) is General.
    pub fn from_id(id: &str) -> Tab {
        Tab::ALL
            .into_iter()
            .find(|t| t.id() == id)
            .unwrap_or(Tab::General)
    }

    /// The neighbouring tab, wrapping round at both ends.
    pub fn step(self, delta: isize) -> Tab {
        let n = Tab::ALL.len() as isize;
        let at = Tab::ALL.iter().position(|t| *t == self).unwrap_or(0) as isize;
        Tab::ALL[(at + delta).rem_euclid(n) as usize]
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
        for t in Tab::ALL {
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
        let first = Tab::ALL[0];
        let last = Tab::ALL[Tab::ALL.len() - 1];
        assert_eq!(first.step(-1), last);
        assert_eq!(last.step(1), first);
        assert_eq!(first.step(1), Tab::ALL[1]);
        assert_eq!(first.step(Tab::ALL.len() as isize), first);
    }

    #[test]
    fn every_kind_of_setting_has_a_home() {
        let labels: Vec<&str> = Tab::ALL.iter().map(|t| t.label()).collect();
        assert_eq!(
            labels,
            [
                "General",
                "Thresholds",
                "Reports",
                "Data & backup",
                "Security"
            ]
        );
    }
}
