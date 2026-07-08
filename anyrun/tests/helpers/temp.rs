use std::path::{Path, PathBuf};

pub struct TestDir {
    inner: tempfile::TempDir,
    config_dir: PathBuf,
    plugins_dir: PathBuf,
    runtime_dir: PathBuf,
    logs_dir: PathBuf,
}

impl TestDir {
    pub fn new() -> Self {
        let inner = tempfile::tempdir().expect("Failed to create temp dir");
        let config_dir = inner.path().join("config");
        let plugins_dir = inner.path().join("plugins");
        let runtime_dir = inner.path().join("runtime");
        let logs_dir = inner.path().join("logs");

        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::create_dir_all(&plugins_dir).unwrap();
        std::fs::create_dir_all(&runtime_dir).unwrap();
        std::fs::create_dir_all(&logs_dir).unwrap();

        TestDir {
            inner,
            config_dir,
            plugins_dir,
            runtime_dir,
            logs_dir,
        }
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }
    pub fn plugins_dir(&self) -> &Path {
        &self.plugins_dir
    }
    pub fn runtime_dir(&self) -> &Path {
        &self.runtime_dir
    }
    pub fn logs_dir(&self) -> &Path {
        &self.logs_dir
    }
    pub fn path(&self) -> &Path {
        self.inner.path()
    }
}
