mod helpers;

use helpers::config::ConfigBuilder;
use helpers::process::DaemonProcess;
use helpers::temp::TestDir;

/// IT-31: D-Bus restart -- daemon handles session bus restart gracefully.
#[tokio::test]
#[ignore = "Complex D-Bus lifecycle management"]
async fn it_31_reliability_dbus_restart() {}

/// IT-32: Provider timeout -- hang-query plugin triggers timeout.
#[tokio::test]
#[ignore = "Requires hang-query mock plugin + timeout config"]
async fn it_32_reliability_provider_timeout() {
    let test_dir = TestDir::new();
    ConfigBuilder::new()
        .settle_delay(100)
        .write(test_dir.config_dir());
    let mut daemon = DaemonProcess::spawn(test_dir).await;
    daemon.quit().await;
}

/// IT-33: Graceful shutdown during active query.
#[tokio::test]
#[ignore = "Requires IPC integration"]
async fn it_33_reliability_shutdown_during_query() {
    let test_dir = TestDir::new();
    ConfigBuilder::new().write(test_dir.config_dir());
    let mut daemon = DaemonProcess::spawn(test_dir).await;
    daemon.quit().await;
}

/// IT-34: Repeated open/close stability (500 cycles).
#[tokio::test]
#[ignore = "Requires D-Bus IPC loop"]
async fn it_34_reliability_repeated_open_close() {}
