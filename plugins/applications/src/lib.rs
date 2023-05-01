use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::{anyrun_interface::HandleResult, *};
use fuzzy_matcher::FuzzyMatcher;
use scrubber::DesktopEntry;
use serde::Deserialize;
use std::{fs, process::Command};

#[derive(Deserialize, Default)]
pub struct Config {
    desktop_actions: bool,
}

mod scrubber;

#[handler]
pub fn handler(selection: Match, entries: &Vec<(DesktopEntry, u64)>) -> HandleResult {
    let entry = entries
        .iter()
        .find_map(|(entry, id)| {
            if *id == selection.id.unwrap() {
                Some(entry)
            } else {
                None
            }
        })
        .unwrap();

    if let Err(why) = Command::new("sh").arg("-c").arg(&entry.exec).spawn() {
        println!("Error running desktop entry: {}", why);
    }

    HandleResult::Close
}

#[init]
pub fn init(config_dir: RString) -> Vec<(DesktopEntry, u64)> {
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

    scrubber::scrubber(config).unwrap_or_else(|why| {
        eprintln!("Failed to load desktop entries: {}", why);
        Vec::new()
    })
}

#[get_matches]
pub fn get_matches(input: RString, entries: &Vec<(DesktopEntry, u64)>) -> RVec<Match> {
    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default().smart_case();
    let mut entries = entries
        .clone()
        .into_iter()
        .filter_map(|(entry, id)| {
            let score = matcher.fuzzy_match(&entry.name, &input).unwrap_or(0)
                + matcher.fuzzy_match(&entry.exec, &input).unwrap_or(0);

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

#[info]
pub fn info() -> PluginInfo {
    PluginInfo {
        name: "Applications".into(),
        icon: "application-x-executable".into(),
    }
}
