use std::path::Path;

pub struct ConfigBuilder {
    plugins: Vec<String>,
    provider: String,
    show_results_immediately: bool,
    settle_delay_ms: u64,
    flush_delay_ms: u64,
    css: String,
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self {
            plugins: vec![],
            provider: "anyrun-provider".into(),
            show_results_immediately: false,
            settle_delay_ms: 150,
            flush_delay_ms: 50,
            css: "".into(),
        }
    }
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn plugins(mut self, plugins: &[String]) -> Self {
        self.plugins = plugins.to_vec();
        self
    }

    pub fn provider(mut self, path: &str) -> Self {
        self.provider = path.into();
        self
    }

    pub fn show_results_immediately(mut self, val: bool) -> Self {
        self.show_results_immediately = val;
        self
    }

    pub fn settle_delay(mut self, ms: u64) -> Self {
        self.settle_delay_ms = ms;
        self
    }

    pub fn flush_delay(mut self, ms: u64) -> Self {
        self.flush_delay_ms = ms;
        self
    }

    pub fn css(mut self, css: &str) -> Self {
        self.css = css.into();
        self
    }

    pub fn write(&self, dir: &Path) {
        let plugin_list = if self.plugins.is_empty() {
            "[]".to_string()
        } else {
            let items: Vec<String> = self
                .plugins
                .iter()
                .map(|p| format!("    \"{}\"", p))
                .collect();
            format!("[\n{}\n]", items.join(",\n"))
        };

        let config = format!(
            r#"Config(
    plugins: {},
    provider: "{}",
    show_results_immediately: {},
    search_ux: (
        settle_delay_ms: {},
        flush_delay_ms: {},
        typing_visual: DimPrevious,
        bare_text_fast_lane: false,
        prefix_routes: [],
    ),
)"#,
            plugin_list,
            self.provider,
            self.show_results_immediately,
            self.settle_delay_ms,
            self.flush_delay_ms,
        );

        std::fs::write(dir.join("config.ron"), &config).unwrap();
        std::fs::write(dir.join("style.css"), &self.css).unwrap();
    }
}
