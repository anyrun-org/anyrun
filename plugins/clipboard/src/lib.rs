use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::*;
use serde::Deserialize;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Deserialize)]
pub struct Config {
    prefix: String,
    max_entries: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            prefix: "".to_string(),
            max_entries: 15,
        }
    }
}

#[init]
pub fn init(config_dir: RString) -> Config {
    match fs::read_to_string(format!("{}/clipboard.ron", config_dir)) {
        Ok(content) => ron::from_str(&content).unwrap_or_default(),
        Err(_) => Config::default(),
    }
}

#[info]
fn info() -> PluginInfo {
    PluginInfo {
        name: "Clipboard".into(),
        icon: "edit-paste-symbolic".into(),
    }
}

#[get_matches]
pub fn get_matches(input: RString, config: &Config) -> RVec<Match> {
    // If prefix is empty string (default), search term is just the input.
    // If prefix is set, we check for it.
    let search_query = if config.prefix.is_empty() {
        input.as_str()
    } else {
        if let Some(stripped) = input.strip_prefix(&config.prefix) {
            stripped.trim()
        } else {
            return RVec::new();
        }
    };

    let output = Command::new("cliphist").arg("list").output();

    match output {
        Ok(out) => {
            let history = String::from_utf8_lossy(&out.stdout);
            history
                .lines()
                .filter(|line| {
                    if search_query.is_empty() {
                        true
                    } else {
                        line.to_lowercase().contains(&search_query.to_lowercase())
                    }
                })
                .take(config.max_entries)
                .map(|line| Match {
                    title: line.trim().into(),
                    description: ROption::RNone,
                    icon: ROption::RSome("edit-paste-symbolic".into()),
                    id: ROption::RNone,
                    use_pango: false,
                })
                .collect::<Vec<_>>()
                .into()
        }
        Err(_) => RVec::new(),
    }
}

#[handler]
pub fn handler(selection: Match) -> HandleResult {
    let mut decode_child = Command::new("cliphist")
        .arg("decode")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn cliphist");

    if let Some(mut stdin) = decode_child.stdin.take() {
        let _ = stdin.write_all(selection.title.as_bytes());
    }

    if let Ok(output) = decode_child.wait_with_output() {
        let mut copy_child = Command::new("wl-copy")
            .arg("-n")
            .stdin(Stdio::piped())
            .spawn()
            .expect("Failed to spawn wl-copy");

        if let Some(mut copy_stdin) = copy_child.stdin.take() {
            let _ = copy_stdin.write_all(&output.stdout);
        }
        let _ = copy_child.wait();
    }

    HandleResult::Close
}
