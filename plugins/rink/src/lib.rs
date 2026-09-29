use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::*;
use serde::Deserialize;
use std::fs;

#[derive(Deserialize, Debug)]
struct Config {
    prefix: String,
    #[serde(default = "Config::default_pull_currencies")]
    pull_currencies: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            prefix: "".to_string(),
            pull_currencies: true,
        }
    }
}

impl Config {
    fn default_pull_currencies() -> bool {
        true
    }
}

struct State {
    ctx: rink_core::Context,
    config: Config,
}

#[init]
fn init(config_dir: RString) -> State {
    let config = match fs::read_to_string(format!("{config_dir}/rink.ron")) {
        Ok(content) => ron::from_str(&content).unwrap_or_else(|why| {
            eprintln!("[rink] Failed to parse config: {why}");
            Config::default()
        }),
        Err(why) => {
            eprintln!("[rink] No config file provided, using default: {why}");
            Config::default()
        }
    };

    let mut ctx = rink_core::simple_context().unwrap();

    let live_data = if config.pull_currencies {
        match reqwest::blocking::get("https://rinkcalc.app/data/currency.json") {
            // The error will just be handled further down the line
            Ok(response) => Some(response.text().unwrap_or_default()),
            Err(why) => {
                eprintln!("[rink] Error fetching up-to-date currency conversions: {why}");
                None
            }
        }
    } else {
        None
    };

    let base_currencies = rink_core::CURRENCY_FILE.unwrap();
    if let Err(why) = ctx.load_currency(live_data.as_deref(), base_currencies) {
        eprintln!("[rink] Error loading currencies: {why}, retrying with static currency data");
        if let Err(why) = ctx.load_currency(None, base_currencies) {
            eprintln!("[rink] Loading static currency data failed as well: {why}");
        }
    }

    State { ctx, config }
}

#[info]
fn info() -> PluginInfo {
    PluginInfo {
        name: "Rink".into(),
        icon: "accessories-calculator".into(),
    }
}

#[get_matches]
fn get_matches(input: RString, state: &mut State) -> RVec<Match> {
    let input = if let Some(input) = input.strip_prefix(&state.config.prefix) {
        input.trim()
    } else {
        return RVec::new();
    };

    match rink_core::one_line(&mut state.ctx, input) {
        Ok(result) => {
            let (title, desc) = parse_result(result);
            vec![Match {
                title: title.into(),
                description: desc.map(RString::from).into(),
                use_pango: false,
                icon: ROption::RNone,
                id: ROption::RNone,
            }]
            .into()
        }
        Err(_) => RVec::new(),
    }
}

#[handler]
fn handler(selection: Match) -> HandleResult {
    HandleResult::Copy(selection.title.into_bytes())
}

/// Extracts the title and description from `rink` result.
/// The description is anything inside brackets from `rink`, if present.
fn parse_result(result: String) -> (String, Option<String>) {
    result
        .split_once(" (")
        .map(|(title, desc)| {
            (
                title.to_string(),
                Some(desc.trim_end_matches(')').to_string()),
            )
        })
        .unwrap_or((result, None))
}
