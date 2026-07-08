pub mod keybind;
pub mod layout;
pub mod search;

pub use keybind::*;
pub use layout::*;
pub use search::*;

use anyrun_macros::ConfigArgs;
use gtk4::gdk;
use serde::Deserialize;
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

    #[config_args(skip)]
    #[serde(default = "Config::default_provider")]
    #[allow(dead_code)]
    pub provider: PathBuf,

    #[serde(default)]
    pub hide_icons: bool,
    #[serde(default)]
    pub hide_plugin_info: bool,
    #[serde(default)]
    pub ignore_exclusive_zones: bool,
    #[serde(default)]
    pub close_on_click: bool,
    #[serde(default)]
    pub close_on_unfocus: bool,
    #[serde(default)]
    pub double_esc_to_close: bool,
    #[serde(default)]
    pub show_results_immediately: bool,
    #[serde(default)]
    pub max_entries: Option<u32>,
    #[config_args(skip)]
    #[serde(default)]
    pub search_ux: SearchUxConfig,
    #[serde(default = "Config::default_layer")]
    pub layer: Layer,
    #[serde(default = "Config::default_keyboard_mode")]
    pub keyboard_mode: KeyboardMode,

    #[config_args(skip)]
    #[serde(default = "Config::default_keybinds")]
    pub keybinds: Vec<Keybind>,
}

impl Config {
    fn default_x() -> RelativeNum {
        RelativeNum::Fraction(0.5)
    }

    fn default_y() -> RelativeNum {
        RelativeNum::Fraction(0.5)
    }

    fn default_width() -> RelativeNum {
        RelativeNum::Absolute(800)
    }

    fn default_height() -> RelativeNum {
        RelativeNum::Fraction(0.45)
    }

    fn default_plugins() -> Vec<PathBuf> {
        vec![
            "libapplications.so".into(),
            "libsymbols.so".into(),
            "libshell.so".into(),
            "libtranslate.so".into(),
        ]
    }

    fn default_provider() -> PathBuf {
        PathBuf::from("anyrun-provider")
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
                shift: false,
                key: gdk::Key::Escape,
                action: Action::Close,
            },
            Keybind {
                ctrl: false,
                alt: false,
                shift: false,
                key: gdk::Key::Return,
                action: Action::Select,
            },
            Keybind {
                ctrl: true,
                alt: false,
                shift: false,
                key: gdk::Key::Return,
                action: Action::OpenActions,
            },
            Keybind {
                ctrl: false,
                alt: false,
                shift: false,
                key: gdk::Key::Up,
                action: Action::Up,
            },
            Keybind {
                ctrl: false,
                alt: false,
                shift: false,
                key: gdk::Key::Down,
                action: Action::Down,
            },
            Keybind {
                ctrl: false,
                alt: false,
                shift: true,
                key: gdk::Key::ISO_Left_Tab,
                action: Action::Up,
            },
            Keybind {
                ctrl: false,
                alt: false,
                shift: false,
                key: gdk::Key::Tab,
                action: Action::Down,
            },
            Keybind {
                ctrl: true,
                alt: false,
                shift: false,
                key: gdk::Key::j,
                action: Action::Down,
            },
            Keybind {
                ctrl: true,
                alt: false,
                shift: false,
                key: gdk::Key::k,
                action: Action::Up,
            },
            Keybind {
                ctrl: true,
                alt: false,
                shift: false,
                key: gdk::Key::n,
                action: Action::Down,
            },
            Keybind {
                ctrl: true,
                alt: false,
                shift: false,
                key: gdk::Key::p,
                action: Action::Up,
            },
            Keybind {
                ctrl: true,
                alt: false,
                shift: false,
                key: gdk::Key::r,
                action: Action::ReloadConfig,
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
            provider: Self::default_provider(),
            hide_icons: false,
            hide_plugin_info: true,
            ignore_exclusive_zones: false,
            close_on_click: false,
            close_on_unfocus: false,
            double_esc_to_close: false,
            show_results_immediately: false,
            max_entries: None,
            search_ux: SearchUxConfig::default(),
            layer: Self::default_layer(),
            keyboard_mode: Self::default_keyboard_mode(),
            keybinds: Self::default_keybinds(),
        }
    }
}

impl ConfigArgs {
    pub fn has_plugins(&self) -> bool {
        self.plugins.is_some()
    }
}

impl Default for ConfigArgs {
    fn default() -> Self {
        Self {
            plugins: None,
            x: None,
            y: None,
            width: None,
            height: None,
            hide_icons: None,
            hide_plugin_info: None,
            ignore_exclusive_zones: None,
            close_on_click: None,
            close_on_unfocus: None,
            double_esc_to_close: None,
            show_results_immediately: None,
            max_entries: None,
            layer: None,
            keyboard_mode: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_ux_defaults() {
        let cfg = SearchUxConfig::default();
        assert_eq!(cfg.settle_delay_ms, 200);
        assert_eq!(cfg.flush_delay_ms, 50);
        assert_eq!(cfg.typing_visual, TypingVisual::DimPrevious);
        assert!(cfg.bare_text_fast_lane.is_empty());
        assert_eq!(cfg.plugin_timeout_ms, 800);
        assert_eq!(cfg.slow_plugin_ms, 250);
        assert_eq!(cfg.prefix_discovery_trigger, "?");
        assert!(cfg.empty_state.enabled);
        assert_eq!(cfg.empty_state.recent_limit, 8);
        let app_cfg = Config::default();
        assert!(matches!(app_cfg.width, RelativeNum::Absolute(800)));
        assert!(matches!(app_cfg.y, RelativeNum::Fraction(y) if (y - 0.5).abs() < f64::EPSILON));
        assert!(app_cfg.hide_plugin_info);
    }

    #[test]
    fn has_plugins_empty_some() {
        let args = ConfigArgs {
            plugins: Some(vec![]),
            ..Default::default()
        };
        assert!(args.has_plugins());
    }

    #[test]
    fn has_plugins_default_none() {
        assert!(!ConfigArgs::default().has_plugins());
    }

    #[test]
    fn has_plugins_multiple() {
        let args = ConfigArgs {
            plugins: Some(vec!["a.so".into(), "b.so".into(), "c.so".into()]),
            ..Default::default()
        };
        assert!(args.has_plugins());
    }

    #[test]
    fn merge_opt_none_keeps_existing() {
        let mut config = Config::default();
        config.plugins = vec!["existing.so".into()];
        let args = ConfigArgs {
            plugins: None,
            ..Default::default()
        };
        config.merge_opt(args);
        assert_eq!(config.plugins, vec![PathBuf::from("existing.so")]);
    }

    #[test]
    fn merge_opt_default_noop() {
        let config = Config::default();
        let original = config.plugins.clone();
        let mut config = config;
        config.merge_opt(ConfigArgs::default());
        assert_eq!(config.plugins, original);
    }

    #[test]
    fn merge_opt_absolute_path() {
        let mut config = Config::default();
        let args = ConfigArgs {
            plugins: Some(vec!["/usr/lib/anyrun/foo.so".into()]),
            ..Default::default()
        };
        config.merge_opt(args);
        assert_eq!(
            config.plugins,
            vec![PathBuf::from("/usr/lib/anyrun/foo.so")]
        );
    }

    #[test]
    fn merge_opt_relative_path() {
        let mut config = Config::default();
        let args = ConfigArgs {
            plugins: Some(vec!["./plugins/foo.so".into()]),
            ..Default::default()
        };
        config.merge_opt(args);
        assert_eq!(config.plugins, vec![PathBuf::from("./plugins/foo.so")]);
    }

    #[test]
    fn merge_opt_mixed_paths() {
        let mut config = Config::default();
        let args = ConfigArgs {
            plugins: Some(vec!["./rel.so".into(), "/abs.so".into(), "../up.so".into()]),
            ..Default::default()
        };
        config.merge_opt(args);
        assert_eq!(
            config.plugins,
            vec![
                PathBuf::from("./rel.so"),
                PathBuf::from("/abs.so"),
                PathBuf::from("../up.so"),
            ]
        );
    }

    #[test]
    fn has_plugins_true_when_specified() {
        let args = ConfigArgs {
            plugins: Some(vec!["libfoo.so".into()]),
            ..Default::default()
        };
        assert!(args.has_plugins());
    }

    #[test]
    fn has_plugins_false_when_unspecified() {
        let args = ConfigArgs {
            plugins: None,
            ..Default::default()
        };
        assert!(!args.has_plugins());
    }

    #[test]
    fn merge_opt_overrides_plugins() {
        let mut config = Config::default();
        let default_plugins = config.plugins.clone();
        assert_eq!(default_plugins.len(), 4);

        let args = ConfigArgs {
            plugins: Some(vec!["libcustom.so".into()]),
            ..Default::default()
        };
        config.merge_opt(args);
        assert_eq!(config.plugins, vec![PathBuf::from("libcustom.so")]);
    }

    #[test]
    fn test_search_ux_deserializes() {
        let config: SearchUxConfig = ron::from_str(
            r#"(
                settle_delay_ms: 180,
                flush_delay_ms: 30,
                typing_visual: KeepPrevious,
                bare_text_fast_lane: ["Applications", "Translate"],
                prefix_routes: [(prefix: "ff ", plugins: ["Browser Tabs"])],
                plugin_timeout_ms: 900,
                slow_plugin_ms: 300,
                prefix_discovery_trigger: "??",
                empty_state: (enabled: false, recent_limit: 3),
            )"#,
        )
        .unwrap();

        assert_eq!(config.settle_delay_ms, 180);
        assert_eq!(config.flush_delay_ms, 30);
        assert_eq!(config.typing_visual, TypingVisual::KeepPrevious);
        assert_eq!(config.prefix_routes[0].plugins, vec!["Browser Tabs"]);
        assert_eq!(config.plugin_timeout_ms, 900);
        assert_eq!(config.slow_plugin_ms, 300);
        assert_eq!(config.prefix_discovery_trigger, "??");
        assert!(!config.empty_state.enabled);
        assert_eq!(config.empty_state.recent_limit, 3);
    }
}
