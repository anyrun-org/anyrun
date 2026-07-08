use adw::prelude::*;
use anyrun_provider_ipc as ipc;
use gtk::glib;
use gtk4 as gtk;
use gtk4::prelude::Cast;
use libadwaita as adw;
use relm4::prelude::*;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::mpsc;

use super::{App, AppInit, DaemonContext, PostRunAction, SendInvocation, DEFAULT_CSS};
use crate::{config::Config, provider};

impl App {
    pub(super) fn init_model(
        (app_init, invocation, daemon_context): (
            AppInit,
            Option<SendInvocation>,
            Option<Rc<DaemonContext>>,
        ),
        root: &gtk::Window,
        sender: relm4::ComponentSender<Self>,
    ) -> (
        Self,
        Arc<Config>,
        Option<String>,
        gtk::CssProvider,
        gtk::ListBox,
    ) {
        let (config, config_dir, css_provider) = if let Some(ctx) = daemon_context.as_ref() {
            gtk::style_context_add_provider_for_display(
                &root.upcast_ref::<gtk::Widget>().display(),
                &ctx.css_provider,
                gtk::STYLE_PROVIDER_PRIORITY_USER,
            );
            (
                ctx.config.clone(),
                ctx.config_dir.clone(),
                ctx.css_provider.clone(),
            )
        } else {
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

            let css_provider = gtk::CssProvider::new();

            let mut config = if let Some(config_dir) = &config_dir {
                match fs::read_to_string(format!("{config_dir}/style.css")) {
                    Ok(style) => {
                        css_provider.load_from_string(&style);
                    }
                    Err(why) => {
                        eprintln!("[anyrun] Failed to load CSS: {why}");
                        css_provider.load_from_string(DEFAULT_CSS);
                    }
                }
                match fs::read(format!("{config_dir}/config.ron")) {
                    Ok(content) => ron::de::from_bytes(&content).unwrap_or_else(|why| {
                        eprintln!(
                            "[anyrun] Failed to parse config file, using default values: {why}"
                        );
                        Config::default()
                    }),
                    Err(why) => {
                        eprintln!(
                            "[anyrun] Failed to read config file, using default values: {why}"
                        );
                        Config::default()
                    }
                }
            } else {
                eprintln!("[anyrun] No config found in any searched paths");
                css_provider.load_from_string(DEFAULT_CSS);
                Config::default()
            };

            gtk::style_context_add_provider_for_display(
                &root.upcast_ref::<gtk::Widget>().display(),
                &css_provider,
                gtk::STYLE_PROVIDER_PRIORITY_USER,
            );

            config.merge_opt(app_init.args.config.clone());

            (Arc::new(config), config_dir, css_provider)
        };

        let matches_list = gtk::ListBox::builder()
            .css_classes(["matches"])
            .hexpand(true)
            .build();

        let matches_factory = FactoryVecDeque::<crate::plugin_box::PluginMatch>::builder()
            .launch(matches_list.clone())
            .detach();

        let (tx, rx) = mpsc::channel(64);

        sender.spawn_command(glib::clone!(
            #[strong]
            config,
            #[strong]
            config_dir,
            move |sender| {
                if let Err(why) = provider::worker_inproc(config, config_dir, rx, sender) {
                    eprintln!("[anyrun] provider worker error: {why}");
                }
            }
        ));

        let model = App {
            invocation,
            config: config.clone(),
            matches: matches_factory,
            plugin_names: Vec::new(),
            plugin_info_map: std::collections::HashMap::new(),
            post_run_action: PostRunAction::None,
            tx,
            css_provider: css_provider.clone(),
            config_dir: config_dir.clone(),
            selected_index: 0,
            is_daemon: daemon_context.is_some(),
            search_cancellable: None,
            last_css_load: Some(std::time::SystemTime::now()),
            last_entry_change: None,
            search_epoch: 0,
            settle_animation_epoch: None,
            settled_once: false,
            settle_query_sent: false,
            current_input: String::new(),
            pending_matches: std::collections::HashMap::new(),
            pending_flush_scheduled: false,
            pending_settle_epoch: None,
            settling_plugins_sent: std::collections::HashSet::new(),
            batch_flushing_results: false,
            matches_entered: false,
            last_scroll_top: 0.0,
            last_visible_count: 0,
            last_flush_hash: 1,
            show_shortcuts: false,
            plugin_health: std::collections::HashMap::new(),
            last_css_check: None,
        };

        (model, config, config_dir, css_provider, matches_list)
    }
}
