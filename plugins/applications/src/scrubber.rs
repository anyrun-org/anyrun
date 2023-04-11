use std::{collections::HashMap, env, ffi::OsStr, fs, io};

use crate::Config;

#[derive(Clone, Debug)]
pub struct DesktopEntry {
    pub exec: String,
    pub name: String,
    pub desc: Option<String>,
    pub icon: String,
}

const FIELD_CODE_LIST: &[&str] = &[
    "%f", "%F", "%u", "%U", "%d", "%D", "%n", "%N", "%i", "%c", "%k", "%v", "%m",
];

impl DesktopEntry {
    fn from_dir_entry(entry: &fs::DirEntry, config: &Config) -> Vec<Self> {
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
                            name: map.get("Name")?.to_string(),
                            desc: None,
                            icon: map
                                .get("Icon")
                                .unwrap_or(&"application-x-executable")
                                .to_string(),
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
                for section in new_sections {
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
                            name: match map.get("Name") {
                                Some(name) => name.to_string(),
                                None => continue,
                            },
                            desc: Some(entry.name.clone()),
                            icon: entry.icon.clone(),
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

pub fn scrubber(config: Config) -> Result<Vec<(DesktopEntry, u64)>, Box<dyn std::error::Error>> {
    // Create iterator over all the files in the XDG_DATA_DIRS
    // XDG compliancy is cool
    let mut paths: Vec<Result<fs::DirEntry, io::Error>> = match env::var("XDG_DATA_DIRS") {
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
    };

    // Go through user directory desktop files for overrides
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

    paths.extend(fs::read_dir(user_path)?);

    Ok(paths
        .iter()
        .filter_map(|entry| {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_why) => return None,
            };
            Some(DesktopEntry::from_dir_entry(entry, &config))
        })
        .flatten()
        .enumerate()
        .map(|(i, val)| (val, i as u64))
        .collect())
}
