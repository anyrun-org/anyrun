use std::{cell::RefCell, env, fs, path::PathBuf, rc::Rc, time::Duration};

use abi_stable::std_types::{ROption, RVec};
use anyrun_interface::{HandleResult, Match, PluginInfo, PluginRef, PollResult};
use gtk::{gdk, gdk_pixbuf, glib, prelude::*};
use nix::unistd;
use serde::Deserialize;
use wl_clipboard_rs::copy;

#[derive(Deserialize)]
struct Config {
    width: u32,
    plugins: Vec<PathBuf>,
}

/// A "view" of plugin's info and matches
#[derive(Clone)]
struct PluginView {
    plugin: PluginRef,
    row: gtk::ListBoxRow,
    list: gtk::ListBox,
}

struct Args {
    override_plugins: Option<Vec<String>>,
    config_dir: Option<String>,
}

/// Actions to run after GTK has finished
enum PostRunAction {
    Copy(Vec<u8>),
    None,
}

/// Some data that needs to be shared between various parts
struct RuntimeData {
    args: Args,
    post_run_action: PostRunAction,
}

/// The naming scheme for CSS styling
///
/// Refer to [GTK 3.0 CSS Overview](https://docs.gtk.org/gtk3/css-overview.html)
/// and [GTK 3.0 CSS Properties](https://docs.gtk.org/gtk3/css-properties.html) for how to style.
mod style_names {
    /// The text entry box
    pub const ENTRY: &str = "entry";
    /// "Main" widgets (main GtkListBox, main GtkBox)
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

fn main() {
    let app = gtk::Application::new(Some("com.kirottu.anyrun"), Default::default());
    let runtime_data: Rc<RefCell<Option<RuntimeData>>> = Rc::new(RefCell::new(None));

    // Add the launch options to the GTK Application
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
            post_run_action: PostRunAction::None,
        });
        -1 // Magic GTK number to continue running
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
                println!("Failed to fork for copy sharing: {}", why);
            }
        },
        PostRunAction::None => (),
    }
}

fn activate(app: &gtk::Application, runtime_data: Rc<RefCell<Option<RuntimeData>>>) {
    // Figure out the config dir
    let config_dir = runtime_data
        .borrow()
        .as_ref()
        .unwrap()
        .args
        .config_dir
        .clone()
        .unwrap_or(format!(
            "{}/.config/anyrun",
            env::var("HOME").expect("Could not determine home directory! Is $HOME set?")
        ));

    // Load config
    let config: Config = ron::from_str(
        &fs::read_to_string(format!("{}/config.ron", config_dir))
            .expect("Unable to read config file!"),
    )
    .expect("Config file malformed!");

    // Create the main window
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .name(style_names::WINDOW)
        .width_request(config.width as i32)
        .build();

    // Init GTK layer shell
    gtk_layer_shell::init_for_window(&window);
    gtk_layer_shell::set_anchor(&window, gtk_layer_shell::Edge::Top, true);
    gtk_layer_shell::set_keyboard_mode(&window, gtk_layer_shell::KeyboardMode::Exclusive);

    // Try to load custom CSS, if it fails load the default CSS
    let provider = gtk::CssProvider::new();
    if let Err(why) = provider.load_from_path(&format!("{}/style.css", config_dir)) {
        println!("Failed to load custom CSS: {}", why);
        provider
            .load_from_data(include_bytes!("../res/style.css"))
            .unwrap();
    }
    gtk::StyleContext::add_provider_for_screen(
        &gdk::Screen::default().expect("Failed to get GDK screen for CSS provider!"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
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
        println!("At least one plugin needs to be enabled!");
        app.quit();
    }

    // Create the main list of plugin views
    let main_list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .name(style_names::MAIN)
        .build();

    // Load plugins from the paths specified in the config file
    let plugins = Rc::new(
        plugins
            .iter()
            .map(|plugin_path| {
                // Load the plugin's dynamic library.
                let plugin = abi_stable::library::lib_header_from_path(
                    if plugin_path.is_absolute() {
                        plugin_path.clone()
                    } else {
                        let mut path = PathBuf::from(&format!("{}/plugins", config_dir));
                        path.extend(plugin_path.iter());
                        path
                    }
                    .as_path(),
                )
                .and_then(|plugin| plugin.init_root_module::<PluginRef>())
                .unwrap();

                // Run the plugin's init code to init static resources etc.
                plugin.init()(config_dir.clone().into());

                let plugin_box = gtk::Box::builder()
                    .orientation(gtk::Orientation::Horizontal)
                    .spacing(10)
                    .name(style_names::PLUGIN)
                    .build();
                plugin_box.add(&create_info_box(&plugin.info()()));
                plugin_box.add(
                    &gtk::Separator::builder()
                        .orientation(gtk::Orientation::Horizontal)
                        .name(style_names::PLUGIN)
                        .build(),
                );
                let list = gtk::ListBox::builder()
                    .name(style_names::PLUGIN)
                    .hexpand(true)
                    .build();

                plugin_box.add(&list);

                let row = gtk::ListBoxRow::builder().name(style_names::PLUGIN).build();
                row.add(&plugin_box);

                main_list.add(&row);

                PluginView { plugin, row, list }
            })
            .collect::<Vec<PluginView>>(),
    );

    // Connect selection events to avoid completely messing up selection logic
    for plugin_view in plugins.iter() {
        let plugins_clone = plugins.clone();
        plugin_view.list.connect_row_selected(move |list, row| {
            if row.is_some() {
                let combined_matches = plugins_clone
                    .iter()
                    .flat_map(|view| {
                        view.list.children().into_iter().map(|child| {
                            (
                                child.dynamic_cast::<gtk::ListBoxRow>().unwrap(),
                                view.list.clone(),
                            )
                        })
                    })
                    .collect::<Vec<(gtk::ListBoxRow, gtk::ListBox)>>();

                // Unselect everything except the new selection
                for (_, _list) in combined_matches {
                    if _list != *list {
                        _list.select_row(None::<&gtk::ListBoxRow>);
                    }
                }
            }
        });
    }

    // Text entry box
    let entry = gtk::Entry::builder()
        .hexpand(true)
        .has_focus(true)
        .name(style_names::ENTRY)
        .build();

    // Refresh the matches when text input changes
    let plugins_clone = plugins.clone();
    entry.connect_changed(move |entry| {
        refresh_matches(entry.text().to_string(), plugins_clone.clone())
    });

    // Handle other key presses for selection control and all other things that may be needed
    let entry_clone = entry.clone();
    window.connect_key_press_event(move |window, event| {
        use gdk::keys::constants;
        match event.keyval() {
            // Close window on escape
            constants::Escape => {
                window.close();
                Inhibit(true)
            }
            // Handle selections
            constants::Down | constants::Tab | constants::Up => {
                // Combine all of the matches into a `Vec` to allow for easier handling of the selection
                let combined_matches = plugins
                    .iter()
                    .flat_map(|view| {
                        view.list.children().into_iter().map(|child| {
                            (
                                // All children of lists are GtkListBoxRow widgets
                                child.dynamic_cast::<gtk::ListBoxRow>().unwrap(),
                                view.list.clone(),
                            )
                        })
                    })
                    .collect::<Vec<(gtk::ListBoxRow, gtk::ListBox)>>();

                // Get the selected match
                let (selected_match, selected_list) = match plugins
                    .iter()
                    .find_map(|view| view.list.selected_row().map(|row| (row, view.list.clone())))
                {
                    Some(selected) => selected,
                    None => {
                        // If nothing is selected select either the top or bottom match based on the input
                        match event.keyval() {
                            constants::Down | constants::Tab => combined_matches[0]
                                .1
                                .select_row(Some(&combined_matches[0].0)),
                            constants::Up => combined_matches[combined_matches.len() - 1]
                                .1
                                .select_row(Some(&combined_matches[combined_matches.len() - 1].0)),
                            _ => unreachable!(),
                        }
                        return Inhibit(true);
                    }
                };

                // Clear the previous selection
                selected_list.select_row(None::<&gtk::ListBoxRow>);

                // Get the index of the current selection
                let index = combined_matches
                    .iter()
                    .position(|(row, _)| *row == selected_match)
                    .unwrap();

                // Move the selection based on the input, loops from top to bottom and vice versa
                match event.keyval() {
                    constants::Down | constants::Tab => {
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
                    constants::Up => {
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
            constants::Return => {
                let (selected_match, plugin) = match plugins
                    .iter()
                    .find_map(|view| view.list.selected_row().map(|row| (row, view.plugin)))
                {
                    Some(selected) => selected,
                    None => {
                        return Inhibit(false);
                    }
                };

                // Perform actions based on the result of handling the selection
                match plugin.handle_selection()(unsafe {
                    (*selected_match.data::<Match>("match").unwrap().as_ptr()).clone()
                }) {
                    HandleResult::Close => {
                        window.close();
                        Inhibit(true)
                    }
                    HandleResult::Refresh => {
                        refresh_matches(entry_clone.text().to_string(), plugins.clone());
                        Inhibit(false)
                    }
                    HandleResult::Copy(bytes) => {
                        runtime_data.borrow_mut().as_mut().unwrap().post_run_action =
                            PostRunAction::Copy(bytes.into());
                        window.close();
                        Inhibit(true)
                    }
                }
            }
            _ => Inhibit(false),
        }
    });

    let main_vbox = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .name(style_names::MAIN)
        .build();
    main_vbox.add(&entry);
    window.add(&main_vbox);
    window.show_all();
    // Add and show the list later, to avoid showing empty plugin categories on launch
    main_vbox.add(&main_list);
    main_list.show();
}

fn handle_matches(plugin_view: PluginView, plugins: Rc<Vec<PluginView>>, matches: RVec<Match>) {
    // Clear out the old matches from the list
    for widget in plugin_view.list.children() {
        plugin_view.list.remove(&widget);
    }

    // If there are no matches, hide the plugin's results
    if matches.is_empty() {
        plugin_view.row.hide();
        return;
    }

    for _match in matches {
        let hbox = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .name(style_names::MATCH)
            .hexpand(true)
            .build();
        if let ROption::RSome(icon) = &_match.icon {
            let mut builder = gtk::Image::builder()
                .name(style_names::MATCH)
                .pixel_size(32);

            let path = PathBuf::from(icon.as_str());

            // If the icon path is absolute, load that file
            if path.is_absolute() {
                match gdk_pixbuf::Pixbuf::from_file_at_size(icon.as_str(), 32, 32) {
                    Ok(pixbuf) => builder = builder.pixbuf(&pixbuf),
                    Err(why) => {
                        println!("Failed to load icon file: {}", why);
                        builder = builder.icon_name("image-missing"); // Set "broken" icon
                    }
                }
            } else {
                builder = builder.icon_name(icon);
            }

            hbox.add(&builder.build());
        }
        let title = gtk::Label::builder()
            .name(style_names::MATCH_TITLE)
            .wrap(true)
            .use_markup(true) // Allow pango markup
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Center)
            .vexpand(true)
            .label(&_match.title)
            .build();

        // If a description is present, make a box with it and the title
        match &_match.description {
            ROption::RSome(desc) => {
                let title_desc_box = gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .name(style_names::MATCH)
                    .hexpand(true)
                    .vexpand(true)
                    .build();
                title_desc_box.add(&title);
                title_desc_box.add(
                    &gtk::Label::builder()
                        .name(style_names::MATCH_DESC)
                        .wrap(true)
                        .use_markup(true) // Allow pango markup
                        .halign(gtk::Align::Start)
                        .valign(gtk::Align::Center)
                        .label(desc)
                        .build(),
                );
                hbox.add(&title_desc_box);
            }
            ROption::RNone => {
                hbox.add(&title);
            }
        }
        let row = gtk::ListBoxRow::builder()
            .name(style_names::MATCH)
            .height_request(32)
            .build();
        row.add(&hbox);
        // GTK data setting is not type checked, so it is unsafe.
        // Only `Match` objects are stored though.
        unsafe {
            row.set_data("match", _match);
        }
        plugin_view.list.add(&row);
    }

    // Refresh the items in the view
    plugin_view.row.show_all();

    let combined_matches = plugins
        .iter()
        .flat_map(|view| {
            view.list.children().into_iter().map(|child| {
                (
                    child.dynamic_cast::<gtk::ListBoxRow>().unwrap(),
                    view.list.clone(),
                )
            })
        })
        .collect::<Vec<(gtk::ListBoxRow, gtk::ListBox)>>();

    if let Some((row, list)) = combined_matches.get(0) {
        list.select_row(Some(row));
    }
}

/// Create the info box for the plugin
fn create_info_box(info: &PluginInfo) -> gtk::Box {
    let info_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .name(style_names::PLUGIN)
        .width_request(200)
        .height_request(32)
        .expand(false)
        .spacing(10)
        .build();
    info_box.add(
        &gtk::Image::builder()
            .icon_name(&info.icon)
            .name(style_names::PLUGIN)
            .pixel_size(32)
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Start)
            .build(),
    );
    info_box.add(
        &gtk::Label::builder()
            .label(&info.name)
            .name(style_names::PLUGIN)
            .halign(gtk::Align::End)
            .valign(gtk::Align::Center)
            .hexpand(true)
            .build(),
    );
    // This is so that we can align the plugin name with the icon. GTK would not let it be properly aligned otherwise.
    let main_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .name(style_names::PLUGIN)
        .build();
    main_box.add(&info_box);
    main_box.add(
        &gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .name(style_names::PLUGIN)
            .build(),
    );
    main_box
}

/// Refresh the matches from the plugins
fn refresh_matches(input: String, plugins: Rc<Vec<PluginView>>) {
    for plugin_view in plugins.iter() {
        let id = plugin_view.plugin.get_matches()(input.clone().into());
        let plugin_view = plugin_view.clone();
        let plugins = plugins.clone();
        // If the input is empty, skip getting matches and just clear everything out.
        if input.is_empty() {
            handle_matches(plugin_view, plugins, RVec::new());
        } else {
            glib::timeout_add_local(Duration::from_micros(1000), move || {
                async_match(plugin_view.clone(), plugins.clone(), id)
            });
        }
    }
}

/// Handle the asynchronously running match task
fn async_match(plugin_view: PluginView, plugins: Rc<Vec<PluginView>>, id: u64) -> glib::Continue {
    match plugin_view.plugin.poll_matches()(id) {
        PollResult::Ready(matches) => {
            handle_matches(plugin_view, plugins, matches);
            glib::Continue(false)
        }
        PollResult::Pending => glib::Continue(true),
        PollResult::Cancelled => glib::Continue(false),
    }
}
