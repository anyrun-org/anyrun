use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::*;
use fuzzy_matcher::FuzzyMatcher;
use serde::Deserialize;
use std::{
    fs,
    process::{Command, Stdio},
};

#[info]
fn info() -> PluginInfo {
    PluginInfo {
        name: "Windows".into(),
        icon: "help-about".into(),
    }
}

#[derive(Deserialize, Debug)]
struct Config {
    prefix: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            prefix: "".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
struct WindowInfo {
    address: String,
    title: String,
}

pub struct State {
    config: Config,
    entries: Vec<WindowInfo>,
}

#[init]
fn init(config_dir: RString) -> State {
    let config = match fs::read_to_string(format!("{}/hyprlandwindows.ron", config_dir)) {
        Ok(content) => ron::from_str(&content).unwrap_or_default(),
        Err(_) => Config::default(),
    };

    let mut entries: Vec<WindowInfo> = vec![];

    let output = Command::new("hyprctl")
        .arg("clients")
        .stdout(Stdio::piped())
        .output()
        .unwrap();

    let info = String::from_utf8(output.stdout).unwrap();

    for line in info.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("Window") {
            let title = trimmed
                .trim_end_matches(":")
                .split("->")
                .nth(1)
                .unwrap_or_default()
                .trim()
                .to_string();

            if title.is_empty() {
                continue;
            }

            entries.push(WindowInfo {
                address: trimmed.split_whitespace().nth(1).unwrap().to_string(),
                title,
            })
        }
    }

    State { config, entries }
}

#[get_matches]
fn get_matches(input: RString, state: &State) -> RVec<Match> {
    if !input.starts_with(&state.config.prefix) {
        return RVec::new();
    }

    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default().smart_case();
    let term = input.trim_start_matches(&state.config.prefix);

    let options = state
        .entries
        .clone()
        .into_iter()
        .filter_map(|info| {
            matcher
                .fuzzy_match(&info.title, &term)
                .map(|score| (info, score))
        })
        .collect::<Vec<_>>();

    options
        .into_iter()
        .map(|(info, _)| Match {
            title: info.title.clone().into(),
            description: ROption::RSome(format!("Window {}", info.address).into()),
            use_pango: false,
            icon: ROption::RNone,
            id: ROption::RNone,
        })
        .collect::<Vec<_>>()
        .into()
}

#[handler]
fn handler(selection: Match) -> HandleResult {
    if let Err(why) = Command::new("hyprctl")
        .arg("dispatch")
        .arg("focuswindow")
        .arg(format!(
            "address:0x{}",
            selection.description.unwrap().trim_start_matches("Window ")
        ))
        .spawn()
    {
        println!("Failed to focus window: {}", why);
    }

    HandleResult::Close
}
