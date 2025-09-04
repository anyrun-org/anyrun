use std::{
    env, fs, io,
    os::unix::net::UnixListener,
    path::PathBuf,
    process::Command,
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
) -> io::Result<()> {
    let socket_path = format!(
        "{}/anyrun.sock",
        env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".to_string())
    );
    // Make sure that it does not exist already
    let _ = fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path).unwrap();

    Command::new(&config.provider)
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
        .spawn()?;

    let (stream, _) = listener.accept()?;
    let mut socket = ipc::Socket::new(stream);
    socket.inner.get_ref().set_nonblocking(true)?;

    loop {
        match rx.try_recv() {
            Ok(request) => {
                socket.send(&request)?;
                if matches!(request, ipc::Request::Quit) {
                    break;
                }
            }
            Err(mpsc::TryRecvError::Empty) => (),
            Err(mpsc::TryRecvError::Disconnected) => {
                eprintln!("[anyrun] GUI thread disconnected");
                break;
            }
        }

        match socket.recv() {
            Ok(response) => sender.emit(response),
            Err(why) => match why.kind() {
                io::ErrorKind::WouldBlock => (),
                io::ErrorKind::ConnectionAborted => break,
                _ => {
                    eprintln!("[anyrun] Error reading from IPC: {why}");
                }
            },
        }

        thread::sleep(Duration::from_millis(10))
    }

    // Remove it after we are done with it
    let _ = fs::remove_file(&socket_path);

    Ok(())
}
