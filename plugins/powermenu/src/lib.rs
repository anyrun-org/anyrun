#![allow(clippy::needless_pass_by_value, clippy::wildcard_imports)]
mod actions;
mod config;

use core::str;
use std::{
    fs,
    io::Error,
    process::{Command, Output},
};

use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::*;
use ron::Result;

use actions::{ConfirmAction, PowerAction};
use config::{Config, PowerActionConfig};

pub struct State {
    config: Config,
    pending_action: Option<PowerAction>,
    error_message: Option<String>,
}

#[init]
fn init(config_dir: RString) -> State {
    let config = fs::read_to_string(format!("{config_dir}/powermenu.ron"))
        .map_or(Config::default(), |content| {
            ron::from_str(&content).unwrap_or_default()
        });

    State {
        config,
        pending_action: None,
        error_message: None,
    }
}

#[info]
fn info() -> PluginInfo {
    PluginInfo {
        name: "Power menu".into(),
        icon: "computer".into(),
    }
}

#[get_matches]
fn get_matches(input: RString, state: &State) -> RVec<Match> {
    if input.is_empty() {
        vec![]
    } else if let Some(ref error_message) = state.error_message {
        get_error_matches(error_message)
    } else if let Some(pending_action) = state.pending_action {
        get_confirm_matches(pending_action)
    } else {
        PowerAction::get_fuzzy_matching_values(&input)
            .map(PowerAction::as_match)
            .collect()
    }
    .into()
}

fn get_error_matches(error_message: &str) -> Vec<Match> {
    vec![Match {
        title: "ERROR!".into(),
        icon: ROption::RSome("dialog-error".into()),
        use_pango: false,
        description: ROption::RSome(error_message.into()),
        id: ROption::RSome(ConfirmAction::Confirm.into()),
    }]
}

fn get_confirm_matches(action_to_confirm: PowerAction) -> Vec<Match> {
    vec![
        Match {
            title: action_to_confirm.get_title().into(),
            icon: ROption::RSome("go-next".into()),
            use_pango: false,
            description: ROption::RSome("Proceed with the selected action".into()),
            id: ROption::RSome(ConfirmAction::Confirm.into()),
        },
        Match {
            title: "Cancel".into(),
            icon: ROption::RSome("go-previous".into()),
            use_pango: false,
            description: ROption::RSome("Abort the selected action".into()),
            id: ROption::RSome(ConfirmAction::Cancel.into()),
        },
    ]
}

#[handler]
fn handler(selection: Match, state: &mut State) -> HandleResult {
    if state.error_message.is_some() {
        return HandleResult::Close;
    }

    let power_action_config = if let Some(ref pending_action) = state.pending_action {
        let confirm_action = ConfirmAction::try_from(selection.id.unwrap()).unwrap();

        if !confirm_action.is_confirmed() {
            state.pending_action = None;
            return HandleResult::Refresh(false);
        }

        state.config.get_action_config(*pending_action)
    } else {
        let power_action = PowerAction::try_from(selection.id.unwrap()).unwrap();
        let power_action_config = state.config.get_action_config(power_action);

        if power_action_config.confirm {
            state.pending_action = Some(power_action);
            return HandleResult::Refresh(true);
        };

        power_action_config
    };

    let action_result = execute_power_action(power_action_config);
    let error_message = get_error_message(action_result);
    if error_message.is_some() {
        state.error_message = error_message;
        return HandleResult::Refresh(true);
    }

    HandleResult::Close
}

fn execute_power_action(action: &PowerActionConfig) -> Result<Output, std::io::Error> {
    Command::new("/usr/bin/env")
        .arg("sh")
        .arg("-c")
        .arg(&action.command)
        .output()
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
