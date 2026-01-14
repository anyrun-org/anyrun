use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::{anyrun_interface::HandleResult, *};
use fuzzy_matcher::FuzzyMatcher;
use scrubber::DesktopEntry;
use serde::Deserialize;
use std::{env, fs, path::PathBuf, process::Command};

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentVariables {
    #[default]
    None,
    Inherit,
    Script(PathBuf),
}

#[derive(Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    #[default]
    Error,
    Debug,
}

#[derive(Deserialize)]
pub struct Config {
    desktop_actions: bool,
    max_entries: usize,
    #[serde(default)]
    hide_description: bool,
    terminal: Option<Terminal>,
    preprocess_exec_script: Option<PathBuf>,
    #[serde(default)]
    environment_variables: EnvironmentVariables,
    #[serde(default)]
    log_level: LogLevel,
}

#[derive(Deserialize)]
pub struct Terminal {
    command: String,
    args: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            desktop_actions: false,
            max_entries: 5,
            hide_description: false,
            preprocess_exec_script: None,
            terminal: None,
            environment_variables: EnvironmentVariables::None,
            log_level: LogLevel::Error,
        }
    }
}

fn get_environment_vars(config: &Config) -> Vec<(String, String)> {
    match &config.environment_variables {
        EnvironmentVariables::None => Vec::new(),
        EnvironmentVariables::Inherit => std::env::vars().collect(),
        EnvironmentVariables::Script(script_path) => {
            let expanded_path = shellexpand::tilde(&script_path.to_string_lossy()).to_string();
            let output = Command::new("sh")
                .arg("-c")
                .arg(&expanded_path)
                .output();

            match output {
                Ok(out) if out.status.success() => {
                    String::from_utf8_lossy(&out.stdout)
                        .lines()
                        .filter_map(|line| {
                            let mut parts = line.splitn(2, '=');
                            match (parts.next(), parts.next()) {
                                (Some(key), Some(value)) if !key.is_empty() => {
                                    Some((key.to_string(), value.to_string()))
                                }
                                _ => None,
                            }
                        })
                        .collect()
                }
                Ok(out) => {
                    eprintln!(
                        "[applications] Environment script failed: {}",
                        String::from_utf8_lossy(&out.stderr)
                    );
                    Vec::new()
                }
                Err(why) => {
                    eprintln!("[applications] Error running environment script: {}", why);
                    Vec::new()
                }
            }
        }
    }
}

pub struct State {
    config: Config,
    entries: Vec<(DesktopEntry, u64)>,
}

mod scrubber;

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

    let exec = if let Some(script) = &state.config.preprocess_exec_script {
        let output = Command::new("sh")
            .arg("-c")
            .arg(format!(
                "{} {} {}",
                script.display(),
                if entry.term { "term" } else { "no-term" },
                &entry.exec
            ))
            .output()
            .unwrap_or_else(|why| {
                eprintln!("[applications] Error running preprocess script: {}", why);
                std::process::exit(1);
            });

        String::from_utf8_lossy(&output.stdout).trim().to_string()
    } else {
        entry.exec.clone()
    };

    if entry.term {
        match &state.config.terminal {
            Some(term) => {
                let arg = format!(
                    "{} {}",
                    term.command,
                    term.args.replace("{}", &exec)
                );
                let envs = get_environment_vars(&state.config);
                if state.config.log_level == LogLevel::Debug {
                    eprintln!("[applications] DEBUG: arg={:?}, envs={:?}", arg, envs);
                }
                if let Err(why) = Command::new("sh")
                    .arg("-c")
                    .arg(&arg)
                    .envs(envs)
                    .spawn()
                {
                    eprintln!("[applications] Error running desktop entry: {}", why);
                }
            }
            None => {
                let sensible_terminals = &[
                    Terminal {
                        command: "alacritty".to_string(),
                        args: "-e {}".to_string(),
                    },
                    Terminal {
                        command: "foot".to_string(),
                        args: "-e \"{}\"".to_string(),
                    },
                    Terminal {
                        command: "kitty".to_string(),
                        args: "-e \"{}\"".to_string(),
                    },
                    Terminal {
                        command: "wezterm".to_string(),
                        args: "-e \"{}\"".to_string(),
                    },
                    Terminal {
                        command: "wterm".to_string(),
                        args: "-e \"{}\"".to_string(),
                    },
                    Terminal {
                        command: "ghostty".to_string(),
                        args: "-e \"{}\"".to_string(),
                    },
                ];
                for term in sensible_terminals {
                    if Command::new("which")
                        .arg(&term.command)
                        .output()
                        .is_ok_and(|output| output.status.success())
                    {
                        let arg = format!(
                            "{} {}",
                            term.command,
                            term.args.replace("{}", &exec)
                        );
                        let envs = get_environment_vars(&state.config);
                        if state.config.log_level == LogLevel::Debug {
                            eprintln!("[applications] DEBUG: arg={:?}, envs={:?}", arg, envs);
                        }
                        if let Err(why) = Command::new("sh")
                            .arg("-c")
                            .arg(&arg)
                            .envs(envs)
                            .spawn()
                        {
                            eprintln!("Error running desktop entry: {}", why);
                        }
                        break;
                    }
                }
            }
        }
    } else {
        let current_dir = env::current_dir().unwrap();
        let cwd = match &entry.path {
            Some(path) if path.exists() => path.clone(),
            _ => current_dir,
        };
        let arg = &entry.exec;
        let envs = get_environment_vars(&state.config);
        if state.config.log_level == LogLevel::Debug {
            eprintln!("[applications] DEBUG: cwd={:?}, arg={:?}, envs={:?}", cwd, arg, envs);
        }
        if let Err(why) = Command::new("sh")
            .arg("-c")
            .arg(arg)
            .current_dir(&cwd)
            .envs(envs)
            .spawn()
        {
            eprintln!("Error running desktop entry: {}", why);
        }
    }

    HandleResult::Close
}

#[init]
pub fn init(config_dir: RString) -> State {
    let config: Config = match fs::read_to_string(format!("{}/applications.ron", config_dir)) {
        Ok(content) => ron::from_str(&content).unwrap_or_else(|why| {
            eprintln!(
                "[applications] Error parsing config, using default: {}",
                why
            );
            Config::default()
        }),
        Err(why) => {
            eprintln!(
                "[applications] Error reading config, using default: {}",
                why
            );
            Config::default()
        }
    };

    let entries = scrubber::scrubber(&config).unwrap_or_else(|why| {
        eprintln!("[applicatiosn] Failed to load desktop entries: {}", why);
        Vec::new()
    });

    State { config, entries }
}

#[get_matches]
pub fn get_matches(input: RString, state: &State) -> RVec<Match> {
    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default().ignore_case();
    let mut entries = state
        .entries
        .iter()
        .filter_map(|(entry, id)| {
            let name_score = matcher.fuzzy_match(&entry.name, &input).unwrap_or(0).max(
                matcher
                    .fuzzy_match(&entry.localized_name(), &input)
                    .unwrap_or(0),
            );
            let desc_score = entry
                .desc
                .as_ref()
                .and_then(|desc| matcher.fuzzy_match(desc, &input))
                .unwrap_or(0);

            let keyword_score = (entry.keywords.iter())
                .chain(entry.localized_keywords.iter().flat_map(|k| k.iter()))
                .filter_map(|keyword| matcher.fuzzy_match(keyword, &input))
                .max()
                .unwrap_or(0);

            let mut score = (name_score * 10 + desc_score + keyword_score) - entry.offset;

            // prioritize actions
            if entry.is_action {
                score *= 2;
            }

            // Score cutoff
            if score > 0 {
                Some((entry, *id, score))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    entries.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.name.cmp(&b.0.name)));

    entries.truncate(state.config.max_entries);
    entries
        .into_iter()
        .map(|(entry, id, _)| Match {
            title: entry.localized_name().into(),
            description: if state.config.hide_description {
                ROption::RNone
            } else {
                entry.desc.clone().map(|desc| desc.into()).into()
            },
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
