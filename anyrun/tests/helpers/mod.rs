pub mod config;
pub mod dbus;
pub mod env;
pub mod mock;
pub mod process;
pub mod temp;

#[allow(unused_imports)]
pub use env::set_headless_env;

use std::path::PathBuf;
use std::process::Command;

pub const BUS_NAME: &str = "org.anyrun.anyrun";
pub const DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
pub const SLOW_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub fn anyrun_bin() -> PathBuf {
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("target")
        .join(profile)
        .join("anyrun");
    assert!(path.exists(), "anyrun binary not found at {:?}", path);
    path
}

pub fn provider_bin() -> PathBuf {
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("target")
        .join(profile)
        .join("anyrun-provider");
    assert!(
        path.exists(),
        "anyrun-provider binary not found at {:?}",
        path
    );
    path
}

pub fn apply_headless_env(cmd: &mut Command) -> &mut Command {
    cmd.env("GSK_RENDERER", "cairo")
        .env("GTK_A11Y", "none")
        .env("NO_AT_BRIDGE", "1")
        .env("GTK_USE_PORTAL", "0")
        .env("GIO_USE_PORTALS", "0")
}
