use abi_stable::{rvec, std_types::{ROption, RString, RVec}};
use anyrun_plugin::{anyrun_interface::HandleResult, *};
use fuzzy_matcher::FuzzyMatcher;
use serde::{Serialize, Deserialize};
use regex::{Captures, Regex};
use std::{fs, process::Command};

#[derive(Deserialize)]
pub struct Config {
    max_entries: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_entries: 5,
        }
    }
}

pub struct State {
    config: Config,
    entries: RVec<NixEntry>,
}

#[derive(Serialize, Deserialize)]
pub struct NixEntry {
    name: RString,
    desc: RString,
}

#[handler]
pub fn handler(selection: Match, state: &State) -> HandleResult {
    let entry = state
        .entries
        .iter()
        .find_map(|entry| {
            if *entry.name == selection.title {
                Some(entry)
            } else {
                None
            }
        })
        .unwrap();

    if let Err(why) = Command::new("nix-shell")
        .arg("-p")
        .arg(&*entry.name)
        .arg("--run")
        .arg(&*entry.name)
        .spawn()
    {
        eprintln!("Error running desktop entry: {}", why);
    }

    HandleResult::Close
}

#[init]
pub fn init(config_dir: RString) -> State {
    let config: Config = match fs::read_to_string(format!("{}/nix.ron", config_dir)) {
        Ok(content) => ron::from_str(&content).unwrap_or_else(|why| {
            eprintln!("Error parsing applications plugin config: {}", why);
            Config::default()
        }),
        Err(why) => {
            eprintln!("Error reading applications plugin config: {}", why);
            Config::default()
        }
    };

    let entries: RVec<NixEntry> = match fs::read_to_string(format!("{}/nix-pkgs.ron", config_dir)) {
        Ok(content) => ron::from_str(&content).unwrap_or_else(|why| {
            eprintln!("Error parsing applications plugin cache: {}\nBuilding new cache...", why);
            let entries = get_entries();
            fs::write(
                format!("{}/nix-pkgs.ron", config_dir), ron::to_string(&entries)
                    .expect("Nix plugin could not parse entries to RON format!").as_bytes()
            ).expect("Updater could not write cache file!");
            entries
        }),
        Err(why) => {
            eprintln!("Error reading applications plugin cache: {}\nBuilding new cache...", why);
            let entries = get_entries();
            fs::write(
                format!("{}/nix-pkgs.ron", config_dir), ron::to_string(&entries)
                    .expect("Nix plugin could not parse entries to RON format!").as_bytes()
            ).expect("Updater could not write cache file!");
            entries
        }
    };

    State { config, entries }
}

fn get_entries() -> RVec<NixEntry> {
    let output = Command::new("nix-env")
        .args(["-qaP", "--description"])
        .output().unwrap();

    let output_str = String::from_utf8(output.stdout).unwrap();
    let re = Regex::new(r"^[^\.]*.(\S*)\s*\S*\s*(.*)$").unwrap();

    let mut entries: RVec<NixEntry> = rvec![];
    for line in output_str.lines() {
        let captures: Captures = re
            .captures(line)
            .expect("Nix could not collect Regex captures for entry!");

        entries.push(NixEntry {
            name: RString::from(
                captures
                    .get(1)
                    .expect("Nix failed to read a package name!")
                    .as_str(),
            ),
            desc: RString::from(
                captures
                    .get(2)
                    .expect("Nix failed to read a package description!")
                    .as_str(),
            )
        });
    }
    entries
}

#[get_matches]
pub fn get_matches(input: RString, state: &State) -> RVec<Match> {
    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default().smart_case();
    let mut entries = state
        .entries
        .iter()
        .filter_map(|entry| {
            let score: i64 = matcher.fuzzy_match(&entry.name, &input).unwrap_or(0)
                + matcher.fuzzy_match(&entry.desc, &input).unwrap_or(0);

            if score > 0 {
                Some((entry, score))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    entries.sort_by(|a, b| b.1.cmp(&a.1));

    entries.truncate(state.config.max_entries);
    entries
        .into_iter()
        .map(|(entry, _)| Match {
            title: entry.name.clone().into(),
            description: ROption::RSome(entry.desc.clone().into()),
            use_pango: false,
            icon: ROption::RNone,
            id: ROption::RNone,
        })
        .collect()
}

#[info]
pub fn info() -> PluginInfo {
    PluginInfo {
        name: "Nix".into(),
        icon: "application-x-executable".into(),
    }
}
