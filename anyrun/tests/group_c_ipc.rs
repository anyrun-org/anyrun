mod helpers;

use std::process::{Command, Stdio};
use std::time::Duration;

use helpers::anyrun_bin;
use helpers::config::ConfigBuilder;
use helpers::process::DaemonProcess;
use helpers::temp::TestDir;

fn client_env<'a>(cmd: &'a mut Command, daemon: &DaemonProcess) -> &'a mut Command {
    cmd.env("DBUS_SESSION_BUS_ADDRESS", daemon.dbus_address())
        .env("GSK_RENDERER", "cairo")
        .env("GTK_A11Y", "none")
        .env("NO_AT_BRIDGE", "1")
        .env("GTK_USE_PORTAL", "0")
        .env("GIO_USE_PORTALS", "0")
}

fn spawn_client(args: &[&str], daemon: &DaemonProcess) -> std::process::Child {
    let mut cmd = Command::new(anyrun_bin());
    client_env(cmd.args(args), daemon)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd.spawn().unwrap()
}

/// IT-18: Show request via D-Bus.
#[tokio::test]
async fn it_18_ipc_show_request() {
    let test_dir = TestDir::new();
    ConfigBuilder::new().write(test_dir.config_dir());
    let mut daemon = DaemonProcess::spawn(test_dir).await;

    let output = client_env(&mut Command::new(anyrun_bin()), &daemon)
        .output()
        .unwrap();
    assert!(output.status.success());

    daemon.quit().await;
}

/// IT-19: STDIN transfer integrity.
#[tokio::test]
#[ignore = "Requires stdin capture from mock provider"]
async fn it_19_ipc_stdin_transfer() {}

/// IT-20: Environment transfer integrity.
#[tokio::test]
#[ignore = "Requires env capture from mock provider"]
async fn it_20_ipc_env_transfer() {}

/// IT-21: Close request.
#[tokio::test]
async fn it_21_ipc_close_request() {
    let test_dir = TestDir::new();
    ConfigBuilder::new().write(test_dir.config_dir());
    let mut daemon = DaemonProcess::spawn(test_dir).await;

    let output = client_env(&mut Command::new(anyrun_bin()).arg("close"), &daemon)
        .output()
        .unwrap();
    assert!(output.status.success());

    daemon.quit().await;
}

/// IT-22: Quit request.
#[tokio::test]
async fn it_22_ipc_quit_request() {
    let test_dir = TestDir::new();
    ConfigBuilder::new().write(test_dir.config_dir());
    let daemon = DaemonProcess::spawn(test_dir).await;

    let output = client_env(&mut Command::new(anyrun_bin()).arg("quit"), &daemon)
        .output()
        .unwrap();
    assert!(output.status.success());
}

/// IT-23: Reload request.
#[tokio::test]
async fn it_23_ipc_reload_request() {
    let test_dir = TestDir::new();
    ConfigBuilder::new().write(test_dir.config_dir());
    let mut daemon = DaemonProcess::spawn(test_dir).await;

    let output = client_env(&mut Command::new(anyrun_bin()).arg("reload"), &daemon)
        .output()
        .unwrap();
    assert!(output.status.success());

    daemon.quit().await;
}

/// IT-24: Daemon unavailable fallback to standalone mode.
#[tokio::test]
async fn it_24_ipc_daemon_unavailable_fallback() {
    let test_dir = TestDir::new();
    ConfigBuilder::new().write(test_dir.config_dir());

    let mut child = Command::new(anyrun_bin())
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent.sock")
        .arg("--config-dir")
        .arg(test_dir.config_dir())
        .env("GSK_RENDERER", "cairo")
        .env("GTK_A11Y", "none")
        .env("NO_AT_BRIDGE", "1")
        .env("GTK_USE_PORTAL", "0")
        .env("GIO_USE_PORTALS", "0")
        .spawn()
        .unwrap();

    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(
        child.try_wait().unwrap().is_none(),
        "Client should still be running in standalone mode"
    );
    child.kill().ok();
    child.wait().unwrap();
}

/// IT-25: Concurrent client requests.
#[tokio::test]
#[ignore = "Requires fixing D-Bus session conflict"]
async fn it_25_ipc_concurrent_requests() {}

/// IT-26: Reload during active query.
#[tokio::test]
#[ignore = "Requires slow mock plugin + timing"]
async fn it_26_ipc_reload_during_query() {}
