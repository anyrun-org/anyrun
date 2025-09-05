use std::{
    cell::RefCell,
    env, fs,
    io::{self, Write},
    path::PathBuf,
    rc::Rc,
    sync::{mpsc, Arc},
};

use anyrun_interface::HandleResult;
use anyrun_provider_ipc as ipc;
use clap::Parser;
use gtk::{gdk, glib, prelude::*};
use gtk4::{
    self as gtk,
    gio::{self, ApplicationFlags},
};
use gtk4_layer_shell::{Edge, KeyboardMode, LayerShell};
use nix::unistd;
use relm4::{prelude::*, ComponentBuilder};
use wl_clipboard_rs::copy;

use crate::{
    config::{Action, Config, ConfigArgs, Keybind},
    plugin_box::{PluginBox, PluginBoxInput, PluginBoxOutput, PluginMatch},
};

mod config;
mod plugin_box;
mod socket;

// /// Actions to run after GTK has finished
// enum PostRunAction {
//     Copy(Vec<u8>),
//     None,
// }

/// A wayland native, highly customizable runner.
#[derive(Parser, Clone)]
#[command(version, about)]
struct Args {
    /// Override the path to the config directory
    #[arg(short, long)]
    config_dir: Option<String>,
    /// Run as a daemon
    #[arg(short, long)]
    daemon: bool,

    #[command(flatten)]
    config: ConfigArgs,
}

struct App {
    config: Arc<Config>,
    plugins: FactoryVecDeque<PluginBox>,
    // post_run_action: PostRunAction,
    tx: mpsc::Sender<anyrun_provider_ipc::Request>,
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
impl Component for App {
    type Input = AppMsg;
    type Output = ();
    type Init = Args;
    type CommandOutput = anyrun_provider_ipc::Response;

    view! {
        gtk::Window {
            init_layer_shell: (),
            set_layer: gtk4_layer_shell::Layer::Top,
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

            add_controller = gtk::GestureClick {
                connect_pressed[sender, config] => move |_, _, _, _| {
                    if config.close_on_click {
                        sender.input(AppMsg::Action(Action::Close));
                    }
                }
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_halign: gtk::Align::Center,
                set_vexpand: false,
                set_hexpand: true,
                set_css_classes: &["main"],

                #[name = "entry"]
                gtk::Text {
                    set_hexpand: true,
                    set_activates_default: false,
                    connect_changed[sender] => move |entry| {
                        sender.input(AppMsg::EntryChanged(entry.text().into()));
                    },

                    add_controller = gtk::EventControllerKey {
                        connect_key_pressed[sender] => move |_, key, _, modifier| {
                            sender.input(AppMsg::KeyPressed { key, modifier});
                            glib::Propagation::Proceed
                        }
                    }
                },
                #[local]
                plugins -> gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_css_classes: &["matches"],
                    set_hexpand: true,
                }
            }
        }
    }

    fn init(
        args: Self::Init,
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
                ipc::CONFIG_DIRS
                    .iter()
                    .map(|path| path.to_string())
                    .find(|path| PathBuf::from(path).exists())
            }
        });

        let mut config = if let Some(config_dir) = &config_dir {
            match fs::read_to_string(format!("{config_dir}/style.css")) {
                Ok(style) => {
                    relm4::set_global_css_with_priority(&style, gtk::STYLE_PROVIDER_PRIORITY_USER)
                }
                Err(why) => {
                    eprintln!("[anyrun] Failed to load CSS: {why}");
                    relm4::set_global_css_with_priority(
                        include_str!("../res/style.css"),
                        gtk::STYLE_PROVIDER_PRIORITY_USER,
                    );
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

        let config = Arc::new(config);

        let plugins = gtk::Box::builder().build();

        let plugins_factory = FactoryVecDeque::<PluginBox>::builder()
            .launch(plugins.clone())
            .forward(sender.input_sender(), AppMsg::PluginOutput);

        let (tx, rx) = mpsc::channel();

        sender.spawn_command(glib::clone!(
            #[strong]
            config,
            #[strong]
            config_dir,
            move |sender| {
                if let Err(why) = socket::worker(config, config_dir, rx, sender) {
                    eprintln!("[anyrun] IPC worker returned an error: {why}");
                }
            }
        ));

        let widgets = view_output!();
        let model = Self {
            config,
            plugins: plugins_factory,
            tx,
        };

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
                let x = self.config.x.to_val(mon_width) - width / 2;
                let height = self.config.height.to_val(mon_height);
                let y = self.config.y.to_val(mon_height) - height / 2;

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
                Action::Close => {
                    root.close();
                    self.tx.send(ipc::Request::Quit).unwrap();
                }
                Action::Select => {
                    if let Some((_, plugin, plugin_match)) = self.current_selection() {
                        self.tx
                            .send(ipc::Request::Handle {
                                plugin: plugin.plugin_info.clone(),
                                selection: plugin_match.content.clone(),
                            })
                            .unwrap();
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
                self.tx.send(ipc::Request::Query { text }).unwrap();
            }
            AppMsg::PluginOutput(PluginBoxOutput::MatchesLoaded) => {
                if let Some((plugin, plugin_match)) = self.combined_matches().first() {
                    plugin.matches.widget().select_row(Some(&plugin_match.row));
                }
            }
            // Handle clicked selections
            AppMsg::PluginOutput(PluginBoxOutput::RowSelected(index)) => {
                for (i, plugin) in self.plugins.iter().enumerate() {
                    if i != index.current_index() {
                        plugin
                            .matches
                            .widget()
                            .select_row(Option::<&gtk::ListBoxRow>::None);
                    }
                }
            }
        }
        self.update_view(widgets, sender);
    }

    fn update_cmd_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::CommandOutput,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        match message {
            ipc::Response::Ready { info } => {
                let mut guard = self.plugins.guard();
                for info in info {
                    guard.push_back((info, self.config.clone()));
                }
            }
            ipc::Response::Matches { plugin, matches } => {
                let i = self
                    .plugins
                    .iter()
                    .enumerate()
                    .find_map(|(i, plugin_box)| {
                        if plugin_box.plugin_info == plugin {
                            Some(i)
                        } else {
                            None
                        }
                    })
                    .unwrap();

                self.plugins.send(i, PluginBoxInput::Matches(matches));
            }
            ipc::Response::Handled { plugin, result } => {
                match result {
                    HandleResult::Close => root.close(),
                    HandleResult::Refresh(exclusive) => {
                        self.tx
                            .send(ipc::Request::Query {
                                text: widgets.entry.text().into(),
                            })
                            .unwrap();
                        if exclusive {
                            for (i, plugin_box) in self.plugins.iter().enumerate() {
                                // While normally true, in this case the function addresses will be consistent
                                // at runtime so it is fine for differentiating between them
                                if plugin_box.plugin_info != plugin {
                                    self.plugins.send(i, PluginBoxInput::Enable(false));
                                }
                            }
                        } else {
                            self.plugins.broadcast(PluginBoxInput::Enable(true));
                        }
                    }
                    HandleResult::Copy(rvec) => {
                        // *self.post_run_action.borrow_mut() = PostRunAction::Copy(rvec.into());
                        // FIXME: Maybe handle more mimetypes?
                        root.clipboard()
                            .set_text(&String::from_utf8(rvec.into()).unwrap());
                        sender.input(AppMsg::Action(Action::Close));
                    }
                    HandleResult::Stdout(rvec) => {
                        io::stdout().lock().write_all(&rvec).unwrap();
                        sender.input(AppMsg::Action(Action::Close));
                    }
                }
            }
        }
        self.update_view(widgets, sender);
    }
}

fn main() {
    let args = Args::parse();
    let flags = if args.daemon {
        gio::ApplicationFlags::IS_SERVICE
    } else {
        Default::default()
    };
    let app = gtk::Application::new(Some("org.anyrun.anyrun"), flags);
    // app.register(Option::<&gio::Cancellable>::None).unwrap();

    let relm_app = RelmApp::from_app(app).with_args(vec![]);

    relm_app.run::<App>(args);

    // Perform a post run action if one is set
    // match &*post_run_action.borrow() {
    //     PostRunAction::Copy(bytes) => match unsafe { unistd::fork() } {
    //         // The parent process just exits and prints that out
    //         Ok(unistd::ForkResult::Parent { .. }) => {
    //             println!("Child spawned to serve copy requests.");
    //         }
    //         // Child process starts serving copy requests
    //         Ok(unistd::ForkResult::Child) => {
    //             let mut opts = copy::Options::new();
    //             opts.foreground(true);
    //             opts.copy(
    //                 copy::Source::Bytes(bytes.clone().into_boxed_slice()),
    //                 copy::MimeType::Autodetect,
    //             )
    //             .expect("Failed to serve copy bytes");
    //         }
    //         Err(why) => {
    //             eprintln!("Failed to fork for copy sharing: {why}");
    //         }
    //     },
    //     PostRunAction::None => (),
    // }; // Load bearing semicolon

    // NOTES:
    // gtk::Application::activate event gets called in the primary process when
    // a remote starts up
    //
    // gtk::Application::startup is only called in the primary (maybe??)
    //
    // Gotta figure out how to transmit args from the remote to the primary
    //
}
