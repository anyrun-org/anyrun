mod helpers;

use helpers::config::ConfigBuilder;
use helpers::process::DaemonProcess;
use helpers::temp::TestDir;

/// IT-27: Immediate search on startup with show_results_immediately=true.
#[tokio::test]
#[ignore = "Requires provider IPC inspection"]
async fn it_27_search_immediate_on_startup() {
    let test_dir = TestDir::new();
    ConfigBuilder::new()
        .show_results_immediately(true)
        .write(test_dir.config_dir());
    let mut daemon = DaemonProcess::spawn(test_dir).await;
    daemon.quit().await;
}

/// IT-28: Debounce behavior -- only last query sent to provider.
#[tokio::test]
#[ignore = "Requires GTK text input simulation"]
async fn it_28_search_debounce() {}

/// IT-29: Query cancellation -- new query replaces pending one.
#[tokio::test]
#[ignore = "Requires slow mock plugin + input simulation"]
async fn it_29_search_query_cancellation() {}

/// IT-30: Large query (10000 chars) handling.
#[tokio::test]
#[ignore = "Requires GTK text input or IPC text injection"]
async fn it_30_search_large_query() {}
