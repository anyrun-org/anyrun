use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::{anyrun_interface::HandleResult, *};
use fuzzy_matcher::FuzzyMatcher;
use scrubber::DesktopEntry;
use std::process::Command;

mod scrubber;

pub fn handler(selection: Match, entries: &mut Vec<(DesktopEntry, u64)>) -> HandleResult {
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

pub fn init(_config_dir: RString) -> Vec<(DesktopEntry, u64)> {
    scrubber::scrubber().expect("Failed to load desktop entries!")
}

pub fn get_matches(input: RString, entries: &mut Vec<(DesktopEntry, u64)>) -> RVec<Match> {
    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default().smart_case();
    let mut entries = entries
        .clone()
        .into_iter()
        .filter_map(|(entry, id)| {
            matcher
                .fuzzy_match(&entry.name, &input)
                .map(|val| (entry, id, val))
        })
        .collect::<Vec<_>>();

    entries.sort_by(|a, b| b.2.cmp(&a.2));

    entries.truncate(5);
    entries
        .into_iter()
        .map(|(entry, id, _)| Match {
            title: entry.name.into(),
            icon: ROption::RSome(entry.icon.into()),
            description: ROption::RNone,
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

plugin!(init, info, get_matches, handler, Vec<(DesktopEntry, u64)>);
