use std::{collections::HashMap, env, ffi::OsStr, fs, path::PathBuf};

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

const SUPPORTED_FIELD_CODES: &[char] = &[ 'i', 'c' ];

const VALID_FIELD_CODES: &[char] = &[
    'f', 'F', 'u', 'U', 'd', 'D', 'n', 'N', 'i', 'c', 'k', 'v', 'm',
];

const DEPRECATED_FIELD_CODES: &[char] = &[
  'd', 'D', 'n', 'N'
];

// See https://specifications.freedesktop.org/desktop-entry-spec/latest/exec-variables.html
const EXEC_ESCAPE_CHARS: &[char] = &['"', '`', '$', '\\'];

/*
Reserved characters are space (" "), tab, newline, double quote,
single quote ("'"), backslash character ("\"), greater-than sign
(">"), less-than sign ("<"), tilde ("~"), vertical bar ("|"),
ampersand ("&"), semicolon (";"), dollar sign ("$"), asterisk ("*"),
question mark ("?"), hash mark ("#"), parenthesis ("(") and (")") and
backtick character ("`").
*/
const EXEC_RESERVED_CHARS: &[char] = &[
    ' ', '\t', '\n', '"', '\'', '\\', '>', '<', '~', '|', '&', ';', '$', '*', '?', '#', '(', ')',
    '`',
];

// \s, \n, \t, \r, and \\ are valid escapes in Desktop strings
const DESKTOP_STRING_ESCAPES: &[(char, char)] = &[
    ('s', ' '),
    ('n', '\n'),
    ('t', '\t'),
    ('r', '\r'),
    ('\\', '\\'),
];

fn get_desktop_string_escapes() -> HashMap<char, char> {
    HashMap::from_iter(DESKTOP_STRING_ESCAPES.iter().cloned())
}

#[derive(Debug, Clone)]
pub struct ExecKeyError(pub String);

#[derive(Debug, Clone)]
enum StringEscapeState {
    Waiting,
    Escape,
}

fn substitute_escapes(s: &str) -> Result<String, ExecKeyError> {
    use StringEscapeState::*;

    let escapes = get_desktop_string_escapes();
    let mut state = Waiting;
    let mut out = Vec::<char>::new();
    for (i, c) in s.chars().enumerate() {
        match state {
            Waiting => match c {
                '\\' => {
                    state = Escape;
                }
                _ => {
                    out.push(c);
                }
            },
            Escape => match c {
                c if escapes.contains_key(&c) => {
                    out.push(*escapes.get(&c).unwrap());
                    state = Waiting;
                }
                _ => {
                    return Err(ExecKeyError(format!(
                        "Escaping invalid character {} at position {}",
                        c, i
                    )))
                }
            },
        }
    }
    if let Escape = state {
        return Err(ExecKeyError("Dangling escape".to_string()));
    }
    Ok(out.into_iter().collect())
}

#[derive(Debug, Clone)]
enum ExecKeyState {
    Waiting,
    Word,
    Quoting,
    Escape,
}

fn unescape_exec(s: &str) -> Result<Vec<String>, ExecKeyError> {
    use ExecKeyState::*;

    let mut state = Waiting;
    let mut out = Vec::<String>::new();
    let mut buffer = Vec::<char>::new();

    for (i, c) in s.chars().enumerate() {
        match state {
            Waiting => {
                match c {
                    '"' => {
                        state = Quoting;
                        continue;
                    }
                    ' ' => continue,
                    c if EXEC_RESERVED_CHARS.contains(&c) => return Err(ExecKeyError(format!(
                        "Starting word with reserved character {} at position {}, consider quoting",
                        c, i
                    ))),
                    _ => {
                        state = Word;
                    }
                };
                buffer.push(c);
            }
            Word => match c {
                ' ' => {
                    state = Waiting;
                    out.push(buffer.iter().collect());
                    buffer.clear();
                }
                c if EXEC_RESERVED_CHARS.contains(&c) => {
                    return Err(ExecKeyError(format!(
                        "Reserved character {} in unquoted word at position {}",
                        c, i
                    )))
                }
                _ => buffer.push(c),
            },
            Quoting => match c {
                '"' => {
                    out.push(buffer.iter().collect());
                    buffer.clear();
                    state = Waiting;
                    continue;
                }
                '\\' => state = Escape,
                c if EXEC_ESCAPE_CHARS.contains(&c) => {
                    return Err(ExecKeyError(format!(
                        "Unescaped character {} in quoted string at position {}",
                        c, i
                    )));
                }
                _ => {
                    buffer.push(c);
                }
            },
            Escape => match c {
                c if EXEC_ESCAPE_CHARS.contains(&c) => {
                    buffer.push(c);
                    state = Quoting;
                }
                _ => {
                    return Err(ExecKeyError(format!(
                        "Escaping invalid character {} in quoted string at position {}",
                        c, i
                    )))
                }
            },
        }
    }
    match state {
        Waiting => {}
        Word => {
            out.push(buffer.iter().collect());
            buffer.clear();
        }
        _ => return Err(ExecKeyError("Invalid state at end of exec key".to_string())),
    }

    Ok(out)
}

#[derive(Debug, Clone)]
enum FieldCodeState {
    Reading,
    Percent
}

fn get_fieldcode(code: char, entry: &DesktopEntry, arg: &str) -> Result<String, ExecKeyError> {
    let result = match code {
        'c' => entry.localized_name(),
        'i' => {
            if arg.len() > 2 {
                return Err(
                    ExecKeyError(
                        format!(
                            "Encountered field code %i in argument {} with other other contents, %i must stand alone.", arg)
                    ))
            }
            format!("--icon {}", entry.icon.clone())
        },
        c => panic!("Function called with unimplemented field code {}!", c)
    };
    Ok(result)
}

fn expand_exec_fieldcodes(entry: &DesktopEntry, arg: String) -> Result<String, ExecKeyError> {
    use FieldCodeState::*;

    let mut out = String::new();
    let mut state = Reading;

    for c in arg.chars() {
        match state {
            Reading => {
                if c == '%' {
                    state = Percent;
                } else {
                    out.push(c);
                }
            }
            Percent => {
                match c {
                    '%' => out.push('%'),
                    c if SUPPORTED_FIELD_CODES.contains(&c) => {
                       let field_code_content = get_fieldcode(c, &entry, &arg)?;
                       out.push_str(&field_code_content);
                    },
                    c if VALID_FIELD_CODES.contains(&c) => {
                        eprintln!(
                            "Argument {} contains field code %{} which is valid but not implemented and will be stripped.",
                            &arg,
                            c
                        )
                    },
                    c if DEPRECATED_FIELD_CODES.contains(&c) => {
                        eprintln!(
                            "Argument {} contains deprecated field code %{} which will be stripped.",
                            &arg,
                            c
                        )
                    },
                    _ => {
                        return Err(ExecKeyError(format!("Argument {} contains unknown field code %{}.", &arg, c)))
                    }
                }
                state = Reading;
            }
        }
    }
    if matches!(state, Percent) {
        return Err(ExecKeyError(format!("Argument {} ends in % which is interpreted as unfinished field code.", &arg)))
    };
    return Ok(out)
}

/*
1. Substitute general desktop string escapes
2. Unescape EXEC_ESCAPE_CHARS in exec key quoted strings
3. Process field codes
4. Throw away empty args
*/
pub(crate) fn lower_exec(entry: &DesktopEntry) -> Result<(String, Vec<String>), ExecKeyError> {
    let subst = substitute_escapes(&entry.exec)?;
    let argvec = unescape_exec(&subst)?;
    if let Some((command, argv)) = argvec.split_first() {
        if command.contains('=') {
            return Err(ExecKeyError("Executable program must not contain '=' character.".to_string()))
        };

        let argv_fieldcodes = argv
            .into_iter()
            .map(|arg| expand_exec_fieldcodes(&entry, arg.clone()))
            .collect::<Result<Vec<_>,_>>()?;
        let argv_stripped = argv_fieldcodes.into_iter().filter(|arg| arg.is_empty()).collect();
        return Ok((command.clone(), argv_stripped));
    } else {
        return Err(ExecKeyError("Empty exec key!".to_string()));
    }
}

impl DesktopEntry {
    pub fn localized_name(&self) -> String {
        self.localized_name
            .clone()
            .unwrap_or_else(|| self.name.clone())
    }

    fn from_dir_entry(
        entry: &fs::DirEntry,
        config: &Config,
        lang_choices: &LangChoices,
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
                            exec: map.get("Exec")?.to_string(),
                            path: map.get("Path").map(PathBuf::from),
                            name: map.get("Name")?.to_string(),
                            localized_name: lang_choices
                                .localized_keys("Name")
                                .find_map(|key| map.get(&*key))
                                .map(ToString::to_string),
                            keywords: map
                                .get("Keywords")
                                .map(|keywords| {
                                    keywords
                                        .split(';')
                                        .map(|s| s.to_owned())
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default(),
                            localized_keywords: lang_choices
                                .localized_keys("Keywords")
                                .find_map(|key| map.get(&*key))
                                .map(|keywords| {
                                    keywords
                                        .split(';')
                                        .map(|s| s.to_owned())
                                        .collect::<Vec<_>>()
                                }),
                            desc: lang_choices
                                .localized_keys("Comment")
                                .find_map(|key| map.get(&*key))
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
                            is_action: false,
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
                                    exec.to_string()
                                }
                                None => continue,
                            },
                            path: entry.path.clone(),
                            name: match map.get("Name") {
                                Some(name) => name.to_string(),
                                None => continue,
                            },
                            localized_name: lang_choices
                                .localized_keys("Name")
                                .find_map(|key| map.get(&*key))
                                .map(ToString::to_string),
                            keywords: map
                                .get("Keywords")
                                .map(|keywords| {
                                    keywords
                                        .split(';')
                                        .map(|s| s.to_owned())
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default(),
                            localized_keywords: lang_choices
                                .localized_keys("Keywords")
                                .find_map(|key| map.get(&*key))
                                .map(|keywords| {
                                    keywords
                                        .split(';')
                                        .map(|s| s.to_owned())
                                        .collect::<Vec<_>>()
                                }),
                            desc: Some(entry.localized_name()),
                            icon: entry.icon.clone(),
                            term: map
                                .get("Terminal")
                                .map(|val| val.to_lowercase() == "true")
                                .unwrap_or(false),
                            offset: i as i64,
                            is_action: true,
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
    let lang_choices = LangChoices::new(lang.as_deref());

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
                        eprintln!("[applications] Error reading directory {}: {}", dir, why);
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
        let entries = DesktopEntry::from_dir_entry(&entry, config, &lang_choices);
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
                    let entries = DesktopEntry::from_dir_entry(&entry, config, &lang_choices);
                    Some(
                        entries
                            .into_iter()
                            .map(|entry| (format!("{}{}", entry.name, entry.icon), entry)),
                    )
                })
                .flatten(),
        ),
        Err(why) => eprintln!(
            "[applications] Error reading directory {}: {}",
            user_path, why
        ),
    }

    Ok(entries
        .into_iter()
        .enumerate()
        .map(|(i, (_, entry))| (entry, i as u64))
        .collect())
}
