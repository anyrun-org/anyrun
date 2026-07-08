use crate::app;
use crate::args::Args;
use crate::dbus::{setup_dbus, DaemonState};
use gtk4::{gio, prelude::*};
use relm4::ComponentController;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

pub fn run_daemon(args: Args) {
    let app = gtk4::Application::new(Some("org.anyrun.anyrun"), gio::ApplicationFlags::IS_SERVICE);
    app.register(None::<&gio::Cancellable>)
        .expect("Failed to register daemon");

    let user_dir = std::env::var("XDG_CONFIG_HOME")
        .map(|c| format!("{c}/anyrun"))
        .or_else(|_| std::env::var("HOME").map(|h| format!("{h}/.config/anyrun")))
        .unwrap();
    let config_dir = args.config_dir.clone().or_else(|| {
        if std::path::PathBuf::from(&user_dir).exists() {
            Some(user_dir.clone())
        } else {
            anyrun_provider_ipc::CONFIG_DIRS
                .iter()
                .map(|path| path.to_string())
                .find(|path| std::path::PathBuf::from(path).exists())
        }
    });

    let css_provider = gtk4::CssProvider::new();
    let mut config = if let Some(config_dir) = &config_dir {
        match std::fs::read_to_string(format!("{config_dir}/style.css")) {
            Ok(style) => {
                css_provider.load_from_string(&style);
            }
            Err(why) => {
                eprintln!("[anyrun] Failed to load CSS: {why}");
                css_provider.load_from_string(app::DEFAULT_CSS);
            }
        }
        match std::fs::read(format!("{config_dir}/config.ron")) {
            Ok(content) => ron::de::from_bytes(&content).unwrap_or_else(|why| {
                eprintln!("[anyrun] Failed to parse config file, using default values: {why}");
                crate::config::Config::default()
            }),
            Err(why) => {
                eprintln!("[anyrun] Failed to read config file, using default values: {why}");
                crate::config::Config::default()
            }
        }
    } else {
        css_provider.load_from_string(app::DEFAULT_CSS);
        crate::config::Config::default()
    };
    config.merge_opt(args.config.clone());

    let context = Rc::new(app::DaemonContext {
        config: Arc::new(config),
        config_dir,
        css_provider,
    });

    // Launch the persistent UI component
    let app_init = app::AppInit {
        args: args.clone(),
        stdin: Vec::new(),
        env: std::env::vars()
            .filter(|(k, _)| {
                k == "HOME"
                    || k.starts_with("XDG_")
                    || k == "PATH"
                    || k == "DISPLAY"
                    || k == "WAYLAND_DISPLAY"
                    || k.starts_with("ANYRUN_")
                    || k == "LANG"
                    || k == "TERM"
            })
            .collect(),
    };

    let controller = app::App::launch(&app, app_init, None, Some(context));
    let _hold = app.hold();
    let state = Rc::new(RefCell::new(DaemonState {
        sender: controller.sender().clone(),
        provider_child: None,
    }));

    setup_dbus(&app, state);

    app.run_with_args(&Vec::<String>::new());
}
