use gtk4::{gio, glib, prelude::*};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

static TEST_MUTEX: Mutex<()> = Mutex::new(());

fn anyrun_bin() -> String {
    if let Ok(dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let ws = PathBuf::from(&dir).parent().unwrap().to_path_buf();
        for sub in &["release", "debug"] {
            let candidate = ws.join("target").join(sub).join("anyrun");
            if candidate.exists() {
                return candidate.to_string_lossy().to_string();
            }
        }
    }
    for p in &[
        "target/release/anyrun",
        "target/debug/anyrun",
        "../target/release/anyrun",
        "../target/debug/anyrun",
    ] {
        let path = PathBuf::from(p);
        if path.exists() {
            return path.canonicalize().unwrap().to_string_lossy().to_string();
        }
    }
    panic!("anyrun binary not found");
}

fn temp_dir() -> PathBuf {
    std::env::temp_dir().join(format!(
        "anyrun-client-daemon-e2e-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn make_config(root: &Path) {
    fs::create_dir_all(root.join("plugins")).unwrap();
    fs::write(
        root.join("config.ron"),
        "(
            x: Fraction(9.0),
            y: Fraction(9.0),
            width: Absolute(800),
            height: Absolute(600),
            plugins: [],
            provider: \"anyrun-provider\",
        )",
    )
    .unwrap();
    fs::write(root.join("style.css"), "").unwrap();
}

struct DaemonProcess {
    child: Child,
    log_path: PathBuf,
}

impl DaemonProcess {
    fn spawn(bin: &str, config_dir: &Path, dbus_address: &str) -> Self {
        let log_path = config_dir.join("daemon.log");
        let log_file = fs::File::create(&log_path).unwrap();

        let child = Command::new(bin)
            .arg("--config-dir")
            .arg(config_dir)
            .arg("daemon")
            .env("DBUS_SESSION_BUS_ADDRESS", dbus_address)
            .env("XDG_RUNTIME_DIR", config_dir)
            .env("GSK_RENDERER", "cairo")
            .env("NO_AT_BRIDGE", "1")
            .env("GTK_A11Y", "none")
            .env("GTK_USE_PORTAL", "0")
            .env("GIO_USE_PORTALS", "0")
            .stdout(log_file.try_clone().unwrap())
            .stderr(log_file)
            .spawn()
            .expect("Failed to spawn anyrun daemon");

        DaemonProcess { child, log_path }
    }

    fn print_logs(&self) {
        if let Ok(logs) = fs::read_to_string(&self.log_path) {
            println!("\n--- Daemon Logs ({}) ---", self.log_path.display());
            println!("{}", logs);
            println!("------------------------\n");
        }
    }
}

fn start_private_dbus() -> (Child, String) {
    let mut dbus_child = Command::new("dbus-daemon")
        .arg("--session")
        .arg("--print-address=1")
        .arg("--nofork")
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to start dbus-daemon");

    let mut dbus_address = String::new();
    if let Some(stdout) = dbus_child.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let l = line.unwrap();
            if l.starts_with("unix:path=") || l.starts_with("unix:abstract=") {
                dbus_address = l;
                break;
            }
        }
    }
    (dbus_child, dbus_address)
}

fn wait_for_dbus_registration(dbus_address: &str, timeout: Duration, daemon: &mut DaemonProcess) {
    let dbus_conn = gio::DBusConnection::for_address_sync(
        dbus_address,
        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
        None,
        None::<&gio::Cancellable>,
    )
    .expect("failed to connect to private dbus-daemon");

    let mut registered = false;
    let start_time = std::time::Instant::now();
    while start_time.elapsed() < timeout {
        let params = ("org.anyrun.anyrun",).to_variant();
        if dbus_conn
            .call_sync(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "GetNameOwner",
                Some(&params),
                None,
                gio::DBusCallFlags::NO_AUTO_START,
                100,
                None::<&gio::Cancellable>,
            )
            .is_ok()
        {
            registered = true;
            break;
        }

        if let Ok(Some(status)) = daemon.child.try_wait() {
            daemon.print_logs();
            panic!(
                "Daemon exited early with status: {} while waiting to register on D-Bus",
                status
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if !registered {
        daemon.print_logs();
        panic!("Daemon failed to register its D-Bus name within timeout");
    }
}

#[tokio::test]
async fn test_client_daemon_e2e_communication() {
    let _guard = TEST_MUTEX.lock().unwrap();

    // 1. Spawn a private dbus-daemon
    let (mut dbus_child, dbus_address) = start_private_dbus();
    assert!(
        !dbus_address.is_empty(),
        "Failed to get D-Bus session address"
    );

    // 2. Set up config directories
    let config_dir = temp_dir();
    make_config(&config_dir);

    let bin = anyrun_bin();

    // 3. Start the daemon under the private DBus address
    let mut daemon = DaemonProcess::spawn(&bin, &config_dir, &dbus_address);

    // Wait for the daemon to register its name on D-Bus Session
    wait_for_dbus_registration(&dbus_address, Duration::from_secs(10), &mut daemon);

    // 4. Start the client under the private DBus address to trigger show
    let mut client_child = Command::new(&bin)
        .arg("--config-dir")
        .arg(&config_dir)
        .env("DBUS_SESSION_BUS_ADDRESS", &dbus_address)
        .env("XDG_RUNTIME_DIR", &config_dir)
        .env("GSK_RENDERER", "cairo")
        .env("NO_AT_BRIDGE", "1")
        .env("GTK_A11Y", "none")
        .env("GTK_USE_PORTAL", "0")
        .env("GIO_USE_PORTALS", "0")
        .spawn()
        .expect("Failed to spawn anyrun client");

    // Wait for the client to connect and communicate
    std::thread::sleep(Duration::from_millis(1000));

    // 5. Send quit command via a new client invocation to shutdown daemon cleanly
    let quit_child = Command::new(&bin)
        .arg("--config-dir")
        .arg(&config_dir)
        .arg("quit")
        .env("DBUS_SESSION_BUS_ADDRESS", &dbus_address)
        .env("XDG_RUNTIME_DIR", &config_dir)
        .env("GSK_RENDERER", "cairo")
        .env("NO_AT_BRIDGE", "1")
        .env("GTK_A11Y", "none")
        .env("GTK_USE_PORTAL", "0")
        .env("GIO_USE_PORTALS", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to run anyrun quit");

    let quit_status = quit_child
        .wait_with_output()
        .expect("Failed to wait for quit command");

    if !quit_status.status.success() {
        daemon.print_logs();
        panic!(
            "anyrun quit command failed - stdout: {} - stderr: {}",
            String::from_utf8_lossy(&quit_status.stdout),
            String::from_utf8_lossy(&quit_status.stderr)
        );
    }

    // 6. Verify daemon exits cleanly
    let daemon_status = daemon
        .child
        .wait_with_timeout(Duration::from_secs(5))
        .expect("Daemon failed to exit in time");

    if daemon_status.is_none() {
        daemon.print_logs();
        panic!("Daemon timed out after quit signal");
    }

    // 7. Verify client also exits
    let client_status = client_child
        .wait_with_timeout(Duration::from_secs(5))
        .expect("Client failed to exit in time");

    if client_status.is_none() {
        daemon.print_logs();
        panic!("Client timed out after daemon quit");
    }

    // Cleanup
    let _ = dbus_child.kill();
    let _ = fs::remove_dir_all(config_dir);
}

#[tokio::test]
async fn test_daemon_collision() {
    let _guard = TEST_MUTEX.lock().unwrap();

    let (mut dbus_child, dbus_address) = start_private_dbus();

    let config_dir = temp_dir();
    make_config(&config_dir);
    let bin = anyrun_bin();

    // Start daemon 1
    let mut daemon1 = DaemonProcess::spawn(&bin, &config_dir, &dbus_address);

    wait_for_dbus_registration(&dbus_address, Duration::from_secs(10), &mut daemon1);

    // Start daemon 2 (should fail to register and exit)
    let log_path2 = config_dir.join("daemon2.log");
    let log_file2 = fs::File::create(&log_path2).unwrap();
    let mut daemon2 = Command::new(&bin)
        .arg("--config-dir")
        .arg(&config_dir)
        .arg("daemon")
        .env("DBUS_SESSION_BUS_ADDRESS", &dbus_address)
        .env("XDG_RUNTIME_DIR", &config_dir)
        .env("GSK_RENDERER", "cairo")
        .env("NO_AT_BRIDGE", "1")
        .env("GTK_A11Y", "none")
        .env("GTK_USE_PORTAL", "0")
        .env("GIO_USE_PORTALS", "0")
        .stdout(log_file2.try_clone().unwrap())
        .stderr(log_file2)
        .spawn()
        .unwrap();

    // Verify daemon 2 exits with non-zero
    let status2 = daemon2.wait_with_timeout(Duration::from_secs(5)).unwrap();
    if status2.is_none() {
        daemon1.print_logs();
        if let Ok(logs) = fs::read_to_string(&log_path2) {
            println!("--- Daemon 2 Logs ---");
            println!("{}", logs);
        }
        panic!("Daemon 2 should exit quickly due to name conflict");
    }
    assert!(
        !status2.unwrap().success(),
        "Daemon 2 should return exit error code"
    );

    // Clean up daemon 1
    let _ = Command::new(&bin)
        .arg("--config-dir")
        .arg(&config_dir)
        .arg("quit")
        .env("DBUS_SESSION_BUS_ADDRESS", &dbus_address)
        .env("XDG_RUNTIME_DIR", &config_dir)
        .env("GTK_USE_PORTAL", "0")
        .env("GIO_USE_PORTALS", "0")
        .status();

    let _ = daemon1.child.wait();
    let _ = dbus_child.kill();
    let _ = fs::remove_dir_all(config_dir);
}

#[tokio::test]
async fn test_client_standalone_fallback() {
    let _guard = TEST_MUTEX.lock().unwrap();

    let config_dir = temp_dir();
    make_config(&config_dir);
    let bin = anyrun_bin();

    // Start client with a fake invalid D-Bus address (forcing fallback to standalone)
    let mut client = Command::new(&bin)
        .arg("--config-dir")
        .arg(&config_dir)
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/tmp/nonexistent-dbus.sock",
        )
        .env("XDG_RUNTIME_DIR", &config_dir)
        .env("GSK_RENDERER", "cairo")
        .env("NO_AT_BRIDGE", "1")
        .env("GTK_A11Y", "none")
        .env("GTK_USE_PORTAL", "0")
        .env("GIO_USE_PORTALS", "0")
        .spawn()
        .unwrap();

    // Client should start standalone.
    std::thread::sleep(Duration::from_millis(1500));
    assert!(
        client.try_wait().unwrap().is_none(),
        "Client should still be running standalone"
    );

    // Terminate the client
    let _ = client.kill();
    let _ = fs::remove_dir_all(config_dir);
}

#[tokio::test]
async fn test_client_startup_speed_benchmark() {
    let _guard = TEST_MUTEX.lock().unwrap();

    let (mut dbus_child, dbus_address) = start_private_dbus();

    let config_dir = temp_dir();
    make_config(&config_dir);
    let bin = anyrun_bin();

    // Start daemon
    let mut daemon = DaemonProcess::spawn(&bin, &config_dir, &dbus_address);

    wait_for_dbus_registration(&dbus_address, Duration::from_secs(10), &mut daemon);

    // Run client and capture stdout to read connection speed
    let client_output = Command::new(&bin)
        .arg("--config-dir")
        .arg(&config_dir)
        .env("DBUS_SESSION_BUS_ADDRESS", &dbus_address)
        .env("XDG_RUNTIME_DIR", &config_dir)
        .env("GSK_RENDERER", "cairo")
        .env("NO_AT_BRIDGE", "1")
        .env("GTK_A11Y", "none")
        .env("GTK_USE_PORTAL", "0")
        .env("GIO_USE_PORTALS", "0")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    std::thread::sleep(Duration::from_millis(1000));

    let _ = Command::new(&bin)
        .arg("--config-dir")
        .arg(&config_dir)
        .arg("close")
        .env("DBUS_SESSION_BUS_ADDRESS", &dbus_address)
        .env("XDG_RUNTIME_DIR", &config_dir)
        .env("GTK_USE_PORTAL", "0")
        .env("GIO_USE_PORTALS", "0")
        .status();

    let output = client_output.wait_with_output().unwrap();
    let stdout_str = String::from_utf8_lossy(&output.stdout);

    // Assert that the client connected to D-Bus and log contains connection time
    if !stdout_str.contains("D-Bus connection established at") {
        daemon.print_logs();
        panic!(
            "Client should establish D-Bus connection. Stdout was: {}",
            stdout_str
        );
    }

    // Assert it contains valid latency unit
    let has_valid_time = stdout_str.lines().any(|line| {
        if line.contains("D-Bus connection established at") {
            if let Some(pos) = line.find("established at ") {
                let duration_str = &line[pos + 15..line.len() - 1];
                return duration_str.ends_with("ms")
                    || duration_str.ends_with("µs")
                    || duration_str.ends_with("ns");
            }
        }
        false
    });

    assert!(
        has_valid_time,
        "D-Bus connection established log should contain valid latency unit"
    );

    // Terminate daemon
    let _ = Command::new(&bin)
        .arg("--config-dir")
        .arg(&config_dir)
        .arg("quit")
        .env("DBUS_SESSION_BUS_ADDRESS", &dbus_address)
        .env("XDG_RUNTIME_DIR", &config_dir)
        .env("GTK_USE_PORTAL", "0")
        .env("GIO_USE_PORTALS", "0")
        .status();

    let _ = daemon.child.wait();
    let _ = dbus_child.kill();
    let _ = fs::remove_dir_all(config_dir);
}

trait WaitTimeout {
    fn wait_with_timeout(
        &mut self,
        timeout: Duration,
    ) -> std::io::Result<Option<std::process::ExitStatus>>;
}

impl WaitTimeout for Child {
    fn wait_with_timeout(
        &mut self,
        timeout: Duration,
    ) -> std::io::Result<Option<std::process::ExitStatus>> {
        let start = std::time::Instant::now();
        loop {
            if let Some(status) = self.try_wait()? {
                return Ok(Some(status));
            }
            if start.elapsed() >= timeout {
                return Ok(None);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}
