use crate::{
    config::{self, Action, Config, Keybind},
    plugin_box::{PluginBox, PluginBoxInput, PluginBoxOutput, PluginMatch},
    provider, Args,
};
use anyrun_interface::HandleResult;
use anyrun_provider_ipc as ipc;
use gtk::{gdk, gio, glib, prelude::*};
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, LayerShell};
use relm4::{prelude::*, ComponentBuilder, Sender};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    io::{self, Write},
    path::PathBuf,
    sync::Arc,
};
use tokio::sync::mpsc;

#[derive(Deserialize, Serialize)]
pub enum PostRunAction {
    Stdout(Vec<u8>),
    None,
}

#[derive(Debug)]
pub enum AppMsg {
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

#[derive(Deserialize, Serialize)]
pub struct AppInit {
    pub args: Args,
    pub stdin: Vec<u8>,
    pub env: Vec<(String, String)>,
}

pub struct App {
    config: Arc<Config>,
    invocation: Option<gio::DBusMethodInvocation>,
    plugins: FactoryVecDeque<PluginBox>,
    post_run_action: PostRunAction,
    tx: mpsc::Sender<anyrun_provider_ipc::Request>,
}

impl App {
    pub fn launch(
        app: &gtk::Application,
        app_init: AppInit,
        invocation: Option<gio::DBusMethodInvocation>,
    ) -> Sender<AppMsg> {
        let builder = ComponentBuilder::<App>::default();

        let connector = builder.launch((app_init, invocation));

        let mut controller = connector.detach();
        let window = controller.widget();
        app.add_window(window);
        window.show();

        controller.detach_runtime();
        controller.sender().clone()
    }

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

#[relm4::component(pub)]
impl Component for App {
    type Input = AppMsg;
    type Output = ();
    type Init = (AppInit, Option<gio::DBusMethodInvocation>);
    type CommandOutput = anyrun_provider_ipc::Response;

    view! {
        gtk::Window {
            init_layer_shell: (),
            set_layer: match config.layer {
                config::Layer::Background => gtk4_layer_shell::Layer::Background,
                config::Layer::Bottom => gtk4_layer_shell::Layer::Bottom,
                config::Layer::Top => gtk4_layer_shell::Layer::Top,
                config::Layer::Overlay => gtk4_layer_shell::Layer::Overlay,
            },
            set_anchor: (Edge::Left, true),
            set_anchor: (Edge::Top, true),
            set_keyboard_mode: match config.keyboard_mode {
                config::KeyboardMode::Exclusive => gtk4_layer_shell::KeyboardMode::Exclusive,
                config::KeyboardMode::OnDemand => gtk4_layer_shell::KeyboardMode::OnDemand,
            },
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

            #[name = "main"]
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
                            match key {
                                gdk::Key::Tab => glib::Propagation::Stop,
                                _ => glib::Propagation::Proceed,
                            }
                        }
                    }
                },
                #[local]
                plugins -> gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_can_focus: false,
                    set_css_classes: &["matches"],
                    set_hexpand: true,
                }
            }
        }
    }

    fn init(
        (app_init, invocation): Self::Init,
        root: Self::Root,
        sender: relm4::ComponentSender<Self>,
    ) -> relm4::ComponentParts<Self> {
        let user_dir = env::var("XDG_CONFIG_HOME")
            .map(|c| format!("{c}/anyrun"))
            .or_else(|_| env::var("HOME").map(|h| format!("{h}/.config/anyrun")))
            .unwrap();

        let config_dir = app_init
            .args
            .config_dir
            .clone()
            .map(Some)
            .unwrap_or_else(|| {
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

        config.merge_opt(app_init.args.config.clone());

        let config = Arc::new(config);

        let plugins = gtk::Box::builder().build();

        let plugins_factory = FactoryVecDeque::<PluginBox>::builder()
            .launch(plugins.clone())
            .forward(sender.input_sender(), AppMsg::PluginOutput);

        let (tx, rx) = mpsc::channel(10);

        sender.spawn_command(glib::clone!(
            #[strong]
            config,
            #[strong]
            config_dir,
            #[strong(rename_to = stdin)]
            app_init.stdin,
            #[strong(rename_to = env)]
            app_init.env,
            move |sender| {
                if let Err(why) = provider::worker(config, config_dir, rx, sender, stdin, env) {
                    eprintln!("[anyrun] IPC worker returned an error: {why}");
                }
            }
        ));

        let widgets = view_output!();
        let model = Self {
            invocation,
            config,
            plugins: plugins_factory,
            post_run_action: PostRunAction::None,
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
                let width = self.config.width.to_val(mon_width);
                let x = self.config.x.to_val(mon_width) - width / 2;
                let height = self.config.height.to_val(mon_height);
                let y = self.config.y.to_val(mon_height) - height / 2;

                if self.config.close_on_click {
                    root.set_anchor(Edge::Bottom, true);
                    root.set_anchor(Edge::Right, true);
                    root.set_default_size(mon_width as i32, mon_height as i32);
                    widgets.main.set_halign(gtk::Align::Fill);
                    widgets.main.set_margin_start(x);
                    widgets.main.set_margin_top(y);
                    widgets.main.set_margin_end(mon_width as i32 - x - width);
                    widgets
                        .main
                        .set_margin_bottom(mon_height as i32 - y - height);
                } else {
                    root.set_default_size(width, height);
                    root.child().unwrap().set_size_request(width, height);
                    root.set_margin(Edge::Left, x);
                    root.set_margin(Edge::Top, y);
                }
                root.show();

                // If show_results_immediately is enabled, trigger initial search with empty input
                if self.config.show_results_immediately {
                    let _ = self.tx.blocking_send(anyrun_provider_ipc::Request::Query {
                        text: String::new(),
                    });
                }
            }
            AppMsg::KeyPressed { key, modifier } => {
                if let Some(Keybind { action, .. }) = self.config.keybinds.iter().find(|keybind| {
                    keybind.key == key
                        && keybind.ctrl == modifier.contains(gdk::ModifierType::CONTROL_MASK)
                        && keybind.alt == modifier.contains(gdk::ModifierType::ALT_MASK)
                        && keybind.shift == modifier.contains(gdk::ModifierType::SHIFT_MASK)
                }) {
                    sender.input(AppMsg::Action(*action));
                }
            }
            AppMsg::Action(action) => match action {
                Action::Close => {
                    if let Some(invocation) = self.invocation.clone() {
                        invocation.return_value(Some(
                            &(serde_json::to_vec(&self.post_run_action).unwrap(),).to_variant(),
                        ));
                    } else {
                        match &self.post_run_action {
                            PostRunAction::Stdout(bytes) => {
                                io::stdout().lock().write_all(bytes).unwrap()
                            }
                            PostRunAction::None => (),
                        }
                        root.application().unwrap().quit();
                    }
                    root.close();
                    // FIXME: Make sure the worker has actually correctly shut down before
                    // exiting
                    let _ = self.tx.blocking_send(ipc::Request::Quit);
                    relm4::runtime_util::shutdown_all();
                }
                Action::Select => {
                    if let Some((_, plugin, plugin_match)) = self.current_selection() {
                        let _ = self.tx.blocking_send(ipc::Request::Handle {
                            plugin: plugin.plugin_info.clone(),
                            selection: plugin_match.content.clone(),
                        });
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
                let _ = self.tx.blocking_send(ipc::Request::Query { text });
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
                    HandleResult::Close => sender.input(AppMsg::Action(Action::Close)),
                    HandleResult::Refresh(exclusive) => {
                        let _ = self.tx.blocking_send(ipc::Request::Query {
                            text: widgets.entry.text().into(),
                        });
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
                        let vec = rvec.to_vec();
                        let mime = tree_magic_mini::from_u8(&rvec);
                        if match mime {
                            "TEXT" | "STRING" | "UTF8_STRING" => true,
                            mime if mime.starts_with("text/") => true,
                            _ => false,
                        } {
                            root.clipboard().set_text(&String::from_utf8_lossy(&rvec));
                        } else {
                            let content = gdk::ContentProvider::for_bytes(
                                mime,
                                &glib::Bytes::from_owned(vec.clone()),
                            );
                            if let Err(why) = root.clipboard().set_content(Some(&content)) {
                                eprintln!("[anyrun] Error setting clipboard content: {why}");
                            }
                        }
                        sender.input(AppMsg::Action(Action::Close));
                    }
                    HandleResult::Stdout(rvec) => {
                        io::stdout().lock().write_all(&rvec).unwrap();
                        self.post_run_action = PostRunAction::Stdout(rvec.into());
                        sender.input(AppMsg::Action(Action::Close));
                    }
                }
            }
        }
        self.update_view(widgets, sender);
    }
}
