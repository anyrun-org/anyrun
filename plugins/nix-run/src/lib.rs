use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::{HandleResult, Match, PluginInfo, get_matches, handler, info, init};
use fuzzy_matcher::FuzzyMatcher;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    io::Read,
    path::Path,
    process::Command,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

const CACHE_FILE: &str = "/anyrun/nix-run/packages.json";
const CACHE_LIFE: Duration = Duration::from_secs(604800); // 7 days, ought to be fine

#[derive(Deserialize, Clone)]
struct Config {
    channel: String,
    max_entries: usize,
    allow_unfree: bool,
    prefix: String,
}

#[derive(Deserialize)]
struct Nixpkgs {
    packages: HashMap<String, Package>,
}

#[derive(Deserialize, Serialize)]
struct Package {
    meta: Meta,
}

#[derive(Deserialize, Serialize)]
struct Meta {
    description: Option<String>,
    #[serde(rename = "mainProgram")]
    main_program: Option<String>,
    unfree: Option<bool>,
}

struct State {
    packages: Arc<Mutex<HashMap<String, Package>>>,
    config: Config,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            channel: "nixos-unstable".to_string(),
            max_entries: 3,
            allow_unfree: false,
            prefix: ":nr ".to_string(),
        }
    }
}

fn get_packages(
    config: Config,
    cache_path: Option<String>,
    packages: Arc<Mutex<HashMap<String, Package>>>,
) {
    eprintln!("[nix-run] Fetching package list...");
    thread::spawn(move || {
        match reqwest::blocking::get(format!(
            "https://channels.nixos.org/{}/packages.json.br",
            config.channel
        )) {
            Ok(response) => {
                eprintln!("[nix-run] Fetch complete");
                let text = response.text().unwrap();
                match serde_json::from_str::<Nixpkgs>(&text) {
                    Ok(nixpkgs) => {
                        let mut packages = packages.lock().unwrap();
                        if let Some(cache_path) = cache_path {
                            let path = Path::new(&cache_path);
                            match fs::create_dir_all(path.ancestors().nth(1).unwrap()) {
                                Ok(_) => {
                                    if let Err(why) = fs::write(
                                        cache_path,
                                        serde_json::to_vec_pretty(&nixpkgs.packages).unwrap(),
                                    ) {
                                        eprintln!("[nix-run] Failed to write package cache: {why}");
                                    }
                                }
                                Err(why) => {
                                    eprintln!("[nix-run] Failed to create cache directory: {why}");
                                }
                            }
                        }
                        *packages = nixpkgs.packages;
                    }
                    Err(why) => eprintln!("[nix-run] Failed to deserialize package list: {why}"),
                }
            }
            Err(why) => eprintln!("[nix-run] Failed to fetch package list: {why}"),
        }
    });
}

#[init]
fn init(config_dir: RString) -> State {
    let config = match fs::read_to_string(format!("{}/nix-run.ron", config_dir)) {
        Ok(content) => ron::from_str(&content).unwrap_or_else(|why| {
            eprintln!("[nix-run] Failed to parse config: {}", why);
            Config::default()
        }),
        Err(why) => {
            eprintln!("[nix-run] No config file provided, using default: {}", why);
            Config::default()
        }
    };

    // TODO: Break API to introduce support for cache path
    let cache_path = if let Ok(path) = env::var("XDG_CACHE_HOME") {
        Some(format!("{}{}", path, CACHE_FILE))
    } else if let Ok(path) = env::var("HOME") {
        Some(format!("{}/.cache{}", path, CACHE_FILE))
    } else {
        eprintln!("[nix-run] Failed to determine cache path, not caching");
        None
    };

    let packages = Arc::new(Mutex::new(HashMap::new()));

    if let Some(cache_path) = cache_path {
        if let Ok(mut file) = File::open(&cache_path) {
            let mut buf = Vec::new();
            file.read_to_end(&mut buf).unwrap();
            match serde_json::from_slice(&buf) {
                Ok(_packages) => {
                    *packages.lock().unwrap() = _packages;
                }
                Err(why) => {
                    eprintln!("[nix-run] Failed to parse cached package list: {why}");
                }
            }
            if file
                .metadata()
                .unwrap()
                .modified()
                .unwrap()
                .elapsed()
                .unwrap()
                > CACHE_LIFE
            {
                get_packages(config.clone(), Some(cache_path), packages.clone());
            }
        } else {
            get_packages(config.clone(), Some(cache_path), packages.clone());
        }
    } else {
        get_packages(config.clone(), cache_path, packages.clone());
    };

    State { packages, config }
}

#[info]
fn info() -> PluginInfo {
    PluginInfo {
        name: "nix-run".into(),
        icon: "weather-snow".into(),
    }
}

#[get_matches]
fn get_matches(input: RString, state: &mut State) -> RVec<Match> {
    let input = if let Some(input) = input.strip_prefix(&state.config.prefix) {
        input.trim()
    } else {
        return RVec::new();
    };

    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default().smart_case();
    let packages = state.packages.lock().unwrap();
    let mut entries = packages
        .iter()
        .filter_map(|(name, package)| {
            // There is no mainProgram, so program is not directly runnable
            package.meta.main_program.as_ref()?;
            if package.meta.unfree.is_some_and(|unfree| unfree) && !state.config.allow_unfree {
                return None;
            }
            let score = matcher.fuzzy_match(name, input).unwrap_or(0)
                + matcher
                    .fuzzy_match(package.meta.main_program.as_ref().unwrap(), input)
                    .unwrap_or(0);

            if score > 0 {
                Some((name, package, score))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    entries.sort_by(|a, b| b.2.cmp(&a.2));
    entries.truncate(state.config.max_entries);

    entries
        .into_iter()
        .map(|(name, package, _)| Match {
            title: name.clone().into(),
            description: package
                .meta
                .description
                .clone()
                .map(|desc| desc.into())
                .into(),
            use_pango: false,
            icon: ROption::RNone,
            id: ROption::RNone,
        })
        .collect()
}

#[handler]
fn handler(selection: Match, state: &State) -> HandleResult {
    let mut command = Command::new("nix");
    command.args(["run", &format!("nixpkgs#{}", selection.title)]);
    if state.config.allow_unfree {
        command.env("NIXPKGS_ALLOW_UNFREE", "1");
        command.arg("--impure");
    }

    // A zombie process is exactly what we want
    #[allow(clippy::zombie_processes)]
    command.spawn().unwrap();

    HandleResult::Close
}
