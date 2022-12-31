use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::{anyrun_interface::HandleResult, plugin, Match, PluginInfo};

fn init(_config_dir: RString) {}

fn info() -> PluginInfo {
    PluginInfo {
        name: "Rink".into(),
        icon: "accessories-calculator".into(),
    }
}

fn get_matches(input: RString, _: &mut ()) -> RVec<Match> {
    let mut ctx = rink_core::simple_context().unwrap();
    match rink_core::one_line(&mut ctx, &input) {
        Ok(result) => vec![Match {
            title: result.into(),
            icon: ROption::RNone,
            description: ROption::RNone,
            id: ROption::RNone,
        }]
        .into(),
        Err(_) => RVec::new(),
    }
}

fn handler(selection: Match, _: &mut ()) -> HandleResult {
    HandleResult::Copy(selection.title.into_bytes())
}

plugin!(init, info, get_matches, handler, ());
