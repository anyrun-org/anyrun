use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::{plugin, anyrun_interface::HandleResult, Match, PluginInfo};

pub fn init(_config_dir: RString) {}

pub fn info() -> PluginInfo {
    PluginInfo {
        name: "Symbols".into(),
        icon: "emblem-mail".into(),
    }
}

pub fn get_matches(input: RString) -> RVec<Match> {
    vec![Match {
        title: "Test".into(),
        description: ROption::RNone,
        icon: "dialog-warning".into(),
        id: 0,
    }]
    .into()
}

pub fn handler(selection: Match) -> HandleResult {
    HandleResult::Close
}

plugin!(init, info, get_matches, handler);
