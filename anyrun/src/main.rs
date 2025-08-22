use std::{
    cell::RefCell,
    env, fs,
    io::{self, Write},
    path::PathBuf,
    rc::Rc,
};

use anyrun_interface::{HandleResult, PluginRef};
use anyrun_macros::ConfigArgs;
use clap::{Parser, ValueEnum};
use gtk::{gdk, glib, prelude::*};
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, KeyboardMode, LayerShell};
use nix::unistd;
use relm4::prelude::*;
use serde::{de::Visitor, Deserialize, Deserializer};
use wl_clipboard_rs::copy;

use crate::plugin_box::{PluginBox, PluginBoxInput, PluginBoxOutput, PluginMatch};

mod plugin_box;

#[derive(Deserialize, ConfigArgs)]
struct Config {
    #[serde(default = "Config::default_x")]
    x: RelativeNum,

    #[serde(default = "Config::default_y")]
    y: RelativeNum,

    #[serde(default = "Config::default_width")]
    width: RelativeNum,

    #[serde(default = "Config::default_height")]
    height: RelativeNum,

    /// Margin to put around the main box, allows for shadow styling
    #[serde(default)]
    margin: u32,

    #[serde(default = "Config::default_plugins")]
    plugins: Vec<PathBuf>,

    #[serde(default)]
    hide_icons: bool,
    #[serde(default)]
    hide_plugin_info: bool,
    #[serde(default)]
    ignore_exclusive_zones: bool,
    #[serde(default)]
    close_on_click: bool,
    #[serde(default)]
    show_results_immediately: bool,
    #[serde(default)]
    max_entries: Option<usize>,
    #[serde(default = "Config::default_layer")]
    layer: Layer,

    #[config_args(skip)]
    #[serde(default = "Config::default_keybinds")]
    keybinds: Vec<Keybind>,
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
        RelativeNum::Absolute(0)
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            x: Self::default_x(),
            y: Self::default_y(),
            width: Self::default_width(),
            height: Self::default_height(),
            margin: 0,
            plugins: Self::default_plugins(),
            hide_icons: false,
            hide_plugin_info: false,
            ignore_exclusive_zones: false,
            close_on_click: false,
            show_results_immediately: false,
            max_entries: None,
            layer: Self::default_layer(),
            keybinds: Self::default_keybinds(),
        }
    }
}

#[derive(Deserialize, Clone, ValueEnum)]
enum Layer {
    Background,
    Bottom,
    Top,
    Overlay,
}

// Could have a better name
#[derive(Deserialize, Clone)]
enum RelativeNum {
    Absolute(i32),
    Fraction(f32),
}

impl RelativeNum {
    fn to_val(&self, val: u32) -> i32 {
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
enum Action {
    Close,
    Select,
    Up,
    Down,
}

#[derive(Deserialize, Clone)]
struct Keybind {
    #[serde(default)]
    ctrl: bool,
    #[serde(default)]
    alt: bool,
    #[serde(deserialize_with = "Keybind::deserialize_key")]
    key: gdk::Key,
    action: Action,
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

#[derive(Parser)]
struct Args {
    /// Override the path to the config directory
    #[arg(short, long)]
    config_dir: Option<String>,
    #[command(flatten)]
    config: ConfigArgs,
}

#[derive(Deserialize, Clone, ValueEnum)]
enum Position {
    Top,
    Center,
}

/// Actions to run after GTK has finished
enum PostRunAction {
    Copy(Vec<u8>),
    None,
}

/// The naming scheme for CSS styling
///
/// Refer to [GTK 3.0 CSS Overview](https://docs.gtk.org/gtk3/css-overview.html)
/// and [GTK 3.0 CSS Properties](https://docs.gtk.org/gtk3/css-properties.html) for how to style.
mod style_names {
    /// The text entry box
    pub const ENTRY: &str = "entry";
    /// The main large box containing every widget
    pub const MAIN: &str = "main";
    /// The list of matches
    pub const MATCHES: &str = "matches";
    /// The window
    pub const WINDOW: &str = "window";
    /// Widgets related to the whole plugin. Including the info box
    pub const PLUGIN: &str = "plugin";
    /// Widgets for the specific match `MATCH_*` names are for more specific parts.
    pub const MATCH: &str = "match";

    pub const MATCH_TITLE: &str = "match-title";
    pub const MATCH_DESC: &str = "match-desc";
}

// Default search paths, maintain backwards compatibility
pub const CONFIG_DIRS: &[&str] = &["/etc/xdg/anyrun", "/etc/anyrun"];
pub const PLUGIN_PATHS: &[&str] = &["/usr/lib/anyrun", "/etc/anyrun/plugins"];

struct App {
    config: Config,
    plugins: FactoryVecDeque<PluginBox>,
    post_run_action: Rc<RefCell<PostRunAction>>,
}

#[derive(Debug)]
enum AppMsg {
    Show {
        width: u32,
        height: u32,
    },
    KeyPressed {
        key: gdk::Key,
        modifier: gdk::ModifierType,
    },
    Action(Action),
    EntryChanged(String),
    PluginOutput(PluginBoxOutput),
}

impl App {
    /// Helper function to get the combined matches of all the plugins
    fn combined_matches(&self) -> Vec<(&PluginBox, &PluginMatch)> {
        self.plugins
            .iter()
            .flat_map(|plugin| {
                plugin
                    .matches
                    .iter()
                    .map(|plugin_match| (plugin, plugin_match))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn current_selection(&self) -> Option<(usize, &PluginBox, &PluginMatch)> {
        self.plugins
            .iter()
            .find_map(|plugin| {
                plugin
                    .matches
                    .widget()
                    .selected_row()
                    .map(|row| (plugin, row))
            })
            .map(|(plugin, row)| {
                let (i, plugin_match) = self
                    .combined_matches()
                    .iter()
                    .enumerate()
                    .find_map(|(i, (_, plugin_match))| {
                        if plugin_match.row == row {
                            Some((i, *plugin_match))
                        } else {
                            None
                        }
                    })
                    .unwrap(); // Unwrap is safe since we just obtained the selected one
                (i, plugin, plugin_match)
            })
    }
}

#[relm4::component]
impl<'a> Component for App {
    type Input = AppMsg;
    type Output = ();
    type Init = (Args, Rc<RefCell<PostRunAction>>);
    type CommandOutput = ();

    view! {
        gtk::Window {
            init_layer_shell: (),
            set_layer: gtk4_layer_shell::Layer::Top,
            set_widget_name: style_names::WINDOW,
            set_anchor: (Edge::Left, true),
            set_anchor: (Edge::Top, true),
            set_keyboard_mode: KeyboardMode::OnDemand,
            set_namespace: Some("anyrun"),

            connect_map[sender] => move |win| {
                let surface = win.surface().unwrap();
                let sender = sender.clone();
                surface.connect_enter_monitor(move |_, monitor| {
                    sender.input(AppMsg::Show {
                        width: monitor.geometry().width() as u32,
                        height: monitor.geometry().height() as u32,
                    });
                });
            },

            add_controller = gtk::EventControllerKey {
                connect_key_pressed[sender] => move |_, key, _, modifier| {
                    sender.input(AppMsg::KeyPressed { key, modifier});
                    glib::Propagation::Stop
                }
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_halign: gtk::Align::Center,
                set_vexpand: false,
                set_hexpand: true,
                set_widget_name: style_names::MAIN,
                set_margin_all: model.config.margin as i32,

                #[name = "entry"]
                gtk::Entry {
                    set_widget_name: style_names::ENTRY,
                    set_hexpand: true,
                    connect_changed[sender] => move |entry| {
                        sender.input(AppMsg::EntryChanged(entry.text().into()));
                    },
                    connect_activate => move |_entry| {
                        sender.input(AppMsg::Action(Action::Select));
                    }
                },
                #[local]
                plugins -> gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_widget_name: style_names::MATCHES,
                    set_hexpand: true,
                }
            }
        }
    }

    fn init(
        (args, post_run_action): Self::Init,
        root: Self::Root,
        sender: relm4::ComponentSender<Self>,
    ) -> relm4::ComponentParts<Self> {
        let user_dir = env::var("XDG_CONFIG_HOME")
            .map(|c| format!("{c}/anyrun"))
            .or_else(|_| env::var("HOME").map(|h| format!("{h}/.config/anyrun")))
            .unwrap();

        let config_dir = args.config_dir.map(Some).unwrap_or_else(|| {
            if PathBuf::from(&user_dir).exists() {
                Some(user_dir.clone())
            } else {
                CONFIG_DIRS
                    .iter()
                    .map(|path| path.to_string())
                    .find(|path| PathBuf::from(path).exists())
            }
        });

        let mut config = if let Some(config_dir) = &config_dir {
            match fs::read_to_string(format!("{config_dir}/style.css")) {
                Ok(style) => relm4::set_global_css(&style),
                Err(why) => {
                    eprintln!("[anyrun] Failed to load CSS: {why}");
                    relm4::set_global_css(include_str!("../res/style.css"));
                }
            }
            match fs::read(format!("{config_dir}/config.ron")) {
                Ok(content) => ron::de::from_bytes(&content).unwrap_or_else(|why| {
                    eprintln!("[anyrun] Failed to parse config file, using default values: {why}");
                    Config::default()
                }),
                Err(why) => {
                    eprintln!("[anyrun] Failed to read config file, using default values: {why}");
                    Config::default()
                }
            }
        } else {
            eprintln!("[anyrun] No config found in any searched paths");
            Config::default()
        };

        config.merge_opt(args.config);

        let plugins = gtk::Box::builder().build();

        let mut plugins_factory = FactoryVecDeque::<PluginBox>::builder()
            .launch(plugins.clone())
            .forward(sender.input_sender(), AppMsg::PluginOutput);

        for plugin in &config.plugins {
            let path = if plugin.is_absolute() {
                plugin.to_owned()
            } else {
                let mut search_dirs = vec![format!("{user_dir}/plugins")];
                search_dirs.extend(PLUGIN_PATHS.iter().map(|path| path.to_string()));

                if let Some(path) = search_dirs.iter().find(|path| PathBuf::from(path).exists()) {
                    PathBuf::from(path)
                } else {
                    eprintln!(
                        "[anyrun] Failed to locate library for plugin {}, not loading",
                        plugin.display()
                    );
                    continue;
                }
            };

            let Ok(header) = abi_stable::library::lib_header_from_path(&path) else {
                eprintln!("[anyrun] Failed to load plugin `{}` header", path.display());
                continue;
            };

            let Ok(plugin) = header.init_root_module::<PluginRef>() else {
                eprintln!(
                    "[anyrun] Failed to init plugin `{}` root module",
                    path.display()
                );
                continue;
            };

            plugin.init()(
                config_dir
                    .as_ref()
                    .cloned()
                    .unwrap_or(CONFIG_DIRS[0].to_string())
                    .into(),
            );

            plugins_factory.guard().push_back(plugin);
        }

        let model = Self {
            post_run_action,
            config,
            plugins: plugins_factory,
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::Input,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        match message {
            AppMsg::Show {
                width: mon_width,
                height: mon_height,
            } => {
                let window = relm4::main_application().active_window().unwrap();
                let width = self.config.width.to_val(mon_width);
                let x =
                    self.config.x.to_val(mon_width) - (width + self.config.margin as i32 * 2) / 2;
                let height = self.config.height.to_val(mon_height);
                let y =
                    self.config.y.to_val(mon_height) - (height + self.config.margin as i32 * 2) / 2;

                window.set_default_size(width, height);
                window.child().unwrap().set_size_request(width, height);
                window.set_margin(Edge::Left, x);
                window.set_margin(Edge::Top, y);
                window.show();
            }
            AppMsg::KeyPressed { key, modifier } => {
                if let Some(Keybind { action, .. }) = self.config.keybinds.iter().find(|keybind| {
                    keybind.key == key
                        && keybind.ctrl == modifier.contains(gdk::ModifierType::CONTROL_MASK)
                        && keybind.alt == modifier.contains(gdk::ModifierType::ALT_MASK)
                }) {
                    sender.input(AppMsg::Action(*action));
                }
            }
            AppMsg::Action(action) => match action {
                Action::Close => root.close(),
                Action::Select => {
                    if let Some((_, plugin, plugin_match)) = self.current_selection() {
                        match plugin.plugin.handle_selection()(plugin_match.content.clone()) {
                            HandleResult::Close => root.close(),
                            HandleResult::Refresh(exclusive) => {
                                if exclusive {
                                    for (i, _plugin) in self.plugins.iter().enumerate() {
                                        // While normally true, in this case the function addresses will be consistent
                                        // at runtime so it is fine for differentiating between them
                                        #[allow(unpredictable_function_pointer_comparisons)]
                                        if plugin.plugin.info() == _plugin.plugin.info() {
                                            self.plugins.send(
                                                i,
                                                PluginBoxInput::EntryChanged(
                                                    widgets.entry.text().into(),
                                                ),
                                            );
                                        } else {
                                            self.plugins.send(i, PluginBoxInput::Enable(false));
                                        }
                                    }
                                } else {
                                    self.plugins.broadcast(PluginBoxInput::Enable(true));
                                    self.plugins.broadcast(PluginBoxInput::EntryChanged(
                                        widgets.entry.text().into(),
                                    ));
                                }
                            }
                            HandleResult::Copy(rvec) => {
                                *self.post_run_action.borrow_mut() =
                                    PostRunAction::Copy(rvec.into());
                                root.close();
                            }
                            HandleResult::Stdout(rvec) => {
                                io::stdout().lock().write_all(&rvec).unwrap();
                                root.close();
                            }
                        }
                    }
                }
                Action::Up => {
                    if let Some((i, plugin, _)) = self.current_selection() {
                        let matches = self.combined_matches();
                        plugin
                            .matches
                            .widget()
                            .select_row(Option::<&gtk::ListBoxRow>::None);
                        if i > 0 {
                            let (plugin, plugin_match) = matches[i - 1];
                            plugin.matches.widget().select_row(Some(&plugin_match.row));
                        } else {
                            let (plugin, plugin_match) = matches.last().unwrap();
                            plugin.matches.widget().select_row(Some(&plugin_match.row));
                        }
                    }
                }
                Action::Down => {
                    if let Some((i, plugin, _)) = self.current_selection() {
                        let matches = self.combined_matches();
                        plugin
                            .matches
                            .widget()
                            .select_row(Option::<&gtk::ListBoxRow>::None);
                        if let Some((plugin, plugin_match)) = matches.get(i + 1) {
                            plugin.matches.widget().select_row(Some(&plugin_match.row));
                        } else {
                            let (plugin, plugin_match) = matches[0];
                            plugin.matches.widget().select_row(Some(&plugin_match.row));
                        }
                    }
                }
            },
            AppMsg::EntryChanged(text) => {
                self.plugins.broadcast(PluginBoxInput::EntryChanged(text));
            }
            AppMsg::PluginOutput(PluginBoxOutput::MatchesLoaded) => {
                if let Some((plugin, plugin_match)) = self.combined_matches().first() {
                    plugin.matches.widget().select_row(Some(&plugin_match.row));
                }
            }
        }
        self.update_view(widgets, sender);
    }
}

fn main() {
    let args = Args::parse();
    // This is done to avoid GTK looking up icons for an icon to match the appid
    // Yes it is dumb
    let gtk_app = gtk::Application::new(Option::<String>::None, Default::default());
    let app = RelmApp::from_app(gtk_app).with_args(vec![]);

    let post_run_action = Rc::new(RefCell::new(PostRunAction::None));

    app.run::<App>((args, post_run_action.clone()));

    // Perform a post run action if one is set
    match &*post_run_action.borrow() {
        PostRunAction::Copy(bytes) => match unsafe { unistd::fork() } {
            // The parent process just exits and prints that out
            Ok(unistd::ForkResult::Parent { .. }) => {
                println!("Child spawned to serve copy requests.");
            }
            // Child process starts serving copy requests
            Ok(unistd::ForkResult::Child) => {
                let mut opts = copy::Options::new();
                opts.foreground(true);
                opts.copy(
                    copy::Source::Bytes(bytes.clone().into_boxed_slice()),
                    copy::MimeType::Autodetect,
                )
                .expect("Failed to serve copy bytes");
            }
            Err(why) => {
                eprintln!("Failed to fork for copy sharing: {why}");
            }
        },
        PostRunAction::None => (),
    }; // Load bearing semicolon
}
