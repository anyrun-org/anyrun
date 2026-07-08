// anyrun-provider/src/lib.rs
pub mod engine;

use std::path::PathBuf;
use std::sync::Arc;

use anyrun_provider_ipc::{PLUGIN_PATHS, Request, Response};
use engine::{FrecencyData, State, load_plugins, rebuild_plugin_map, spawn_file_watcher, worker};
use tokio::net::UnixStream;
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
    mut request_rx: mpsc::Receiver<Request>,
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

    // Bridge via in-process socket pair
    let (engine_stream, client_stream) =
        UnixStream::pair().expect("[provider] failed to create socket pair");

    // Spawn relay to forward requests to client_stream and read responses back to response_tx
    tokio::spawn(async move {
        let mut socket = anyrun_provider_ipc::Socket::new(client_stream);
        loop {
            tokio::select! {
                req = request_rx.recv() => {
                    let Some(req) = req else { break; };
                    let is_quit = matches!(req, Request::Quit);
                    if socket.send(&req).await.is_err() { break; }
                    if is_quit { break; }
                }
                res = socket.recv::<Response>() => {
                    match res {
                        Ok(resp) => {
                            if response_tx.send(resp).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        }
    });

    if let Err(e) = worker(engine_stream, &mut state, reload_rx).await {
        eprintln!("[provider] engine worker error: {e}");
    }
}
