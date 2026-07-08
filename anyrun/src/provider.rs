use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyrun_provider::{spawn_provider_thread, ProviderConfig};
use anyrun_provider_ipc as ipc;
use relm4::Sender;
use tokio::sync::mpsc::Receiver;

use crate::config::Config;

fn expand_tilde(path: &Path) -> PathBuf {
    if let Some(path_str) = path.to_str() {
        if path_str.starts_with("~/") {
            if let Ok(home) = std::env::var("HOME") {
                return PathBuf::from(path_str.replacen('~', &home, 1));
            }
        }
    }
    path.to_path_buf()
}



pub fn worker_inproc(
    config: Arc<Config>,
    config_dir: Option<String>,
    mut rx: Receiver<anyrun_provider_ipc::Request>,
    sender: Sender<anyrun_provider_ipc::Response>,
) -> io::Result<()> {
    let config_dir_str = config_dir.unwrap_or_else(|| ipc::CONFIG_DIRS[0].to_string());

    let plugin_specs: Vec<PathBuf> = config
        .plugins
        .iter()
        .map(|p| {
            let p = expand_tilde(p);
            if p.is_relative() && p.components().count() > 1 {
                std::env::current_dir()
                    .ok()
                    .map(|cwd| cwd.join(&p))
                    .unwrap_or(p)
            } else {
                p
            }
        })
        .collect();

    let provider_config = ProviderConfig {
        config_dir: config_dir_str,
        plugin_specs,
    };

    let (resp_tx, mut resp_rx) =
        tokio::sync::mpsc::unbounded_channel::<anyrun_provider_ipc::Response>();
    let handle = spawn_provider_thread(provider_config, resp_tx);

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async move {
            let sender_clone = sender.clone();
            tokio::spawn(async move {
                while let Some(resp) = resp_rx.recv().await {
                    sender_clone.emit(resp);
                }
            });

            while let Some(req) = rx.recv().await {
                let is_quit = matches!(req, ipc::Request::Quit);
                if handle.request_tx.send(req).await.is_err() {
                    break;
                }
                if is_quit {
                    break;
                }
            }
        });

    Ok(())
}

