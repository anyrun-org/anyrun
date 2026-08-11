use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::{anyrun_interface::HandleResult, *};
use fuzzy_matcher::FuzzyMatcher;
use scrubber::DesktopEntry;
use serde::Deserialize;
use std::{
    env, fs, io, path::{Path, PathBuf}, process::{Command, Stdio},
};

#[derive(Deserialize)]
pub struct Config {
    desktop_actions: bool,
    max_entries: usize,
    #[serde(default)]
    hide_description: bool,
    terminal: Option<Terminal>,
    preprocess_exec_script: Option<PathBuf>,
    prioritize_actions: bool,
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
            prioritize_actions: true,
        }
    }
}

pub struct State {
    config: Config,
    entries: Vec<DesktopEntry>,
}

mod scrubber;

#[handler]
pub fn handler(selection: Match, state: &State) -> HandleResult {
    let entry = state.entries.get(selection.id.unwrap() as usize).unwrap();

    let mut command = if let Some(script) = &state.config.preprocess_exec_script {
        let output_res = Command::new("sh")
            .arg("-c")
            .arg(format!(
                "{} {} {}",
                script.display(),
                if entry.term { "term" } else { "no-term" },
                entry.exec
            ))
            .stdout(Stdio::piped())
            .spawn()
            .and_then(|c| c.wait_with_output());

        match output_res {
            Ok(output) if output.status.success() => {
                let stdout = match String::from_utf8(output.stdout) {
                    Ok(out) => out,
                    Err(why) => {
                        // Stop here instead of using String::from_utf8_lossy(), which may introduce unexpected behaviours
                        eprintln!("[applications] Preprocess script did output a non-valid UTF-8 string: {}", why);

                        return HandleResult::Close;
                    }
                };
                stdout.trim().to_string()
            }
            Ok(output_failed) => {
                eprintln!("[applications] Preprocess script failed with status code: {}", output_failed.status);
                return HandleResult::Close;
            }
            Err(why) => {
                eprintln!("[applications] Error running preprocess script: {}", why);
                return HandleResult::Close;
            }
        }
    } else {
        entry.exec.clone()
    };

    if command.is_empty() {
        return HandleResult::Close;
    }

    if entry.term {
        command = match get_terminal_command_format(&state.config) {
            Some(cmd_fmt) => cmd_fmt.replace("{}", &command),
            None => {
                eprintln!("[applications] Error running terminal desktop entry: No terminal found");
                return HandleResult::Close;
            }
        };
    }

    if let Err(why) = run_command(&command, entry.path.as_deref()) {
        eprintln!("[applications] Error running desktop entry: {}", why);
    };

    HandleResult::Close
}

fn run_command(command: &str, path: Option<&Path>) -> io::Result<std::process::Child> {
    let current_dir = &env::current_dir().unwrap();

    Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(match path {
            Some(path) if path.exists() => path,
            _ => current_dir,
        })
        .spawn()
}

const TERMINAL_COMMAND_FORMATS: &[&str] = &[
    "alacritty -e {}",
    "foot -- {}",
    "kitty -- {}",
    "wezterm start -- {}",
    "wterm -e {}",
    "ghostty -e {}",
    "terminator -x {}",
    "gnome-terminal -- {}",
    "konsole -e {}",
    "xterm -e {}",

    // Check that the terminal handle unquoted commands correctly before adding it here
    // Also, the order matter, the first ones are tried first
];

fn get_terminal_command_format(config: &Config) -> Option<String> {
    if let Some(term) = &config.terminal {
        return Some(format!("{} {}", term.command, term.args));
    }

    for cmd_fmt in TERMINAL_COMMAND_FORMATS {
        if Command::new("which")
            .arg(cmd_fmt.split(' ').next().unwrap())
            .output()
            .is_ok_and(|output| output.status.success())
        {
            return Some(cmd_fmt.to_string());
        }
    }

    None
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

    let entries = scrubber::scrubber(&config);

    if entries.is_empty() {
        eprintln!("[applications] Warning: no desktop entries found")
    }

    State { config, entries }
}

#[get_matches]
pub fn get_matches(input: RString, state: &State) -> RVec<Match> {
    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default().ignore_case();
    let mut matching_entries = state
        .entries
        .iter()
        .enumerate()
        .filter_map(|(i, entry)| {
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

            if state.config.prioritize_actions && entry.is_action {
                score *= 2;
            }

            // Score cutoff
            if score > 0 {
                Some((entry, i as u64, score))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    matching_entries.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.name.cmp(&b.0.name)));

    matching_entries.truncate(state.config.max_entries);
    matching_entries
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
