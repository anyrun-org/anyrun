use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::{anyrun_interface::HandleResult, *};
use fuzzy_matcher::FuzzyMatcher;
use scrubber::DesktopEntry;
use serde::Deserialize;
use std::{env, fs, path::PathBuf, process::Command};

#[derive(Deserialize)]
#[serde(default)]
pub struct Config {
    desktop_actions: bool,
    max_entries: usize,
    #[serde(default)]
    hide_description: bool,
    terminal: Option<Terminal>,
    preprocess_exec_script: Option<PathBuf>,
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
        }
    }
}

pub struct State {
    config: Config,
    entries: Vec<(DesktopEntry, u64)>,
    mru: std::collections::HashMap<String, u64>,
}

mod scrubber;

fn mru_path() -> PathBuf {
    let cache_dir = env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let mut path = PathBuf::from(env::var("HOME").expect("HOME directory not set"));
            path.push(".cache");
            path
        });
    let anyrun_cache = cache_dir.join("anyrun");
    if !anyrun_cache.exists() {
        let _ = fs::create_dir_all(&anyrun_cache);
    }
    anyrun_cache.join("applications_mru.ron")
}

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

    let mut mru = state.mru.clone();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    mru.insert(entry.name.clone(), timestamp);
    if let Ok(mru_str) = ron::ser::to_string_pretty(&mru, ron::ser::PrettyConfig::default()) {
        let _ = fs::write(mru_path(), mru_str);
    }

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
                if let Err(why) = Command::new("sh")
                    .arg("-c")
                    .arg(format!(
                        "{} {}",
                        term.command,
                        term.args.replace("{}", &exec)
                    ))
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
                        if let Err(why) = Command::new("sh")
                            .arg("-c")
                            .arg(format!(
                                "{} {}",
                                term.command,
                                term.args.replace("{}", &exec)
                            ))
                            .spawn()
                        {
                            eprintln!("Error running desktop entry: {}", why);
                        }
                        break;
                    }
                }
            }
        }
    } else if let Err(why) = {
        let current_dir = &env::current_dir().unwrap();

        Command::new("sh")
            .arg("-c")
            .arg(&exec)
            .current_dir(match &entry.path {
                Some(path) if path.exists() => path,
                _ => current_dir,
            })
            .spawn()
    } {
        eprintln!("Error running desktop entry: {}", why);
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

    let mru = fs::read_to_string(mru_path())
        .ok()
        .and_then(|content| ron::from_str(&content).ok())
        .unwrap_or_default();

    State { config, entries, mru }
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

            let base_score = (name_score * 100 + desc_score + keyword_score) - entry.offset;
            let mut score = base_score;

            let input_lower = input.to_lowercase();
            let entry_name_lower = entry.name.to_lowercase();
            let entry_local_name_lower = entry.localized_name().to_lowercase();

            let is_exact = input_lower == entry_name_lower || input_lower == entry_local_name_lower;
            let is_substring = entry_name_lower.contains(&input_lower)
                || entry_local_name_lower.contains(&input_lower);

            // Only apply boosts if the app actually matches the query in some way
            if base_score > 0 || is_exact || is_substring {
                // Priority 1: Exact Match Boost
                if is_exact {
                    score += 100_000_000_000;
                } 
                
                // Priority 2: Substring Match Boost
                if is_substring {
                    score += 10_000_000_000;
                }

                // Priority 3: MRU Boost (Natural Reciprocal Decay)
                if let Some(&timestamp) = state.mru.get(&entry.name) {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    let age = now.saturating_sub(timestamp);

                    // Boost = 1,000,000,000 / (1 + age / 3600)
                    // Halves every hour, stays as a small tie-breaker long-term.
                    let boost = 1_000_000_000 / (1 + (age / 3600) as i64);
                    score += boost;
                }
            }

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

    entries.sort_by(|a, b| {
        let ord = b.2.cmp(&a.2);
        if ord == std::cmp::Ordering::Equal {
            let mru_a = state.mru.get(&a.0.name).copied().unwrap_or(0);
            let mru_b = state.mru.get(&b.0.name).copied().unwrap_or(0);
            mru_b.cmp(&mru_a) // higher timestamp is better
        } else {
            ord
        }
        .then_with(|| a.0.name.cmp(&b.0.name))
    });

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
