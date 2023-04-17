use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::{anyrun_interface::HandleResult, *};
use fuzzy_matcher::FuzzyMatcher;
use scrubber::DesktopEntry;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    env, fs,
    process::Command,
};

#[derive(Deserialize)]
pub struct Config {
    desktop_actions: bool,
    history_size: u32,
    history_weight: u32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            desktop_actions: false,
            history_weight: 2,
            history_size: 10,
        }
    }
}

mod scrubber;

#[derive(Deserialize, Serialize, Default)]
pub struct History(HashMap<RString, VecDeque<DesktopEntry>>);

pub struct State {
    entries: Vec<(DesktopEntry, u64)>,
    config: Config,
    cache_path: String,
    history: History,
}

impl State {
    fn new(config_dir: RString) -> Self {
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

        let cache_path = format!(
            "{}/.cache/anyrun-applications-history",
            env::var("HOME").expect("Unable to determine HOME directory")
        );

        let history: History = if let Ok(Ok(history)) =
            fs::read_to_string(&cache_path).map(|content| ron::from_str(&content))
        {
            history
        } else {
            History::default()
        };

        let entries = scrubber::scrubber(&config).unwrap_or_else(|why| {
            eprintln!("Failed to load desktop entries: {}", why);
            Vec::new()
        });

        State {
            config,
            history,
            cache_path,
            entries,
        }
    }
}

pub fn handler(selection: Match, input: RString, state: &mut State) -> HandleResult {
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
        .unwrap()
        .clone();

    if let Err(why) = Command::new("sh").arg("-c").arg(&entry.exec).spawn() {
        println!("Error running desktop entry: {}", why);
    }

    let history_entry = state.history.0.entry(input).or_insert(VecDeque::new());

    history_entry.push_front(entry);
    history_entry.truncate(state.config.history_size as usize);

    if let Err(why) = fs::write(
        &state.cache_path,
        ron::to_string(&state.history).expect("Failed to serialize history!"),
    ) {
        eprintln!("Failed to write history: {}", why);
    }

    HandleResult::Close
}

pub fn init(config_dir: RString) -> State {
    State::new(config_dir)
}

pub fn get_matches(input: RString, state: &mut State) -> RVec<Match> {
    let history_entry = state.history.0.get(&input);
    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default().smart_case();
    let mut entries = state
        .entries
        .clone()
        .into_iter()
        .filter_map(|(entry, id)| {
            let score = matcher.fuzzy_match(&entry.name, &input).unwrap_or(0)
                + matcher.fuzzy_match(&entry.exec, &input).unwrap_or(0)
                + match history_entry {
                    Some(history_entry) => {
                        history_entry
                            .iter()
                            .filter(|_entry| **_entry == entry)
                            .count() as u32
                            * state.config.history_weight
                    }
                    None => 0,
                } as i64;

            if score > 0 {
                Some((entry, id, score))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    entries.sort_by(|a, b| b.2.cmp(&a.2));

    entries.truncate(5);
    entries
        .into_iter()
        .map(|(entry, id, _)| Match {
            title: entry.name.into(),
            description: entry.desc.map(|desc| desc.into()).into(),
            use_pango: false,
            icon: ROption::RSome(entry.icon.into()),
            id: ROption::RSome(id),
        })
        .collect()
}

pub fn info() -> PluginInfo {
    PluginInfo {
        name: "Applications".into(),
        icon: "application-x-executable".into(),
    }
}

plugin!(init, info, get_matches, handler, State);
