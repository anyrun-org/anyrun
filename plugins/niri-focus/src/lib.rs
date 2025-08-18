use std::{convert::Into, fs};

use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::{HandleResult, Match, PluginInfo, get_matches, handler, info, init};
use fuzzy_matcher::FuzzyMatcher;
use niri_ipc::{Action, Request, Window, socket::Socket};
use serde::Deserialize;

#[derive(Deserialize)]
struct Config {
    max_entries: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self { max_entries: 2 }
    }
}

struct State {
    config: Config,
    socket: Socket,
    windows: Vec<Window>,
}

#[init]
fn init(config_dir: RString) -> Option<State> {
    let config = match fs::read_to_string(format!("{config_dir}/niri-focus.ron")) {
        Ok(content) => ron::from_str(&content).unwrap_or_else(|why| {
            eprintln!("[niri-focus] Failed to parse config: {why}");
            Config::default()
        }),
        Err(why) => {
            eprintln!("[niri-focus] No config file provided, using default: {why}");
            Config::default()
        }
    };

    let Ok(mut socket) = Socket::connect() else {
        eprintln!("[niri-focus] Failed to connect to niri socket");
        return None;
    };

    let windows = if let Ok(Ok(niri_ipc::Response::Windows(windows))) =
        socket.send(niri_ipc::Request::Windows)
    {
        windows
    } else {
        eprintln!("[niri-focus] Failed to get window list from niri");
        return None;
    };

    Some(State {
        config,
        socket,
        windows,
    })
}

#[info]
fn info() -> PluginInfo {
    PluginInfo {
        name: "niri-focus".into(),
        icon: "preferences-system-windows".into(),
    }
}

#[get_matches]
fn get_matches(input: RString, state: &Option<State>) -> RVec<Match> {
    let Some(state) = state else {
        return RVec::new();
    };
    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default().smart_case();
    let mut entries = state
        .windows
        .iter()
        .filter_map(|window| {
            let score = window
                .title
                .as_ref()
                .and_then(|title| matcher.fuzzy_match(title, &input))
                .unwrap_or(0)
                + window
                    .app_id
                    .as_ref()
                    .and_then(|app_id| matcher.fuzzy_match(app_id, &input))
                    .unwrap_or(0);

            if score > 0 {
                Some((window, score))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    entries.sort_by(|a, b| b.1.cmp(&a.1));
    entries.truncate(state.config.max_entries);

    entries
        .into_iter()
        .map(|(window, _)| Match {
            title: window.title.clone().unwrap_or_default().into(),
            description: ROption::RSome(
                window
                    .app_id
                    .clone()
                    .map(|app_id| format!("Focus window - {app_id}"))
                    .unwrap_or("Focus window".to_string())
                    .into(),
            ),
            use_pango: false,
            icon: window.app_id.clone().map(Into::<RString>::into).into(),
            id: ROption::RSome(window.id),
        })
        .collect()
}

#[handler]
fn handler(sel: Match, state: &mut Option<State>) -> HandleResult {
    if let Err(why) = state
        .as_mut()
        .unwrap()
        .socket
        .send(Request::Action(Action::FocusWindow {
            id: sel.id.unwrap(),
        }))
    {
        eprintln!("[niri-focus] Error focusing window: {why}");
    }

    HandleResult::Close
}
