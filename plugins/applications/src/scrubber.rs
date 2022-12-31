use std::{collections::HashMap, env, ffi::OsStr, fs, io};

#[derive(Clone, Debug)]
pub struct DesktopEntry {
    pub exec: String,
    pub name: String,
    pub icon: String,
}

const FIELD_CODE_LIST: &[&str] = &[
    "%f", "%F", "%u", "%U", "%d", "%D", "%n", "%N", "%i", "%c", "%k", "%v", "%m",
];

impl DesktopEntry {
    fn from_dir_entry(entry: &fs::DirEntry) -> Option<Self> {
        if entry.path().extension() == Some(OsStr::new("desktop")) {
            let content = match fs::read_to_string(entry.path()) {
                Ok(content) => content,
                Err(_) => return None,
            };

            let mut map = HashMap::new();
            for line in content.lines() {
                if line.starts_with("[") && line != "[Desktop Entry]" {
                    break;
                }
                let (key, val) = match line.split_once("=") {
                    Some(keyval) => keyval,
                    None => continue,
                };
                map.insert(key, val);
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
    }
}

pub fn scrubber() -> Result<Vec<(DesktopEntry, u64)>, Box<dyn std::error::Error>> {
    // Create iterator over all the files in the XDG_DATA_DIRS
    // XDG compliancy is cool
    let mut paths: Vec<Result<fs::DirEntry, io::Error>> = match env::var("XDG_DATA_DIRS") {
        Ok(data_dirs) => {
            // The vec for all the DirEntry objects
            let mut paths = Vec::new();
            // Parse the XDG_DATA_DIRS variable and list files of all the paths
            for dir in data_dirs.split(":") {
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

    paths.extend(fs::read_dir(&user_path)?);

    // Keeping track of the entries
    let mut id = 0;

    Ok(paths
        .iter()
        .filter_map(|entry| {
            id += 1;
            let entry = match entry {
                Ok(entry) => entry,
                Err(_why) => return None,
            };
            DesktopEntry::from_dir_entry(entry).map(|val| (val, id))
        })
        .collect())
}
