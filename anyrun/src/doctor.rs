use crate::config::Config;
use anyrun_interface::PluginRef;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub struct DoctorReport {
    pub required_failures: usize,
    pub lines: Vec<String>,
}

pub fn run(config_dir: Option<&str>) -> i32 {
    let config_dir = config_dir
        .map(PathBuf::from)
        .unwrap_or_else(default_config_dir);
    let report = inspect_config_dir(&config_dir);
    for line in report.lines {
        println!("{line}");
    }
    if report.required_failures == 0 {
        0
    } else {
        1
    }
}

fn inspect_config_dir(config_dir: &Path) -> DoctorReport {
    let mut report = DoctorReport {
        required_failures: 0,
        lines: Vec::new(),
    };

    if !config_dir.exists() {
        report.required_failures += 1;
        report
            .lines
            .push(format!("config dir missing: {}", config_dir.display()));
        return report;
    }

    let config_path = config_dir.join("config.ron");
    let config = match fs::read(&config_path) {
        Ok(content) => match ron::de::from_bytes::<Config>(&content) {
            Ok(config) => {
                report
                    .lines
                    .push(format!("config ok: {}", config_path.display()));
                config
            }
            Err(why) => {
                report.required_failures += 1;
                report.lines.push(format!(
                    "config parse failed: {}: {why}",
                    config_path.display()
                ));
                return report;
            }
        },
        Err(why) => {
            report.required_failures += 1;
            report.lines.push(format!(
                "config read failed: {}: {why}",
                config_path.display()
            ));
            return report;
        }
    };

    report
        .lines
        .push("provider ok: built-in (single-binary mode)".to_string());

    let plugin_dirs = plugin_dirs(config_dir);
    for plugin in &config.plugins {
        match find_plugin(plugin, &plugin_dirs) {
            Some(path) => match abi_stable::library::lib_header_from_path(&path)
                .and_then(|header| header.init_root_module::<PluginRef>())
            {
                Ok(_) => report.lines.push(format!("plugin ok: {}", path.display())),
                Err(why) => {
                    report.required_failures += 1;
                    report
                        .lines
                        .push(format!("plugin load failed: {}: {why}", path.display()));
                }
            },
            None => {
                report.required_failures += 1;
                report
                    .lines
                    .push(format!("plugin missing: {}", plugin.display()));
            }
        }
    }

    report
}

fn default_config_dir() -> PathBuf {
    let user_dir = env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let mut p = PathBuf::from(env::var("HOME").unwrap_or_else(|_| ".".into()));
            p.push(".config");
            p
        })
        .join("anyrun");

    if user_dir.exists() {
        user_dir
    } else {
        anyrun_provider_ipc::CONFIG_DIRS
            .iter()
            .map(PathBuf::from)
            .find(|path| path.exists())
            .unwrap_or_else(|| PathBuf::from(anyrun_provider_ipc::CONFIG_DIRS[0]))
    }
}

fn plugin_dirs(config_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![config_dir.join("plugins")];
    if let Ok(path) = env::var("ANYRUN_PLUGINS") {
        dirs.push(PathBuf::from(path));
    }
    dirs.extend(anyrun_provider_ipc::PLUGIN_PATHS.iter().map(PathBuf::from));
    dirs
}

fn find_plugin(name: &Path, dirs: &[PathBuf]) -> Option<PathBuf> {
    let name = expand_tilde(name);
    if name.is_absolute() && name.exists() {
        return Some(name);
    }
    for dir in dirs {
        let p = dir.join(&name);
        if p.exists() {
            return Some(p);
        }

        let lib_name = format!("lib{}.so", name.to_string_lossy().replace('-', "_"));
        let p = dir.join(lib_name);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

#[allow(dead_code)]
fn resolve_executable(path: &Path) -> Option<PathBuf> {
    if path.components().count() > 1 {
        return path.exists().then(|| path.to_path_buf());
    }

    let path_var = env::var_os("PATH")?;
    env::split_paths(&path_var)
        .map(|dir| dir.join(path))
        .find(|candidate| candidate.exists())
}

fn expand_tilde(path: &Path) -> PathBuf {
    if let Some(path_str) = path.to_str() {
        if path_str.starts_with("~/") {
            if let Ok(home) = env::var("HOME") {
                return PathBuf::from(path_str.replacen('~', &home, 1));
            }
        }
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "anyrun-doctor-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn temp_config(provider: &str, plugins: &[&str]) -> PathBuf {
        let root = temp_dir();
        fs::create_dir_all(root.join("plugins")).unwrap();
        let plugins_list = plugins
            .iter()
            .map(|p| format!("\"{p}\""))
            .collect::<Vec<_>>()
            .join(", ");
        fs::write(
            root.join("config.ron"),
            format!("(provider: \"{provider}\", plugins: [{plugins_list}])"),
        )
        .unwrap();
        root
    }

    #[test]
    fn doctor_missing_config_dir() {
        let root = temp_dir();
        // Don't create the dir
        let report = inspect_config_dir(&root);
        assert!(report.required_failures > 0);
        assert!(report
            .lines
            .iter()
            .any(|l| l.contains("config dir missing")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn doctor_missing_config_file() {
        let root = temp_dir();
        fs::create_dir_all(root.join("plugins")).unwrap();
        // No config.ron
        let report = inspect_config_dir(&root);
        assert!(report.required_failures > 0);
        assert!(report
            .lines
            .iter()
            .any(|l| l.contains("config read failed")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn doctor_invalid_ron_syntax() {
        let root = temp_dir();
        fs::create_dir_all(root.join("plugins")).unwrap();
        fs::write(root.join("config.ron"), "not valid ron {{{").unwrap();
        let report = inspect_config_dir(&root);
        assert!(report.required_failures > 0);
        assert!(report
            .lines
            .iter()
            .any(|l| l.contains("config parse failed")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn doctor_accepts_empty_valid_config() {
        let provider = std::env::current_exe().unwrap();
        let root = temp_config(provider.to_str().unwrap(), &[]);
        let report = inspect_config_dir(&root);
        assert_eq!(report.required_failures, 0);
        assert!(report.lines.iter().any(|l| l.contains("config ok")));
        assert!(report.lines.iter().any(|l| l.contains("provider ok: built-in")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn doctor_missing_plugin_path() {
        let provider = std::env::current_exe().unwrap();
        let root = temp_config(provider.to_str().unwrap(), &["nonexistent_plugin"]);
        let report = inspect_config_dir(&root);
        assert!(report.required_failures > 0);
        assert!(report.lines.iter().any(|l| l.contains("plugin missing")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn doctor_empty_plugins_list_succeeds() {
        let provider = std::env::current_exe().unwrap();
        let root = temp_config(provider.to_str().unwrap(), &[]);
        let report = inspect_config_dir(&root);
        assert_eq!(report.required_failures, 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn doctor_multiple_failures_accumulate() {
        let root = temp_dir();
        fs::create_dir_all(root.join("plugins")).unwrap();
        fs::write(
            root.join("config.ron"),
            r#"(plugins: ["p1", "p2"])"#,
        )
        .unwrap();
        let report = inspect_config_dir(&root);
        // p1 missing + p2 missing = 2
        assert_eq!(report.required_failures, 2);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn doctor_resolve_executable_with_path() {
        // Should find 'sh' in PATH
        let found = resolve_executable(&PathBuf::from("sh"));
        assert!(found.is_some());
        assert!(found.unwrap().exists());
    }

    #[test]
    fn doctor_resolve_executable_absolute_path() {
        let found = resolve_executable(&PathBuf::from("/bin/sh"));
        assert!(found.is_some());
    }

    #[test]
    fn doctor_resolve_executable_missing() {
        let found = resolve_executable(&PathBuf::from("definitely-not-a-real-binary-9999"));
        assert!(found.is_none());
    }

    #[test]
    fn doctor_expand_tilde() {
        let home = env::var("HOME").unwrap();
        let expanded = expand_tilde(&PathBuf::from("~/test"));
        assert_eq!(expanded, PathBuf::from(format!("{home}/test")));
    }

    #[test]
    fn doctor_expand_tilde_no_tilde() {
        let expanded = expand_tilde(&PathBuf::from("/absolute/path"));
        assert_eq!(expanded, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn doctor_plugin_dirs_includes_config_plugins() {
        let config_dir = PathBuf::from("/tmp/test-anyrun-config");
        let dirs = plugin_dirs(&config_dir);
        assert!(dirs.iter().any(|d| d == &config_dir.join("plugins")));
    }

    #[test]
    fn doctor_plugin_dirs_includes_env_var() {
        let prev = env::var_os("ANYRUN_PLUGINS");
        env::set_var("ANYRUN_PLUGINS", "/custom/plugins");
        let dirs = plugin_dirs(&PathBuf::from("/tmp/test-anyrun-config"));
        assert!(dirs.iter().any(|d| d == &PathBuf::from("/custom/plugins")));
        // Restore
        match prev {
            Some(v) => env::set_var("ANYRUN_PLUGINS", v),
            None => env::remove_var("ANYRUN_PLUGINS"),
        }
    }

    #[test]
    fn doctor_run_exit_code_zero_on_success() {
        let provider = std::env::current_exe().unwrap();
        let root = temp_config(provider.to_str().unwrap(), &[]);
        let exit_code = run(Some(root.to_str().unwrap()));
        assert_eq!(exit_code, 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn doctor_run_exit_code_one_on_failure() {
        let provider = std::env::current_exe().unwrap();
        let root = temp_config(provider.to_str().unwrap(), &["nonexistent_plugin"]);
        let exit_code = run(Some(root.to_str().unwrap()));
        assert_eq!(exit_code, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn doctor_with_plugins_in_subdir_absolute_path_found() {
        let provider = std::env::current_exe().unwrap();
        let root = temp_config(provider.to_str().unwrap(), &[]);
        // Create a valid (dummy) .so file in plugins dir to test plugin loading
        // We can't easily create a valid abi_stable plugin, so we test the path finding
        let plugin_path = root.join("plugins").join("libtest.so");
        fs::write(&plugin_path, "not a real plugin").unwrap();

        let found = find_plugin(&PathBuf::from("test"), &[root.join("plugins")]);
        assert_eq!(found, Some(plugin_path));
        let _ = fs::remove_dir_all(root);
    }
}
