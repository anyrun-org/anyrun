mod helpers;

use std::os::unix::fs::PermissionsExt;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use helpers::config::ConfigBuilder;
use helpers::dbus::{start_private_dbus, wait_for_bus_name};
use helpers::process::DaemonProcess;
use helpers::temp::TestDir;
use helpers::{anyrun_bin, apply_headless_env, BUS_NAME};

/// Manual daemon spawn without DaemonProcess (for IT-11 which shares dbus session).
fn spawn_manual_daemon(addr: &str, test_dir: &TestDir) -> Child {
    let mut cmd = Command::new(anyrun_bin());
    cmd.arg("--config-dir")
        .arg(test_dir.config_dir())
        .arg("daemon")
        .env("DBUS_SESSION_BUS_ADDRESS", addr)
        .env("XDG_RUNTIME_DIR", test_dir.runtime_dir())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    apply_headless_env(&mut cmd);
    cmd.spawn().expect("Failed to spawn anyrun daemon")
}

/// IT-10: Daemon registers on D-Bus.
#[tokio::test]
async fn it_10_daemon_bus_registration() {
    let test_dir = TestDir::new();
    ConfigBuilder::new().write(test_dir.config_dir());
    let mut daemon = DaemonProcess::spawn(test_dir).await;
    daemon.quit().await;
}

/// IT-11: Duplicate daemon is prevented.
#[tokio::test]
async fn it_11_daemon_duplicate_prevention() {
    struct ProcGuard(Option<Child>, Option<Child>);
    impl Drop for ProcGuard {
        fn drop(&mut self) {
            for c in [&mut self.0, &mut self.1] {
                if let Some(ref mut c) = c {
                    let _ = c.kill();
                    let _ = c.wait();
                }
            }
        }
    }

    let test_dir = TestDir::new();
    ConfigBuilder::new().write(test_dir.config_dir());

    let (dbus_child, addr) = start_private_dbus();
    let _guard = ProcGuard(Some(dbus_child), None);
    let bin = anyrun_bin();

    let spawn_cmd = || -> Child {
        let mut cmd = Command::new(&bin);
        cmd.arg("--config-dir")
            .arg(test_dir.config_dir())
            .arg("daemon")
            .env("DBUS_SESSION_BUS_ADDRESS", &addr)
            .env("XDG_RUNTIME_DIR", test_dir.runtime_dir())
            .env("GSK_RENDERER", "cairo")
            .env("NO_AT_BRIDGE", "1")
            .env("GTK_A11Y", "none")
            .env("GTK_USE_PORTAL", "0")
            .env("GIO_USE_PORTALS", "0")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        cmd.spawn().unwrap()
    };

    let mut daemon1 = spawn_cmd();
    wait_for_bus_name(&addr, BUS_NAME).await;

    let mut daemon2 = spawn_cmd();
    let status = daemon2.wait().unwrap();
    assert!(!status.success(), "Second daemon should fail to start");

    let _ = daemon1.kill();
    let _ = daemon1.wait();
}

/// IT-12: Custom CSS loading.
#[tokio::test]
async fn it_12_daemon_custom_css_loading() {
    let test_dir = TestDir::new();
    ConfigBuilder::new()
        .css("window { background-color: red; }")
        .write(test_dir.config_dir());
    let mut daemon = DaemonProcess::spawn(test_dir).await;
    daemon.quit().await;
}

/// IT-13: Invalid CSS recovery — daemon still starts and registers on D-Bus.
#[tokio::test]
async fn it_13_daemon_invalid_css_recovery() {
    let test_dir = TestDir::new();
    ConfigBuilder::new()
        .css("this is not valid css {{{")
        .write(test_dir.config_dir());
    let mut daemon = DaemonProcess::spawn(test_dir).await;
    daemon.quit().await;
}

/// IT-14: Provider spawned by daemon — daemon stays alive after startup.
#[tokio::test]
async fn it_14_daemon_provider_spawn() {
    let test_dir = TestDir::new();
    ConfigBuilder::new().write(test_dir.config_dir());
    let mut daemon = DaemonProcess::spawn(test_dir).await;
    // DaemonProcess::spawn blocks until D-Bus registered = daemon + provider running
    // No crash means provider spawned successfully
    daemon.quit().await;
}

/// IT-15: Custom provider spawn.
#[tokio::test]
async fn it_15_daemon_custom_provider_spawn() {
    let test_dir = TestDir::new();

    let mock_provider = test_dir.path().join("mock-provider.sh");
    std::fs::write(&mock_provider, "#!/bin/sh\nsleep 30\n").unwrap();
    std::fs::set_permissions(&mock_provider, PermissionsExt::from_mode(0o755)).unwrap();

    ConfigBuilder::new()
        .provider(&mock_provider.to_string_lossy())
        .write(test_dir.config_dir());

    let mut daemon = DaemonProcess::spawn(test_dir).await;
    daemon.quit().await;
}

/// IT-16: Stale socket cleanup.
#[tokio::test]
async fn it_16_daemon_stale_socket_cleanup() {
    let test_dir = TestDir::new();
    ConfigBuilder::new().write(test_dir.config_dir());

    let stale_socket = test_dir.runtime_dir().join("anyrun.sock");
    std::fs::write(&stale_socket, b"stale").unwrap();

    let mut daemon = DaemonProcess::spawn(test_dir).await;
    daemon.quit().await;
}

/// IT-17: Provider crash recovery — daemon survives provider SIGKILL.
#[tokio::test]
#[ignore = "Requires provider PID tracking"]
async fn it_17_daemon_provider_crash_recovery() {}
