use std::io::stdin;

use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::{anyrun_interface::HandleResult, plugin, Match, PluginInfo};
use fuzzy_matcher::FuzzyMatcher;

fn init(_config_dir: RString) -> Vec<String> {
    stdin().lines().filter_map(|line| line.ok()).collect()
}

fn handler(_match: Match, _lines: &mut Vec<String>) -> HandleResult {
    HandleResult::Stdout(_match.title.into_bytes())
}

fn get_matches(input: RString, lines: &mut Vec<String>) -> RVec<Match> {
    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default().smart_case();

    let mut lines = lines
        .clone()
        .into_iter()
        .filter_map(|line| {
            matcher
                .fuzzy_match(&line, &input)
                .map(|score| (line, score))
        })
        .collect::<Vec<_>>();

    lines.sort_by(|a, b| b.1.cmp(&a.1));

    lines.truncate(5);

    lines
        .into_iter()
        .map(|(line, _)| Match {
            title: line.into(),
            description: ROption::RNone,
            use_pango: false,
            icon: ROption::RNone,
            id: ROption::RNone,
        })
        .collect::<Vec<_>>()
        .into()
}

fn plugin_info() -> PluginInfo {
    PluginInfo {
        name: "Stdin".into(),
        icon: "format-indent-more".into(),
    }
}

plugin!(init, plugin_info, get_matches, handler, Vec<String>);
