use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::{anyrun_interface::HandleResult, plugin, Match, PluginInfo};

fn init(_config_dir: RString) {
    // Currently broken due to limitations with the declarative macro anyrun_plugin crate.
    // For any plugin that does something relatively time intensive like fetching something
    // from the internet, the internal Mutex would block making requests way too long when typed rapidly.

    // TODO: Redesign the anyrun_plugin crate to allow for both mutable and non mutable borrows of the
    // shared data for functions

    /*let mut ctx = rink_core::Context::new();

    let units = gnu_units::parse_str(rink_core::DEFAULT_FILE.unwrap());
    let dates = date::parse_datefile(rink_core::DATES_FILE);

    let mut currency_defs = Vec::new();

    match reqwest::blocking::get("https://rinkcalc.app/data/currency.json") {
        Ok(response) => match response.json::<ast::Defs>() {
            Ok(mut live_defs) => {
                currency_defs.append(&mut live_defs.defs);
            }
            Err(why) => println!("Error parsing currency json: {}", why),
        },
        Err(why) => println!("Error fetching up-to-date currency conversions: {}", why),
    }

    currency_defs.append(&mut gnu_units::parse_str(CURRENCY_FILE).defs);

    ctx.load(units);
    ctx.load(ast::Defs {
        defs: currency_defs,
    });
    ctx.load_dates(dates);

    ctx*/
}

fn info() -> PluginInfo {
    PluginInfo {
        name: "Rink".into(),
        icon: "accessories-calculator".into(),
    }
}

fn get_matches(input: RString, _: &()) -> RVec<Match> {
    match rink_core::one_line(
        &mut rink_core::simple_context().expect("Failed to create rink context"),
        &input,
    ) {
        Ok(result) => vec![Match {
            title: result.into(),
            description: ROption::RNone,
            use_pango: false,
            icon: ROption::RNone,
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
