mod helpers;

use std::time::Duration;

use helpers::config::ConfigBuilder;
use helpers::process::DaemonProcess;
use helpers::temp::TestDir;

/// IT-35: Client startup latency (P95 < baseline + 10%).
#[tokio::test]
#[ignore = "Performance benchmark -- run with --release -- --ignored"]
async fn it_35_perf_client_startup_latency() {
    let test_dir = TestDir::new();
    ConfigBuilder::new().write(test_dir.config_dir());
    let mut daemon = DaemonProcess::spawn(test_dir).await;
    daemon.quit().await;
}

/// IT-36: IPC round-trip latency (P95 < 20ms).
#[tokio::test]
#[ignore = "Performance benchmark -- run with --release -- --ignored"]
async fn it_36_perf_ipc_roundtrip() {}

/// IT-37: CSS reload throttle.
#[tokio::test]
#[ignore = "Performance benchmark -- run with --release -- --ignored"]
async fn it_37_perf_css_reload_throttle() {}

/// IT-38: Burst request stress test (100 requests).
#[tokio::test]
#[ignore = "Performance benchmark -- run with --release -- --ignored"]
async fn it_38_perf_burst_request_stress() {
    let test_dir = TestDir::new();
    ConfigBuilder::new().write(test_dir.config_dir());
    let mut daemon = DaemonProcess::spawn(test_dir).await;
    daemon.quit().await;
}
