use anyrun_macros::ConfigArgs;
use clap::ValueEnum;
use gtk::gdk;
use gtk4 as gtk;
use serde::{de::Visitor, Deserialize, Deserializer};
use std::path::PathBuf;

#[derive(Deserialize, ConfigArgs)]
#[config_args(pub)]
pub struct Config {
    #[serde(default = "Config::default_x")]
    pub x: RelativeNum,
    #[serde(default = "Config::default_y")]
    pub y: RelativeNum,
    #[serde(default = "Config::default_width")]
    pub width: RelativeNum,
    #[serde(default = "Config::default_height")]
    pub height: RelativeNum,

    #[serde(default = "Config::default_plugins")]
    pub plugins: Vec<PathBuf>,

    #[serde(default)]
    pub hide_icons: bool,
    #[serde(default)]
    pub hide_plugin_info: bool,
    #[serde(default)]
    pub ignore_exclusive_zones: bool,
    #[serde(default)]
    pub close_on_click: bool,
    #[serde(default)]
    pub show_results_immediately: bool,
    #[serde(default)]
    pub max_entries: Option<usize>,
    #[serde(default = "Config::default_layer")]
    pub layer: Layer,
    #[serde(default = "Config::default_keyboard_mode")]
    pub keyboard_mode: KeyboardMode,

    #[config_args(skip)]
    #[serde(default = "Config::default_keybinds")]
    pub keybinds: Vec<Keybind>,

    #[config_args(skip)]
    #[serde(default = "Config::default_mousebinds")]
    pub mousebinds: Vec<Mousebind>,
}
impl Config {
    fn default_x() -> RelativeNum {
        RelativeNum::Fraction(0.5)
    }

    fn default_y() -> RelativeNum {
        RelativeNum::Absolute(0)
    }

    fn default_width() -> RelativeNum {
        RelativeNum::Fraction(0.5)
    }

    fn default_height() -> RelativeNum {
        RelativeNum::Absolute(1)
    }

    fn default_plugins() -> Vec<PathBuf> {
        vec![
            "libapplications.so".into(),
            "libsymbols.so".into(),
            "libshell.so".into(),
            "libtranslate.so".into(),
        ]
    }

    fn default_layer() -> Layer {
        Layer::Overlay
    }

    fn default_keyboard_mode() -> KeyboardMode {
        KeyboardMode::Exclusive
    }

    fn default_keybinds() -> Vec<Keybind> {
        vec![
            Keybind {
                ctrl: false,
                alt: false,
                key: gdk::Key::Escape,
                action: Action::Close,
            },
            Keybind {
                ctrl: false,
                alt: false,
                key: gdk::Key::Return,
                action: Action::Select,
            },
            Keybind {
                ctrl: false,
                alt: false,
                key: gdk::Key::Up,
                action: Action::Up,
            },
            Keybind {
                ctrl: false,
                alt: false,
                key: gdk::Key::Down,
                action: Action::Down,
            },
        ]
    }

    fn default_mousebinds() -> Vec<Mousebind> {
        vec![
            Mousebind {
                button: MouseButton::Primary,
                action: Action::Select,
            },
            Mousebind {
                button: MouseButton::Secondary,
                action: Action::Nop,
            },
        ]
    }
}
impl Default for Config {
    fn default() -> Self {
        Self {
            x: Self::default_x(),
            y: Self::default_y(),
            width: Self::default_width(),
            height: Self::default_height(),
            plugins: Self::default_plugins(),
            hide_icons: false,
            hide_plugin_info: false,
            ignore_exclusive_zones: false,
            close_on_click: false,
            show_results_immediately: false,
            max_entries: None,
            layer: Self::default_layer(),
            keyboard_mode: Self::default_keyboard_mode(),
            keybinds: Self::default_keybinds(),
            mousebinds: Self::default_mousebinds(),
        }
    }
}

#[derive(Deserialize, Clone, ValueEnum)]
pub enum Layer {
    Background,
    Bottom,
    Top,
    Overlay,
}

#[derive(Deserialize, Clone, ValueEnum)]
pub enum KeyboardMode {
    Exclusive,
    OnDemand,
}

// Could have a better name
#[derive(Deserialize, Clone)]
pub enum RelativeNum {
    Absolute(i32),
    Fraction(f32),
}

impl RelativeNum {
    pub fn to_val(&self, val: u32) -> i32 {
        match self {
            RelativeNum::Absolute(num) => *num,
            RelativeNum::Fraction(frac) => (frac * val as f32) as i32,
        }
    }
}

impl From<&str> for RelativeNum {
    fn from(value: &str) -> Self {
        let (ty, val) = value.split_once(':').expect("Invalid RelativeNum value");

        match ty {
            "absolute" => Self::Absolute(val.parse().unwrap()),
            "fraction" => Self::Fraction(val.parse().unwrap()),
            _ => panic!("Invalid type of value"),
        }
    }
}

#[derive(Deserialize, Debug, Clone, Copy)]
pub enum Action {
    Close,
    Select,
    Up,
    Down,
    Nop,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum MouseButton {
    Primary,
    Secondary,
    Middle,
    Unknown,
}

#[derive(Deserialize, Debug, Clone, Copy)]
pub struct Mousebind {
    pub button: MouseButton,
    pub action: Action,
}

#[derive(Deserialize, Clone)]
pub struct Keybind {
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(deserialize_with = "Keybind::deserialize_key")]
    pub key: gdk::Key,
    pub action: Action,
}

impl Keybind {
    fn deserialize_key<'de, D>(deserializer: D) -> Result<gdk::Key, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct KeyVisitor;

        impl<'de> Visitor<'de> for KeyVisitor {
            type Value = gdk::Key;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("A plaintext key in the GDK format")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                gdk::Key::from_name(v).ok_or(E::custom("Key name is not valid"))
            }
        }

        deserializer.deserialize_str(KeyVisitor)
    }
}

#[derive(Deserialize, Clone, ValueEnum)]
enum Position {
    Top,
    Center,
}
