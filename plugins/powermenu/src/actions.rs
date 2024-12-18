use abi_stable::std_types::ROption;
use anyrun_plugin::Match;
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use num_enum::{IntoPrimitive, TryFromPrimitive};

#[derive(Clone, Copy, IntoPrimitive, TryFromPrimitive)]
#[repr(u64)]
pub enum PowerAction {
    Lock,
    Logout,
    Poweroff,
    Reboot,
    Suspend,
    Hibernate,
}

impl PowerAction {
    const VALUES: [Self; 6] = [
        Self::Lock,
        Self::Logout,
        Self::Poweroff,
        Self::Reboot,
        Self::Suspend,
        Self::Hibernate,
    ];

    pub const fn get_title(&self) -> &str {
        match self {
            Self::Lock => "Lock",
            Self::Logout => "Log out",
            Self::Poweroff => "Power off",
            Self::Reboot => "Reboot",
            Self::Suspend => "Suspend",
            Self::Hibernate => "Hibernate",
        }
    }
    pub const fn get_description(&self) -> &str {
        match self {
            Self::Lock => "Lock the session screen",
            Self::Logout => "Terminate the session",
            Self::Poweroff => "Shut down the system",
            Self::Reboot => "Restart the system",
            Self::Suspend => "Suspend the system to RAM",
            Self::Hibernate => "Suspend the system to disk",
        }
    }

    pub const fn get_icon_name(&self) -> &str {
        match self {
            Self::Lock => "system-lock-screen",
            Self::Logout => "system-log-out",
            Self::Poweroff => "system-shutdown",
            Self::Reboot => "system-reboot",
            Self::Suspend => "system-suspend",
            Self::Hibernate => "system-suspend-hibernate",
        }
    }

    pub fn as_match(self) -> Match {
        Match {
            title: self.get_title().into(),
            icon: ROption::RSome(self.get_icon_name().into()),
            use_pango: false,
            description: ROption::RSome(self.get_description().into()),
            id: ROption::RSome(self.into()),
        }
    }

    pub fn get_fuzzy_matching_values(phrase: &str) -> impl Iterator<Item = Self> {
        let fuzzy_matcher = SkimMatcherV2::default().ignore_case();
        let mut matches_with_scores = Self::VALUES
            .into_iter()
            .filter_map(|action| {
                action
                    .get_fuzzy_score(&fuzzy_matcher, phrase)
                    .map(|score| (action, score))
            })
            .collect::<Vec<_>>();
        matches_with_scores.sort_by_key(|(_action, score)| *score);
        matches_with_scores
            .into_iter()
            .map(|(action, _score)| action)
    }

    fn get_fuzzy_score(self, matcher: &impl FuzzyMatcher, phrase: &str) -> Option<i64> {
        matcher
            .fuzzy_match(self.get_title(), phrase)
            .max(matcher.fuzzy_match(self.get_description(), phrase))
    }
}

#[derive(PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u64)]
pub enum ConfirmAction {
    Confirm,
    Cancel,
}

impl ConfirmAction {
    pub fn is_confirmed(&self) -> bool {
        *self == Self::Confirm
    }
}
