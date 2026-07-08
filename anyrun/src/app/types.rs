use crate::{
    config::{Action, Config},
    plugin_box::PluginMatch,
    Args,
};
use gtk::{gdk, gio};
use gtk4 as gtk;
use relm4::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct App {
    pub(super) config: Arc<Config>,
    pub(super) invocation: Option<SendInvocation>,
    pub(super) matches: FactoryVecDeque<PluginMatch>,
    pub(super) plugin_names: Vec<String>,
    pub(super) plugin_info_map: HashMap<String, anyrun_interface::PluginInfo>,
    pub(super) post_run_action: PostRunAction,
    pub(super) tx: mpsc::Sender<anyrun_provider_ipc::Request>,
    pub(super) css_provider: gtk::CssProvider,
    pub(super) selected_index: usize,
    pub(super) config_dir: Option<String>,
    pub(super) is_daemon: bool,
    pub(super) search_cancellable: Option<gio::Cancellable>,
    pub(super) last_css_load: Option<std::time::SystemTime>,
    pub(super) last_entry_change: Option<std::time::Instant>,
    pub(super) search_epoch: u64,
    pub(super) settle_animation_epoch: Option<u64>,
    pub(super) settled_once: bool,
    pub(super) settle_query_sent: bool,
    pub(super) current_input: String,
    pub(super) pending_matches:
        HashMap<String, abi_stable::std_types::RVec<anyrun_interface::Match>>,
    pub(super) pending_flush_scheduled: bool,
    pub(super) pending_settle_epoch: Option<u64>,
    pub(super) settling_plugins_sent: HashSet<String>,
    pub(super) batch_flushing_results: bool,
    pub(super) matches_entered: bool,
    pub(super) last_scroll_top: f64,
    pub(super) last_visible_count: usize,
    pub(super) last_flush_hash: u64,
    pub(super) show_shortcuts: bool,
    pub(super) plugin_health: HashMap<String, anyrun_provider_ipc::PluginHealth>,
    pub(super) last_css_check: Option<std::time::Instant>,
}

#[derive(Debug, Clone)]
pub struct SendInvocation(pub gio::DBusMethodInvocation);
unsafe impl Send for SendInvocation {}
unsafe impl Sync for SendInvocation {}

pub const DEFAULT_CSS: &str = include_str!("../../res/style.css");

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
    EntryActivated,
    EntryChanged(String),
    Activate(Option<SendInvocation>),
    SyncShortcuts,
    ReloadPlugins,
    RowSelected(usize),
    FlushPendingMatches(u64),
    TriggerSettledQuery(u64, String),
    ShowShortcuts(bool),
}

#[derive(Deserialize, Serialize, Clone)]
pub struct AppInit {
    pub args: Args,
    pub stdin: Vec<u8>,
    pub env: Vec<(String, String)>,
}

pub struct DaemonContext {
    pub config: Arc<Config>,
    pub config_dir: Option<String>,
    pub css_provider: gtk::CssProvider,
}
