// anyrun-provider/src/lib.rs
pub mod engine;

use std::path::PathBuf;
use std::sync::Arc;

use anyrun_provider_ipc::{PLUGIN_PATHS, Request, Response};
use engine::{
    FrecencyData, State, load_plugins, rebuild_plugin_map, spawn_file_watcher, worker_inner,
};
use tokio::sync::{broadcast, mpsc};

pub struct ProviderConfig {
    pub config_dir: String,
    pub plugin_specs: Vec<PathBuf>,
}

pub struct ProviderHandle {
    pub request_tx: mpsc::Sender<Request>,
}

/// Spawns the provider engine on a dedicated std::thread containing a tokio runtime.
/// Responses are forwarded via `response_tx` (a tokio UnboundedSender<Response>).
pub fn spawn_provider_thread(
    config: ProviderConfig,
    response_tx: mpsc::UnboundedSender<Response>,
) -> ProviderHandle {
    let (request_tx, request_rx) = mpsc::channel::<Request>(64);

    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("[provider] failed to build tokio runtime")
            .block_on(async move {
                run_engine(config, request_rx, response_tx).await;
            });
    });

    ProviderHandle { request_tx }
}

async fn run_engine(
    config: ProviderConfig,
    request_rx: mpsc::Receiver<Request>,
    response_tx: mpsc::UnboundedSender<Response>,
) {
    use std::env;

    let config_dir: Arc<str> = config.config_dir.clone().into();

    let user_dir = env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let mut p = PathBuf::from(env::var("HOME").unwrap_or_else(|_| ".".into()));
            p.push(".config");
            p
        })
        .join("anyrun");

    let mut plugin_dirs = vec![user_dir.join("plugins")];
    if let Ok(path) = env::var("ANYRUN_PLUGINS") {
        plugin_dirs.push(PathBuf::from(path));
    }
    plugin_dirs.extend(PLUGIN_PATHS.iter().map(PathBuf::from));

    let mut frecency = FrecencyData::load(&config_dir);
    frecency.cleanup();
    let frecency = Arc::new(tokio::sync::Mutex::new(frecency));

    let (initial_plugins, initial_temp_files) =
        match load_plugins(&config.plugin_specs, &plugin_dirs, &config_dir, false) {
            Ok(result) => result,
            Err(err) => {
                eprintln!("[provider] Failed to load plugins: {err}");
                return;
            }
        };

    let mut state = State {
        plugin_map: rebuild_plugin_map(&initial_plugins),
        plugins: initial_plugins,
        plugin_dirs: plugin_dirs.clone(),
        config_dir: config_dir.clone(),
        frecency,
        plugin_specs: config.plugin_specs.clone(),
        temp_so_files: initial_temp_files,
    };

    let (reload_tx, reload_rx) = broadcast::channel::<()>(4);
    spawn_file_watcher(reload_tx.clone(), &plugin_dirs, &config_dir);

    if let Err(e) = worker_inner(request_rx, response_tx, &mut state, reload_rx).await {
        eprintln!("[provider] engine worker error: {e}");
    }
}
