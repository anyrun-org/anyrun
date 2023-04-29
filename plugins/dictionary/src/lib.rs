use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::{anyrun_interface::HandleResult, plugin, Match, PluginInfo};
use serde::Deserialize;

#[derive(Deserialize)]
struct ApiResponse {
    word: String,
    phonetic: Option<String>,
    phonetics: Vec<Phonetic>,
    origin: Option<String>,
    meanings: Vec<Meaning>,
}

#[derive(Deserialize)]
struct Phonetic {
    text: String,
    audio: Option<String>,
}

#[derive(Deserialize)]
struct Meaning {
    #[serde(rename = "partOfSpeech")]
    part_of_speech: String,
    definitions: Vec<Definition>,
}

#[derive(Deserialize)]
struct Definition {
    definition: String,
    example: Option<String>,
    synonyms: Vec<String>,
    antonyms: Vec<String>,
}

pub fn init(_config_dir: RString) {}

pub fn handler(_match: Match, _: &mut ()) -> HandleResult {
    HandleResult::Copy(_match.title.into_bytes())
}

pub fn get_matches(input: RString, _: &()) -> RVec<Match> {
    if !input.starts_with(":def") {
        return RVec::new();
    }

    let input = &input[4..].trim();

    let responses: Vec<ApiResponse> = match reqwest::blocking::get(format!(
        "https://api.dictionaryapi.dev/api/v2/entries/en/{}",
        input
    )) {
        Ok(response) => match response.json() {
            Ok(response) => response,
            Err(why) => {
                eprintln!("Error deserializing response: {}", why);
                return RVec::new();
            }
        },
        Err(why) => {
            eprintln!("Error fetching dictionary result: {}", why);
            return RVec::new();
        }
    };

    responses
        .into_iter()
        .flat_map(|response| {
            response
                .meanings
                .into_iter()
                .flat_map(|meaning| {
                    meaning
                        .definitions
                        .into_iter()
                        .map(|definition| Match {
                            title: definition.definition.into(),
                            description: ROption::RSome(meaning.part_of_speech.clone().into()),
                            use_pango: false,
                            icon: ROption::RSome("accessories-dictionary".into()),
                            id: ROption::RNone,
                        })
                        .collect::<RVec<_>>()
                })
                .collect::<RVec<_>>()
        })
        .take(3)
        .collect()
}

fn info() -> PluginInfo {
    PluginInfo {
        name: "Dictionary".into(),
        icon: "accessories-dictionary".into(),
    }
}

plugin!(init, info, get_matches, handler, ());
