use std::{env, fs, process::Command};

use abi_stable::std_types::{ROption, RString, RVec};
use fuzzy_matcher::FuzzyMatcher;
use serde::Deserialize;

use anyrun_plugin::{*, anyrun_interface::HandleResult};
use scrubber::DesktopEntry;

use crate::execution_stats::ExecutionStats;

#[derive(Deserialize)]
pub struct Config {
    /// Limit amount of entries shown by the applications plugin (default: 5)
    max_entries: usize,
    /// Whether to evaluate desktop actions as well as desktop applications (default: false)
    desktop_actions: bool,
    /// Whether to use a specific terminal or just the first terminal available (default: None)
    terminal: Option<String>,
    /// Whether to put more often used applications higher in the search rankings (default: true)
    use_usage_statistics: bool,
    /// How much score to add for every usage of an application (default: 50)
    /// Each matching letter is 25 points
    usage_score_multiplier: i64,
    /// Maximum amount of usages to count (default: 10)
    /// This is to limit the added score, so often used apps don't get too big of a boost
    max_counted_usages: i64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            desktop_actions: false,
            max_entries: 5,
            terminal: None,
            use_usage_statistics: true,
            usage_score_multiplier: 50,
            max_counted_usages: 10,
        }
    }
}

pub struct State {
    config: Config,
    entries: Vec<(DesktopEntry, u64)>,
    execution_stats: Option<ExecutionStats>,
}

mod scrubber;
mod execution_stats;

const SENSIBLE_TERMINALS: &[&str] = &["alacritty", "foot", "kitty", "wezterm", "wterm"];

#[handler]
pub fn handler(selection: Match, state: &State) -> HandleResult {
    let entry = state
        .entries
        .iter()
        .find_map(|(entry, id)| {
            if *id == selection.id.unwrap() {
                Some(entry)
            } else {
                None
            }
        })
        .unwrap();

    // count the usage for the statistics
    if let Some(stats) = &state.execution_stats {
        stats.register_usage(&entry);
    }

    if entry.term {
        match &state.config.terminal {
            Some(term) => {
                if let Err(why) = Command::new(term).arg("-e").arg(&entry.exec).spawn() {
                    eprintln!("Error running desktop entry: {}", why);
                }
            }
            None => {
                for term in SENSIBLE_TERMINALS {
                    if Command::new(term)
                        .arg("-e")
                        .arg(&entry.exec)
                        .spawn()
                        .is_ok()
                    {
                        break;
                    }
                }
            }
        }
    } else if let Err(why) = {
        let current_dir = &env::current_dir().unwrap();

        Command::new("sh")
            .arg("-c")
            .arg(&entry.exec)
            .current_dir(if let Some(path) = &entry.path {
                if path.exists() { path } else { current_dir }
            } else {
                current_dir
            })
            .spawn()
    }
    {
        eprintln!("Error running desktop entry: {}", why);
    }

    HandleResult::Close
}

#[init]
pub fn init(config_dir: RString) -> State {
    let config: Config = match fs::read_to_string(format!("{}/applications.ron", config_dir)) {
        Ok(content) => ron::from_str(&content).unwrap_or_else(|why| {
            eprintln!("Error parsing applications plugin config: {}", why);
            Config::default()
        }),
        Err(why) => {
            eprintln!("Error reading applications plugin config: {}", why);
            Config::default()
        }
    };

    // only load execution stats, if needed
    let execution_stats = if config.use_usage_statistics {
        let execution_stats_path = format!("{}/execution_statistics.ron", config_dir);
        Some(ExecutionStats::from_file_or_default(&execution_stats_path, &config))
    } else {
        None
    };

    let entries = scrubber::scrubber(&config).unwrap_or_else(|why| {
        eprintln!("Failed to load desktop entries: {}", why);
        Vec::new()
    });

    State { config, entries, execution_stats }
}

#[get_matches]
pub fn get_matches(input: RString, state: &State) -> RVec<Match> {
    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default().smart_case();
    let mut entries = state
        .entries
        .iter()
        .filter_map(|(entry, id)| {
            let app_score = match &entry.desc {
                None => matcher.fuzzy_match(&entry.name, &input).unwrap_or(0),
                Some(val) => matcher
                    .fuzzy_match(&format!("{} {}", &val, &entry.name).to_string(), &input)
                    .unwrap_or(0),
            };

            let keyword_score = entry
                .keywords
                .iter()
                .map(|keyword| matcher.fuzzy_match(keyword, &input).unwrap_or(0))
                .sum::<i64>();

            let mut score = (app_score * 25 + keyword_score) - entry.offset;

            // add score for often used apps
            if let Some(stats) = &state.execution_stats {
                score += stats.get_weight(entry) * state.config.usage_score_multiplier;
            }

            // prioritize actions
            if entry.desc.is_some() {
                score = score * 2;
            }

            if score > 0 {
                Some((entry, *id, score))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    entries.sort_by(|a, b| b.2.cmp(&a.2));

    entries.truncate(state.config.max_entries);
    entries
        .into_iter()
        .map(|(entry, id, _)| Match {
            title: entry.name.clone().into(),
            description: entry.desc.clone().map(|desc| desc.into()).into(),
            use_pango: false,
            icon: ROption::RSome(entry.icon.clone().into()),
            id: ROption::RSome(id),
        })
        .collect()
}

#[info]
pub fn info() -> PluginInfo {
    PluginInfo {
        name: "Applications".into(),
        icon: "application-x-executable".into(),
    }
}
