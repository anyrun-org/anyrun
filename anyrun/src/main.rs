use std::{cell::RefCell, env, fs, mem, path::PathBuf, rc::Rc, time::Duration};

use abi_stable::std_types::{ROption, RVec};
use anyrun_interface::{HandleResult, Match, PluginInfo, PluginRef, PollResult};
use gtk4::{
    gdk, gdk_pixbuf,
    glib::{self, signal::Inhibit},
    prelude::*,
};
use nix::unistd;
use serde::Deserialize;
use wl_clipboard_rs::copy;

#[derive(Deserialize)]
struct Config {
    width: RelativeNum,
    vertical_offset: RelativeNum,
    position: Position,
    plugins: Vec<PathBuf>,
    hide_icons: bool,
    hide_plugin_info: bool,
    ignore_exclusive_zones: bool,
    layer: Layer,
}

#[derive(Deserialize)]
enum Layer {
    Background,
    Bottom,
    Top,
    Overlay,
}

// Could have a better name
#[derive(Deserialize)]
enum RelativeNum {
    Absolute(i32),
    Fraction(f32),
}

/// A "view" of plugin's info and matches
#[derive(Clone)]
struct PluginView {
    plugin: PluginRef,
    row: gtk4::ListBoxRow,
    list: gtk4::ListBox,
}

struct Args {
    override_plugins: Option<Vec<String>>,
    config_dir: Option<String>,
}

#[derive(Deserialize)]
enum Position {
    Top,
    Center,
}

/// Actions to run after GTK4 has finished
enum PostRunAction {
    Copy(Vec<u8>),
    None,
}

/// Some data that needs to be shared between various parts
struct RuntimeData {
    args: Args,
    /// A plugin may request exclusivity which is set with this
    exclusive: Option<PluginView>,
    plugins: Vec<PluginView>,
    post_run_action: PostRunAction,
}

/// The naming scheme for CSS styling
///
/// 4Refer to [GTK 3.0 CSS Overview](https://docs.gtk.org/gtk3/css-overview.html)
/// and [GTK4 3.0 CSS Properties](https://docs.gtk4.org/gtk43/css-properties.html) for how to style.
mod style_names {
    /// The text entry box
    pub const ENTRY: &str = "entry";
    /// "Main" widgets (main Gtk4ListBox, main Gtk4Box)
    pub const MAIN: &str = "main";
    /// The window
    pub const WINDOW: &str = "window";
    /// Widgets related to the whole plugin. Including the info box
    pub const PLUGIN: &str = "plugin";
    /// Widgets for the specific match `MATCH_*` names are for more specific parts.
    pub const MATCH: &str = "match";

    pub const MATCH_TITLE: &str = "match-title";
    pub const MATCH_DESC: &str = "match-desc";
}

/// Default config directory
pub const DEFAULT_CONFIG_DIR: &str = "/etc/anyrun";

fn main() {
    let app = gtk4::Application::new(Some("com.kirottu.anyrun"), Default::default());
    let runtime_data: Rc<RefCell<Option<RuntimeData>>> = Rc::new(RefCell::new(None));

    // Append the launch options to the GTK4 Application
    app.add_main_option(
        "override-plugins",
        glib::Char('o' as i8),
        glib::OptionFlags::IN_MAIN,
        glib::OptionArg::StringArray,
        "Override plugins. Provide paths in same format as in the config file",
        None,
    );
    app.add_main_option(
        "config-dir",
        glib::Char('c' as i8),
        glib::OptionFlags::IN_MAIN,
        glib::OptionArg::String,
        "Override the config directory from the default (~/.config/anyrun/)",
        None,
    );

    let runtime_data_clone = runtime_data.clone();
    app.connect_handle_local_options(move |_app, dict| {
        let override_plugins = dict.lookup::<Vec<String>>("override-plugins").unwrap();
        let config_dir = dict.lookup::<String>("config-dir").unwrap();

        *runtime_data_clone.borrow_mut() = Some(RuntimeData {
            args: Args {
                override_plugins,
                config_dir,
            },
            exclusive: None,
            plugins: Vec::new(),
            post_run_action: PostRunAction::None,
        });
        -1 // Magic GTK4 number to continue running
    });

    let runtime_data_clone = runtime_data.clone();
    app.connect_activate(move |app| activate(app, runtime_data_clone.clone()));

    app.run();

    let runtime_data = runtime_data.borrow_mut().take().unwrap();

    // Perform a post run action if one is set
    match runtime_data.post_run_action {
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
                    copy::Source::Bytes(bytes.into_boxed_slice()),
                    copy::MimeType::Autodetect,
                )
                .expect("Failed to serve copy bytes");
            }
            Err(why) => {
                eprintln!("Failed to fork for copy sharing: {}", why);
            }
        },
        PostRunAction::None => (),
    }
}

fn activate(app: &gtk4::Application, runtime_data: Rc<RefCell<Option<RuntimeData>>>) {
    // Figure out the config dir
    let user_dir = format!(
        "{}/.config/anyrun",
        env::var("HOME").expect("Could not determine home directory! Is $HOME set?")
    );
    let config_dir =
        if let Some(config_dir) = &runtime_data.borrow().as_ref().unwrap().args.config_dir {
            config_dir.clone()
        } else if PathBuf::from(&user_dir).exists() {
            user_dir
        } else {
            DEFAULT_CONFIG_DIR.to_string()
        };
    // Load config
    let config: Config = ron::from_str(
        &fs::read_to_string(format!("{}/config.ron", config_dir))
            .expect("Unable to read config file"),
    )
    .expect("Config file malformed");

    // Create the main window
    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .name(style_names::WINDOW)
        .build();

    // Init GTK4 layer shell
    gtk4_layer_shell::init_for_window(&window);

    // Make layer-window fullscreen
    gtk4_layer_shell::set_anchor(&window, gtk4_layer_shell::Edge::Top, true);
    gtk4_layer_shell::set_anchor(&window, gtk4_layer_shell::Edge::Bottom, true);
    gtk4_layer_shell::set_anchor(&window, gtk4_layer_shell::Edge::Left, true);
    gtk4_layer_shell::set_anchor(&window, gtk4_layer_shell::Edge::Right, true);

    gtk4_layer_shell::set_namespace(&window, "anyrun");

    if config.ignore_exclusive_zones {
        gtk4_layer_shell::set_exclusive_zone(&window, -1);
    }

    gtk4_layer_shell::set_keyboard_mode(&window, gtk4_layer_shell::KeyboardMode::Exclusive);

    match config.layer {
        Layer::Background => {
            gtk4_layer_shell::set_layer(&window, gtk4_layer_shell::Layer::Background)
        }
        Layer::Bottom => gtk4_layer_shell::set_layer(&window, gtk4_layer_shell::Layer::Bottom),
        Layer::Top => gtk4_layer_shell::set_layer(&window, gtk4_layer_shell::Layer::Top),
        Layer::Overlay => gtk4_layer_shell::set_layer(&window, gtk4_layer_shell::Layer::Overlay),
    };

    // Try to load custom CSS, if it fails load the default CSS
    let provider = gtk4::CssProvider::new();
    if let Ok(content) = fs::read_to_string(format!("{}/style.css", config_dir)) {
        provider.load_from_data(&content);
    } else {
        provider.load_from_data(include_str!("../res/style.css"));
    }
    gtk4::style_context_add_provider_for_display(
        &gdk::Display::default().expect("Failed to get GDK display for CSS provider!"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // Use the plugins in the config file, or the plugins specified with the override
    let plugins = match &runtime_data
        .borrow()
        .as_ref()
        .unwrap()
        .args
        .override_plugins
    {
        Some(plugins) => plugins.iter().map(PathBuf::from).collect(),
        None => config.plugins,
    };

    // Make sure at least one plugin is specified
    if plugins.is_empty() {
        eprintln!("At least one plugin needs to be enabled!");
        app.quit();
    }

    // Create the main list of plugin views
    let main_list = gtk4::ListBox::builder()
        .selection_mode(gtk4::SelectionMode::None)
        .name(style_names::MAIN)
        .build();

    // Load plugins from the paths specified in the config file
    runtime_data.borrow_mut().as_mut().unwrap().plugins = plugins
        .iter()
        .map(|plugin_path| {
            // Load the plugin's dynamic library.
            let mut user_path = PathBuf::from(&format!("{}/plugins", config_dir));
            let mut global_path = PathBuf::from("/etc/anyrun/plugins");
            user_path.extend(plugin_path.iter());
            global_path.extend(plugin_path.iter());

            // Load the plugin's dynamic library.
            let plugin = if plugin_path.is_absolute() {
                abi_stable::library::lib_header_from_path(plugin_path)
            } else if user_path.exists() {
                abi_stable::library::lib_header_from_path(&user_path)
            } else {
                abi_stable::library::lib_header_from_path(&global_path)
            }
            .and_then(|plugin| plugin.init_root_module::<PluginRef>())
            .expect("Failed to load plugin");

            // Run the plugin's init code to init static resources etc.
            plugin.init()(config_dir.clone().into());

            let plugin_box = gtk4::Box::builder()
                .orientation(gtk4::Orientation::Horizontal)
                .spacing(10)
                .name(style_names::PLUGIN)
                .build();
            if !config.hide_plugin_info {
                plugin_box.append(&create_info_box(&plugin.info()(), config.hide_icons));
                plugin_box.append(
                    &gtk4::Separator::builder()
                        .orientation(gtk4::Orientation::Horizontal)
                        .name(style_names::PLUGIN)
                        .build(),
                );
            }
            let list = gtk4::ListBox::builder()
                .name(style_names::PLUGIN)
                .hexpand(true)
                .build();

            plugin_box.append(&list);

            let row = gtk4::ListBoxRow::builder()
                .name(style_names::PLUGIN)
                .build();
            row.set_child(Some(&plugin_box));

            main_list.append(&row);

            PluginView { plugin, row, list }
        })
        .collect::<Vec<PluginView>>();

    // Connect selection events to avoid completely messing up selection logic
    for plugin_view in runtime_data.borrow().as_ref().unwrap().plugins.iter() {
        let plugins_clone = runtime_data.borrow().as_ref().unwrap().plugins.clone();
        plugin_view.list.connect_row_selected(move |list, row| {
            if row.is_some() {
                // Unselect everything except the new selection
                for view in &plugins_clone {
                    if view.list != *list {
                        view.list.select_row(None::<&gtk4::ListBoxRow>);
                    }
                }
            }
        });
    }

    // Text entry box
    let entry = gtk4::Entry::builder()
        .hexpand(true)
        .name(style_names::ENTRY)
        .build();

    // Refresh the matches when text input changes
    let runtime_data_clone = runtime_data.clone();
    entry.connect_changed(move |entry| {
        refresh_matches(
            entry.text().to_string(),
            runtime_data_clone.clone(),
            config.hide_icons,
        )
    });

    // Handle other key presses for selection control and all other things that may be needed

    let event_controller_key = gtk4::EventControllerKey::new();

    let entry_clone = entry.clone();
    event_controller_key.connect_key_pressed(move |_, key, _, _| {
        match key {
            // Close window on escape
            gdk::Key::Escape => {
                window.close();
                Inhibit(true)
            }
            // Handle selections
            gdk::Key::Down | gdk::Key::Tab | gdk::Key::Up => {
                // Combine all of the matches into a `Vec` to allow for easier handling of the selection
                let combined_matches = runtime_data
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .plugins
                    .iter()
                    .flat_map(|view| {
                        view.list.children().into_iter().map(|child| {
                            (
                                // All children of lists are Gtk4ListBoxRow widgets
                                child.dynamic_cast::<gtk4::ListBoxRow>().unwrap(),
                                view.list.clone(),
                            )
                        })
                    })
                    .collect::<Vec<(gtk4::ListBoxRow, gtk4::ListBox)>>();

                // Get the selected match
                let (selected_match, selected_list) = match runtime_data
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .plugins
                    .iter()
                    .find_map(|view| view.list.selected_row().map(|row| (row, view.list.clone())))
                {
                    Some(selected) => selected,
                    None => {
                        // If nothing is selected select either the top or bottom match based on the input
                        if !combined_matches.is_empty() {
                            match key {
                                gdk::Key::Down | gdk::Key::Tab => combined_matches[0]
                                    .1
                                    .select_row(Some(&combined_matches[0].0)),
                                gdk::Key::Up => combined_matches[combined_matches.len() - 1]
                                    .1
                                    .select_row(Some(
                                        &combined_matches[combined_matches.len() - 1].0,
                                    )),
                                _ => unreachable!(),
                            }
                        }
                        return Inhibit(true);
                    }
                };

                // Clear the previous selection
                selected_list.select_row(None::<&gtk4::ListBoxRow>);

                // Get the index of the current selection
                let index = combined_matches
                    .iter()
                    .position(|(row, _)| *row == selected_match)
                    .unwrap();

                // Move the selection based on the input, loops from top to bottom and vice versa
                match key {
                    gdk::Key::Down | gdk::Key::Tab => {
                        if index < combined_matches.len() - 1 {
                            combined_matches[index + 1]
                                .1
                                .select_row(Some(&combined_matches[index + 1].0));
                        } else {
                            combined_matches[0]
                                .1
                                .select_row(Some(&combined_matches[0].0));
                        }
                    }
                    gdk::Key::Up => {
                        if index > 0 {
                            combined_matches[index - 1]
                                .1
                                .select_row(Some(&combined_matches[index - 1].0));
                        } else {
                            combined_matches[combined_matches.len() - 1]
                                .1
                                .select_row(Some(&combined_matches[combined_matches.len() - 1].0));
                        }
                    }
                    _ => unreachable!(),
                }

                Inhibit(true)
            }
            // Handle when the selected match is "activated"
            gdk::Key::Return => {
                let mut _runtime_data = runtime_data.borrow_mut();

                let (selected_match, plugin_view) = match _runtime_data
                    .as_ref()
                    .unwrap()
                    .plugins
                    .iter()
                    .find_map(|view| view.list.selected_row().map(|row| (row, view)))
                {
                    Some(selected) => selected,
                    None => {
                        return Inhibit(false);
                    }
                };

                // Perform actions based on the result of handling the selection
                match plugin_view.plugin.handle_selection()(unsafe {
                    (*selected_match.data::<Match>("match").unwrap().as_ptr()).clone()
                }) {
                    HandleResult::Close => {
                        window.close();
                        Inhibit(true)
                    }
                    HandleResult::Refresh(exclusive) => {
                        if exclusive {
                            _runtime_data.as_mut().unwrap().exclusive = Some(plugin_view.clone());
                        } else {
                            _runtime_data.as_mut().unwrap().exclusive = None;
                        }
                        mem::drop(_runtime_data); // Drop the mutable borrow
                        refresh_matches(
                            entry_clone.text().into(),
                            runtime_data.clone(),
                            config.hide_icons,
                        );
                        Inhibit(false)
                    }
                    HandleResult::Copy(bytes) => {
                        _runtime_data.as_mut().unwrap().post_run_action =
                            PostRunAction::Copy(bytes.into());
                        window.close();
                        Inhibit(true)
                    }
                }
            }
            _ => Inhibit(false),
        }
    });

    // Show the window initially, so it gets allocated and configured
    window.show();

    // Create widgets here for proper positioning
    window
        .surface()
        .connect_notify(Some("state"), move |surface, _| {
            let width = match config.width {
                RelativeNum::Absolute(width) => width,
                RelativeNum::Fraction(fraction) => (surface.width() as f32 * fraction) as i32,
            };
            // The Gtk4Fixed widget is used for absolute positioning of the main box
            let fixed = gtk4::Fixed::builder().build();
            let main_vbox = gtk4::Box::builder()
                .orientation(gtk4::Orientation::Vertical)
                .halign(gtk4::Align::Center)
                .vexpand(false)
                .width_request(width)
                .name(style_names::MAIN)
                .build();
            main_vbox.append(&entry);

            let vertical_offset = match config.vertical_offset {
                RelativeNum::Absolute(offset) => offset,
                RelativeNum::Fraction(fraction) => (surface.height() as f32 * fraction) as i32,
            } as f64;

            fixed.put(
                &main_vbox,
                (surface.width() - width) as f64 / 2.0,
                match config.position {
                    Position::Top => vertical_offset as f64,
                    Position::Center => {
                        (surface.height() - entry.allocated_height()) as f64 / 2.0 + vertical_offset
                    }
                },
            );
            window.set_child(Some(&fixed));
            window.show();

            // Append and show the list later, to avoid showing empty plugin categories on launch
            main_vbox.append(&main_list);
            main_list.show();
            entry.grab_focus(); // Grab the focus so typing is immediately accepted by the entry box
        });
}

fn handle_matches(
    plugin_view: PluginView,
    runtime_data: Rc<RefCell<Option<RuntimeData>>>,
    matches: RVec<Match>,
    hide_icons: bool,
) {
    // Clear out the old matches from the list
    while let Some(child) = plugin_view.list.row_at_index(0) {
        plugin_view.list.remove(&child);
    }

    // If there are no matches, hide the plugin's results
    if matches.is_empty() {
        plugin_view.row.hide();
        return;
    }

    for _match in matches {
        let hbox = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(10)
            .name(style_names::MATCH)
            .hexpand(true)
            .build();
        if !hide_icons {
            if let ROption::RSome(icon) = &_match.icon {
                let mut builder = gtk4::Image::builder()
                    .name(style_names::MATCH)
                    .pixel_size(32);

                let path = PathBuf::from(icon.as_str());

                // If the icon path is absolute, load that file
                builder = if path.is_absolute() {
                    builder.file(path.to_string_lossy())
                } else {
                    builder.icon_name(icon.to_string())
                };

                hbox.append(&builder.build());
            }
        }
        let title = gtk4::Label::builder()
            .name(style_names::MATCH_TITLE)
            .wrap(true)
            .use_markup(_match.use_pango)
            .halign(gtk4::Align::Start)
            .valign(gtk4::Align::Center)
            .vexpand(true)
            .label(_match.title.to_string())
            .build();

        // If a description is present, make a box with it and the title
        match &_match.description {
            ROption::RSome(desc) => {
                let title_desc_box = gtk4::Box::builder()
                    .orientation(gtk4::Orientation::Vertical)
                    .name(style_names::MATCH)
                    .hexpand(true)
                    .vexpand(true)
                    .build();
                title_desc_box.append(&title);
                title_desc_box.append(
                    &gtk4::Label::builder()
                        .name(style_names::MATCH_DESC)
                        .wrap(true)
                        .use_markup(_match.use_pango)
                        .halign(gtk4::Align::Start)
                        .valign(gtk4::Align::Center)
                        .label(desc.to_string())
                        .build(),
                );
                hbox.append(&title_desc_box);
            }
            ROption::RNone => {
                hbox.append(&title);
            }
        }
        let row = gtk4::ListBoxRow::builder()
            .name(style_names::MATCH)
            .height_request(32)
            .build();
        row.set_child(Some(&hbox));
        // GTK4 data setting is not type checked, so it is unsafe.
        // Only `Match` objects are stored though.
        unsafe {
            row.set_data("match", _match);
        }
        plugin_view.list.append(&row);
    }

    // Refresh the items in the view
    plugin_view.row.show();

    let combined_matches = runtime_data
        .borrow()
        .as_ref()
        .unwrap()
        .plugins
        .iter()
        .flat_map(|view| {
            view.list.children().into_iter().map(|child| {
                (
                    child.dynamic_cast::<gtk4::ListBoxRow>().unwrap(),
                    view.list.clone(),
                )
            })
        })
        .co4llect::<Vec<(gtk::ListBoxRow, gtk::ListBox)>>();

    if let Some((row, list)) = combined_matches.get(0) {
        list.select_row(Some(row));
    }
}

/// Create the info box for the plugin
fn create_info_box(info: &PluginInfo, hide_icons: bool) -> gtk4::Box {
    let info_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .name(style_names::PLUGIN)
        .width_request(200)
        .height_request(32)
        .spacing(10)
        .build();
    if !hide_icons {
        info_box.append(
            &gtk4::Image::builder()
                .icon_name(&info.icon)
                .name(style_names::PLUGIN)
                .pixel_size(32)
                .halign(gtk4::Align::Start)
                .valign(gtk4::Align::Start)
                .build(),
        );
    }
    info_box.append(
        &gtk4::Label::builder()
            .label(&info.name)
            .name(style_names::PLUGIN)
            .halign(gtk4::Align::End)
            .valign(gtk4::Align::Center)
            .hexpand(true)
            .build(),
    );
    // This is so that we can align the plugin name with the icon. GTK4 would not let it be properly aligned otherwise.
    let main_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .name(style_names::PLUGIN)
        .build();
    main_box.append(&info_box);
    main_box.append(
        &gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .name(style_names::PLUGIN)
            .build(),
    );
    main_box
}

/// Refresh the matches from the plugins
fn refresh_matches(
    input: String,
    runtime_data: Rc<RefCell<Option<RuntimeData>>>,
    hide_icons: bool,
) {
    for plugin_view in runtime_data.borrow().as_ref().unwrap().plugins.iter() {
        let id = plugin_view.plugin.get_matches()(input.clone().into());
        let plugin_view = plugin_view.clone();
        let runtime_data_clone = runtime_data.clone();
        // If the input is empty, skip getting matches and just clear everything out.
        if input.is_empty() {
            handle_matches(plugin_view, runtime_data_clone, RVec::new(), hide_icons);
        // If a plugin has requested exclusivity, respect it
        } else if let Some(exclusive) = &runtime_data.borrow().as_ref().unwrap().exclusive {
            if plugin_view.plugin.info() == exclusive.plugin.info() {
                glib::timeout_append_local(Duration::from_micros(1000), move || {
                    async_match(
                        plugin_view.clone(),
                        runtime_data_clone.clone(),
                        id,
                        hide_icons,
                    )
                });
            } else {
                handle_matches(
                    plugin_view.clone(),
                    runtime_data_clone,
                    RVec::new(),
                    hide_icons,
                );
            }
        } else {
            glib::timeout_append_local(Duration::from_micros(1000), move || {
                async_match(
                    plugin_view.clone(),
                    runtime_data_clone.clone(),
                    id,
                    hide_icons,
                )
            });
        }
    }
}

/// Handle the asynchronously running match task
fn async_match(
    plugin_view: PluginView,
    runtime_data: Rc<RefCell<Option<RuntimeData>>>,
    id: u64,
    hide_icons: bool,
) -> glib::Continue {
    match plugin_view.plugin.poll_matches()(id) {
        PollResult::Ready(matches) => {
            handle_matches(plugin_view, runtime_data, matches, hide_icons);
            glib::Continue(false)
        }
        PollResult::Pending => glib::Continue(true),
        PollResult::Cancelled => glib::Continue(false),
    }
}
