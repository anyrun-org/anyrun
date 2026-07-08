use std::process::{Child, Command, Stdio};

use super::dbus;
use super::temp::TestDir;
use super::{anyrun_bin, apply_headless_env, BUS_NAME};

pub struct DaemonProcess {
    child: Option<Child>,
    dbus_child: Option<Child>,
    dbus_address: String,
    test_dir: Option<TestDir>,
}

impl DaemonProcess {
    pub async fn spawn(test_dir: TestDir) -> Self {
        let (dbus_child, addr) = dbus::start_private_dbus();
        Self::spawn_with_dbus_args(test_dir, dbus_child, addr, Vec::new()).await
    }

    pub async fn spawn_with_env(test_dir: TestDir, extra_env: Vec<(&str, &str)>) -> Self {
        let (dbus_child, addr) = dbus::start_private_dbus();
        Self::spawn_with_dbus_args(test_dir, dbus_child, addr, extra_env).await
    }

    async fn spawn_with_dbus_args(
        test_dir: TestDir,
        dbus_child: Child,
        dbus_address: String,
        extra_env: Vec<(&str, &str)>,
    ) -> Self {
        let mut cmd = Command::new(anyrun_bin());
        cmd.arg("--config-dir")
            .arg(test_dir.config_dir())
            .arg("daemon")
            .env("DBUS_SESSION_BUS_ADDRESS", &dbus_address)
            .env("XDG_RUNTIME_DIR", test_dir.runtime_dir())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        apply_headless_env(&mut cmd);

        for (k, v) in extra_env {
            cmd.env(k, v);
        }

        let child = cmd.spawn().expect("Failed to spawn anyrun daemon");

        dbus::wait_for_bus_name(&dbus_address, BUS_NAME).await;

        DaemonProcess {
            child: Some(child),
            dbus_child: Some(dbus_child),
            dbus_address,
            test_dir: Some(test_dir),
        }
    }

    pub fn dbus_address(&self) -> &str {
        &self.dbus_address
    }

    pub fn runtime_dir(&self) -> &std::path::Path {
        self.test_dir.as_ref().unwrap().runtime_dir()
    }

    pub async fn quit(&mut self) {
        if let Some(ref mut child) = self.child {
            child.kill().ok();
            let _ = child.wait();
        }
    }
}

impl Drop for DaemonProcess {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(ref mut child) = self.dbus_child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
