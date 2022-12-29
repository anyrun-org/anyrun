use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::{plugin, anyrun_interface::HandleResult, Match, PluginInfo};

pub fn init(_config_dir: RString) {}

pub fn info() -> PluginInfo {
    PluginInfo {
        name: "Web search".into(),
        icon: "system-search".into(),
    }
}

pub fn get_matches(input: RString) -> RVec<Match> {
    vec![
        Match {
            title: "DDG it!".into(),
            description: ROption::RSome(format!(r#"Look up "{}" with DuckDuckGo"#, input).into()),
            icon: "emblem-web".into(),
            id: 0,
        },
        Match {
            title: "Startpage it!".into(),
            description: ROption::RSome(format!(r#"Look up "{}" with Startpage"#, input).into()),
            icon: "emblem-web".into(),
            id: 0,
        },
    ]
    .into()
}

pub fn handler(selection: Match) -> HandleResult {
    HandleResult::Close
}

plugin!(init, info, get_matches, handler);
