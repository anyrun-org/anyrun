use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::*;
use serde::Deserialize;
use std::{fmt, fs, process::Command};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use urlencoding::encode;

#[derive(Debug, Clone, Deserialize, EnumIter)]
enum Engine {
    Google,
    Ecosia,
    Bing,
    DuckDuckGo,
    Custom { name: String, url: String },
}

impl Engine {
    fn value(&self) -> &str {
        match self {
            Self::Google => "google.com/search?q=",
            Self::Ecosia => "www.ecosia.org/search?q=",
            Self::Bing => "www.bing.com/search?q=",
            Self::DuckDuckGo => "duckduckgo.com/?q=",
            Self::Custom { url, .. } => url,
        }
    }
}

impl fmt::Display for Engine {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Google => write!(f, "Google"),
            Self::Ecosia => write!(f, "Ecosia"),
            Self::Bing => write!(f, "Bing"),
            Self::DuckDuckGo => write!(f, "DuckDuckGo"),
            Self::Custom { name, .. } => write!(f, "{}", name),
        }
    }
}

#[derive(Deserialize)]
struct Config {
    prefix: String,
    engines: Vec<Engine>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            prefix: "?".to_string(),
            engines: vec![Engine::Google],
        }
    }
}

#[init]
fn init(config_dir: RString) -> Config {
    match fs::read_to_string(format!("{}/websearch.ron", config_dir)) {
        Ok(content) => ron::from_str(&content).unwrap_or_default(),
        Err(_) => Config::default(),
    }
}

#[info]
fn info() -> PluginInfo {
    PluginInfo {
        name: "Websearch".into(),
        icon: "help-about".into(),
    }
}

#[get_matches]
fn get_matches(input: RString, config: &Config) -> RVec<Match> {
    if !input.starts_with(&config.prefix) {
        RVec::new()
    } else {
        config
            .engines
            .clone()
            .into_iter()
            .map(|engine| Match {
                title: input.trim_start_matches(&config.prefix).into(),
                description: ROption::RSome(format!("Search with {}", engine).into()),
                use_pango: false,
                icon: ROption::RNone,
                id: ROption::RNone,
            })
            .collect()
    }
}

#[handler]
fn handler(selection: Match) -> HandleResult {
    for engine in Engine::iter() {
        if selection
            .description
            .clone()
            .unwrap()
            .to_string()
            .contains(&engine.to_string())
        {
            if let Err(why) = Command::new("sh")
                .arg("-c")
                .arg(format!(
                    "xdg-open https://{}{}",
                    engine.value(),
                    encode(&selection.title.to_string())
                ))
                .spawn()
            {
                println!("Failed to perform websearch: {}", why);
            }
        }
    }

    HandleResult::Close
}
