use std::{
    env, fs,
    io::{self, Write},
    os::unix::net::UnixListener,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{mpsc, Arc},
    thread,
    time::Duration,
};

use anyrun_provider_ipc as ipc;
use relm4::Sender;

use crate::config::Config;

pub fn worker(
    config: Arc<Config>,
    config_dir: Option<String>,
    rx: mpsc::Receiver<anyrun_provider_ipc::Request>,
    sender: Sender<anyrun_provider_ipc::Response>,
    // The stdin received by the launching command
    stdin: Vec<u8>,
    // The environment of the launching command
    env: Vec<(String, String)>,
) -> io::Result<()> {
    let socket_path = format!(
        "{}/anyrun.sock",
        env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".to_string())
    );
    // Make sure that it does not exist already
    let _ = fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path).unwrap();

    let mut child = match Command::new(&config.provider)
        .stdin(Stdio::piped())
        .arg("--config-dir")
        .arg(config_dir.unwrap_or(ipc::CONFIG_DIRS[0].to_string()))
        .args(
            config
                .plugins
                .iter()
                .flat_map(|plugin| [PathBuf::from("-p"), plugin.to_owned()]),
        )
        .arg("connect-to")
        .arg(&socket_path)
        .envs(env)
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

    let (stream, _) = listener.accept()?;
    let mut socket = ipc::Socket::new(stream);
    socket.inner.get_ref().set_nonblocking(true)?;

    'outer: loop {
        for req in rx.try_iter() {
            socket.send(&req)?;
            if matches!(req, ipc::Request::Quit) {
                break 'outer;
            }
        }

        match socket.recv() {
            Ok(response) => sender.emit(response),
            Err(why) => match why.kind() {
                io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(1)),
                _ => {
                    eprintln!("[anyrun] Error reading from IPC: {why}");
                    break;
                }
            },
        }
    }

    // Remove it after we are done with it
    let _ = fs::remove_file(&socket_path);
    // Make sure it exits properly and doesn't leave a zombie process
    let _ = child.wait();

    Ok(())
}
