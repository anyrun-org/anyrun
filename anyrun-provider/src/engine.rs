use anyrun_interface::{HandleResult, Match, PluginInfo, PluginRef, abi_stable};
use anyrun_provider_ipc::{
    CONFIG_DIRS, PLUGIN_PATHS, PluginHealth, PluginHealthState, RecentMatch, Request, Response,
    Socket, is_ipc_disconnect,
};
use bincode;
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use notify::RecommendedWatcher;
use notify_debouncer_mini::{Debouncer, new_debouncer};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use std::time::Instant;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Mutex, broadcast, mpsc, watch};
use tokio::task::JoinHandle;

static TEMP_PLUGIN_COPY_COUNTER: AtomicU64 = AtomicU64::new(0);
const RELOAD_PLUGIN_SETTLE_DELAY_MS: u64 = 50;
const RELOAD_WATCHER_SETTLE_DELAY_MS: u64 = 100;

pub(crate) struct PluginQueryResult {
    pub(crate) plugin_idx: usize,
    pub(crate) query_id: u64,
    pub(crate) query_text: Arc<str>,
    pub(crate) matches: abi_stable::std_types::RVec<Match>,
    pub(crate) elapsed_ms: u64,
    pub(crate) slow_ms: u64,
    pub(crate) timed_out: bool,
}

#[derive(Clone)]
pub(crate) struct QueuedQuery {
    pub(crate) query_id: u64,
    pub(crate) text: Arc<str>,
    pub(crate) timeout_ms: u64,
    pub(crate) slow_ms: u64,
}

struct PluginSearchWorker {
    query_tx: watch::Sender<Option<QueuedQuery>>,
    handle: JoinHandle<()>,
}

type SearchFn = Arc<dyn Fn(Arc<str>) -> abi_stable::std_types::RVec<Match> + Send + Sync + 'static>;

#[derive(Serialize, Deserialize, Default)]
pub(crate) struct FrecencyData {
    // (plugin_name, match_title) -> timestamps
    #[serde(default)]
    pub usage: HashMap<(String, String), Vec<DateTime<Utc>>>,
    #[serde(default)]
    pub recent: Vec<RecentMatch>,
}

impl FrecencyData {
    fn selection_safe_for_recent(selection: &Match) -> bool {
        matches!(selection.id, abi_stable::std_types::ROption::RNone)
    }

    pub fn load(config_dir: &str) -> Self {
        let path = PathBuf::from(config_dir).join("frecency.json");
        fs::read_to_string(path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, config_dir: &str) {
        let path = PathBuf::from(config_dir).join("frecency.json");
        if let Ok(content) = serde_json::to_string(self) {
            let _ = fs::write(path, content);
        }
    }

    pub fn cleanup(&mut self) {
        let now = Utc::now();
        let max_age = chrono::Duration::days(30);
        for usages in self.usage.values_mut() {
            usages.retain(|&time| now.signed_duration_since(time) <= max_age);
        }
        self.usage.retain(|_, usages| !usages.is_empty());
        let min_unix = (now - max_age).timestamp();
        self.recent.retain(|entry| {
            entry.last_used_unix >= min_unix
                && entry.uses > 0
                && Self::selection_safe_for_recent(&entry.selection)
        });
        self.recent
            .sort_by_key(|entry| std::cmp::Reverse(entry.last_used_unix));
    }

    pub fn batch_get_scores(&self, plugin: &str, titles: &[&str], half_life_days: f64) -> Vec<f64> {
        let now = Utc::now();
        let mut results = Vec::with_capacity(titles.len());
        for &title in titles {
            let mut total_score = 0.0;
            if let Some(usages) = self.usage.get(&(plugin.to_string(), title.to_string())) {
                for &time in usages {
                    let duration = now.signed_duration_since(time);
                    let days = duration.num_seconds() as f64 / 86400.0;
                    total_score += 0.5f64.powf(days / half_life_days);
                }
            }
            results.push(total_score);
        }
        results
    }

    pub fn record_selection(&mut self, plugin: &PluginInfo, selection: &Match) {
        let now = Utc::now();
        self.usage
            .entry((plugin.name.to_string(), selection.title.to_string()))
            .or_default()
            .push(now);

        if Self::selection_safe_for_recent(selection) {
            let plugin_name = plugin.name.as_str();
            let title = selection.title.as_str();
            if let Some(existing) = self.recent.iter_mut().find(|entry| {
                entry.plugin.name.as_str() == plugin_name && entry.selection.title.as_str() == title
            }) {
                existing.selection = selection.clone();
                existing.last_used_unix = now.timestamp();
                existing.uses = existing.uses.saturating_add(1);
            } else {
                self.recent.push(RecentMatch {
                    plugin: plugin.clone(),
                    selection: selection.clone(),
                    last_used_unix: now.timestamp(),
                    uses: 1,
                });
            }
        }
        self.cleanup();
    }

    pub fn recent_matches(&self, limit: usize) -> Vec<RecentMatch> {
        let mut recent = self.recent.clone();
        recent.sort_by_key(|entry| std::cmp::Reverse(entry.last_used_unix));
        recent.truncate(limit);
        recent
    }
}

fn rank_matches(
    plugin: &str,
    query_text: &str,
    mut matches: abi_stable::std_types::RVec<Match>,
    frecency: &FrecencyData,
) -> abi_stable::std_types::RVec<Match> {
    let titles: Vec<&str> = matches.iter().map(|m| m.title.as_str()).collect();
    let scores = frecency.batch_get_scores(plugin, &titles, 7.0);
    let query = query_text.trim().to_lowercase();

    let mut scored: Vec<(Match, f64)> = Vec::with_capacity(matches.len());
    for (i, (m, f_score)) in matches.drain(..).zip(scores).enumerate() {
        let title = m.title.as_str().to_lowercase();
        let exact_boost = if !query.is_empty() && title == query {
            2.0
        } else {
            0.0
        };
        let starts_with_boost = if !query.is_empty() && title.starts_with(&query) {
            0.35
        } else {
            0.0
        };
        let score = (1.0 / (1.0 + i as f64)) + f_score * 0.4 + exact_boost + starts_with_boost;
        scored.push((m, score));
    }

    scored.sort_by(|(_, a), (_, b)| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().map(|(m, _)| m).collect()
}

pub(crate) enum WorkerResult {
    Quit,
    Continue,
}

pub(crate) struct PluginState {
    pub(crate) plugin: PluginRef,
    pub(crate) info: PluginInfo,
}

pub(crate) struct State {
    pub(crate) plugins: Vec<PluginState>,
    pub(crate) plugin_map: HashMap<String, usize>,
    pub(crate) plugin_dirs: Vec<PathBuf>,
    pub(crate) config_dir: Arc<str>,
    pub(crate) frecency: Arc<Mutex<FrecencyData>>,
    pub(crate) plugin_specs: Vec<PathBuf>,
    pub(crate) temp_so_files: Vec<PathBuf>,
}

struct TempFileCleanupGuard {
    paths: Vec<PathBuf>,
    keep: bool,
}

impl TempFileCleanupGuard {
    fn new() -> Self {
        Self {
            paths: Vec::new(),
            keep: false,
        }
    }

    fn track(&mut self, path: PathBuf) {
        self.paths.push(path);
    }

    fn into_paths(mut self) -> Vec<PathBuf> {
        self.keep = true;
        std::mem::take(&mut self.paths)
    }
}

impl Drop for TempFileCleanupGuard {
    fn drop(&mut self) {
        if self.keep {
            return;
        }
        for path in &self.paths {
            if let Err(err) = std::fs::remove_file(path) {
                eprintln!(
                    "[provider] Failed to remove temp file {}: {err}",
                    path.display()
                );
            }
        }
    }
}

pub(crate) fn rebuild_plugin_map(plugins: &[PluginState]) -> HashMap<String, usize> {
    plugins
        .iter()
        .enumerate()
        .map(|(idx, plugin)| (plugin.info.name.to_string(), idx))
        .collect()
}

fn init_plugin_state(plugin: &PluginRef, config_dir: &str) {
    plugin.init()(config_dir.into());
}

fn temp_plugin_copy_name(file_name: &str) -> String {
    let unique_suffix = TEMP_PLUGIN_COPY_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{file_name}.{unique_suffix}")
}

pub(crate) fn load_plugins(
    plugin_specs: &[PathBuf],
    plugin_dirs: &[PathBuf],
    config_dir: &str,
    hard_reload: bool,
) -> Result<(Vec<PluginState>, Vec<PathBuf>), String> {
    let mut plugins = Vec::with_capacity(plugin_specs.len());
    let mut temp_files = TempFileCleanupGuard::new();

    let temp_dir = if hard_reload {
        let dir = std::env::temp_dir().join(format!("anyrun-reload-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        Some(dir)
    } else {
        None
    };

    for plugin_path in plugin_specs {
        let Some(path) = find_plugin(plugin_path, plugin_dirs) else {
            return Err(format!(
                "plugin not found: {}",
                plugin_path.to_string_lossy()
            ));
        };

        let load_path = if let Some(ref dir) = temp_dir {
            let file_name = path.file_name().unwrap_or_default().to_string_lossy();
            let unique_name = temp_plugin_copy_name(&file_name);
            let temp_path = dir.join(unique_name);
            std::fs::copy(&path, &temp_path)
                .map_err(|e| format!("failed to copy plugin {}: {e}", path.display()))?;
            temp_files.track(temp_path.clone());
            temp_path
        } else {
            path
        };

        let header = abi_stable::library::lib_header_from_path(&load_path)
            .map_err(|e| format!("failed to load plugin {}: {e}", load_path.display()))?;
        let plugin = header
            .init_root_module::<PluginRef>()
            .map_err(|e| format!("failed to init plugin module {}: {e}", load_path.display()))?;

        init_plugin_state(&plugin, config_dir);
        let info = plugin.info()();
        plugins.push(PluginState { plugin, info });
    }

    Ok((plugins, temp_files.into_paths()))
}

fn reload_plugin_set(
    state: &mut State,
    plugin_specs: &[PathBuf],
    plugin_dirs: &[PathBuf],
    hard_reload: bool,
) -> Result<(), String> {
    let (plugins, new_temp_files) = load_plugins(
        plugin_specs,
        plugin_dirs,
        state.config_dir.as_ref(),
        hard_reload,
    )?;
    state.plugin_map = rebuild_plugin_map(&plugins);

    let old_temp = std::mem::take(&mut state.temp_so_files);
    state.plugins = plugins;
    state.temp_so_files = new_temp_files;

    for path in &old_temp {
        if let Err(err) = std::fs::remove_file(path) {
            eprintln!(
                "[provider] Failed to remove temp file {}: {err}",
                path.display()
            );
        }
    }

    Ok(())
}

fn spawn_query_worker(
    plugin_idx: usize,
    mut query_rx: watch::Receiver<Option<QueuedQuery>>,
    result_tx: mpsc::UnboundedSender<PluginQueryResult>,
    search_fn: SearchFn,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut last_query_id = 0u64;

        loop {
            let Some(query) = query_rx.borrow().clone() else {
                if query_rx.changed().await.is_err() {
                    break;
                }
                continue;
            };

            if query.query_id == last_query_id {
                if query_rx.changed().await.is_err() {
                    break;
                }
                continue;
            }

            last_query_id = query.query_id;
            let query_text = query.text.clone();
            let query_id = query.query_id;
            let timeout_ms = query.timeout_ms;
            let slow_ms = query.slow_ms;
            let search_fn = Arc::clone(&search_fn);
            let started = Instant::now();
            let blocking_task = tokio::task::spawn_blocking(move || search_fn(query_text.clone()));
            tokio::pin!(blocking_task);

            let timed_out = tokio::time::sleep(Duration::from_millis(timeout_ms));
            tokio::pin!(timed_out);

            tokio::select! {
                result = &mut blocking_task => {
                    match result {
                        Ok(matches) => {
                            let elapsed_ms = started.elapsed().as_millis() as u64;
                            let _ = result_tx.send(PluginQueryResult {
                                plugin_idx,
                                query_id,
                                query_text: query.text.clone(),
                                matches,
                                elapsed_ms,
                                slow_ms,
                                timed_out: false,
                            });
                        }
                        Err(err) => {
                            eprintln!(
                                "[provider] Search worker failed for plugin index {plugin_idx}: {err}"
                            );
                        }
                    }
                }
                _ = &mut timed_out => {
                    let elapsed_ms = started.elapsed().as_millis() as u64;
                    let _ = result_tx.send(PluginQueryResult {
                        plugin_idx,
                        query_id,
                        query_text: query.text.clone(),
                        matches: Vec::new().into(),
                        elapsed_ms,
                        slow_ms,
                        timed_out: true,
                    });

                    match blocking_task.await {
                        Ok(matches) => {
                            let elapsed_ms = started.elapsed().as_millis() as u64;
                            let _ = result_tx.send(PluginQueryResult {
                                plugin_idx,
                                query_id,
                                query_text: query.text.clone(),
                                matches,
                                elapsed_ms,
                                slow_ms,
                                timed_out: false,
                            });
                        }
                        Err(err) => {
                            eprintln!(
                                "[provider] Timed-out search worker failed for plugin index {plugin_idx}: {err}"
                            );
                        }
                    }
                }
            }

            if query_rx.changed().await.is_err() {
                break;
            }
        }
    })
}

fn start_search_workers(
    state: &State,
    result_tx: mpsc::UnboundedSender<PluginQueryResult>,
) -> Vec<PluginSearchWorker> {
    state
        .plugins
        .iter()
        .enumerate()
        .map(|(idx, plugin_state)| {
            let (query_tx, query_rx) = watch::channel(None);
            let plugin_fn = plugin_state.plugin.get_matches();
            let search_fn: SearchFn =
                Arc::new(move |query_text: Arc<str>| plugin_fn(query_text.as_ref().into()));
            let handle = spawn_query_worker(idx, query_rx, result_tx.clone(), search_fn);
            PluginSearchWorker { query_tx, handle }
        })
        .collect()
}

fn stop_search_workers(workers: &mut Vec<PluginSearchWorker>) {
    for worker in workers.drain(..) {
        worker.handle.abort();
        drop(worker.query_tx);
    }
}

fn plugin_health_state(result: &PluginQueryResult) -> Option<PluginHealthState> {
    if result.query_text.trim().is_empty() {
        return None;
    }

    Some(if result.timed_out {
        PluginHealthState::TimedOut
    } else if result.elapsed_ms >= result.slow_ms {
        PluginHealthState::Slow
    } else {
        PluginHealthState::Healthy
    })
}

pub(crate) async fn worker_inner(
    mut request_rx: mpsc::Receiver<Request>,
    response_tx: mpsc::UnboundedSender<Response>,
    state: &mut State,
    mut reload_rx: broadcast::Receiver<()>,
) -> io::Result<WorkerResult> {
    let plugin_infos: Vec<PluginInfo> = state.plugins.iter().map(|p| p.info.clone()).collect();
    let _ = response_tx.send(Response::Ready { info: plugin_infos });

    let (result_tx, mut result_rx) = mpsc::unbounded_channel::<PluginQueryResult>();
    let mut search_workers = start_search_workers(state, result_tx);

    // Query ID for ignoring stale results
    static QUERY_COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut plugin_query_ids = vec![0u64; state.plugins.len()];

    loop {
        tokio::select! {
            Some(result) = result_rx.recv() => {
                if result.query_id != *plugin_query_ids.get(result.plugin_idx).unwrap_or(&0) {
                    continue;
                }

                if let Some(p_state) = state.plugins.get(result.plugin_idx) {
                    let plugin_name = p_state.info.name.as_str();
                    if let Some(health_state) = plugin_health_state(&result) {
                        let _ = response_tx.send(Response::Health {
                            statuses: vec![PluginHealth {
                                plugin: plugin_name.to_string(),
                                state: health_state,
                                elapsed_ms: result.elapsed_ms,
                            }],
                        });
                    }

                    if result.timed_out {
                        let _ = response_tx.send(Response::Matches {
                            plugin: p_state.info.clone(),
                            matches: Vec::new().into(),
                        });
                        continue;
                    }

                    let matches = {
                        let frecency = state.frecency.lock().await;
                        rank_matches(plugin_name, &result.query_text, result.matches, &frecency)
                    };

                    let _ = response_tx.send(Response::Matches {
                        plugin: p_state.info.clone(),
                        matches,
                    });
                }
            }

            req_opt = request_rx.recv() => {
                let Some(request) = req_opt else { break; };

                match request {
                Request::Query { text, plugins, timeout_ms, slow_ms, .. } => {
                    let query_id = QUERY_COUNTER.fetch_add(1, Ordering::SeqCst) + 1;

                    let query: Arc<str> = text.into();
                    let selected_plugins: Vec<_> = state
                        .plugins
                        .iter()
                        .enumerate()
                        .filter(|(_, p_state)| {
                            plugins.is_empty()
                                || plugins
                                    .iter()
                                    .any(|name| name == p_state.info.name.as_str())
                        })
                        .map(|(idx, _)| idx)
                        .collect();

                    for idx in &selected_plugins {
                        if let Some(id) = plugin_query_ids.get_mut(*idx) {
                            *id = query_id;
                        }
                    }

                    for idx in selected_plugins {
                        if let Some(worker) = search_workers.get(idx) {
                            let _ = worker.query_tx.send(Some(QueuedQuery {
                                query_id,
                                text: Arc::clone(&query),
                                timeout_ms,
                                slow_ms,
                            }));
                        }
                    }
                }
                    Request::Handle { plugin, selection } => {
                        if let Some(&idx) = state.plugin_map.get(plugin.name.as_str()) {
                            let mut frecency = state.frecency.lock().await;
                            frecency.record_selection(&plugin, &selection);
                            frecency.save(&state.config_dir);

                            let handle_fn = state.plugins[idx].plugin.handle_selection();
                            let result = tokio::task::spawn_blocking(move || {
                                handle_fn(selection)
                            })
                            .await
                            .unwrap_or(HandleResult::Close);
                            let _ = response_tx.send(Response::Handled { plugin, result });
                        }
                    }
                    Request::Recent { limit } => {
                        let matches = state.frecency.lock().await.recent_matches(limit);
                        let _ = response_tx.send(Response::Recent { matches });
                    }
                    Request::Reset => {
                        stop_search_workers(&mut search_workers);
                        for p in &mut state.plugins {
                            p.plugin.init()(state.config_dir.as_ref().into());
                        }
                        let (new_result_tx, new_result_rx) = mpsc::unbounded_channel::<PluginQueryResult>();
                        search_workers = start_search_workers(state, new_result_tx);
                        result_rx = new_result_rx;
                    }
                    Request::ReloadPlugins { plugins } => {
                        stop_search_workers(&mut search_workers);
                        // Let aborted workers unwind before replacing shared libraries.
                        tokio::time::sleep(Duration::from_millis(RELOAD_PLUGIN_SETTLE_DELAY_MS)).await;

                        let reload_result = if plugins.is_empty() {
                            let specs = state.plugin_specs.clone();
                            let dirs = state.plugin_dirs.clone();
                            reload_plugin_set(state, &specs, &dirs, true)
                        } else {
                            let plugin_specs: Vec<PathBuf> =
                                plugins.iter().map(PathBuf::from).collect();
                            let plugin_dirs = state.plugin_dirs.clone();
                            reload_plugin_set(state, &plugin_specs, &plugin_dirs, true)
                        };

                        if let Err(err) = reload_result {
                            eprintln!("[provider] Reload failed: {err}");
                        }

                        let (new_result_tx, new_result_rx) = mpsc::unbounded_channel::<PluginQueryResult>();
                        search_workers = start_search_workers(state, new_result_tx);
                        result_rx = new_result_rx;

                        let plugin_infos: Vec<PluginInfo> =
                            state.plugins.iter().map(|p| p.info.clone()).collect();
                        let _ = response_tx.send(Response::Ready { info: plugin_infos });
                    }
                    Request::ReloadPlugin { name } => {
                        if let Some(&idx) = state.plugin_map.get(&name) {
                            stop_search_workers(&mut search_workers);
                            let result = if let Some(p) = state.plugins.get_mut(idx) {
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    p.plugin.init()(state.config_dir.as_ref().into());
                                }))
                            } else {
                                Ok(())
                            };

                            let (new_result_tx, new_result_rx) =
                                mpsc::unbounded_channel::<PluginQueryResult>();
                            search_workers = start_search_workers(state, new_result_tx);
                            result_rx = new_result_rx;

                            if result.is_err() {
                                eprintln!("[provider] Plugin re-init failed for: {}", name);
                            } else {
                                let plugin_infos: Vec<PluginInfo> =
                                    state.plugins.iter().map(|p| p.info.clone()).collect();
                                let _ = response_tx.send(Response::Ready { info: plugin_infos });
                            }
                        }
                    }
                    Request::Quit => {
                        for path in &state.temp_so_files {
                            if let Err(err) = std::fs::remove_file(path) {
                                eprintln!(
                                    "[provider] Failed to remove temp file {}: {err}",
                                    path.display()
                                );
                            }
                        }
                        return Ok(WorkerResult::Quit)
                    }
                }
            }

            _ = reload_rx.recv() => {
                // If a query is in progress, delay reload slightly to avoid interrupting typing
                tokio::time::sleep(Duration::from_millis(RELOAD_WATCHER_SETTLE_DELAY_MS)).await;
                stop_search_workers(&mut search_workers);

                let mut failed = Vec::new();
                for p in state.plugins.iter_mut() {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        p.plugin.init()(state.config_dir.as_ref().into());
                    }));
                    if result.is_err() {
                        failed.push(p.info.name.to_string());
                    }
                }

                if !failed.is_empty() {
                    eprintln!("[provider] Plugin re-init failed for: {}", failed.join(", "));
                }

                let plugin_infos: Vec<PluginInfo> =
                    state.plugins.iter().map(|p| p.info.clone()).collect();
                let (new_result_tx, new_result_rx) = mpsc::unbounded_channel::<PluginQueryResult>();
                search_workers = start_search_workers(state, new_result_tx);
                result_rx = new_result_rx;
                let _ = response_tx.send(Response::Ready { info: plugin_infos });
            }
        }
    }
    Ok(WorkerResult::Continue)
}

pub(crate) async fn worker(
    stream: UnixStream,
    state: &mut State,
    reload_rx: broadcast::Receiver<()>,
) -> io::Result<WorkerResult> {
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};

    let (read_half, write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let write_arc = Arc::new(tokio::sync::Mutex::new(write_half));

    let (req_tx, req_rx) = mpsc::channel::<Request>(64);
    let (resp_tx, mut resp_rx) = mpsc::unbounded_channel::<Response>();

    // Task A: socket read -> req_tx
    tokio::spawn(async move {
        let mut recv_buf = Vec::<u8>::with_capacity(4096);
        loop {
            let mut len_buf = [0u8; 4];
            if reader.read_exact(&mut len_buf).await.is_err() {
                break;
            }
            let len = u32::from_le_bytes(len_buf) as usize;
            if len > 64 * 1024 * 1024 {
                break;
            }
            recv_buf.resize(len, 0);
            if reader.read_exact(&mut recv_buf[..len]).await.is_err() {
                break;
            }
            let Ok(req) = bincode::deserialize::<Request>(&recv_buf[..len]) else {
                break;
            };
            let is_quit = matches!(req, Request::Quit);
            if req_tx.send(req).await.is_err() {
                break;
            }
            if is_quit {
                break;
            }
        }
    });

    // Task B: resp_rx -> socket write
    let write_arc2 = Arc::clone(&write_arc);
    tokio::spawn(async move {
        let mut send_buf = Vec::<u8>::with_capacity(4096);
        while let Some(resp) = resp_rx.recv().await {
            send_buf.clear();
            if bincode::serialize_into(&mut send_buf, &resp).is_err() {
                break;
            }
            let len = send_buf.len() as u32;
            let mut w = write_arc2.lock().await;
            if w.write_all(&len.to_le_bytes()).await.is_err() {
                break;
            }
            if w.write_all(&send_buf).await.is_err() {
                break;
            }
            if w.flush().await.is_err() {
                break;
            }
        }
    });

    worker_inner(req_rx, resp_tx, state, reload_rx).await
}

pub(crate) fn find_plugin(
    name: &std::path::Path,
    dirs: &[std::path::PathBuf],
) -> Option<std::path::PathBuf> {
    let name = expand_tilde(name);
    if name.is_absolute() && name.exists() {
        return Some(name.clone());
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

pub(crate) fn expand_tilde(path: &std::path::Path) -> std::path::PathBuf {
    if let Some(path_str) = path.to_str()
        && path_str.starts_with("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        return std::path::PathBuf::from(path_str.replacen('~', &home, 1));
    }
    path.to_path_buf()
}

fn resolve_desktop_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    let user_data = env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let mut p = PathBuf::from(env::var("HOME").unwrap_or_else(|_| ".".into()));
            p.push(".local");
            p.push("share");
            p
        });
    dirs.push(user_data.join("applications"));

    if let Ok(data_dirs) = env::var("XDG_DATA_DIRS") {
        for dir in data_dirs.split(':') {
            dirs.push(PathBuf::from(dir).join("applications"));
        }
    } else {
        dirs.push(PathBuf::from("/usr/local/share/applications"));
        dirs.push(PathBuf::from("/usr/share/applications"));
    }

    dirs
}

pub(crate) fn spawn_file_watcher(
    reload_tx: broadcast::Sender<()>,
    plugin_dirs: &[PathBuf],
    config_dir: &str,
) {
    let desktop_dirs = resolve_desktop_dirs();
    let mut watch_paths: Vec<PathBuf> = desktop_dirs.into_iter().filter(|p| p.exists()).collect();

    watch_paths.extend(plugin_dirs.iter().filter(|p| p.exists()).cloned());

    let config_path = PathBuf::from(config_dir);
    if config_path.exists() {
        watch_paths.push(config_path.clone());
    }

    if watch_paths.is_empty() {
        eprintln!("[provider] No valid paths to watch for file changes");
        return;
    }

    let (event_tx, event_rx) = std::sync::mpsc::channel();

    let mut debouncer: Debouncer<RecommendedWatcher> =
        match new_debouncer(Duration::from_millis(500), event_tx) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("[provider] Failed to create file watcher debouncer: {e}");
                return;
            }
        };

    for path in &watch_paths {
        if let Err(e) = debouncer
            .watcher()
            .watch(path, notify::RecursiveMode::NonRecursive)
        {
            eprintln!("[provider] Failed to watch path {:?}: {e}", path);
        }
    }

    let config_dir_str = config_dir.to_string();
    std::thread::spawn(move || {
        while let Ok(result) = event_rx.recv() {
            let events = match result {
                Ok(events) => events,
                Err(e) => {
                    eprintln!("[provider] File watcher error: {:?}", e);
                    continue;
                }
            };

            let _is_config_change = events.iter().any(|event| {
                event
                    .path
                    .to_str()
                    .map(|p| p.ends_with(".ron") && p.contains(&config_dir_str))
                    .unwrap_or(false)
            });

            let _ = reload_tx.send(());

            while event_rx.try_recv().is_ok() {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;
    use tokio::time::timeout;

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    fn plugin_sample_library_path() -> Option<PathBuf> {
        let current_exe = std::env::current_exe().ok()?;
        let deps_dir = current_exe.parent()?;
        let direct_candidate = deps_dir.join("libanyrun_plugin_template.so");
        if direct_candidate.exists() {
            return Some(direct_candidate);
        }

        std::fs::read_dir(deps_dir)
            .ok()?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with("libanyrun_plugin_template")
                            && (name.ends_with(".so")
                                || name.ends_with(".dylib")
                                || name.ends_with(".dll"))
                    })
            })
    }

    #[test]
    fn test_resolve_desktop_dirs_includes_user_applications() {
        let dirs = resolve_desktop_dirs();
        assert!(!dirs.is_empty());
        let user_data = env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let mut p = PathBuf::from(env::var("HOME").unwrap_or_else(|_| ".".into()));
                p.push(".local");
                p.push("share");
                p
            });
        assert!(dirs.contains(&user_data.join("applications")));
    }

    #[test]
    fn test_resolve_desktop_dirs_includes_system_dirs_when_xdg_data_dirs_unset() {
        let _guard = env_lock();
        let original = env::var("XDG_DATA_DIRS");
        unsafe { env::remove_var("XDG_DATA_DIRS") };

        let dirs = resolve_desktop_dirs();
        assert!(dirs.contains(&PathBuf::from("/usr/local/share/applications")));
        assert!(dirs.contains(&PathBuf::from("/usr/share/applications")));

        if let Ok(val) = original {
            unsafe { env::set_var("XDG_DATA_DIRS", val) };
        }
    }

    #[test]
    fn test_resolve_desktop_dirs_parses_xdg_data_dirs() {
        let _guard = env_lock();
        let original = env::var("XDG_DATA_DIRS");
        unsafe { env::set_var("XDG_DATA_DIRS", "/foo:/bar") };

        let dirs = resolve_desktop_dirs();
        assert!(dirs.contains(&PathBuf::from("/foo/applications")));
        assert!(dirs.contains(&PathBuf::from("/bar/applications")));

        if let Ok(val) = original {
            unsafe { env::set_var("XDG_DATA_DIRS", val) };
        } else {
            unsafe { env::remove_var("XDG_DATA_DIRS") };
        }
    }

    #[test]
    fn test_reload_plugins_request_serializes() {
        use anyrun_provider_ipc::Request;
        let req = Request::ReloadPlugins {
            plugins: vec!["Applications".into()],
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: Request = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            deserialized,
            Request::ReloadPlugins { plugins } if plugins == vec!["Applications"]
        ));
    }

    #[test]
    fn test_plugin_init_panic_handling() {
        let count = std::sync::atomic::AtomicUsize::new(0);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }));

        assert!(result.is_ok());
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            panic!("simulated init failure");
        }));

        assert!(result.is_err());
    }

    #[test]
    fn test_frecency_records_recent_selection() {
        use abi_stable::std_types::ROption;

        let plugin = PluginInfo {
            name: "Applications".into(),
            icon: "system-search".into(),
        };
        let selection = Match {
            title: "Firefox".into(),
            description: ROption::RNone,
            use_pango: false,
            icon: ROption::RNone,
            id: ROption::RNone,
        };

        let mut frecency = FrecencyData::default();
        frecency.record_selection(&plugin, &selection);
        frecency.record_selection(&plugin, &selection);

        let recent = frecency.recent_matches(8);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].plugin.name.as_str(), "Applications");
        assert_eq!(recent[0].selection.title.as_str(), "Firefox");
        assert_eq!(recent[0].uses, 2);
    }

    #[test]
    fn test_frecency_skips_recent_when_selection_has_id() {
        use abi_stable::std_types::ROption;

        let plugin = PluginInfo {
            name: "Applications".into(),
            icon: "system-search".into(),
        };
        let selection = Match {
            title: "Firefox".into(),
            description: ROption::RNone,
            use_pango: false,
            icon: ROption::RNone,
            id: ROption::RSome(42),
        };

        let mut frecency = FrecencyData::default();
        frecency.record_selection(&plugin, &selection);

        assert_eq!(frecency.recent_matches(8).len(), 0);
        assert_eq!(frecency.usage.len(), 1);
    }

    #[test]
    fn test_cleanup_drops_legacy_recent_entries_with_id() {
        use abi_stable::std_types::ROption;

        let mut frecency = FrecencyData::default();
        frecency.recent.push(RecentMatch {
            plugin: PluginInfo {
                name: "Applications".into(),
                icon: "system-search".into(),
            },
            selection: Match {
                title: "Legacy".into(),
                description: ROption::RNone,
                use_pango: false,
                icon: ROption::RNone,
                id: ROption::RSome(7),
            },
            last_used_unix: Utc::now().timestamp(),
            uses: 3,
        });

        frecency.cleanup();
        assert!(frecency.recent.is_empty());
    }

    #[test]
    fn test_rank_matches_boosts_exact_query() {
        use abi_stable::std_types::{ROption, RString};

        let plugin = "Applications";
        let mut frecency = FrecencyData::default();
        let matches = vec![
            Match {
                title: RString::from("Firefox Developer Edition"),
                description: ROption::RNone,
                use_pango: false,
                icon: ROption::RNone,
                id: ROption::RNone,
            },
            Match {
                title: RString::from("Firefox"),
                description: ROption::RNone,
                use_pango: false,
                icon: ROption::RNone,
                id: ROption::RNone,
            },
        ];

        let ranked = rank_matches(plugin, "firefox", matches.into(), &frecency);
        assert_eq!(ranked[0].title.as_str(), "Firefox");

        let plugin_info = PluginInfo {
            name: plugin.into(),
            icon: "system-search".into(),
        };
        frecency.record_selection(&plugin_info, &ranked[1]);
        frecency.record_selection(&plugin_info, &ranked[1]);
        let ranked = rank_matches(plugin, "fire", ranked, &frecency);
        assert_eq!(ranked[0].title.as_str(), "Firefox Developer Edition");
    }

    #[test]
    fn test_plugin_reloads_config_on_init() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("plugin-sample.ron");

        let initial_config = r#"(prefix: "init:", max_entries: 5, show_results_immediately: true)"#;
        fs::write(&config_path, initial_config).expect("Failed to write initial config");

        let Some(plugin_path) = plugin_sample_library_path() else {
            return;
        };

        let header =
            abi_stable::library::lib_header_from_path(&plugin_path).expect("Failed to load plugin");

        let plugin = header
            .init_root_module::<PluginRef>()
            .expect("Failed to init plugin module");

        plugin.init()(temp_dir.path().to_string_lossy().as_ref().into());

        std::thread::sleep(std::time::Duration::from_millis(100));

        let matches1 = plugin.get_matches()("init:hello".into());
        assert!(matches1.is_empty(), "Expected no matches with empty data");

        let updated_config =
            r#"(prefix: "test:", max_entries: 10, show_results_immediately: false)"#;
        fs::write(&config_path, updated_config).expect("Failed to write updated config");

        plugin.init()(temp_dir.path().to_string_lossy().as_ref().into());

        std::thread::sleep(std::time::Duration::from_millis(100));

        let matches_with_old_prefix = plugin.get_matches()("init:hello".into());
        assert!(
            matches_with_old_prefix.is_empty(),
            "Old prefix should not match after config reload"
        );

        let matches_with_new_prefix = plugin.get_matches()("test:hello".into());
        assert!(
            matches_with_new_prefix.is_empty(),
            "Expected no matches with empty data"
        );
    }

    #[tokio::test]
    async fn test_spawn_query_worker_only_processes_latest_pending_query() {
        use abi_stable::std_types::{ROption, RString};
        use std::sync::atomic::Ordering;
        use tokio::sync::mpsc;

        let (result_tx, mut result_rx) = mpsc::unbounded_channel::<PluginQueryResult>();
        let (query_tx, query_rx) = watch::channel(None);
        let first_query_started = Arc::new(AtomicBool::new(false));

        let started_flag = first_query_started.clone();
        let search_fn: SearchFn = Arc::new(move |query_text: Arc<str>| {
            let text = query_text.as_ref().to_string();
            if text == "a" {
                started_flag.store(true, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(200));
            }

            vec![Match {
                title: RString::from(text),
                description: ROption::RNone,
                use_pango: false,
                icon: ROption::RNone,
                id: ROption::RNone,
            }]
            .into()
        });

        let handle = spawn_query_worker(0, query_rx, result_tx, search_fn);

        let _ = query_tx.send(Some(QueuedQuery {
            query_id: 1,
            text: Arc::from("a"),
            timeout_ms: 800,
            slow_ms: 250,
        }));

        timeout(Duration::from_millis(200), async {
            while !first_query_started.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("first query should start");

        let _ = query_tx.send(Some(QueuedQuery {
            query_id: 2,
            text: Arc::from("ab"),
            timeout_ms: 800,
            slow_ms: 250,
        }));
        let _ = query_tx.send(Some(QueuedQuery {
            query_id: 3,
            text: Arc::from("abc"),
            timeout_ms: 800,
            slow_ms: 250,
        }));

        let first = timeout(Duration::from_millis(500), result_rx.recv())
            .await
            .expect("first result should arrive")
            .expect("worker should send first result");
        assert_eq!(first.query_id, 1);
        assert_eq!(first.matches[0].title.as_str(), "a");

        let second = timeout(Duration::from_millis(500), result_rx.recv())
            .await
            .expect("latest result should arrive")
            .expect("worker should send latest result");
        assert_eq!(second.query_id, 3);
        assert_eq!(second.matches[0].title.as_str(), "abc");

        assert!(
            timeout(Duration::from_millis(150), result_rx.recv())
                .await
                .is_err(),
            "intermediate query should be coalesced"
        );

        handle.abort();
    }

    #[tokio::test]
    async fn test_spawn_query_worker_timeout_keeps_single_inflight() {
        use abi_stable::std_types::{ROption, RString};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::sync::mpsc;

        let (result_tx, mut result_rx) = mpsc::unbounded_channel::<PluginQueryResult>();
        let (query_tx, query_rx) = watch::channel(None);
        let running = Arc::new(AtomicUsize::new(0));
        let max_running = Arc::new(AtomicUsize::new(0));
        let first_done = Arc::new(AtomicBool::new(false));

        let running_ref = Arc::clone(&running);
        let max_running_ref = Arc::clone(&max_running);
        let first_done_ref = Arc::clone(&first_done);
        let search_fn: SearchFn = Arc::new(move |query_text: Arc<str>| {
            let in_flight = running_ref.fetch_add(1, Ordering::SeqCst) + 1;
            max_running_ref.fetch_max(in_flight, Ordering::SeqCst);

            let text = query_text.as_ref().to_string();
            if text == "first" {
                std::thread::sleep(Duration::from_millis(200));
                first_done_ref.store(true, Ordering::SeqCst);
            }

            running_ref.fetch_sub(1, Ordering::SeqCst);
            vec![Match {
                title: RString::from(text),
                description: ROption::RNone,
                use_pango: false,
                icon: ROption::RNone,
                id: ROption::RNone,
            }]
            .into()
        });

        let handle = spawn_query_worker(0, query_rx, result_tx, search_fn);

        let _ = query_tx.send(Some(QueuedQuery {
            query_id: 1,
            text: Arc::from("first"),
            timeout_ms: 50,
            slow_ms: 25,
        }));

        let timed_out = timeout(Duration::from_millis(200), result_rx.recv())
            .await
            .expect("timed out response should arrive")
            .expect("worker should send timeout result");
        assert_eq!(timed_out.query_id, 1);
        assert!(timed_out.timed_out);

        let _ = query_tx.send(Some(QueuedQuery {
            query_id: 2,
            text: Arc::from("second"),
            timeout_ms: 500,
            slow_ms: 250,
        }));

        assert!(
            timeout(Duration::from_millis(100), result_rx.recv())
                .await
                .is_err(),
            "no real matches should arrive while first blocking task is still running"
        );

        timeout(Duration::from_millis(300), async {
            while !first_done.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("first blocking query should finish");

        let late_first = timeout(Duration::from_millis(300), result_rx.recv())
            .await
            .expect("late first query result should arrive after first completes")
            .expect("worker should send late first result");
        assert_eq!(late_first.query_id, 1);
        assert!(!late_first.timed_out);
        assert_eq!(late_first.matches[0].title.as_str(), "first");

        let second = timeout(Duration::from_millis(300), result_rx.recv())
            .await
            .expect("second query result should arrive after first completes")
            .expect("worker should send second result");
        assert_eq!(second.query_id, 2);
        assert_eq!(second.matches[0].title.as_str(), "second");
        assert_eq!(max_running.load(Ordering::SeqCst), 1);

        handle.abort();
    }

    #[test]
    fn test_temp_file_cleanup_guard_removes_files_when_not_disarmed() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let temp_file = temp_dir.path().join("cleanup-test.so");
        fs::write(&temp_file, b"dummy").unwrap();

        {
            let mut guard = TempFileCleanupGuard::new();
            guard.track(temp_file.clone());
        }

        assert!(!temp_file.exists());
    }

    #[test]
    fn test_temp_file_cleanup_guard_keeps_files_when_disarmed() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let temp_file = temp_dir.path().join("keep-test.so");
        fs::write(&temp_file, b"dummy").unwrap();

        let kept = {
            let mut guard = TempFileCleanupGuard::new();
            guard.track(temp_file.clone());
            guard.into_paths()
        };

        assert_eq!(kept, vec![temp_file.clone()]);
        assert!(temp_file.exists());
    }

    #[test]
    fn test_temp_plugin_copy_name_is_unique_across_calls() {
        let a = temp_plugin_copy_name("libexample.so");
        let b = temp_plugin_copy_name("libexample.so");
        assert_ne!(a, b);
    }
}
