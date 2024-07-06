use std::{collections::HashMap, env, ffi::OsStr, fs, path::PathBuf};

use crate::Config;

#[derive(Clone, Debug)]
pub struct DesktopEntry {
    pub exec: String,
    pub path: Option<PathBuf>,
    pub name: String,
    pub display_name: Option<String>,
    pub keywords: Vec<String>,
    pub display_keywords: Option<Vec<String>>,
    pub desc: Option<String>,
    pub icon: String,
    pub term: bool,
    pub offset: i64,
}

const FIELD_CODE_LIST: &[&str] = &[
    "%f", "%F", "%u", "%U", "%d", "%D", "%n", "%N", "%i", "%c", "%k", "%v", "%m",
];

impl DesktopEntry {
    pub fn display_name(&self) -> String {
        self.display_name
            .clone()
            .unwrap_or_else(|| self.name.clone())
    }

    fn display_keys<'a>(
        key: &'a str,
        lang_choices: &'a [&str],
    ) -> impl Iterator<Item = String> + 'a {
        lang_choices
            .iter()
            .map(move |lang| format!("{key}[{lang}]"))
    }

    fn from_dir_entry(
        entry: &fs::DirEntry,
        config: &Config,
        lang_choices: Option<&[&str]>,
    ) -> Vec<Self> {
        if entry.path().extension() == Some(OsStr::new("desktop")) {
            let content = match fs::read_to_string(entry.path()) {
                Ok(content) => content,
                Err(_) => return Vec::new(),
            };

            let lines = content.lines().collect::<Vec<_>>();

            let sections = lines
                .split_inclusive(|line| line.starts_with('['))
                .collect::<Vec<_>>();

            let mut line = None;
            let mut new_sections = Vec::new();

            for (i, section) in sections.iter().enumerate() {
                if let Some(line) = line {
                    let mut section = section.to_vec();
                    section.insert(0, line);

                    // Only pop the last redundant entry if it isn't the last item
                    if i < sections.len() - 1 {
                        section.pop();
                    }
                    new_sections.push(section);
                }
                line = Some(section.last().unwrap_or(&""));
            }

            let mut ret = Vec::new();

            let entry = match new_sections.iter().find_map(|section| {
                if section[0].starts_with("[Desktop Entry]") {
                    let mut map = HashMap::new();

                    for line in section.iter().skip(1) {
                        if let Some((key, val)) = line.split_once('=') {
                            map.insert(key, val);
                        }
                    }

                    if map.get("Type")? == &"Application"
                        && match map.get("NoDisplay") {
                            Some(no_display) => !no_display.parse::<bool>().unwrap_or(true),
                            None => true,
                        }
                    {
                        Some(DesktopEntry {
                            exec: {
                                let mut exec = map.get("Exec")?.to_string();

                                for field_code in FIELD_CODE_LIST {
                                    exec = exec.replace(field_code, "");
                                }
                                exec
                            },
                            path: map.get("Path").map(PathBuf::from),
                            name: map.get("Name")?.to_string(),
                            display_name: lang_choices.and_then(|lang_choices| {
                                Self::display_keys("Name", lang_choices)
                                    .find_map(|key| map.get(&*key))
                                    .map(ToString::to_string)
                            }),
                            keywords: map
                                .get("Keywords")
                                .map(|keywords| {
                                    keywords
                                        .split(';')
                                        .map(|s| s.to_owned())
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default(),
                            display_keywords: lang_choices.and_then(|lang_choices| {
                                Self::display_keys("Keywords", lang_choices)
                                    .find_map(|key| map.get(&*key))
                                    .map(|keywords| {
                                        keywords
                                            .split(';')
                                            .map(|s| s.to_owned())
                                            .collect::<Vec<_>>()
                                    })
                            }),
                            desc: lang_choices
                                .and_then(|lang_choices| {
                                    Self::display_keys("Comment", lang_choices)
                                        .find_map(|key| map.get(&*key))
                                })
                                .or_else(|| map.get("Comment"))
                                .map(ToString::to_string),
                            icon: map
                                .get("Icon")
                                .unwrap_or(&"application-x-executable")
                                .to_string(),
                            term: map
                                .get("Terminal")
                                .map(|val| val.to_lowercase() == "true")
                                .unwrap_or(false),
                            offset: 0,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            }) {
                Some(entry) => entry,
                None => return Vec::new(),
            };

            if config.desktop_actions {
                for (i, section) in new_sections.iter().enumerate() {
                    let mut map = HashMap::new();

                    for line in section.iter().skip(1) {
                        if let Some((key, val)) = line.split_once('=') {
                            map.insert(key, val);
                        }
                    }

                    if section[0].starts_with("[Desktop Action") {
                        ret.push(DesktopEntry {
                            exec: match map.get("Exec") {
                                Some(exec) => {
                                    let mut exec = exec.to_string();

                                    for field_code in FIELD_CODE_LIST {
                                        exec = exec.replace(field_code, "");
                                    }
                                    exec
                                }
                                None => continue,
                            },
                            path: entry.path.clone(),
                            name: match map.get("Name") {
                                Some(name) => name.to_string(),
                                None => continue,
                            },
                            display_name: lang_choices.and_then(|lang_choices| {
                                Self::display_keys("Name", lang_choices)
                                    .find_map(|key| map.get(&*key))
                                    .map(ToString::to_string)
                            }),
                            keywords: map
                                .get("Keywords")
                                .map(|keywords| {
                                    keywords
                                        .split(';')
                                        .map(|s| s.to_owned())
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default(),
                            display_keywords: lang_choices.and_then(|lang_choices| {
                                Self::display_keys("Keywords", lang_choices)
                                    .find_map(|key| map.get(&*key))
                                    .map(|keywords| {
                                        keywords
                                            .split(';')
                                            .map(|s| s.to_owned())
                                            .collect::<Vec<_>>()
                                    })
                            }),
                            desc: Some(entry.display_name()),
                            icon: entry.icon.clone(),
                            term: map
                                .get("Terminal")
                                .map(|val| val.to_lowercase() == "true")
                                .unwrap_or(false),
                            offset: i as i64,
                        })
                    }
                }
            }

            ret.push(entry);
            ret
        } else {
            Vec::new()
        }
    }
}

fn lang_choices(lang: &str) -> Vec<&str> {
    // example: en_US.UTF-8
    let whole = lang;
    // example: en_US
    let Some((prefix, _)) = whole.split_once('.') else {
        return vec![whole];
    };
    // example: en
    let Some((short, _)) = prefix.split_once('_') else {
        return vec![whole, prefix];
    };
    vec![whole, prefix, short]
}

pub fn scrubber(config: &Config) -> Result<Vec<(DesktopEntry, u64)>, Box<dyn std::error::Error>> {
    // Create iterator over all the files in the XDG_DATA_DIRS
    // XDG compliancy is cool
    let user_path = match env::var("XDG_DATA_HOME") {
        Ok(data_home) => {
            format!("{}/applications/", data_home)
        }
        Err(_) => {
            format!(
                "{}/.local/share/applications/",
                env::var("HOME").expect("Unable to determine home directory!")
            )
        }
    };

    let lang = env::var("LANG").ok();
    let lang_choices = lang.as_deref().map(lang_choices);

    let mut entries: HashMap<String, DesktopEntry> = match env::var("XDG_DATA_DIRS") {
        Ok(data_dirs) => {
            // The vec for all the DirEntry objects
            let mut paths = Vec::new();
            // Parse the XDG_DATA_DIRS variable and list files of all the paths
            for dir in data_dirs.split(':') {
                match fs::read_dir(format!("{}/applications/", dir)) {
                    Ok(dir) => {
                        paths.extend(dir);
                    }
                    Err(why) => {
                        eprintln!("Error reading directory {}: {}", dir, why);
                    }
                }
            }
            // Make sure the list of paths isn't empty
            if paths.is_empty() {
                return Err("No valid desktop file dirs found!".into());
            }

            // Return it
            paths
        }
        Err(_) => fs::read_dir("/usr/share/applications")?.collect(),
    }
    .into_iter()
    .filter_map(|entry| {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_why) => return None,
        };
        let entries = DesktopEntry::from_dir_entry(&entry, config, lang_choices.as_deref());
        Some(
            entries
                .into_iter()
                .map(|entry| (format!("{}{}", entry.name, entry.icon), entry)),
        )
    })
    .flatten()
    .collect();

    // Go through user directory desktop files for overrides
    match fs::read_dir(&user_path) {
        Ok(dir_entries) => entries.extend(
            dir_entries
                .into_iter()
                .filter_map(|entry| {
                    let entry = match entry {
                        Ok(entry) => entry,
                        Err(_why) => return None,
                    };
                    let entries =
                        DesktopEntry::from_dir_entry(&entry, config, lang_choices.as_deref());
                    Some(
                        entries
                            .into_iter()
                            .map(|entry| (format!("{}{}", entry.name, entry.icon), entry)),
                    )
                })
                .flatten(),
        ),
        Err(why) => eprintln!("Error reading directory {}: {}", user_path, why),
    }

    Ok(entries
        .into_iter()
        .enumerate()
        .map(|(i, (_, entry))| (entry, i as u64))
        .collect())
}
