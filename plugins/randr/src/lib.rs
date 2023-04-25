use std::env;

use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::{anyrun_interface::HandleResult, plugin, Match, PluginInfo};
use fuzzy_matcher::FuzzyMatcher;
use randr::{dummy::Dummy, hyprland::Hyprland, Configure, Monitor, Randr};

mod randr;

enum InnerState {
    None,
    Position(Monitor),
}

pub struct State {
    randr: Box<dyn Randr + Send + Sync>,
    inner: InnerState,
}

pub fn init(_config_dir: RString) -> State {
    // Determine which Randr implementation should be used
    let randr: Box<dyn Randr + Send + Sync> = if env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
        Box::new(Hyprland::new())
    } else {
        Box::new(Dummy)
    };

    State {
        randr,
        inner: InnerState::None,
    }
}

pub fn info() -> PluginInfo {
    PluginInfo {
        name: "Randr".into(),
        icon: "video-display".into(),
    }
}

pub fn handler(_match: Match, state: &mut State) -> HandleResult {
    match &state.inner {
        InnerState::None => {
            state.inner = InnerState::Position(
                state
                    .randr
                    .get_monitors()
                    .into_iter()
                    .find(|mon| mon.id == _match.id.unwrap())
                    .unwrap(),
            );
            HandleResult::Refresh(true)
        }
        InnerState::Position(mon) => {
            if _match.id.unwrap() == u64::MAX {
                state.inner = InnerState::None;
                return HandleResult::Refresh(false);
            }

            let rel_id = (_match.id.unwrap() >> 32) as u32;
            let action = _match.id.unwrap() as u32;

            let rel_mon = state
                .randr
                .get_monitors()
                .into_iter()
                .find(|mon| mon.id == rel_id as u64)
                .unwrap();

            state
                .randr
                .configure(mon, Configure::from_id(action, &rel_mon));

            HandleResult::Close
        }
    }
}

pub fn get_matches(input: RString, state: &mut State) -> RVec<Match> {
    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default().smart_case();
    let mut vec = match &state.inner {
        InnerState::None => state
            .randr
            .get_monitors()
            .into_iter()
            .map(|mon| Match {
                title: format!("Change position of {}", mon.name).into(),
                description: ROption::RSome(
                    format!("{}x{} at {}x{}", mon.width, mon.height, mon.x, mon.y).into(),
                ),
                use_pango: false,
                icon: ROption::RSome("object-flip-horizontal".into()),
                id: ROption::RSome(mon.id),
            })
            .collect::<RVec<_>>(),
        InnerState::Position(mon) => {
            let mut vec = state
                .randr
                .get_monitors()
                .into_iter()
                .filter_map(|_mon| {
                    if _mon == *mon {
                        None
                    } else {
                        Some(
                            [
                                Configure::Mirror(&_mon),
                                Configure::LeftOf(&_mon),
                                Configure::RightOf(&_mon),
                                Configure::Below(&_mon),
                                Configure::Above(&_mon),
                            ]
                            .iter()
                            .map(|configure| Match {
                                title: format!("{} {}", configure.to_string(), _mon.name).into(),
                                description: ROption::RNone,
                                use_pango: false,
                                icon: ROption::RSome(configure.icon().into()),
                                // Store 2 32 bit IDs in the single 64 bit integer, a bit of a hack
                                id: ROption::RSome(_mon.id << 32 | Into::<u64>::into(configure)),
                            })
                            .collect::<Vec<_>>(),
                        )
                    }
                })
                .flatten()
                .collect::<RVec<_>>();

            vec.push(Match {
                title: "Reset position".into(),
                description: ROption::RNone,
                use_pango: false,
                icon: ROption::RSome(Configure::Zero.icon().into()),
                id: ROption::RSome((&Configure::Zero).into()),
            });

            vec.push(Match {
                title: "Back".into(),
                description: ROption::RSome("Return to the previous menu".into()),
                use_pango: false,
                icon: ROption::RSome("edit-undo".into()),
                id: ROption::RSome(u64::MAX),
            });

            vec
        }
    }
    .into_iter()
    .filter_map(|_match| {
        matcher
            .fuzzy_match(&_match.title, &input)
            .map(|score| (_match, score))
    })
    .collect::<Vec<_>>();

    vec.sort_by(|a, b| b.1.cmp(&a.1));

    vec.truncate(5);

    vec.into_iter().map(|(_match, _)| _match).collect()
}

plugin!(init, info, get_matches, handler, State);
