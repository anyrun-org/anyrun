use std::{env, fs, process::Command};

use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::{anyrun_interface::HandleResult, plugin, Match, PluginInfo};
use serde::Deserialize;

#[derive(Deserialize)]
struct Config {
    prefix: String,
    shell: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            prefix: ":sh".to_string(),
            shell: None,
        }
    }
}

fn init(config_dir: RString) -> Config {
    match fs::read_to_string(format!("{}/shell.ron", config_dir)) {
        Ok(content) => ron::from_str(&content).unwrap_or_default(),
        Err(_) => Config::default(),
    }
}

fn info() -> PluginInfo {
    PluginInfo {
        name: "Shell".into(),
        icon: "utilities-terminal".into(),
    }
}

fn get_matches(input: RString, config: &mut Config) -> RVec<Match> {
    let prefix_with_delim = format!("{} ", config.prefix);
    if input.starts_with(&prefix_with_delim) {
        let (_, command) = input.split_once(&prefix_with_delim).unwrap();
        if !command.is_empty() {
            vec![Match {
                title: command.into(),
                description: ROption::RSome(
                    config
                        .shell
                        .clone()
                        .unwrap_or_else(|| {
                            env::var("SHELL").unwrap_or_else(|_| {
                                "The shell could not be determined!".to_string()
                            })
                        })
                        .into(),
                ),
                icon: ROption::RNone,
                id: ROption::RNone,
            }]
            .into()
        } else {
            RVec::new()
        }
    } else {
        RVec::new()
    }
}

fn handler(selection: Match, _config: &mut Config) -> HandleResult {
    if let Err(why) = Command::new(selection.description.unwrap().as_str())
        .arg("-c")
        .arg(selection.title.as_str())
        .spawn()
    {
        println!("Failed to run command: {}", why);
    }

    HandleResult::Close
}

plugin!(init, info, get_matches, handler, Config);
