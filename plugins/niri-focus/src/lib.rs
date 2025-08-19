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
    windows: Vec<(Option<String>, Window)>,
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

    let entries = freedesktop_desktop_entry::desktop_entries(
        &freedesktop_desktop_entry::get_languages_from_env(),
    );

    Some(State {
        config,
        socket,
        windows: windows
            .into_iter()
            .map(|win| {
                (
                    win.app_id
                        .as_ref()
                        .and_then(|app_id| {
                            freedesktop_desktop_entry::find_app_by_id(
                                &entries,
                                freedesktop_desktop_entry::unicase::Ascii::new(app_id),
                            )
                        })
                        .and_then(|entry| entry.icon())
                        .map(|icon| icon.to_string()),
                    win,
                )
            })
            .collect(),
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
        .filter_map(|(icon, window)| {
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
                Some(((icon, window), score))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    entries.sort_by(|a, b| b.1.cmp(&a.1));
    entries.truncate(state.config.max_entries);

    entries
        .into_iter()
        .map(|((icon, window), _)| Match {
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
            icon: icon.as_ref().map(|icon| icon.clone().into()).into(),
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
