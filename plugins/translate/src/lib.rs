use std::fs;

use abi_stable::std_types::{ROption, RString, RVec};
use anyrun_plugin::*;
use fuzzy_matcher::FuzzyMatcher;
use serde::Deserialize;

#[derive(Deserialize)]
struct Config {
    prefix: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            prefix: ":".to_string(),
        }
    }
}

struct State {
    config: Config,
    langs: Vec<(&'static str, &'static str)>,
}

#[init]
fn init(config_dir: RString) -> State {
    State {
        config: match fs::read_to_string(format!("{}/translate.ron", config_dir)) {
            Ok(content) => ron::from_str(&content).unwrap_or_default(),
            Err(_) => Config::default(),
        },
        langs: vec![
            ("af", "Afrikaans"),
            ("sq", "Albanian"),
            ("am", "Amharic"),
            ("ar", "Arabic"),
            ("hy", "Armenian"),
            ("az", "Azerbaijani"),
            ("eu", "Basque"),
            ("be", "Belarusian"),
            ("bn", "Bengali"),
            ("bs", "Bosnian"),
            ("bg", "Bulgarian"),
            ("ca", "Catalan"),
            ("ceb", "Cebuano"),
            ("ny", "Chichewa"),
            ("zh-CN", "Chinese-(Simplified)"),
            ("zh-TW", "Chinese-(Traditional)"),
            ("co", "Corsican"),
            ("hr", "Croatian"),
            ("cs", "Czech"),
            ("da", "Danish"),
            ("nl", "Dutch"),
            ("en", "English"),
            ("eo", "Esperanto"),
            ("et", "Estonian"),
            ("tl", "Filipino"),
            ("fi", "Finnish"),
            ("fr", "French"),
            ("fy", "Frisian"),
            ("gl", "Galician"),
            ("ka", "Georgian"),
            ("de", "German"),
            ("el", "Greek"),
            ("gu", "Gujarati"),
            ("ht", "Haitian Creole"),
            ("ha", "Hausa"),
            ("haw", "Hawaiian"),
            ("iw", "Hebrew"),
            ("hi", "Hindi"),
            ("hmn", "Hmong"),
            ("hu", "Hungarian"),
            ("is", "Icelandic"),
            ("ig", "Igbo"),
            ("id", "Indonesian"),
            ("ga", "Irish"),
            ("it", "Italian"),
            ("ja", "Japanese"),
            ("jw", "Javanese"),
            ("kn", "Kannada"),
            ("kk", "Kazakh"),
            ("km", "Khmer"),
            ("ko", "Korean"),
            ("ku", "Kurdish-(Kurmanji)"),
            ("ky", "Kyrgyz"),
            ("lo", "Lao"),
            ("la", "Latin"),
            ("lv", "Latvian"),
            ("lt", "Lithuanian"),
            ("lb", "Luxembourgish"),
            ("mk", "Macedonian"),
            ("mg", "Malagasy"),
            ("ms", "Malay"),
            ("ml", "Malayalam"),
            ("mt", "Maltese"),
            ("mi", "Maori"),
            ("mr", "Marathi"),
            ("mn", "Mongolian"),
            ("my", "Myanmar-(Burmese)"),
            ("ne", "Nepali"),
            ("no", "Norwegian"),
            ("ps", "Pashto"),
            ("fa", "Persian"),
            ("pl", "Polish"),
            ("pt", "Portuguese"),
            ("ma", "Punjabi"),
            ("ro", "Romanian"),
            ("ru", "Russian"),
            ("sm", "Samoan"),
            ("gd", "Scots-Gaelic"),
            ("sr", "Serbian"),
            ("st", "Sesotho"),
            ("sn", "Shona"),
            ("sd", "Sindhi"),
            ("si", "Sinhala"),
            ("sk", "Slovak"),
            ("sl", "Slovenian"),
            ("so", "Somali"),
            ("es", "Spanish"),
            ("su", "Sundanese"),
            ("sw", "Swahili"),
            ("sv", "Swedish"),
            ("tg", "Tajik"),
            ("ta", "Tamil"),
            ("te", "Telugu"),
            ("th", "Thai"),
            ("tr", "Turkish"),
            ("uk", "Ukrainian"),
            ("ur", "Urdu"),
            ("uz", "Uzbek"),
            ("vi", "Vietnamese"),
            ("cy", "Welsh"),
            ("xh", "Xhosa"),
            ("yi", "Yiddish"),
            ("yo", "Yoruba"),
            ("zu", "Zulu"),
        ],
    }
}

#[info]
fn info() -> PluginInfo {
    PluginInfo {
        name: "Translate".into(),
        icon: "preferences-desktop-locale".into(),
    }
}

#[get_matches]
fn get_matches(input: RString, data: &State) -> RVec<Match> {
    if !input.starts_with(&data.config.prefix) {
        return RVec::new();
    }

    // Ignore the prefix
    let input = &input[data.config.prefix.len()..];
    let (lang, text) = match input.split_once(' ') {
        Some(split) => split,
        None => return RVec::new(),
    };

    if text.is_empty() {
        return RVec::new();
    }

    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default().ignore_case();

    // Fuzzy match the input language with the languages in the Vec
    let mut matches = data
        .langs
        .clone()
        .into_iter()
        .filter_map(|(code, name)| {
            matcher
                .fuzzy_match(code, lang)
                .max(matcher.fuzzy_match(name, lang))
                .map(|score| (code, name, score))
        })
        .collect::<Vec<_>>();

    matches.sort_by(|a, b| b.2.cmp(&a.2));

    // We only want 3 matches
    matches.truncate(3);

    tokio::runtime::Runtime::new().expect("Failed to spawn tokio runtime!").block_on(async move {
        // Create the futures for fetching the translation results
        let futures = matches
            .into_iter()
            .map(|(code, name, _)| async move {
                (name, reqwest::get(format!("https://translate.googleapis.com/translate_a/single?client=gtx&sl=auto&tl={}&dt=t&q={}", code, text)).await)
            });
        futures::future::join_all(futures) // Wait for all futures to complete
            .await
            .into_iter()
            .filter_map(|(name, res)| res
                .ok()
                .map(|response| futures::executor::block_on(response.json())
                    .ok()
                    .map(|json: serde_json::Value|
                        Match {
                            title: json[0]
                                .as_array()
                                .expect("Malformed JSON!")
                                .iter()
                                .map(|val| val.as_array().expect("Malformed JSON!")[0].as_str()
                                    .expect("Malformed JSON!")
                                ).collect::<Vec<_>>()
                                .join(" ")
                                .into(),
                            description: ROption::RSome(
                                format!(
                                    "{} -> {}",
                                    data.langs.iter()
                                    .find_map(|(code, name)| if *code == json[2].as_str().expect("Malformed JSON!") {
                                            Some(name)
                                        } else {
                                            None
                                    }).expect("Google API returned unknown language code!"),
                                    name)
                                .into()),
                            use_pango: false,
                            icon: ROption::RNone,
                            id: ROption::RNone
                        }
                    )
                )
            ).flatten().collect::<RVec<_>>()
    })
}

#[handler]
fn handler(selection: Match) -> HandleResult {
    HandleResult::Copy(selection.title.into_bytes())
}
