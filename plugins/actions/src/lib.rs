#![allow(clippy::needless_pass_by_value, clippy::wildcard_imports)]

use core::str;
use std::{
    fs,
    io::Error,
    process::{Command, Output},
};

use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::*;
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use serde::Deserialize;

const CONFIRM_ID: u64 = u64::MAX;
const CANCEL_ID: u64 = u64::MAX - 1;

#[derive(Deserialize)]
struct Action {
    title: String,
    command: String,
    #[serde(default)]
    description: String,
    #[serde(default = "Action::default_icon")]
    icon: String,
    #[serde(default)]
    confirm: bool,
}

impl Action {
    fn default_icon() -> String {
        String::from("system-run")
    }

    fn as_match(&self, index: usize) -> Match {
        Match {
            title: self.title.as_str().into(),
            icon: ROption::RSome(self.icon.as_str().into()),
            use_pango: false,
            description: ROption::RSome(self.description.as_str().into()),
            id: ROption::RSome(index as u64),
        }
    }

    fn fuzzy_score(&self, matcher: &impl FuzzyMatcher, phrase: &str) -> Option<i64> {
        matcher.fuzzy_match(&self.title, phrase).max(
            matcher
                .fuzzy_match(&self.description, phrase)
                .map(|x| x / 2),
        )
    }

    fn get_confirmation_matches(&self) -> Vec<Match> {
        vec![
            Match {
                title: self.title.as_str().into(),
                icon: ROption::RSome("go-next".into()),
                use_pango: false,
                description: ROption::RSome("Proceed with the selected action".into()),
                id: ROption::RSome(CONFIRM_ID),
            },
            Match {
                title: "Cancel".into(),
                icon: ROption::RSome("go-previous".into()),
                use_pango: false,
                description: ROption::RSome("Abort the selected action".into()),
                id: ROption::RSome(CANCEL_ID),
            },
        ]
    }

    fn execute(&self) -> Result<Output, std::io::Error> {
        Command::new("/usr/bin/env")
            .arg("sh")
            .arg("-c")
            .arg(&self.command)
            .output()
    }
}

fn power_actions() -> Vec<Action> {
    vec![
        Action {
            title: String::from("Lock"),
            command: String::from("loginctl lock-session"),
            description: String::from("Lock the session screen"),
            icon: String::from("system-lock-screen"),
            confirm: false,
        },
        Action {
            title: String::from("Log out"),
            command: String::from("loginctl terminate-session $XDG_SESSION_ID"),
            description: String::from("Terminate the session"),
            icon: String::from("system-log-out"),
            confirm: true,
        },
        Action {
            title: String::from("Power off"),
            command: String::from("systemctl poweroff || poweroff"),
            description: String::from("Shut down the system"),
            icon: String::from("system-shutdown"),
            confirm: true,
        },
        Action {
            title: String::from("Reboot"),
            command: String::from("systemctl reboot || reboot"),
            description: String::from("Restart the system"),
            icon: String::from("system-reboot"),
            confirm: true,
        },
        Action {
            title: String::from("Suspend"),
            command: String::from("systemctl suspend || pm-suspend"),
            description: String::from("Suspend the system to RAM"),
            icon: String::from("system-suspend"),
            confirm: false,
        },
        Action {
            title: String::from("Hibernate"),
            command: String::from("systemctl hibernate || pm-hibernate"),
            description: String::from("Suspend the system to disk"),
            icon: String::from("system-suspend-hibernate"),
            confirm: false,
        },
    ]
}

#[derive(Deserialize)]
struct Config {
    #[serde(default = "Config::default_enable_power_actions")]
    enable_power_actions: bool,
    #[serde(default)]
    custom_actions: Vec<Action>,
}

impl Config {
    const fn default_enable_power_actions() -> bool {
        true
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enable_power_actions: Self::default_enable_power_actions(),
            custom_actions: Vec::new(),
        }
    }
}

pub struct State {
    actions: Vec<Action>,
    pending_action: Option<usize>,
    error_message: Option<String>,
}

#[init]
fn init(config_dir: RString) -> State {
    let config = match fs::read_to_string(format!("{config_dir}/actions.ron")) {
        Ok(content) => ron::from_str(&content).unwrap_or_else(|why| {
            eprintln!("[actions] Failed to parse config: {why}");
            Config::default()
        }),
        Err(why) => {
            eprintln!("[actions] Failed to read config: {why}");
            Config::default()
        }
    };

    let mut actions: Vec<Action> = Vec::new();
    if config.enable_power_actions {
        actions.extend(power_actions());
    }
    actions.extend(config.custom_actions);

    State {
        actions,
        pending_action: None,
        error_message: None,
    }
}

#[info]
fn info() -> PluginInfo {
    PluginInfo {
        name: "Actions".into(),
        icon: "system-run".into(),
    }
}

#[get_matches]
fn get_matches(input: RString, state: &State) -> RVec<Match> {
    if input.is_empty() {
        vec![]
    } else if let Some(error_message) = &state.error_message {
        get_error_matches(error_message)
    } else if let Some(pending_index) = state.pending_action {
        state.actions[pending_index].get_confirmation_matches()
    } else {
        get_fuzzy_matches(&state.actions, &input)
    }
    .into()
}

fn get_fuzzy_matches(actions: &[Action], phrase: &str) -> Vec<Match> {
    let fuzzy_matcher = SkimMatcherV2::default().ignore_case();
    let mut matches_with_scores: Vec<(usize, i64)> = actions
        .iter()
        .enumerate()
        .filter_map(|(index, action)| {
            action
                .fuzzy_score(&fuzzy_matcher, phrase)
                .map(|score| (index, score))
        })
        .collect();
    matches_with_scores.sort_by_key(|(_index, score)| *score);
    matches_with_scores
        .into_iter()
        .rev()
        .map(|(index, _score)| actions[index].as_match(index))
        .collect()
}

fn get_error_matches(error_message: &str) -> Vec<Match> {
    vec![Match {
        title: "ERROR!".into(),
        icon: ROption::RSome("dialog-error".into()),
        use_pango: false,
        description: ROption::RSome(error_message.into()),
        id: ROption::RNone,
    }]
}

const fn is_response_to_pending(id: u64) -> Option<bool> {
    match id {
        CONFIRM_ID => Some(true),
        CANCEL_ID => Some(false),
        _ => None,
    }
}

#[handler]
fn handler(selection: Match, state: &mut State) -> HandleResult {
    if state.error_message.is_some() {
        return HandleResult::Close;
    }

    let id = selection.id.unwrap();

    let action_index = if let Some(is_confirmed) = is_response_to_pending(id) {
        if !is_confirmed {
            state.pending_action = None;
            return HandleResult::Refresh(false);
        }
        state.pending_action.unwrap()
    } else {
        let index = id as usize;
        if state.actions[index].confirm {
            state.pending_action = Some(index);
            return HandleResult::Refresh(true);
        }
        index
    };

    let action = &state.actions[action_index];
    let action_result = action.execute();
    let error_message = get_error_message(action_result);
    if error_message.is_some() {
        state.error_message = error_message;
        return HandleResult::Refresh(true);
    }

    HandleResult::Close
}

fn get_error_message(command_result: Result<Output, Error>) -> Option<String> {
    match command_result {
        Err(err) => Some(format!("Could not run command: {err}")),
        Ok(output) if !output.status.success() => Some(format!(
            "{}, stderr: {}",
            output.status,
            String::from_utf8_lossy(output.stderr.as_ref())
        )),
        Ok(_) => None,
    }
}
