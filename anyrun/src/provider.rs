use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

use anyrun_provider::{spawn_provider_thread, ProviderConfig};
use anyrun_provider_ipc as ipc;
use relm4::Sender;
use tokio::{net::UnixListener, sync::mpsc::Receiver};

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

pub(crate) fn build_provider_command(
    provider_path: &Path,
    config_dir: &str,
    plugins: &[PathBuf],
    socket_path: &str,
    env: Vec<(String, String)>,
) -> Command {
    let cwd = std::env::current_dir().ok();
    let resolved_plugins: Vec<PathBuf> = plugins
        .iter()
        .map(|p| {
            let p = expand_tilde(p);
            if p.is_relative() {
                cwd.as_ref()
                    .map(|cwd| cwd.join(&p))
                    .unwrap_or_else(|| p.clone())
            } else {
                p
            }
        })
        .collect();

    let mut cmd = Command::new(expand_tilde(provider_path));
    cmd.stdin(Stdio::piped())
        .arg("--config-dir")
        .arg(config_dir)
        .args(
            resolved_plugins
                .iter()
                .flat_map(|plugin| [PathBuf::from("-p"), plugin.to_owned()]),
        )
        .arg("connect-to")
        .arg(socket_path)
        .envs(env);
    cmd
}

pub fn worker_spawn(
    config: Arc<Config>,
    config_dir: Option<String>,
    mut rx: Receiver<anyrun_provider_ipc::Request>,
    sender: Sender<anyrun_provider_ipc::Response>,
    // The stdin received by the launching command
    stdin: Vec<u8>,
    // The environment of the launching command
    env: Vec<(String, String)>,
) -> io::Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let socket_path = format!(
                "{}/anyrun.sock",
                env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".to_string())
            );
            // Make sure that it does not exist already
            let _ = fs::remove_file(&socket_path);
            let listener = UnixListener::bind(&socket_path).unwrap();

            let config_dir = config_dir.unwrap_or_else(|| ipc::CONFIG_DIRS[0].to_string());
            let mut child = match build_provider_command(
                &config.provider,
                &config_dir,
                &config.plugins,
                &socket_path,
                env,
            )
            .spawn()
            {
                Ok(child) => child,
                Err(why) => match why.kind() {
                    io::ErrorKind::NotFound => {
                        eprintln!("[anyrun] `{}` Not found, make sure `anyrun-provider` is installed and available in $PATH, \
                             or configure an alternative path via the `provider` config option.", config.provider.display());
                        return Ok(());
                    }
                    _ => return Err(why),
                },
            };

            if let Some(mut child_stdin) = child.stdin.take() {
                child_stdin.write_all(&stdin).unwrap();
            };

            let (stream, _) = listener.accept().await?;
            let mut socket = ipc::Socket::new(stream);

            relay_loop(&mut socket, &mut rx, &sender).await?;

            // Remove it after we are done with it
            let _ = fs::remove_file(&socket_path);
            // Make sure it exits properly and doesn't leave a zombie process
            let _ = child.wait();

            Ok(())
        }
    )
}

pub fn worker_connect(
    socket_path: PathBuf,
    mut rx: Receiver<anyrun_provider_ipc::Request>,
    sender: Sender<anyrun_provider_ipc::Response>,
) -> io::Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let stream = connect_with_retry(
                &socket_path,
                Duration::from_secs(12),
                Duration::from_millis(100),
            )
            .await?;
            let mut socket = ipc::Socket::new(stream);
            relay_loop(&mut socket, &mut rx, &sender).await
        })
}

async fn connect_with_retry(
    socket_path: &Path,
    timeout: Duration,
    interval: Duration,
) -> io::Result<tokio::net::UnixStream> {
    let started = Instant::now();
    let mut last_error = None;

    while started.elapsed() < timeout {
        match tokio::net::UnixStream::connect(socket_path).await {
            Ok(stream) => return Ok(stream),
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                ) =>
            {
                last_error = Some(err);
                tokio::time::sleep(interval).await;
            }
            Err(err) => return Err(err),
        }
    }

    let cause = last_error
        .map(|e| e.to_string())
        .unwrap_or_else(|| "timeout".to_string());
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        format!(
            "failed to connect to IPC socket `{}` within {:?}: {cause}",
            socket_path.display(),
            timeout
        ),
    ))
}

async fn relay_loop(
    socket: &mut ipc::Socket,
    rx: &mut Receiver<anyrun_provider_ipc::Request>,
    sender: &Sender<anyrun_provider_ipc::Response>,
) -> io::Result<()> {
    loop {
        tokio::select! {
            req = rx.recv() => {
                let Some(req) = req else {
                    break;
                };
                socket.send(&req).await?;
                if matches!(req, ipc::Request::Quit) {
                    break;
                }
            }
            res = socket.recv() => {
                match res {
                    Ok(response) => sender.emit(response),
                    Err(why) => {
                        if !is_expected_ipc_disconnect(&why) {
                            eprintln!("[anyrun] Error reading from IPC: {why}");
                        }
                        break;
                    },
                }
            }
        }
    }
    Ok(())
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
            if p.is_relative() {
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

fn is_expected_ipc_disconnect(err: &io::Error) -> bool {
    ipc::is_ipc_disconnect(err)
}

#[cfg(test)]
mod tests {
    use super::{connect_with_retry, is_expected_ipc_disconnect};
    use std::ffi::OsStr;
    use std::io;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use tokio::net::UnixListener;

    fn test_socket_path(name: &str) -> PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("anyrun-{name}-{ts}.sock"))
    }

    #[tokio::test(flavor = "current_thread")]
    async fn connect_with_retry_succeeds_when_socket_appears_later() {
        let socket_path = test_socket_path("delayed-connect");
        let path_for_server = socket_path.clone();

        let server = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            let _ = std::fs::remove_file(&path_for_server);
            let listener = UnixListener::bind(&path_for_server).unwrap();
            let _ = listener.accept().await;
        });

        let stream = connect_with_retry(
            &socket_path,
            Duration::from_secs(2),
            Duration::from_millis(50),
        )
        .await
        .expect("socket should become available");
        drop(stream);
        let _ = server.await;
        let _ = std::fs::remove_file(socket_path);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn connect_with_retry_times_out_for_missing_socket() {
        let socket_path = test_socket_path("missing-socket");
        let err = connect_with_retry(
            &socket_path,
            Duration::from_millis(250),
            Duration::from_millis(50),
        )
        .await
        .expect_err("missing socket should time out");

        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn expected_ipc_disconnect_kinds() {
        let eof = io::Error::new(io::ErrorKind::UnexpectedEof, "peer closed");
        assert!(is_expected_ipc_disconnect(&eof));

        let reset = io::Error::new(io::ErrorKind::ConnectionReset, "reset");
        assert!(is_expected_ipc_disconnect(&reset));

        let invalid = io::Error::new(io::ErrorKind::InvalidData, "invalid");
        assert!(!is_expected_ipc_disconnect(&invalid));
    }

    #[test]
    fn build_command_includes_plugins() {
        let cmd = super::build_provider_command(
            &PathBuf::from("anyrun-provider"),
            "/tmp/anyrun-config",
            &[PathBuf::from("libfoo.so"), PathBuf::from("libbar.so")],
            "/tmp/anyrun.sock",
            vec![],
        );
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.contains(&"--config-dir".to_string()));
        assert!(args.contains(&"/tmp/anyrun-config".to_string()));
        assert!(args.contains(&"-p".to_string()));
        assert!(args.iter().any(|a| a.ends_with("libfoo.so")));
        assert!(args.iter().any(|a| a.ends_with("libbar.so")));
        assert!(args.contains(&"connect-to".to_string()));
        assert!(args.contains(&"/tmp/anyrun.sock".to_string()));
    }

    #[test]
    fn build_command_single_plugin() {
        let cmd = super::build_provider_command(
            &PathBuf::from("anyrun-provider"),
            "/tmp/anyrun-config",
            &[PathBuf::from("libfoo.so")],
            "/tmp/anyrun.sock",
            vec![],
        );
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let p_count = args.iter().filter(|a| *a == "-p").count();
        assert_eq!(p_count, 1);
        assert!(args.iter().any(|a| a.ends_with("libfoo.so")));
    }

    #[test]
    fn build_command_absolute_path() {
        let cmd = super::build_provider_command(
            &PathBuf::from("anyrun-provider"),
            "/tmp/anyrun-config",
            &[PathBuf::from("/usr/lib/anyrun/foo.so")],
            "/tmp/anyrun.sock",
            vec![],
        );
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let idx = args.iter().position(|a| a == "-p").unwrap();
        assert_eq!(args[idx + 1], "/usr/lib/anyrun/foo.so");
    }

    #[test]
    fn build_command_relative_path() {
        let cmd = super::build_provider_command(
            &PathBuf::from("anyrun-provider"),
            "/tmp/anyrun-config",
            &[PathBuf::from("./plugins/foo.so")],
            "/tmp/anyrun.sock",
            vec![],
        );
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let idx = args.iter().position(|a| a == "-p").unwrap();
        assert!(
            args[idx + 1].starts_with('/'),
            "relative path should be resolved to absolute: {}",
            args[idx + 1]
        );
    }

    #[test]
    fn build_command_path_dotdot() {
        let cmd = super::build_provider_command(
            &PathBuf::from("anyrun-provider"),
            "/tmp/anyrun-config",
            &[PathBuf::from("../anyrun/plugins/foo.so")],
            "/tmp/anyrun.sock",
            vec![],
        );
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let idx = args.iter().position(|a| a == "-p").unwrap();
        assert!(
            args[idx + 1].starts_with('/'),
            "relative path should be resolved to absolute: {}",
            args[idx + 1]
        );
    }

    #[test]
    fn build_command_path_with_spaces() {
        let cmd = super::build_provider_command(
            &PathBuf::from("anyrun-provider"),
            "/tmp/anyrun-config",
            &[PathBuf::from("lib/my plugin.so")],
            "/tmp/anyrun.sock",
            vec![],
        );
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let idx = args.iter().position(|a| a == "-p").unwrap();
        assert!(
            args[idx + 1].starts_with('/'),
            "relative path should be resolved to absolute: {}",
            args[idx + 1]
        );
    }

    #[test]
    fn build_command_includes_env() {
        let cmd = super::build_provider_command(
            &PathBuf::from("anyrun-provider"),
            "/tmp/anyrun-config",
            &[],
            "/tmp/anyrun.sock",
            vec![("KEY".to_string(), "val".to_string())],
        );
        let envs: Vec<_> = cmd.get_envs().collect();
        assert!(envs.contains(&(OsStr::new("KEY"), Some(OsStr::new("val")))));
    }

    #[test]
    fn build_command_uses_correct_program() {
        let cmd = super::build_provider_command(
            &PathBuf::from("/custom/path/anyrun-provider"),
            "/tmp/anyrun-config",
            &[],
            "/tmp/anyrun.sock",
            vec![],
        );
        assert_eq!(
            cmd.get_program(),
            OsStr::new("/custom/path/anyrun-provider")
        );
    }

    #[test]
    fn build_command_no_plugins_no_p_flags() {
        let cmd = super::build_provider_command(
            &PathBuf::from("anyrun-provider"),
            "/tmp/anyrun-config",
            &[],
            "/tmp/anyrun.sock",
            vec![],
        );
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(!args.iter().any(|a| a == "-p"));
    }

    #[test]
    fn build_command_relative_path_resolved() {
        let cmd = super::build_provider_command(
            &PathBuf::from("anyrun-provider"),
            "/tmp/anyrun-config",
            &[PathBuf::from("./plugins/foo.so")],
            "/tmp/anyrun.sock",
            vec![],
        );
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let idx = args.iter().position(|a| a == "-p").unwrap();
        assert!(
            args[idx + 1].starts_with('/'),
            "relative path should be resolved to absolute: {}",
            args[idx + 1]
        );
    }

    #[test]
    fn build_command_absolute_path_unchanged() {
        let cmd = super::build_provider_command(
            &PathBuf::from("anyrun-provider"),
            "/tmp/anyrun-config",
            &[PathBuf::from("/usr/lib/foo.so")],
            "/tmp/anyrun.sock",
            vec![],
        );
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let idx = args.iter().position(|a| a == "-p").unwrap();
        assert_eq!(args[idx + 1], "/usr/lib/foo.so");
    }
}
