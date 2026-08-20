use std::{fs, io::stdin};

use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::*;
use fuzzy_matcher::FuzzyMatcher;
use serde::Deserialize;

#[derive(Deserialize)]
struct Config {
    #[serde(default)]
    allow_invalid: bool,
    #[serde(default = "Config::default_max_entries")]
    max_entries: usize,
    #[serde(default)]
    preserve_order: bool,
}

impl Config {
    fn default_max_entries() -> usize {
        5
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_entries: Config::default_max_entries(),
            allow_invalid: false,
            preserve_order: false,
        }
    }
}

struct State {
    config: Config,
    lines: Vec<String>,
}

#[init]
fn init(config_dir: RString) -> State {
    let config = if let Ok(content) = fs::read_to_string(format!("{}/stdin.ron", config_dir)) {
        ron::from_str(&content).unwrap_or_default()
    } else {
        Config::default()
    };

    State {
        config,
        lines: stdin().lines().map_while(Result::ok).collect(),
    }
}

#[handler]
fn handler(_match: Match) -> HandleResult {
    HandleResult::Stdout(_match.title.into_bytes())
}

#[get_matches]
fn get_matches(input: RString, state: &State) -> RVec<Match> {
    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default().smart_case();

    let mut lines = state
        .lines
        .clone()
        .into_iter()
        .filter_map(|line| {
            matcher
                .fuzzy_match(&line, &input)
                .map(|score| (line, score))
        })
        .collect::<Vec<_>>();

    if !lines.is_empty() {
        if !state.config.preserve_order {
            lines.sort_by(|a, b| b.1.cmp(&a.1));
        }
        lines.truncate(state.config.max_entries);
    } else if state.config.allow_invalid {
        lines.push((input.into(), 0));
    }

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

#[info]
fn plugin_info() -> PluginInfo {
    PluginInfo {
        name: "Stdin".into(),
        icon: "format-indent-more".into(),
    }
}
