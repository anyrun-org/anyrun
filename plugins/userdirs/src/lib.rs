use std::fs::File;

use std::io::{self, BufRead};
use std::path::Path;
use std::process::Command;

use fuzzy_matcher::FuzzyMatcher;

use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::*;

pub struct State {
    dirs: Vec<String>,
}

#[init]
fn init(_config_dir: RString) -> State {
    let mut dirs: Vec<String> = vec![];

    match home::home_dir() {
        Some(path) => {
            if let Ok(lines) = read_lines(format!("{}/.config/user-dirs.dirs", path.display())) {
                for line in lines {
                    if let Ok(entry) = line {
                        if entry.starts_with("XDG_") {
                            match entry.split_once("=") {
                                Some((_, suffix)) => {
                                    let cleaned = suffix
                                        .replace("\"", "")
                                        .trim_start_matches("$HOME/")
                                        .to_string();
                                    dirs.push(cleaned);
                                }
                                None => (),
                            };
                        }
                    }
                }
            }
        }
        None => println!("Impossible to get your home dir!"),
    }

    State { dirs }
}

#[info]
fn info() -> PluginInfo {
    PluginInfo {
        name: "User Dirs".into(),
        icon: "help-about".into(),
    }
}

fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}

#[get_matches]
fn get_matches(input: RString, state: &State) -> RVec<Match> {
    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default().smart_case();

    let matches = state
        .dirs
        .clone()
        .into_iter()
        .filter_map(|line| {
            matcher
                .fuzzy_match(&line, &input)
                .map(|score| (line, score))
        })
        .collect::<Vec<_>>();

    matches
        .into_iter()
        .map(|(line, _)| Match {
            title: line.into(),
            description: ROption::RSome("Folder".into()),
            use_pango: false,
            icon: ROption::RNone,
            id: ROption::RNone,
        })
        .collect::<Vec<_>>()
        .into()
}

#[handler]
fn handler(selection: Match) -> HandleResult {
    if let Err(why) = Command::new("sh")
        .arg("-c")
        .arg(format!("xdg-open file://$HOME/{}", selection.title))
        .spawn()
    {
        println!("Failed to open folder: {}", why);
    }

    HandleResult::Close
}
