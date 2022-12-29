use std::{
    cell::RefCell,
    env, fs,
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

use abi_stable::std_types::{ROption, RVec};
use gtk::{gdk, glib, prelude::*};
use serde::Deserialize;
use anyrun_interface::{HandleResult, Match, PluginInfo, PluginRef, PollResult};

#[derive(Deserialize)]
struct Config {
    width: u32,
    plugins: Vec<PathBuf>,
}

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

mod style_names {
    pub const ENTRY: &str = "entry";
    pub const MAIN: &str = "main";
    pub const WINDOW: &str = "window";
    pub const PLUGIN: &str = "plugin";
    pub const MATCH: &str = "match";

    pub const MATCH_TITLE: &str = "match-title";
    pub const MATCH_DESC: &str = "match-desc";
    pub const TITLE_DESC_BOX: &str = "title-desc-box";
}

fn main() {
    let app = gtk::Application::new(Some("com.kirottu.anyrun"), Default::default());
    let args: Rc<RefCell<Option<Args>>> = Rc::new(RefCell::new(None));

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

    let args_clone = args.clone();
    app.connect_handle_local_options(move |_app, dict| {
        let override_plugins = dict.lookup::<Vec<String>>("override-plugins").unwrap();
        let config_dir = dict.lookup::<String>("config-dir").unwrap();

        *args_clone.borrow_mut() = Some(Args {
            override_plugins,
            config_dir,
        });
        -1 // Magic GTK number to continue running
    });

    let args_clone = args.clone();
    app.connect_activate(move |app| activate(app, args_clone.clone()));

    app.run();
}

fn activate(app: &gtk::Application, args: Rc<RefCell<Option<Args>>>) {
    // Figure out the config dir
    let config_dir = args
        .borrow()
        .as_ref()
        .unwrap()
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
    let plugins = match &args.borrow().as_ref().unwrap().override_plugins {
        Some(plugins) => plugins.iter().map(|path| PathBuf::from(path)).collect(),
        None => config.plugins,
    };

    // Make sure at least one plugin is specified
    if plugins.len() == 0 {
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
            if let Some(_) = row {
                let combined_matches = plugins_clone
                    .iter()
                    .map(|view| {
                        view.list.children().into_iter().map(|child| {
                            (
                                child.dynamic_cast::<gtk::ListBoxRow>().unwrap(),
                                view.list.clone(),
                            )
                        })
                    })
                    .flatten()
                    .collect::<Vec<(gtk::ListBoxRow, gtk::ListBox)>>();

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
    let plugins_clone = plugins.clone();
    window.connect_key_press_event(move |window, event| {
        use gdk::keys::constants;
        match event.keyval() {
            constants::Escape => {
                window.close();
                Inhibit(true)
            }
            constants::Down | constants::Tab | constants::Up => {
                let combined_matches = plugins_clone
                    .iter()
                    .map(|view| {
                        view.list.children().into_iter().map(|child| {
                            (
                                child.dynamic_cast::<gtk::ListBoxRow>().unwrap(),
                                view.list.clone(),
                            )
                        })
                    })
                    .flatten()
                    .collect::<Vec<(gtk::ListBoxRow, gtk::ListBox)>>();

                let (selected_match, selected_list) = match plugins_clone
                    .iter()
                    .find_map(|view| view.list.selected_row().map(|row| (row, view.list.clone())))
                {
                    Some(selected) => selected,
                    None => {
                        if event.keyval() != constants::Up {
                            combined_matches[0]
                                .1
                                .select_row(Some(&combined_matches[0].0));
                        }
                        return Inhibit(true);
                    }
                };

                selected_list.select_row(None::<&gtk::ListBoxRow>);

                let index = combined_matches
                    .iter()
                    .position(|(row, _)| *row == selected_match)
                    .unwrap();

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
            constants::Return => {
                let (selected_match, plugin) = match plugins_clone.iter().find_map(|view| {
                    view.list
                        .selected_row()
                        .map(|row| (row, view.plugin.clone()))
                }) {
                    Some(selected) => selected,
                    None => {
                        return Inhibit(false);
                    }
                };

                match plugin.handle_selection()(unsafe {
                    (*selected_match.data::<Match>("match").unwrap().as_ptr()).clone()
                }) {
                    HandleResult::Close => {
                        window.close();
                        Inhibit(true)
                    }
                    HandleResult::Refresh => {
                        refresh_matches(entry_clone.text().to_string(), plugins_clone.clone());
                        Inhibit(false)
                    }
                }
            }
            _ => Inhibit(false),
        }
    });

    let main_vbox = gtk::Box::new(gtk::Orientation::Vertical, 10);
    main_vbox.add(&entry);
    window.add(&main_vbox);
    window.show_all();
    // Add and show the list later, to avoid showing empty plugin categories on launch
    main_vbox.add(&main_list);
    main_list.show();
}

fn handle_matches(matches: RVec<Match>, plugin_view: PluginView) {
    for widget in plugin_view.list.children() {
        plugin_view.list.remove(&widget);
    }

    if matches.len() == 0 {
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
        hbox.add(
            &gtk::Image::builder()
                .icon_name(&_match.icon)
                .name(style_names::MATCH)
                .pixel_size(32)
                .build(),
        );
        let title = gtk::Label::builder()
            .name(style_names::MATCH_TITLE)
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Center)
            .label(&_match.title)
            .build();

        // If a description is present, make a box with it and the title
        match &_match.description {
            ROption::RSome(desc) => {
                let title_desc_box = gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .name(style_names::TITLE_DESC_BOX)
                    .hexpand(true)
                    .vexpand(true)
                    .build();
                title_desc_box.add(&title);
                title_desc_box.add(
                    &gtk::Label::builder()
                        .name(style_names::MATCH_DESC)
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
        let row = gtk::ListBoxRow::builder().name(style_names::MATCH).build();
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
}

fn create_info_box(info: &PluginInfo) -> gtk::Box {
    let info_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .name(style_names::PLUGIN)
        .width_request(200)
        .expand(false)
        .spacing(10)
        .build();
    info_box.add(
        &gtk::Image::builder()
            .icon_name(&info.icon)
            .name(style_names::PLUGIN)
            .pixel_size(48)
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Start)
            .build(),
    );
    info_box.add(
        &gtk::Label::builder()
            .label(&info.name)
            .name(style_names::PLUGIN)
            .halign(gtk::Align::End)
            .valign(gtk::Align::Start)
            .hexpand(true)
            .build(),
    );
    info_box
}

/// Refresh the matches from the plugins
fn refresh_matches(input: String, plugins: Rc<Vec<PluginView>>) {
    for plugin_view in plugins.iter() {
        let id = plugin_view.plugin.get_matches()(input.clone().into());
        let plugin_view = plugin_view.clone();
        glib::timeout_add_local(Duration::from_micros(1000), move || {
            async_match(plugin_view.clone(), id)
        });
    }
}

/// Handle the asynchronously running match task
fn async_match(plugin_view: PluginView, id: u64) -> glib::Continue {
    match plugin_view.plugin.poll_matches()(id) {
        PollResult::Ready(matches) => {
            handle_matches(matches, plugin_view);
            glib::Continue(false)
        }
        PollResult::Pending => glib::Continue(true),
        PollResult::Cancelled => glib::Continue(false),
    }
}
