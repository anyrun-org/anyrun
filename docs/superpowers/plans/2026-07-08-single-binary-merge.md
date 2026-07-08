# Single Binary Merge: anyrun + anyrun-provider Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Hợp nhất `anyrun` và `anyrun-provider` thành một binary duy nhất bằng cách chuyển provider logic thành một library crate, sau đó link in-process, thay thế toàn bộ Unix socket IPC bằng typed in-memory channels.

**Architecture:** Provider logic (`anyrun-provider/src/main.rs`) được tách thành hai phần: `lib.rs` (core engine — public API) và `main.rs` (thin CLI wrapper, giữ standalone binary cho backward compat). `anyrun` thêm dependency vào `anyrun-provider` dưới dạng lib, khởi chạy provider engine trên một luồng tokio riêng, và giao tiếp qua `tokio::sync::mpsc` thay vì Unix socket.

**Tech Stack:** Rust 2024 edition, tokio 1.48 (đã có ở cả hai crates), `anyrun-provider-ipc::Request/Response` (tái dùng làm typed channel messages), relm4 `sender.emit()`, `std::thread::spawn` để isolate tokio runtime.

## Global Constraints

- **Không dùng `thiserror` hay `anyhow`** — lỗi qua `eprintln!("[name] {}", err)`
- **Không thay đổi plugin interface** — `anyrun-plugin`, `anyrun-macros`, mọi plugin `.so` không bị ảnh hưởng
- **Backward compat**: `anyrun-provider` vẫn build được như một standalone binary (`just build bin` vẫn ra cả 2 binary)
- **Config backward compat**: field `provider:` trong `config.ron` được giữ lại trong struct (annotate `#[serde(default)]`, bỏ qua runtime) để user cũ không bị parse error
- **Rust edition**: `anyrun` dùng `2021`, `anyrun-provider` dùng `2024` — không thay đổi
- **Tests phải xanh**: `cargo test --workspace` phải pass trước và sau từng commit
- **Commit conventions**: Conventional Commits — `feat(provider)`, `refactor(anyrun)`, `chore(build)`, etc.

---

## File Structure (After Change)

**Files được tạo mới:**
- `anyrun-provider/src/engine.rs` — core logic tách từ `main.rs`: tất cả structs, fns nội bộ
- `anyrun-provider/src/lib.rs` — public API: `ProviderConfig`, `ProviderHandle`, `spawn_provider_thread()`
- `anyrun/src/provider_inproc.rs` — (optional) hoặc thêm `worker_inproc()` vào `anyrun/src/provider.rs`

**Files được sửa:**
- `anyrun-provider/Cargo.toml` — thêm `[lib]` target
- `anyrun-provider/src/main.rs` — thin wrapper gọi vào engine
- `anyrun/Cargo.toml` — thêm dep `anyrun-provider = { path = "../anyrun-provider" }`
- `anyrun/src/provider.rs` — xóa `worker_spawn` / `build_provider_command`; thêm `worker_inproc`
- `anyrun/src/app/init.rs` — cập nhật nhánh `worker_spawn` → `worker_inproc`
- `anyrun/src/daemon.rs` — xóa subprocess spawn logic
- `anyrun/src/app/types.rs` — xóa `socket_path` khỏi `DaemonContext`
- `anyrun/src/config/mod.rs` — deprecate `provider` field
- `justfile` — cập nhật `core_pkgs`, `install`, `run`, `daemon`
- `PKGBUILD` — xóa install của `anyrun-provider` binary
- `nix/packages/anyrun.nix` — xóa `wrapProgram` với anyrun-provider PATH
- `nix/modules/home-manager.nix` — xóa `config.provider` option, xóa assertion

**Files không thay đổi:**
- `anyrun-provider/anyrun-provider-ipc/` — giữ nguyên
- `anyrun-provider/tests/ipc_e2e.rs` — vẫn test standalone binary path
- Tất cả plugin crates

---

## Task 1: Tách provider engine thành lib.rs + engine.rs

**Mục tiêu:** `anyrun-provider` vừa là `[[bin]]` vừa là `[lib]`, expose public API để `anyrun` gọi trực tiếp in-process.

**Files:**
- Tạo: `anyrun-provider/src/engine.rs`
- Tạo: `anyrun-provider/src/lib.rs`
- Sửa: `anyrun-provider/src/main.rs`
- Sửa: `anyrun-provider/Cargo.toml`

**Interfaces:**
- Produces:
  ```rust
  // anyrun_provider::lib.rs — public API
  pub struct ProviderConfig {
      pub config_dir: String,
      pub plugin_specs: Vec<PathBuf>,
  }

  pub struct ProviderHandle {
      pub request_tx: tokio::sync::mpsc::Sender<anyrun_provider_ipc::Request>,
  }

  pub fn spawn_provider_thread(
      config: ProviderConfig,
      response_tx: relm4::Sender<anyrun_provider_ipc::Response>,
  ) -> ProviderHandle
  ```

- [ ] **Step 1: Thêm `[lib]` target vào `anyrun-provider/Cargo.toml`**

  Thêm vào cuối file (sau `[dev-dependencies]`):
  ```toml
  [[bin]]
  name = "anyrun-provider"
  path = "src/main.rs"

  [lib]
  name = "anyrun_provider"
  path = "src/lib.rs"
  ```

- [ ] **Step 2: Verify Cargo.toml thay đổi đúng**

  Run: `cargo build -p anyrun-provider 2>&1 | head -10`
  Expected: BUILD FAIL vì `src/lib.rs` chưa tồn tại — xác nhận Cargo.toml parse OK (không có TOML error)

- [ ] **Step 3: Tạo `anyrun-provider/src/engine.rs` — tách core logic từ `main.rs`**

  Tạo file mới `anyrun-provider/src/engine.rs` với nội dung là toàn bộ `main.rs` hiện tại, **ngoại trừ**:
  - Bỏ `use clap::{Parser, Subcommand};`
  - Bỏ `struct Args`, `enum Command` (các type CLI-specific)
  - Bỏ `fn main()`
  - Bỏ `#[tokio::main]`

  Đổi visibility của các items cần dùng từ `lib.rs` và `main.rs`:
  ```rust
  pub(crate) struct State { ... }
  pub(crate) struct PluginState { ... }
  pub(crate) struct FrecencyData { ... }
  pub(crate) struct PluginQueryResult { ... }
  pub(crate) enum WorkerResult { Quit, Continue }
  pub(crate) fn load_plugins(...) -> Result<(Vec<PluginState>, Vec<PathBuf>), String>
  pub(crate) fn rebuild_plugin_map(...) -> HashMap<String, usize>
  pub(crate) fn spawn_file_watcher(...)
  pub(crate) fn expand_tilde(...) -> PathBuf
  pub(crate) fn find_plugin(...) -> Option<PathBuf>
  // Giữ tất cả các fn khác pub(crate)
  ```

  Giữ nguyên tất cả `#[cfg(test)]` modules từ `main.rs` gốc.

- [ ] **Step 4: Tạo `anyrun-provider/src/lib.rs` với public API**

  ```rust
  // anyrun-provider/src/lib.rs
  pub(crate) mod engine;

  use std::path::PathBuf;
  use std::sync::Arc;

  use anyrun_provider_ipc::{CONFIG_DIRS, PLUGIN_PATHS, Request, Response};
  use engine::{FrecencyData, State, WorkerResult, load_plugins, rebuild_plugin_map, spawn_file_watcher};
  use relm4::Sender;
  use tokio::sync::{broadcast, mpsc};

  pub struct ProviderConfig {
      pub config_dir: String,
      pub plugin_specs: Vec<PathBuf>,
  }

  pub struct ProviderHandle {
      pub request_tx: mpsc::Sender<Request>,
  }

  /// Spawns the full provider engine in a dedicated OS thread (with its own current_thread tokio runtime).
  /// Results are forwarded to `response_tx` via `sender.emit()`, which is thread-safe in relm4.
  pub fn spawn_provider_thread(
      config: ProviderConfig,
      response_tx: Sender<Response>,
  ) -> ProviderHandle {
      let (request_tx, request_rx) = mpsc::channel::<Request>(64);

      std::thread::spawn(move || {
          tokio::runtime::Builder::new_current_thread()
              .enable_all()
              .build()
              .expect("[provider] failed to build tokio runtime")
              .block_on(run_engine(config, request_rx, response_tx));
      });

      ProviderHandle { request_tx }
  }

  async fn run_engine(
      config: ProviderConfig,
      request_rx: mpsc::Receiver<Request>,
      response_tx: Sender<Response>,
  ) {
      use std::env;

      let config_dir: Arc<str> = config.config_dir.clone().into();

      let user_dir = env::var("XDG_CONFIG_HOME")
          .map(PathBuf::from)
          .unwrap_or_else(|_| {
              let mut p = PathBuf::from(env::var("HOME").unwrap_or_else(|_| ".".into()));
              p.push(".config");
              p
          })
          .join("anyrun");

      let mut plugin_dirs = vec![user_dir.join("plugins")];
      if let Ok(path) = env::var("ANYRUN_PLUGINS") {
          plugin_dirs.push(PathBuf::from(path));
      }
      plugin_dirs.extend(PLUGIN_PATHS.iter().map(PathBuf::from));

      let mut frecency = FrecencyData::load(&config_dir);
      frecency.cleanup();
      let frecency = Arc::new(tokio::sync::Mutex::new(frecency));

      let (initial_plugins, initial_temp_files) =
          match load_plugins(&config.plugin_specs, &plugin_dirs, &config_dir, false) {
              Ok(result) => result,
              Err(err) => {
                  eprintln!("[provider] Failed to load plugins: {err}");
                  return;
              }
          };

      let mut state = State {
          plugin_map: rebuild_plugin_map(&initial_plugins),
          plugins: initial_plugins,
          plugin_dirs: plugin_dirs.clone(),
          config_dir: config_dir.clone(),
          frecency,
          plugin_specs: config.plugin_specs.clone(),
          temp_so_files: initial_temp_files,
      };

      let (reload_tx, reload_rx) = broadcast::channel::<()>(4);
      spawn_file_watcher(reload_tx.clone(), &plugin_dirs, &config_dir);

      // Use Task 2's worker_inner() here (after Task 2 is complete).
      // For now, bridge via in-process socket pair as placeholder:
      let (engine_stream, client_stream) = tokio::net::UnixStream::pair()
          .expect("[provider] failed to create socket pair");

      // Relay: forward Request from mpsc → client_stream socket (bincode-framed)
      tokio::spawn(async move {
          use anyrun_provider_ipc::Socket;
          let mut socket = Socket::new(client_stream);
          let mut rx = request_rx;
          while let Some(req) = rx.recv().await {
              let is_quit = matches!(req, Request::Quit);
              if let Err(e) = socket.send(&req).await {
                  eprintln!("[provider] relay send error: {e}");
                  break;
              }
              if is_quit { break; }
          }
      });

      // Read responses from engine_stream socket → response_tx.emit()
      // worker() writes to its socket; we read from the other half.
      // This is replaced by worker_inner() in Task 2.
      if let Err(e) = engine::worker(engine_stream, &mut state, reload_rx).await {
          eprintln!("[provider] engine error: {e}");
      }
  }
  ```

  **Lưu ý**: `run_engine` ở Task 1 dùng `UnixStream::pair()` như bridge tạm thời. Task 2 sẽ refactor `engine::worker()` thành `engine::worker_inner()` với typed channels — lúc đó xóa bridge này.

- [ ] **Step 5: Cập nhật `anyrun-provider/src/main.rs` thành thin wrapper**

  Thay toàn bộ `main.rs` bằng version import từ `engine`:
  ```rust
  // anyrun-provider/src/main.rs
  mod engine;

  use engine::{
      FrecencyData, State, WorkerResult, load_plugins, rebuild_plugin_map, spawn_file_watcher,
      worker,
  };
  use anyrun_provider_ipc::{CONFIG_DIRS, PLUGIN_PATHS};
  use clap::{Parser, Subcommand};
  use std::path::PathBuf;
  use std::sync::Arc;
  use tokio::net::{UnixListener, UnixStream};
  use tokio::sync::{Mutex, broadcast};
  use std::env;

  #[derive(Parser)]
  #[command(version)]
  struct Args {
      #[command(subcommand)]
      command: Command,
      #[arg(short, long)]
      plugins: Vec<PathBuf>,
      #[arg(short, long)]
      config_dir: Option<String>,
  }

  #[derive(Clone, Subcommand)]
  enum Command {
      Socket { path: PathBuf },
      ConnectTo { path: PathBuf },
  }

  #[tokio::main]
  async fn main() -> std::io::Result<()> {
      let args = Args::parse();

      let user_dir = env::var("XDG_CONFIG_HOME")
          .map(PathBuf::from)
          .unwrap_or_else(|_| {
              let mut p = PathBuf::from(env::var("HOME").unwrap_or_else(|_| ".".into()));
              p.push(".config");
              p
          })
          .join("anyrun");

      let config_dir: Arc<str> = args.config_dir.map(Into::into).unwrap_or_else(|| {
          if user_dir.exists() {
              user_dir.to_string_lossy().into()
          } else {
              CONFIG_DIRS
                  .iter()
                  .find(|p| PathBuf::from(p).exists())
                  .map(|&p| p.into())
                  .unwrap_or_else(|| CONFIG_DIRS[0].into())
          }
      });

      let mut frecency = FrecencyData::load(&config_dir);
      frecency.cleanup();
      let frecency = Arc::new(Mutex::new(frecency));

      let mut plugin_dirs = vec![user_dir.join("plugins")];
      if let Ok(path) = env::var("ANYRUN_PLUGINS") {
          plugin_dirs.push(PathBuf::from(path));
      }
      plugin_dirs.extend(PLUGIN_PATHS.iter().map(PathBuf::from));

      let (initial_plugins, initial_temp_files) =
          load_plugins(&args.plugins, &plugin_dirs, &config_dir, false)
              .map_err(std::io::Error::other)?;

      let mut state = State {
          plugin_map: rebuild_plugin_map(&initial_plugins),
          plugins: initial_plugins,
          plugin_dirs: plugin_dirs.clone(),
          config_dir: config_dir.clone(),
          frecency,
          plugin_specs: args.plugins.clone(),
          temp_so_files: initial_temp_files,
      };

      let (reload_tx, _) = broadcast::channel::<()>(4);
      spawn_file_watcher(reload_tx.clone(), &plugin_dirs, &config_dir);

      match args.command {
          Command::Socket { path } => {
              let _ = std::fs::remove_file(&path);
              let listener = UnixListener::bind(path)?;
              loop {
                  let (stream, _) = listener.accept().await?;
                  let reload_rx = reload_tx.subscribe();
                  if let WorkerResult::Quit = worker(stream, &mut state, reload_rx).await? {
                      break;
                  }
              }
          }
          Command::ConnectTo { path } => {
              let stream = UnixStream::connect(path).await?;
              let reload_rx = reload_tx.subscribe();
              worker(stream, &mut state, reload_rx).await?;
          }
      }
      Ok(())
  }
  ```

- [ ] **Step 6: Build và test**

  Run: `cargo build -p anyrun-provider`
  Expected: PASS

  Run: `cargo test -p anyrun-provider`
  Expected: PASS — tất cả tests trong `engine.rs` và `tests/ipc_e2e.rs` xanh

- [ ] **Step 7: Commit**

  ```bash
  git add anyrun-provider/Cargo.toml anyrun-provider/src/engine.rs anyrun-provider/src/lib.rs anyrun-provider/src/main.rs
  git commit -m "refactor(provider): extract engine into lib crate, keep thin binary wrapper"
  ```

---

## Task 2: Refactor `worker()` thành `worker_inner()` với typed channels

**Mục tiêu:** Xóa bỏ `UnixStream::pair()` bridge tạm thời. `worker_inner()` nhận `mpsc::Receiver<Request>` + `mpsc::UnboundedSender<Response>` trực tiếp. `worker()` cũ trở thành thin wrapper tạo socket bridge cho standalone binary.

**Files:**
- Sửa: `anyrun-provider/src/engine.rs`
- Sửa: `anyrun-provider/src/lib.rs`

**Interfaces:**
- Consumes từ Task 1: `engine::State`, `engine::load_plugins`, `engine::spawn_file_watcher`
- Produces:
  ```rust
  // engine.rs
  pub(crate) async fn worker_inner(
      mut request_rx: mpsc::Receiver<Request>,
      response_tx: mpsc::UnboundedSender<Response>,
      state: &mut State,
      mut reload_rx: broadcast::Receiver<()>,
  ) -> io::Result<WorkerResult>

  pub(crate) async fn worker(
      stream: UnixStream,   // giữ nguyên cho standalone binary
      state: &mut State,
      reload_rx: broadcast::Receiver<()>,
  ) -> io::Result<WorkerResult>
  ```

- [ ] **Step 1: Thêm `worker_inner()` vào `engine.rs`**

  Tạo `worker_inner()` bằng cách copy `worker()` hiện tại và thay thế:
  - Xóa `let mut socket = Socket::new(stream);`
  - Xóa dòng `socket.send(&Response::Ready {...}).await?;`
  - Thay bằng: `response_tx.send(Response::Ready { info: plugin_infos }).ok();`
  - Thay mọi `socket.send(&resp).await?;` → `response_tx.send(resp).ok();`
  - Thay `socket.recv()` branch:
    ```rust
    // Trước:
    req_result = socket.recv() => {
        let request = match req_result {
            Ok(req) => req,
            Err(e) if is_ipc_disconnect(&e) => break,
            Err(e) => return Err(e),
        };

    // Sau:
    req_opt = request_rx.recv() => {
        let Some(request) = req_opt else { break; };
    ```
  - Signature của `worker_inner`:
    ```rust
    pub(crate) async fn worker_inner(
        mut request_rx: mpsc::Receiver<Request>,
        response_tx: mpsc::UnboundedSender<Response>,
        state: &mut State,
        mut reload_rx: broadcast::Receiver<()>,
    ) -> io::Result<WorkerResult>
    ```

- [ ] **Step 2: Refactor `worker()` để gọi `worker_inner()` qua split socket**

  `worker()` (giữ cho standalone binary path) split socket thành read/write halves:

  ```rust
  pub(crate) async fn worker(
      stream: tokio::net::UnixStream,
      state: &mut State,
      reload_rx: broadcast::Receiver<()>,
  ) -> io::Result<WorkerResult> {
      use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
      use std::sync::Arc;

      let (read_half, write_half) = stream.into_split();
      let mut reader = BufReader::new(read_half);
      let write_arc = Arc::new(tokio::sync::Mutex::new(write_half));

      let (req_tx, req_rx) = mpsc::channel::<Request>(64);
      let (resp_tx, mut resp_rx) = mpsc::unbounded_channel::<Response>();

      // Task A: socket read → req_tx
      tokio::spawn(async move {
          let mut recv_buf = Vec::<u8>::with_capacity(4096);
          loop {
              let mut len_buf = [0u8; 4];
              if reader.read_exact(&mut len_buf).await.is_err() { break; }
              let len = u32::from_le_bytes(len_buf) as usize;
              if len > 64 * 1024 * 1024 { break; }
              recv_buf.resize(len, 0);
              if reader.read_exact(&mut recv_buf[..len]).await.is_err() { break; }
              let Ok(req) = bincode::deserialize::<Request>(&recv_buf[..len]) else { break; };
              let is_quit = matches!(req, Request::Quit);
              if req_tx.send(req).await.is_err() { break; }
              if is_quit { break; }
          }
      });

      // Task B: resp_rx → socket write
      let write_arc2 = Arc::clone(&write_arc);
      tokio::spawn(async move {
          let mut send_buf = Vec::<u8>::with_capacity(4096);
          while let Some(resp) = resp_rx.recv().await {
              send_buf.clear();
              if bincode::serialize_into(&mut send_buf, &resp).is_err() { break; }
              let len = send_buf.len() as u32;
              let mut w = write_arc2.lock().await;
              if w.write_all(&len.to_le_bytes()).await.is_err() { break; }
              if w.write_all(&send_buf).await.is_err() { break; }
              if w.flush().await.is_err() { break; }
          }
      });

      worker_inner(req_rx, resp_tx, state, reload_rx).await
  }
  ```

  Thêm `bincode` vào `engine.rs` use declarations.

- [ ] **Step 3: Cập nhật `lib.rs` — gọi thẳng `worker_inner()`, bỏ UnixStream::pair()**

  Thay `run_engine` trong `lib.rs`:
  ```rust
  async fn run_engine(
      config: ProviderConfig,
      request_rx: mpsc::Receiver<Request>,
      response_tx: Sender<Response>,
  ) {
      use std::env;
      use engine::{FrecencyData, State, WorkerResult, load_plugins, rebuild_plugin_map, spawn_file_watcher, worker_inner};

      // ... (setup config_dir, plugin_dirs, load_plugins — giống Task 1)

      let (resp_inner_tx, mut resp_inner_rx) = mpsc::unbounded_channel::<Response>();

      // Bridge: resp_inner_rx → response_tx (relm4 Sender, thread-safe)
      tokio::spawn(async move {
          while let Some(resp) = resp_inner_rx.recv().await {
              response_tx.emit(resp);
          }
      });

      let (reload_tx, reload_rx) = broadcast::channel::<()>(4);
      spawn_file_watcher(reload_tx.clone(), &plugin_dirs, &state.config_dir);

      if let Err(e) = worker_inner(request_rx, resp_inner_tx, &mut state, reload_rx).await {
          eprintln!("[provider] engine error: {e}");
      }
  }
  ```

- [ ] **Step 4: Build và test**

  Run: `cargo build -p anyrun-provider`
  Expected: PASS

  Run: `cargo test -p anyrun-provider`
  Expected: PASS

- [ ] **Step 5: Commit**

  ```bash
  git add anyrun-provider/src/engine.rs anyrun-provider/src/lib.rs
  git commit -m "refactor(provider): replace worker() with worker_inner() using typed channels"
  ```

---

## Task 3: Thêm anyrun-provider lib dependency vào anyrun

**Mục tiêu:** `anyrun` link provider in-process qua `anyrun_provider::spawn_provider_thread()`. Thêm `worker_inproc()` vào `anyrun/src/provider.rs`.

**Files:**
- Sửa: `anyrun/Cargo.toml`
- Sửa: `anyrun/src/provider.rs`
- Sửa: `anyrun/src/app/init.rs`

**Interfaces:**
- Consumes từ Task 2:
  ```rust
  anyrun_provider::spawn_provider_thread(config: ProviderConfig, response_tx: Sender<Response>) -> ProviderHandle
  anyrun_provider::ProviderHandle { request_tx: mpsc::Sender<Request> }
  anyrun_provider::ProviderConfig { config_dir: String, plugin_specs: Vec<PathBuf> }
  ```
- Produces:
  ```rust
  // anyrun/src/provider.rs
  pub fn worker_inproc(
      config: Arc<Config>,
      config_dir: Option<String>,
      rx: Receiver<anyrun_provider_ipc::Request>,
      sender: relm4::Sender<anyrun_provider_ipc::Response>,
  ) -> io::Result<()>
  ```

- [ ] **Step 1: Thêm dep vào `anyrun/Cargo.toml`**

  Thêm vào `[dependencies]`:
  ```toml
  anyrun-provider = { path = "../anyrun-provider" }
  ```

  Giữ nguyên `anyrun-provider-ipc` (vẫn cần cho các imports trực tiếp trong anyrun).

- [ ] **Step 2: Build để xác nhận dep resolve**

  Run: `cargo build -p anyrun 2>&1 | head -20`
  Expected: Có thể có compiler warnings, nhưng không có error từ dep resolution

- [ ] **Step 3: Thêm `worker_inproc()` vào `anyrun/src/provider.rs`**

  Thêm vào đầu file (sau các `use` hiện tại):
  ```rust
  use anyrun_provider::{ProviderConfig, spawn_provider_thread};
  ```

  Thêm function mới vào cuối file:
  ```rust
  /// In-process provider: runs the provider engine in a background thread,
  /// bridging the relm4 mpsc channel to anyrun_provider's typed channels.
  /// No subprocess is spawned; no Unix socket is used.
  pub fn worker_inproc(
      config: Arc<Config>,
      config_dir: Option<String>,
      mut rx: Receiver<anyrun_provider_ipc::Request>,
      sender: Sender<anyrun_provider_ipc::Response>,
  ) -> io::Result<()> {
      let config_dir_str = config_dir
          .unwrap_or_else(|| ipc::CONFIG_DIRS[0].to_string());

      let plugin_specs: Vec<PathBuf> = config.plugins.iter()
          .map(|p| {
              let p = expand_tilde(p);
              if p.is_relative() {
                  std::env::current_dir()
                      .ok()
                      .map(|cwd| cwd.join(&p))
                      .unwrap_or(p)
              } else {
                  p
              }
          })
          .collect();

      let provider_config = ProviderConfig {
          config_dir: config_dir_str,
          plugin_specs,
      };

      // spawn_provider_thread starts its own tokio runtime in a dedicated OS thread
      let handle = spawn_provider_thread(provider_config, sender);

      // Forward requests from relm4's mpsc channel to provider's request_tx
      // This runs on the current thread (already inside a spawned command thread)
      tokio::runtime::Builder::new_current_thread()
          .enable_all()
          .build()
          .unwrap()
          .block_on(async move {
              while let Some(req) = rx.recv().await {
                  let is_quit = matches!(req, ipc::Request::Quit);
                  if handle.request_tx.send(req).await.is_err() {
                      break;
                  }
                  if is_quit {
                      break;
                  }
              }
          });

      Ok(())
  }
  ```

- [ ] **Step 4: Cập nhật `anyrun/src/app/init.rs`**

  Tại lines 132-140, thay nhánh `else if` dùng `worker_spawn`:
  ```rust
  // Trước:
  } else if let Err(why) =
      provider::worker_spawn(config, config_dir, rx, sender, stdin, env)
  {
      eprintln!("[anyrun] IPC worker returned an error: {why}");
  }

  // Sau:
  } else if let Err(why) =
      provider::worker_inproc(config, config_dir, rx, sender)
  {
      eprintln!("[anyrun] provider worker error: {why}");
  }
  ```

  Xóa các captures không còn cần trong `glib::clone!`:
  ```rust
  // Xóa nếu không còn dùng trong closure:
  // #[strong(rename_to = stdin)] app_init.stdin,
  // #[strong(rename_to = env)] app_init.env,
  ```

- [ ] **Step 5: Build toàn workspace**

  Run: `cargo build --workspace`
  Expected: PASS

- [ ] **Step 6: Test toàn workspace**

  Run: `cargo test --workspace`
  Expected: PASS

- [ ] **Step 7: Commit**

  ```bash
  git add anyrun/Cargo.toml anyrun/src/provider.rs anyrun/src/app/init.rs
  git commit -m "feat(anyrun): replace subprocess provider with in-process anyrun_provider lib"
  ```

---

## Task 4: Cập nhật daemon mode — xóa subprocess spawn

**Mục tiêu:** `anyrun daemon` không còn spawn `anyrun-provider` như subprocess. `DaemonContext` không còn `socket_path`. Xóa `worker_connect`, `worker_spawn`, `build_provider_command` khỏi `provider.rs`.

**Files:**
- Sửa: `anyrun/src/app/types.rs` — xóa `socket_path` khỏi `DaemonContext`
- Sửa: `anyrun/src/daemon.rs` — xóa subprocess spawn
- Sửa: `anyrun/src/app/init.rs` — simplify provider bridge logic
- Sửa: `anyrun/src/provider.rs` — xóa dead code
- Sửa: `anyrun/src/dbus.rs` — xóa `provider_child` nếu còn

**Interfaces:**
- Consumes từ Task 3: `provider::worker_inproc()`
- `DaemonContext` sau thay đổi:
  ```rust
  pub struct DaemonContext {
      pub config: Arc<Config>,
      pub config_dir: Option<String>,
      pub css_provider: gtk::CssProvider,
      // socket_path đã xóa
  }
  ```

- [ ] **Step 1: Xóa `socket_path` khỏi `DaemonContext` trong `anyrun/src/app/types.rs`**

  Đọc file, xóa field `pub socket_path: PathBuf`.
  Xóa `use std::path::PathBuf;` nếu không còn dùng ở chỗ nào khác trong file.

- [ ] **Step 2: Cập nhật `anyrun/src/daemon.rs`**

  Xóa các dòng:
  ```rust
  let socket_path = PathBuf::from(format!(...));
  let _ = std::fs::remove_file(&socket_path);
  let provider_path = expand_tilde(&config.provider);
  let provider_child = std::process::Command::new(&provider_path)...spawn()...;
  ```

  Cập nhật `DaemonContext` construction (bỏ `socket_path`):
  ```rust
  let context = Rc::new(app::DaemonContext {
      config: Arc::new(config),
      config_dir,
      css_provider,
  });
  ```

  Cập nhật `DaemonState` construction (bỏ `provider_child` nếu field đó tồn tại):
  ```rust
  let state = Rc::new(RefCell::new(DaemonState {
      sender: controller.sender().clone(),
  }));
  ```

  Xóa `fn expand_tilde()` cục bộ trong `daemon.rs` nếu chỉ dùng cho provider path.
  Xóa `use std::process::Command;` và `use std::path::PathBuf;` nếu không còn dùng.

- [ ] **Step 3: Simplify `anyrun/src/app/init.rs` — xóa socket_path logic**

  ```rust
  // Xóa:
  let socket_path = daemon_context.as_ref().map(|ctx| ctx.socket_path.clone());

  // Thay toàn bộ if/else spawn/connect:
  // Trước:
  if let Some(socket_path) = socket_path {
      if let Err(why) = provider::worker_connect(socket_path, rx, sender) { ... }
  } else if let Err(why) = provider::worker_inproc(...) { ... }

  // Sau (always use inproc):
  if let Err(why) = provider::worker_inproc(config, config_dir, rx, sender) {
      eprintln!("[anyrun] provider worker error: {why}");
  }
  ```

- [ ] **Step 4: Xóa dead code trong `anyrun/src/provider.rs`**

  Xóa các functions:
  - `worker_spawn()` — replaced by `worker_inproc()`
  - `build_provider_command()` — no subprocess anymore
  - `worker_connect()` — no daemon socket anymore
  - `connect_with_retry()` — internal to removed functions

  Xóa các `use` imports liên quan:
  - `std::process::{Command, Stdio}`
  - `std::io::Write`
  - `tokio::net::UnixListener`

  Xóa tests liên quan đến `build_provider_command` và `connect_with_retry` (họ test logic đã xóa).
  Giữ lại `expand_tilde()` (vẫn dùng trong `worker_inproc()`).

- [ ] **Step 5: Xóa `provider_child` khỏi `DaemonState` trong `anyrun/src/dbus.rs`**

  Đọc `anyrun/src/dbus.rs`. Tìm `DaemonState`:
  ```rust
  // Xóa field này:
  // pub provider_child: Option<Child>,
  ```

  Nếu có `Quit` handler trong D-Bus code dùng `provider_child.kill()`, thay bằng comment hoặc xóa (provider tự shutdown khi nhận `Request::Quit`).

- [ ] **Step 6: Build toàn workspace**

  Run: `cargo build --workspace`
  Expected: PASS — không có unused import warnings từ các file đã cleanup

- [ ] **Step 7: Test toàn workspace**

  Run: `cargo test --workspace`
  Expected: PASS

- [ ] **Step 8: Commit**

  ```bash
  git add anyrun/src/app/types.rs anyrun/src/daemon.rs anyrun/src/app/init.rs anyrun/src/provider.rs anyrun/src/dbus.rs
  git commit -m "refactor(anyrun): remove subprocess provider in daemon mode, cleanup dead code"
  ```

---

## Task 5: Deprecate `provider` config field

**Mục tiêu:** `provider:` field trong `config.ron` không gây error khi parse. Được giữ trong struct nhưng bị bỏ qua. `doctor.rs` không còn validate provider binary path.

**Files:**
- Sửa: `anyrun/src/config/mod.rs`
- Sửa: `anyrun/src/doctor.rs`

- [ ] **Step 1: Annotate `provider` field trong `Config`**

  ```rust
  // anyrun/src/config/mod.rs
  /// Deprecated since single-binary mode. Accepted for backward compat, ignored at runtime.
  #[serde(default = "Config::default_provider")]
  pub provider: PathBuf,
  ```

  Giữ nguyên `default_provider()` fn. Thêm `#[config_args(skip)]` nếu `ConfigArgs` derive đang generate CLI `--provider` flag (để không expose flag không cần thiết):
  ```rust
  #[config_args(skip)]
  #[serde(default = "Config::default_provider")]
  pub provider: PathBuf,
  ```

- [ ] **Step 2: Cập nhật `anyrun/src/doctor.rs`**

  Tìm và xóa/thay phần check provider binary existence. Thêm informational print:
  ```rust
  // Thay provider binary check bằng:
  println!("[doctor] provider: built-in (single-binary mode, no external binary needed)");
  ```

- [ ] **Step 3: Build + test**

  Run: `cargo test --workspace`
  Expected: PASS

- [ ] **Step 4: Commit**

  ```bash
  git add anyrun/src/config/mod.rs anyrun/src/doctor.rs
  git commit -m "chore(config): deprecate provider field, update doctor for single-binary"
  ```

---

## Task 6: Cập nhật build system

**Mục tiêu:** justfile, PKGBUILD, Nix phản ánh thực tế — 1 binary duy nhất `anyrun`.

**Files:**
- Sửa: `justfile`
- Sửa: `PKGBUILD`
- Sửa: `nix/packages/anyrun.nix`
- Sửa: `nix/modules/home-manager.nix`

- [ ] **Step 1: Cập nhật `justfile`**

  ```just
  # Trước:
  core_pkgs := "-p anyrun -p anyrun-provider"
  # Sau:
  core_pkgs := "-p anyrun"
  ```

  ```just
  # Cập nhật run:
  run: build
      ./target/release/anyrun daemon &
      ./target/release/anyrun
  ```

  ```just
  # Cập nhật daemon:
  daemon: (build "bin")
      ./target/release/anyrun daemon
  ```

  ```just
  # Cập nhật install:
  install: (build "bin")
      sudo cp ./target/release/anyrun /usr/bin
  ```

- [ ] **Step 2: Cập nhật `PKGBUILD`**

  Xóa dòng:
  ```bash
  install -Dm755 target/release/anyrun-provider -t "$pkgdir/usr/bin"
  ```

- [ ] **Step 3: Cập nhật `nix/packages/anyrun.nix`**

  Xóa `postFixup` block dùng `wrapProgram`:
  ```nix
  # Xóa:
  # postFixup = ''
  #   wrapProgram $out/bin/anyrun --prefix PATH ":" ${lib.makeBinPath [ anyrun-provider ]}
  # '';
  ```

  Xóa `anyrun-provider` khỏi function inputs nếu không còn dùng ở chỗ nào khác.
  Xóa hoặc update `passthru.anyrun-provider`.

- [ ] **Step 4: Cập nhật `nix/modules/home-manager.nix`**

  Xóa option definition `config.provider`.
  Xóa assertion về anyrun-provider.
  Xóa `provider: "${lib.getExe cfg.config.provider}"` khỏi generated `config.ron`.
  Kiểm tra systemd service — `ExecStart = "${lib.getExe cfg.package} daemon"` vẫn valid.

- [ ] **Step 5: Verify justfile**

  Run: `just build bin`
  Expected: PASS

  Run: `just build plugins`
  Expected: PASS

- [ ] **Step 6: Commit**

  ```bash
  git add justfile PKGBUILD nix/packages/anyrun.nix nix/modules/home-manager.nix
  git commit -m "chore(build): update justfile, PKGBUILD, nix to single-binary packaging"
  ```

---

## Task 7: Smoke test end-to-end

**Mục tiêu:** Xác nhận binary mới hoạt động đúng, tất cả tests xanh.

- [ ] **Step 1: Build release binary**

  Run: `cargo build --release -p anyrun`
  Expected: PASS

- [ ] **Step 2: Verify binary size (informational)**

  Run: `ls -lh target/release/anyrun`
  Expected: Binary lớn hơn trước (đã link thêm provider logic)

- [ ] **Step 3: Chạy `cargo check` để verify không còn dead imports**

  Run: `cargo check --workspace 2>&1 | grep -E "warning|error"`
  Expected: Không có `unused import` hay `dead_code` warnings từ code đã sửa

- [ ] **Step 4: Chạy toàn bộ test suite**

  Run: `cargo test --workspace`
  Expected: PASS — tất cả tests xanh

  Run: `cargo test -p anyrun-provider -- --test-threads=1`
  Expected: PASS — standalone path (`ipc_e2e.rs`) vẫn hoạt động

- [ ] **Step 5: Commit cuối**

  ```bash
  git commit --allow-empty -m "chore: single-binary merge complete (anyrun + anyrun-provider)"
  ```

---

## Tóm tắt thay đổi theo file

| File | Thay đổi | Task |
|------|----------|------|
| `anyrun-provider/Cargo.toml` | Thêm `[lib]` target | 1 |
| `anyrun-provider/src/engine.rs` | **Tạo mới** — core logic tách từ main.rs | 1 |
| `anyrun-provider/src/lib.rs` | **Tạo mới** — public API `spawn_provider_thread()` | 1, 2 |
| `anyrun-provider/src/main.rs` | Thin CLI wrapper | 1, 2 |
| `anyrun/Cargo.toml` | Thêm `anyrun-provider` dep | 3 |
| `anyrun/src/provider.rs` | Xóa subprocess logic, thêm `worker_inproc()` | 3, 4 |
| `anyrun/src/app/init.rs` | Đổi `worker_spawn` → `worker_inproc` | 3, 4 |
| `anyrun/src/app/types.rs` | Xóa `socket_path` khỏi `DaemonContext` | 4 |
| `anyrun/src/daemon.rs` | Xóa subprocess spawn logic | 4 |
| `anyrun/src/dbus.rs` | Xóa `provider_child` khỏi `DaemonState` | 4 |
| `anyrun/src/config/mod.rs` | Deprecate `provider` field | 5 |
| `anyrun/src/doctor.rs` | Xóa provider binary check | 5 |
| `justfile` | Single binary targets | 6 |
| `PKGBUILD` | Xóa anyrun-provider install | 6 |
| `nix/packages/anyrun.nix` | Xóa PATH wrapping | 6 |
| `nix/modules/home-manager.nix` | Xóa provider option + assertion | 6 |
