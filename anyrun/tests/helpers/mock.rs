use std::path::{Path, PathBuf};

pub enum MockPluginType {
    Basic,
    PanicInit,
    HangQuery,
}

impl MockPluginType {
    fn lib_name(&self) -> &str {
        match self {
            MockPluginType::Basic => "libmock_plugin_basic.so",
            MockPluginType::PanicInit => "libmock_plugin_panic_init.so",
            MockPluginType::HangQuery => "libmock_plugin_hang_query.so",
        }
    }

    pub fn so_path(&self) -> PathBuf {
        let profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("target")
            .join(profile)
            .join(self.lib_name())
    }

    pub fn copy_to(&self, dest: &Path) -> PathBuf {
        let src = self.so_path();
        let dest_path = dest.join(self.lib_name());
        std::fs::copy(&src, &dest_path).unwrap();
        dest_path
    }
}
