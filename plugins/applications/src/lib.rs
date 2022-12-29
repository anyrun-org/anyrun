use abi_stable::std_types::{ROption, RString, RVec};
use scrubber::DesktopEntry;
use std::{process::Command, sync::RwLock, thread};
use anyrun_plugin::{anyrun_interface::HandleResult, *};

mod scrubber;

static ENTRIES: RwLock<Vec<(DesktopEntry, u64)>> = RwLock::new(Vec::new());

pub fn handler(selection: Match) -> HandleResult {
    let entries = ENTRIES.read().unwrap();
    let entry = entries
        .iter()
        .find_map(|(entry, id)| {
            if *id == selection.id {
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

pub fn init(_config_dir: RString) {
    thread::spawn(|| {
        *ENTRIES.write().unwrap() = match scrubber::scrubber() {
            Ok(results) => results,
            Err(why) => {
                println!("Error reading desktop entries: {}", why);
                return;
            }
        };
    });
}

pub fn get_matches(input: RString) -> RVec<Match> {
    if input.len() == 0 {
        return RVec::new();
    }

    let mut entries = ENTRIES
        .read()
        .unwrap()
        .clone()
        .into_iter()
        .filter_map(|(entry, id)| {
            match sublime_fuzzy::best_match(&input.to_lowercase(), &entry.name.to_lowercase()) {
                Some(val) => Some((entry, id, val.score())),
                None => None,
            }
        })
        .collect::<Vec<(DesktopEntry, u64, isize)>>();

    entries.sort_by(|a, b| b.1.cmp(&a.1));

    entries.truncate(5);
    entries
        .into_iter()
        .map(|(entry, id, _)| Match {
            title: entry.name.into(),
            icon: entry.icon.into(),
            description: ROption::RNone,
            id,
        })
        .collect()
}

pub fn info() -> PluginInfo {
    PluginInfo {
        name: "Applications".into(),
        icon: "application-x-executable".into(),
    }
}

plugin!(init, info, get_matches, handler);
