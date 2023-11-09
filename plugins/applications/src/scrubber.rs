use std::{collections::HashMap, env, ffi::OsStr, fs, path::PathBuf};

use crate::Config;

#[derive(Clone, Debug)]
pub struct DesktopEntry {
    pub exec: String,
    pub path: Option<PathBuf>,
    pub name: String,
    pub keywords: Vec<String>,
    pub desc: Option<String>,
    pub icon: String,
    pub term: bool,
    pub offset: i64,
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
                            path: map.get("Path").map(PathBuf::from),
                            name: map.get("Name")?.to_string(),
                            keywords: map
                                .get("Keywords")
                                .map(|keywords| {
                                    keywords
                                        .split(';')
                                        .map(|s| s.to_owned())
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default(),
                            desc: None,
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
                            keywords: map
                                .get("Keywords")
                                .map(|keywords| {
                                    keywords
                                        .split(';')
                                        .map(|s| s.to_owned())
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default(),
                            desc: Some(entry.name.clone()),
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

fn generate_desktop_entry_id(entry: &fs::DirEntry, folder: &PathBuf) -> String {
    let mut id = entry.path()
        .strip_prefix(folder).unwrap()        
        .to_string_lossy().to_string();

    id = id.replace("/", "-");
    id = id.replace(".desktop", "");        
    id
}

fn get_desktop_files(path: &str, entries: &mut Vec<Result<fs::DirEntry,std::io::Error>>) {    
    match fs::read_dir(path) {
        Ok(dir_entries) => {
            for entry in dir_entries {
                match entry {
                    Ok(entry) => {
                        if entry.path().is_dir() {
                            get_desktop_files(entry.path().to_str().unwrap(), entries);
                        } else {
                            entries.push(Ok(entry));
                        }
                    }
                    Err(why) => {
                        entries.push(Err(why));
                    }
                }
            }
        }
        Err(why) => eprintln!("Error reading directory {}: {}", path, why),
    }    
}
fn get_desktop_files_and_ids(path: &str, entries: &mut HashMap<String, fs::DirEntry>) {    
    let mut files = Vec::new();
    get_desktop_files(path, &mut files);
    entries.extend(
        files.into_iter().filter_map(|entry| {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_why) => return None,
            };
            let id = generate_desktop_entry_id(&entry, &PathBuf::from(path));
            Some((id, entry))
        })
    )
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
    
    let entries: Vec<DesktopEntry> = match env::var("XDG_DATA_DIRS") {
        Ok(data_dirs) => {
            let mut paths = HashMap::new();
            // The vec for all the DirEntry objects
            
            // Parse the XDG_DATA_DIRS variable and list files of all the paths.
            // Precedence is from first to last in the list, so reverse execution order 
            // so that entries in the first directory are the last to be assigned.            
            data_dirs.split(':').rev()
                .map(|dir| format!("{}/applications/", dir))
                .for_each(|dir| get_desktop_files_and_ids(&dir, &mut paths));
            
            // Finally, the user directory takes precedence over all, so assign it after
            // all the other directories.
            get_desktop_files_and_ids(&user_path, &mut paths);

            // Make sure the list of paths isn't empty
            if paths.is_empty() {
                return Err("No valid desktop file dirs found!".into());
            }
            paths
        }
        Err(_) => {
            let mut paths = HashMap::new();
            get_desktop_files_and_ids("/usr/share/applications", &mut paths);
            paths
        }
    }
    .into_iter()
    .map(|(_, entry)| DesktopEntry::from_dir_entry(&entry, config))
    .flatten()
    .collect();

    

    Ok(entries
        .into_iter()
        .enumerate()
        .map(|(i, entry)| (entry, i as u64))
        .collect())
}
