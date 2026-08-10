use std::{
    collections::HashMap,
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use crate::Config;

#[derive(Clone, Debug)]
pub struct DesktopEntry {
    pub exec: String,
    pub path: Option<PathBuf>,
    pub name: String,
    pub localized_name: Option<String>,
    pub keywords: Vec<String>,
    pub localized_keywords: Option<Vec<String>>,
    pub desc: Option<String>,
    pub icon: String,
    pub term: bool,
    pub offset: i64,
    pub is_action: bool,
}

const FIELD_CODE_LIST: &[&str] = &[
    "%f", "%F", "%u", "%U", "%d", "%D", "%n", "%N", "%i", "%c", "%k", "%v", "%m",
];

impl DesktopEntry {
    pub fn localized_name(&self) -> String {
        self.localized_name
            .clone()
            .unwrap_or_else(|| self.name.clone())
    }

    fn from_path(path: &Path, config: &Config, lang_choices: &LangChoices) -> Vec<Self> {
        if path.extension() != Some(OsStr::new("desktop")) {
            return Vec::new();
        }

        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(_) => return Vec::new(),
        };

        let lines = content
            .lines()
            // Ignore comments
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .collect::<Vec<_>>();

        let sections = lines
            .chunk_by(|_, line| !line.starts_with('['))
            // Remove the potential lines before the first section
            // `section` is at least 1 element long so `section[0]` cannot panic
            .skip_while(|section| !section[0].starts_with('['))
            .collect::<Vec<_>>();

        let mut ret = Vec::new();

        let Some(entry) = sections.iter().find_map(|section| {
            if !section[0].starts_with("[Desktop Entry]") {
                return None;
            }

            // Let's call them properties as specs call them entries but
            // it is confusing with DesktopEntry.
            // (see https://specifications.freedesktop.org/desktop-entry/latest/basic-format.html#entries)
            let mut props = HashMap::new();

            for line in section.iter().skip(1) {
                if let Some((key, val)) = line.split_once('=') {
                    props.insert(key, val);
                }
            }

            if *props.get("Type")? != "Application" {
                return None;
            }

            if props
                .get("NoDisplay")
                .map(|x| x.to_lowercase() == "true")
                .unwrap_or(false)
            {
                return None;
            }

            DesktopEntry::from_props(&props, lang_choices, None, 0, false)
        }) else {
            // If no appropriate [Desktop Entry] section is found
            return Vec::new();
        };

        if config.desktop_actions {
            for (i, section) in sections.iter().enumerate() {
                let mut action_props = HashMap::new();

                for line in section.iter().skip(1) {
                    if let Some((key, val)) = line.split_once('=') {
                        action_props.insert(key, val);
                    }
                }

                if section[0].starts_with("[Desktop Action") {
                    if let Some(action_entry) = DesktopEntry::from_props(
                        &action_props,
                        lang_choices,
                        Some(entry.icon.clone()),
                        i as i64,
                        true,
                    ) {
                        ret.push(action_entry);
                    }
                }
            }
        }

        ret.push(entry);
        ret
    }

    fn from_props(
        props: &HashMap<&str, &str>,
        lang_choices: &LangChoices,
        icon: Option<String>,
        offset: i64,
        is_action: bool,
    ) -> Option<DesktopEntry> {
        Some(DesktopEntry {
            exec: {
                let mut exec = props.get("Exec")?.to_string();
                for field_code in FIELD_CODE_LIST {
                    exec = exec.replace(field_code, "");
                }
                exec
            },
            path: props.get("Path").map(PathBuf::from),
            name: props.get("Name")?.to_string(),
            localized_name: lang_choices
                .get_localized(&props, "Name")
                .map(ToString::to_string),
            keywords: props
                .get("Keywords")
                .map(|keywords| {
                    keywords
                        .split(';')
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
            localized_keywords: lang_choices
                .get_localized(&props, "Keywords")
                .map(|keywords| {
                    keywords
                        .split(';')
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                }),
            desc: lang_choices
                .get_localized(&props, "Comment")
                .or_else(|| props.get("Comment"))
                .map(ToString::to_string),
            icon: icon.unwrap_or_else(|| {
                props
                    .get("Icon")
                    .unwrap_or(&"application-x-executable")
                    .to_string()
            }),
            term: props
                .get("Terminal")
                .map(|val| val.to_lowercase() == "true")
                .unwrap_or(false),
            offset: offset,
            is_action: is_action,
        })
    }
}

#[derive(Debug, Default)]
struct LangChoices<'a> {
    whole: Option<&'a str>,
    prefix: Option<&'a str>,
    short: Option<&'a str>,
}

impl<'a> LangChoices<'a> {
    fn new(lang: Option<&'a str>) -> Self {
        let mut ret = Self::default();

        // example: en_US.UTF-8
        let Some(whole) = lang else {
            return ret;
        };
        ret.whole = Some(whole);

        // example: en_US
        let Some((prefix, _)) = whole.split_once('.') else {
            return ret;
        };
        ret.prefix = Some(prefix);

        // example: en
        let Some((short, _)) = prefix.split_once('_') else {
            return ret;
        };
        ret.short = Some(short);

        ret
    }

    fn localized_keys(&self, key: &'a str) -> impl Iterator<Item = String> + 'a {
        let choices = (self.whole.into_iter())
            .chain(self.prefix)
            .chain(self.short);
        choices.map(move |choice| format!("{key}[{choice}]"))
    }

    fn get_localized<'b>(
        &self,
        map: &'b HashMap<&'b str, &'b str>,
        key: &'b str,
    ) -> Option<&'b &'b str> {
        self.localized_keys(key).find_map(|key| map.get(&*key))
    }
}

pub fn scrubber(config: &Config) -> Vec<DesktopEntry> {
    let xdg_data_dirs = env::var("XDG_DATA_DIRS").unwrap_or("/usr/share".to_owned());

    // Create iterator over all the files in the XDG_DATA_DIRS
    // XDG compliancy is cool
    let xdg_data_home = env::var("XDG_DATA_HOME").unwrap_or_else(|_why| {
        format!(
            "{}/.local/share",
            env::var("HOME").expect("Unable to determine home directory!")
        )
    });

    let lang = env::var("LANG").ok();
    let lang_choices = LangChoices::new(lang.as_deref());

    // Parse the XDG_DATA_DIRS variable and list files of all the paths
    let entry_dirs = xdg_data_dirs
        .split(':')
        .chain(Some(xdg_data_home.as_str()))
        .map(|dir| format!("{}/applications/", dir));

    let entries = entry_dirs
        .filter_map(|dir| match fs::read_dir(&dir) {
            Ok(files) => Some(files),
            Err(why) => {
                eprintln!("[applications] Error reading directory {}: {}", dir, why);
                None
            }
        })
        .flatten()
        .filter_map(|entry_res| {
            let entry = entry_res.ok()?;

            Some(DesktopEntry::from_path(
                &entry.path(),
                config,
                &lang_choices,
            ))
        })
        .flatten();

    entries.collect()
}
