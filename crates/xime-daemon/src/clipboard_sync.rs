//! 剪贴板同步桥：本地剪贴板（`xime-clipboard`）↔ 远端。
//!
//! 对齐 Android 版 `ClipboardSyncBridge` 的语义，但支持两种互斥的传输方式
//! （由 `clipboard_sync.toml` 的 `enabled` + `transport` 决定，设置程序
//! 「剪贴板」页的三选一与这里一一对应）：
//!
//! - `plugin`：经 `clipboard_sync` JS 插件（WebDAV / 自建服务器）。实际加载哪个
//!   插件由 `plugin_id` 偏好决定——解析语义对齐 Android
//!   `ActivePluginSelection.resolve`（偏好有效用偏好，否则取首个已启用项）；
//! - `p2p`：局域网直连（`xime-p2p`），不经任何服务器。身份与已配对表在
//!   `~/.config/<app>/p2p/`，与设置程序的「设备关联」页共用同一目录。
//!
//! 共同语义：
//! - **push**：watcher 捕获文本 → hash 去重 → 插件 `clipboardSync.push(profile)`
//!   或 `Node::set_clipboard(profile)`；
//! - **pull**：插件模式下 daemon 启动/打开面板时拉取，P2P 模式是推送式的，
//!   事件由桥线程每轮轮询 `Node::poll_event` 取出；
//! - **回声抑制**：自己刚推出去的 hash 不再写回本地；
//! - **独立线程**：插件网络请求、P2P 握手都不阻塞按键处理。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use sha2::{Digest, Sha256};
use tracing::{debug, error, info, warn};
use xime_p2p::{Profile, ProfileType};
use xime_plugin::{PluginManager, PluginManifest, PluginRuntime};

use xime_clipboard::store::ClipboardStore;

/// P2P 模式下桥线程的轮询间隔。
///
/// `Node` 是轮询式 API（剪贴板到达、连接变化都进事件队列），所以桥线程不能
/// 在 `recv()` 上无限期阻塞；200ms 对剪贴板这种交互级延迟足够，又不至于空转。
const P2P_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// 同步插件加载描述（运行时在桥线程内创建，规避 QuickJS Context 跨线程约束）。
#[derive(Debug, Clone)]
pub struct SyncPluginDescriptor {
    pub id: String,
    pub manifest: PluginManifest,
    pub plugin_dir: PathBuf,
    pub config_file: PathBuf,
}

/// 同步桥消息。
pub enum SyncMessage {
    /// watcher 捕获到文本（push 触发）。
    Captured(String),
    /// 请求拉取（daemon 启动 / 打开剪贴板面板）。
    Pull,
    /// 切换/重载后端（启动、安装/卸载插件、设置页改配置后经 DBus 通知）。
    Reload(Box<SyncBackend>),
}

/// 文本 SHA-256 摘要（hex 小写，对齐 Android ClipboardProfile.hash）。
pub fn text_hash(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// 构造 profile JSON（字段对齐 Android `ClipboardProfile`，snake_case）。
fn profile_json(text: &str, hash: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "text",
        "hash": hash,
        "text": text,
        "size": text.len(),
        "source": "linux",
    })
}

/// 剪贴板同步选择（`clipboard_sync.toml`，与设置程序 xime-setup 共享同一文件）。
///
/// 字段名/取值必须与 `xime-setup-lib` 的 `ClipboardSyncConfig` 一致：两个进程
/// 各读写一份同一个文件，字段对不上就是「设置里选了互传、输入法仍当插件跑」。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SyncSelection {
    /// 总开关：false 时同步桥不加载任何后端。
    #[serde(default)]
    pub enabled: bool,
    /// 选中的同步插件 id（空或失效时回落到首个已启用的 clipboard_sync 插件）。
    #[serde(default)]
    pub plugin_id: String,
    /// 传输方式：`plugin`（经插件/服务器）或 `p2p`（局域网直连）。
    ///
    /// 老配置没有这个字段时必须默认成 `plugin`——默认 `p2p` 会让所有存量
    /// 服务器用户升级后静默停同步（只会留一句「未配对」的日志）。
    #[serde(default = "default_transport")]
    pub transport: String,
}

fn default_transport() -> String {
    "plugin".to_string()
}

impl Default for SyncSelection {
    fn default() -> Self {
        Self {
            enabled: false,
            plugin_id: String::new(),
            transport: default_transport(),
        }
    }
}

impl SyncSelection {
    /// 是否选择了局域网直连。未识别的 transport 一律按插件处理：宁可保持既有
    /// 行为，也不要因为一个拼错的字段让同步静默失效。
    fn is_p2p(&self) -> bool {
        self.transport == "p2p"
    }
}

/// 同步后端（三选一，互斥）。
///
/// 原来是「enabled + 有没有插件」两件事拼出来的，选了「设备互传」也只是在
/// 插件列表里找，结果什么都不做且无日志。收成显式枚举后，桥只有一个分支判断。
#[derive(Debug, Clone)]
pub enum SyncBackend {
    /// 关闭：不加载任何后端。
    Off,
    /// 经 clipboard_sync 插件（单插件生效）。
    Plugins(Vec<SyncPluginDescriptor>),
    /// 局域网直连（xime-p2p）。
    P2p(Box<P2pConfig>),
}

/// P2P 后端启动参数。
#[derive(Clone)]
pub struct P2pConfig {
    /// 本机长期身份（`p2p/identity.bin`）。
    pub identity: Arc<xime_p2p::Identity>,
    /// 已配对表（`p2p/peers.json`）。
    pub store_path: PathBuf,
}

/// `Identity` 未实现 `Debug`（内部含私钥，刻意不打印），这里只暴露 did 与路径。
impl std::fmt::Debug for P2pConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("P2pConfig")
            .field("did", &self.identity.did().to_hex())
            .field("store_path", &self.store_path)
            .finish()
    }
}

/// `clipboard_sync.toml` 路径（`~/.config/<app>/clipboard_sync.toml`）。
pub fn sync_config_path() -> PathBuf {
    crate::plugin_host::plugins_dir()
        .parent()
        .expect("plugins dir has parent")
        .join("clipboard_sync.toml")
}

/// P2P 数据目录（`~/.config/<app>/p2p/`）：长期身份 + 已配对表。
///
/// 必须与设置程序 `xime-setup-lib::p2p_dir()` 指向同一目录——两边各算各的，
/// 就成了「设置里配对成功、输入法仍认为没配对」，界面上看不出任何异常。
pub fn p2p_dir() -> PathBuf {
    crate::plugin_host::plugins_dir()
        .parent()
        .expect("plugins dir has parent")
        .join("p2p")
}

/// 配置文件 mtime（不存在/无权限时为 None）。
fn config_mtime_of(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

pub fn read_sync_selection() -> SyncSelection {
    read_sync_selection_from(&sync_config_path())
}

fn read_sync_selection_from(path: &std::path::Path) -> SyncSelection {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| toml::from_str(&content).ok())
        .unwrap_or_default()
}

/// 解析当前应使用的后端（daemon 启动 / ReloadPlugins 时调用）。
pub fn scan_backend() -> SyncBackend {
    scan_backend_from(
        &crate::plugin_host::plugins_dir(),
        &p2p_dir(),
        &read_sync_selection(),
    )
}

/// [`scan_backend`] 的可测试内核：目录由参数传入。
pub fn scan_backend_from(plugins_root: &Path, p2p_root: &Path, sel: &SyncSelection) -> SyncBackend {
    if !sel.enabled {
        return SyncBackend::Off;
    }
    if sel.is_p2p() {
        if let Err(e) = std::fs::create_dir_all(p2p_root) {
            error!("Sync p2p: create {} failed: {e}", p2p_root.display());
            return SyncBackend::Off;
        }
        let Some(identity) = xime_p2p::load_or_create_identity(p2p_root) else {
            error!(
                "Sync p2p: identity unavailable under {}",
                p2p_root.display()
            );
            return SyncBackend::Off;
        };
        return SyncBackend::P2p(Box::new(P2pConfig {
            identity,
            store_path: p2p_root.join("peers.json"),
        }));
    }
    SyncBackend::Plugins(resolve_descriptors(plugins_root, sel))
}

/// 按 `SyncSelection` 解析要加载的插件：偏好插件有效则只用它，否则回落首个
/// 已启用的 clipboard_sync 插件；总开关关闭或无候选返回空。
pub fn resolve_descriptors(
    root: &std::path::Path,
    sel: &SyncSelection,
) -> Vec<SyncPluginDescriptor> {
    if !sel.enabled {
        return Vec::new();
    }
    let manager = PluginManager::new(root);
    let mut candidates: Vec<SyncPluginDescriptor> = manager
        .list()
        .into_iter()
        .filter(|r| r.enabled)
        .filter_map(|record| {
            let manifest = match manager.load_manifest(&record.id) {
                Ok(m) if m.plugin_type() == xime_plugin::PluginType::ClipboardSync => m,
                Ok(_) => return None,
                Err(e) => {
                    warn!("Sync scan: manifest of '{}' failed: {e}", record.id);
                    return None;
                }
            };
            Some(SyncPluginDescriptor {
                id: record.id.clone(),
                plugin_dir: manager.plugin_dir(&record.id),
                config_file: manager.config_path(&record.id),
                manifest,
            })
        })
        .collect();
    if let Some(pos) = candidates.iter().position(|d| d.id == sel.plugin_id) {
        let preferred = candidates.swap_remove(pos);
        return vec![preferred];
    }
    candidates.truncate(1);
    candidates
}

/// 桥线程状态。
struct SyncState {
    store: Arc<ClipboardStore>,
    runtimes: HashMap<String, PluginRuntime>,
    /// P2P 节点（transport = p2p 时才有；节点自持 tokio runtime）。
    node: Option<xime_p2p::Node>,
    /// 最近一次成功推送的 hash（push 去重 + 回声抑制）。
    last_pushed_hash: Option<String>,
    /// `clipboard_sync.toml` 的最近 mtime。
    ///
    /// 设置程序改完「关闭/同步插件/设备互传」并不会通知 daemon（它只写文件），
    /// 不盯着这个文件就得重启输入法才生效——用户看到的正是「选了没反应」。
    config_mtime: Option<std::time::SystemTime>,
    /// 是否有已建立的 P2P 连接（`Event::Peer { connected }` 维护）。
    ///
    /// 只用于日志/测试与「连上后补推」的时机判断：`Node` 不暴露连接状态，
    /// 而补推必须在连接建立之后，否则指令又被丢掉。
    peer_connected: bool,
    /// 最近一次本地剪贴板文本。
    ///
    /// `Node::set_clipboard` 在无连接时会把指令丢掉（剪贴板只关心最新值），
    /// 所以对端连上时（`Event::Peer { connected: true }`）要补推一次，
    /// 否则「先复制、后连上」的内容永远同步不过去。
    last_local_text: Option<String>,
}

impl SyncState {
    fn handle_captured(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.node.is_some() {
            self.push_p2p(text);
            return;
        }
        if self.runtimes.is_empty() {
            return;
        }
        let hash = text_hash(text);
        if self.last_pushed_hash.as_deref() == Some(hash.as_str()) {
            debug!("Sync push: identical to last pushed, skipping");
            return;
        }
        let profile = profile_json(text, &hash);
        let mut pushed = false;
        for (id, runtime) in &self.runtimes {
            if runtime.clipboard_push(&profile) {
                pushed = true;
                debug!("Sync pushed to '{}' ({} chars)", id, text.chars().count());
            } else {
                debug!("Sync push to '{}' failed/unsupported", id);
            }
        }
        if pushed {
            self.last_pushed_hash = Some(hash);
        }
    }

    /// P2P 推送：profile 由 `xime-p2p` 构造（hash 算法与协议一致）。
    fn push_p2p(&mut self, text: &str) {
        self.last_local_text = Some(text.to_string());
        let hash = text_hash(text);
        if self.last_pushed_hash.as_deref() == Some(hash.as_str()) {
            debug!("Sync p2p: identical to last pushed, skipping");
            return;
        }
        let Some(node) = self.node.as_ref() else {
            return;
        };
        let profile = Profile::from_text(text, Some(node.did().to_string()));
        if node.set_clipboard(profile) {
            self.last_pushed_hash = Some(hash);
            debug!("Sync p2p: queued ({} chars)", text.chars().count());
        }
    }

    fn handle_pull(&mut self) {
        if self.node.is_some() {
            // P2P 是推送式：远端变更由 `poll_events` 实时取，没有「拉」这一步。
            debug!("Sync p2p: pull is a no-op (push based)");
            return;
        }
        if self.runtimes.is_empty() {
            return;
        }
        for (id, runtime) in &self.runtimes {
            let Some(value) = runtime.clipboard_pull() else {
                debug!("Sync pull from '{}': no change", id);
                continue;
            };
            let profiles: Vec<serde_json::Value> = match value {
                serde_json::Value::Array(items) => items,
                v @ serde_json::Value::Object(_) => vec![v],
                _ => continue,
            };
            for profile in profiles {
                let Some(text) = profile.get("text").and_then(|t| t.as_str()) else {
                    continue;
                };
                if text.is_empty() {
                    continue;
                }
                // 回声抑制：自己刚推送的内容不写回本地
                let remote_hash = profile
                    .get("hash")
                    .and_then(|h| h.as_str())
                    .map(str::to_owned);
                if remote_hash.as_deref() == self.last_pushed_hash.as_deref() {
                    debug!("Sync pull: echo of own push, skipping");
                    continue;
                }
                if let Err(e) = self
                    .store
                    .upsert_and_trim(text, xime_clipboard::store::now_millis())
                {
                    error!("Sync pull: store failed: {e}");
                } else {
                    debug!("Sync pulled from '{}': {} chars", id, text.chars().count());
                }
            }
        }
    }

    fn handle_reload(&mut self, backend: SyncBackend) {
        // 旧后端统一先卸：插件运行时与 P2P 节点都持有线程/网络资源。
        for runtime in self.runtimes.values() {
            runtime.call_on_unload();
        }
        self.runtimes.clear();
        if let Some(node) = self.node.take() {
            node.shutdown();
            info!("Sync p2p: node stopped");
        }
        self.last_pushed_hash = None;
        self.peer_connected = false;

        match backend {
            SyncBackend::Off => info!("Sync bridge: disabled"),
            SyncBackend::Plugins(descriptors) => self.load_plugins(&descriptors),
            SyncBackend::P2p(cfg) => self.start_p2p(*cfg),
        }
    }

    fn load_plugins(&mut self, descriptors: &[SyncPluginDescriptor]) {
        let mut loaded = 0;
        for d in descriptors {
            match PluginRuntime::load(&d.plugin_dir, &d.manifest.entry, &d.config_file) {
                Ok(runtime) => {
                    runtime.call_on_load();
                    self.runtimes.insert(d.id.clone(), runtime);
                    loaded += 1;
                }
                Err(e) => error!("Sync plugin '{}' load failed: {e}", d.id),
            }
        }
        info!(
            "Sync bridge: {}/{} clipboard_sync plugins loaded",
            loaded,
            descriptors.len()
        );
    }

    fn start_p2p(&mut self, cfg: P2pConfig) {
        let node_cfg = xime_p2p::NodeConfig::new(cfg.identity, cfg.store_path);
        match xime_p2p::Node::start(node_cfg) {
            Ok(node) => {
                info!(
                    "Sync p2p: node started (did={}, port={})",
                    node.did(),
                    node.local_port()
                );
                self.node = Some(node);
                // 连接建立后要补推最近一条：启动瞬间通常还没有连接，
                // 先把它记下来，等 `Event::Peer { connected: true }` 再推。
                self.last_local_text = self
                    .store
                    .list_clipboard(1)
                    .ok()
                    .and_then(|items| items.first().map(|e| e.text.clone()));
            }
            Err(e) => error!("Sync p2p: node start failed: {e}"),
        }
    }

    /// 配置文件变了就重选后端（设置程序只写文件，不通知 daemon）。
    ///
    /// 返回是否真的重载了（测试用；生产里忽略返回值）。
    fn reload_if_config_changed(&mut self) -> bool {
        self.reload_if_config_changed_at(&sync_config_path())
    }

    fn reload_if_config_changed_at(&mut self, path: &Path) -> bool {
        let mtime = config_mtime_of(path);
        if mtime == self.config_mtime {
            return false;
        }
        self.config_mtime = mtime;
        info!("Sync bridge: config changed, reselecting backend");
        let sel = read_sync_selection_from(path);
        let backend = scan_backend_from(&crate::plugin_host::plugins_dir(), &p2p_dir(), &sel);
        self.handle_reload(backend);
        true
    }

    /// 取走节点队列里的全部事件（无节点时是空操作）。
    fn poll_events(&mut self) {
        while let Some(event) = self.node.as_ref().and_then(|n| n.poll_event()) {
            match event {
                xime_p2p::Event::Clipboard(profile) => self.handle_remote_profile(profile),
                xime_p2p::Event::Peer {
                    did,
                    name,
                    connected,
                } => {
                    info!("Sync p2p: peer {name} ({did}) connected={connected}");
                    self.peer_connected = connected;
                    if connected {
                        // 见 `last_local_text` 注释：连上后补推一次最近内容。
                        if let Some(text) = self.last_local_text.clone() {
                            self.push_p2p(&text);
                            // 补推后 hash 已被记为「自己推的」，避免把对端回传
                            // 的同一条当新内容再入库。
                        }
                    }
                }
                xime_p2p::Event::Warning(msg) => warn!("Sync p2p: {msg}"),
            }
        }
    }

    fn handle_remote_profile(&mut self, profile: Profile) {
        if profile.r#type != ProfileType::Text {
            debug!("Sync p2p: skip non-text profile ({:?})", profile.r#type);
            return;
        }
        if profile.text.is_empty() {
            return;
        }
        // 回声抑制：自己刚推出去的内容不写回本地。
        if !profile.hash.is_empty()
            && Some(profile.hash.as_str()) == self.last_pushed_hash.as_deref()
        {
            debug!("Sync p2p: echo of own push, skipping");
            return;
        }
        match self
            .store
            .upsert_and_trim(&profile.text, xime_clipboard::store::now_millis())
        {
            Ok(()) => debug!(
                "Sync p2p: pulled ({} chars, from {:?})",
                profile.text.chars().count(),
                profile.source
            ),
            Err(e) => error!("Sync p2p: store failed: {e}"),
        }
    }
}

/// 启动同步桥线程（进程生命周期内常驻）。
pub fn spawn_bridge(store: Arc<ClipboardStore>, rx: Receiver<SyncMessage>) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("xime-clipboard-sync".into())
        .spawn(move || {
            let mut state = SyncState {
                store,
                runtimes: HashMap::new(),
                node: None,
                config_mtime: config_mtime_of(&sync_config_path()),
                peer_connected: false,
                last_pushed_hash: None,
                last_local_text: None,
            };
            loop {
                // 轮询而不是阻塞 recv：P2P 的入站内容只在 `Node` 的事件队列里，
                // 没有人会替我们把它推成一条 `SyncMessage`。
                match rx.recv_timeout(P2P_POLL_INTERVAL) {
                    Ok(SyncMessage::Captured(text)) => state.handle_captured(&text),
                    Ok(SyncMessage::Pull) => state.handle_pull(),
                    Ok(SyncMessage::Reload(backend)) => state.handle_reload(*backend),
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => {
                        debug!("Sync bridge: channel closed, exiting");
                        break;
                    }
                }
                state.poll_events();
                let _ = state.reload_if_config_changed();
            }
            if let Some(node) = state.node.take() {
                node.shutdown();
            }
        })
        .expect("spawn clipboard sync bridge thread")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 远端模拟插件：push 写入 host.config，pull 读回（沙箱无 io，用 host.config 充当远端）。
    const FAKE_SYNC_JS: &str = r#"(function () {
  globalThis.plugin = {
    clipboardSync: {
      push: function (profile) {
        var list = host.config.getJson("remote") || [];
        list.push(profile);
        host.config.set("remote", JSON.stringify(list));
        return true;
      },
      pull: function () {
        var raw = host.config.get("remote");
        if (raw == null) return null;
        var list = JSON.parse(raw);
        return list.length ? list[list.length - 1] : null;
      },
      test: function () { return null; },
      remoteCount: function () {
        var list = host.config.getJson("remote") || [];
        return list.length;
      },
    },
  };
})();
"#;

    fn test_manifest() -> PluginManifest {
        PluginManifest::parse(
            "id: com.test.sync\nname: Test Sync\nversion: 1.0.0\ntype: clipboard_sync\nentry: main.js\n",
        )
        .unwrap()
    }

    fn setup() -> (tempfile::TempDir, Arc<ClipboardStore>, SyncState) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(ClipboardStore::open(dir.path().join("clipboard.db")).unwrap());
        let state = SyncState {
            store: store.clone(),
            runtimes: HashMap::new(),
            node: None,
            config_mtime: None,
            peer_connected: false,
            last_pushed_hash: None,
            last_local_text: None,
        };
        (dir, store, state)
    }

    fn load_fake_plugin(state: &mut SyncState, dir: &std::path::Path) {
        std::fs::write(dir.join("main.js"), FAKE_SYNC_JS).unwrap();
        let manifest = test_manifest();
        let runtime = PluginRuntime::load(dir, &manifest.entry, &dir.join("config.yaml")).unwrap();
        runtime.call_on_load();
        state.runtimes.insert(manifest.id.clone(), runtime);
    }

    /// 读取模拟插件远端（host.config["remote"]）条目数。
    fn remote_count(state: &SyncState) -> usize {
        state
            .runtimes
            .values()
            .next()
            .and_then(|r| r.call_plugin_fn("clipboardSync.remoteCount", &[]))
            .and_then(|v| v.as_u64())
            .unwrap_or(u64::from(u32::MAX)) as usize
    }

    #[test]
    fn test_text_hash() {
        assert_eq!(text_hash("hello").len(), 64);
        assert_eq!(text_hash("hello"), text_hash("hello"));
        assert_ne!(text_hash("hello"), text_hash("hello "));
    }

    #[test]
    fn test_profile_json_fields() {
        let hash = text_hash("abc");
        let p = profile_json("abc", &hash);
        assert_eq!(p["type"], "text");
        assert_eq!(p["text"], "abc");
        assert_eq!(p["hash"], hash);
        assert_eq!(p["size"], 3);
        assert_eq!(p["source"], "linux");
    }

    #[test]
    fn test_captured_pushes_and_dedups() {
        let (dir, store, mut state) = setup();
        load_fake_plugin(&mut state, dir.path());

        state.handle_captured("hello");
        assert_eq!(remote_count(&state), 1);
        // 第一次 pull 带回自己的推送（回声抑制）→ 本地不入库
        state.handle_pull();
        assert_eq!(store.count_clipboard().unwrap(), 0, "pull 回声应被抑制");

        // 相同文本再次捕获：push 去重跳过（远端仍只有 1 条）
        state.handle_captured("hello");
        assert_eq!(remote_count(&state), 1, "相同文本不应重复推送");
        state.handle_pull();
        assert_eq!(store.count_clipboard().unwrap(), 0);
    }

    #[test]
    fn test_pull_writes_remote_profiles() {
        let (dir, store, mut state) = setup();
        load_fake_plugin(&mut state, dir.path());

        // 本机推 A
        state.handle_captured("A");
        // 模拟另一台设备写入远端 B：直接调插件 push 一个 B profile
        let b_hash = text_hash("B");
        let profile = profile_json("B", &b_hash);
        if let Some(runtime) = state.runtimes.values().next() {
            assert!(runtime.clipboard_push(&profile));
        }

        // 拉取：A 是回声（跳过），B 应入库
        state.handle_pull();
        let items = store.list_clipboard(10).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "B");
    }

    #[test]
    fn test_empty_state_noop() {
        let (_dir, store, mut state) = setup();
        state.handle_captured("hello");
        state.handle_pull();
        state.handle_reload(SyncBackend::Off);
        assert_eq!(store.count_clipboard().unwrap(), 0);
    }

    #[test]
    fn test_reload_loads_and_unloads() {
        let (dir, _store, mut state) = setup();
        let d = SyncPluginDescriptor {
            id: "com.test.sync".into(),
            manifest: test_manifest(),
            plugin_dir: dir.path().to_path_buf(),
            config_file: dir.path().join("config.yaml"),
        };
        std::fs::write(dir.path().join("main.js"), FAKE_SYNC_JS).unwrap();
        state.handle_reload(SyncBackend::Plugins(vec![d.clone()]));
        state.handle_captured("x");
        assert_eq!(remote_count(&state), 1);
        // 重载为空：运行时清空（无法查询远端），push 无操作
        state.handle_reload(SyncBackend::Off);
        state.handle_captured("y");
        // 重新加载后 last_pushed_hash 重置 → "y" 可推
        state.handle_reload(SyncBackend::Plugins(vec![d]));
        state.handle_captured("y");
        assert_eq!(remote_count(&state), 2);
    }

    // ---- SyncSelection / resolve_descriptors ----

    /// 造一个插件目录（manifest.yaml，无需 main.js：resolve 不加载运行时）。
    fn write_plugin_dir(root: &std::path::Path, id: &str, name: &str, ptype: &str) {
        let dir = root.join(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("manifest.yaml"),
            format!("id: {id}\nname: {name}\nversion: 1.0.0\ntype: {ptype}\nentry: main.js\n"),
        )
        .unwrap();
    }

    /// 造 registry.yaml（id, enabled）条目。
    fn write_registry(root: &std::path::Path, entries: &[(&str, bool)]) {
        let mut yaml = String::from("plugins:\n");
        for (id, enabled) in entries {
            yaml.push_str(&format!(
                "  - id: {id}\n    name: {id}\n    version: 1.0.0\n    type: clipboard_sync\n    enabled: {enabled}\n"
            ));
        }
        std::fs::write(root.join("registry.yaml"), yaml).unwrap();
    }

    fn ids(descriptors: &[SyncPluginDescriptor]) -> Vec<String> {
        descriptors.iter().map(|d| d.id.clone()).collect()
    }

    #[test]
    fn test_resolve_switch_off_empty() {
        let dir = tempfile::tempdir().unwrap();
        write_plugin_dir(dir.path(), "com.a.sync", "A", "clipboard_sync");
        write_registry(dir.path(), &[("com.a.sync", true)]);
        let sel = SyncSelection {
            enabled: false,
            plugin_id: "com.a.sync".into(),
            ..SyncSelection::default()
        };
        assert!(resolve_descriptors(dir.path(), &sel).is_empty());
    }

    #[test]
    fn test_resolve_prefers_plugin_id() {
        let dir = tempfile::tempdir().unwrap();
        write_plugin_dir(dir.path(), "com.a.sync", "A", "clipboard_sync");
        write_plugin_dir(dir.path(), "com.b.sync", "B", "clipboard_sync");
        write_registry(dir.path(), &[("com.a.sync", true), ("com.b.sync", true)]);
        let sel = SyncSelection {
            enabled: true,
            plugin_id: "com.b.sync".into(),
            ..SyncSelection::default()
        };
        assert_eq!(
            ids(&resolve_descriptors(dir.path(), &sel)),
            vec!["com.b.sync".to_string()]
        );
    }

    #[test]
    fn test_resolve_fallback_first_enabled() {
        let dir = tempfile::tempdir().unwrap();
        write_plugin_dir(dir.path(), "com.a.sync", "A", "clipboard_sync");
        write_plugin_dir(dir.path(), "com.b.sync", "B", "clipboard_sync");
        write_registry(dir.path(), &[("com.a.sync", true), ("com.b.sync", true)]);
        // 未选择（plugin_id 空）→ 首个已启用项，且单插件语义
        let sel = SyncSelection {
            enabled: true,
            plugin_id: String::new(),
            ..SyncSelection::default()
        };
        assert_eq!(
            ids(&resolve_descriptors(dir.path(), &sel)),
            vec!["com.a.sync".to_string()]
        );
        // 偏好失效（未安装）→ 同样回落
        let sel = SyncSelection {
            enabled: true,
            plugin_id: "com.gone.sync".into(),
            ..SyncSelection::default()
        };
        assert_eq!(
            ids(&resolve_descriptors(dir.path(), &sel)),
            vec!["com.a.sync".to_string()]
        );
    }

    #[test]
    fn test_resolve_skips_disabled_and_other_types() {
        let dir = tempfile::tempdir().unwrap();
        write_plugin_dir(dir.path(), "com.a.sync", "A", "clipboard_sync");
        write_plugin_dir(dir.path(), "com.b.sync", "B", "clipboard_sync");
        write_plugin_dir(dir.path(), "com.c.emoji", "C", "emoji");
        write_registry(
            dir.path(),
            &[
                ("com.a.sync", false),
                ("com.b.sync", true),
                ("com.c.emoji", true),
            ],
        );
        let sel = SyncSelection {
            enabled: true,
            plugin_id: String::new(),
            ..SyncSelection::default()
        };
        assert_eq!(
            ids(&resolve_descriptors(dir.path(), &sel)),
            vec!["com.b.sync".to_string()]
        );
    }

    #[test]
    fn test_read_sync_selection_parses_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clipboard_sync.toml");
        assert_eq!(read_sync_selection_from(&path), SyncSelection::default());

        std::fs::write(&path, "enabled = true\nplugin_id = \"com.b.sync\"\n").unwrap();
        assert_eq!(
            read_sync_selection_from(&path),
            SyncSelection {
                enabled: true,
                plugin_id: "com.b.sync".into(),
                ..SyncSelection::default()
            }
        );
        // 老配置没有 transport：默认 plugin（不能默认 p2p，否则存量用户静默停同步）
        assert_eq!(read_sync_selection_from(&path).transport, "plugin");

        // 设置程序写出的「设备互传」
        std::fs::write(
            &path,
            "enabled = true\nplugin_id = \"\"\ntransport = \"p2p\"\n",
        )
        .unwrap();
        let sel = read_sync_selection_from(&path);
        assert!(sel.enabled);
        assert!(sel.is_p2p());
        assert_eq!(sel.transport, "p2p");
    }

    // ---- scan_backend ----

    #[test]
    fn test_scan_backend_off_when_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let sel = SyncSelection::default(); // enabled = false
        assert!(matches!(
            scan_backend_from(dir.path(), dir.path(), &sel),
            SyncBackend::Off
        ));
    }

    #[test]
    fn test_scan_backend_plugin_transport() {
        let dir = tempfile::tempdir().unwrap();
        write_plugin_dir(dir.path(), "com.a.sync", "A", "clipboard_sync");
        write_registry(dir.path(), &[("com.a.sync", true)]);
        let sel = SyncSelection {
            enabled: true,
            plugin_id: "com.a.sync".into(),
            ..SyncSelection::default()
        };
        match scan_backend_from(dir.path(), dir.path(), &sel) {
            SyncBackend::Plugins(d) => assert_eq!(ids(&d), vec!["com.a.sync".to_string()]),
            other => panic!("expected plugin backend, got {other:?}"),
        }
    }

    #[test]
    fn test_scan_backend_p2p_creates_identity() {
        let plugins = tempfile::tempdir().unwrap();
        let p2p = tempfile::tempdir().unwrap();
        // identity 目录尚不存在：scan 要自己建（daemon 可能先于设置程序启动）
        let p2p_root = p2p.path().join("p2p");
        let sel = SyncSelection {
            enabled: true,
            plugin_id: String::new(),
            transport: "p2p".into(),
        };
        match scan_backend_from(plugins.path(), &p2p_root, &sel) {
            SyncBackend::P2p(cfg) => {
                assert!(
                    p2p_root.join("identity.bin").is_file(),
                    "identity.bin 应已生成"
                );
                assert_eq!(cfg.store_path, p2p_root.join("peers.json"));
                assert!(!cfg.identity.did().to_hex().is_empty());
            }
            other => panic!("expected p2p backend, got {other:?}"),
        }
    }

    #[test]
    fn test_scan_backend_p2p_wins_over_plugins() {
        // 选了设备互传就不该再加载插件：两条路同时同步会让同一内容有
        // 两份来源（一份去服务器、一份走局域网），出问题无法定位。
        let plugins = tempfile::tempdir().unwrap();
        write_plugin_dir(plugins.path(), "com.a.sync", "A", "clipboard_sync");
        write_registry(plugins.path(), &[("com.a.sync", true)]);
        let p2p = tempfile::tempdir().unwrap();
        let sel = SyncSelection {
            enabled: true,
            plugin_id: "com.a.sync".into(),
            transport: "p2p".into(),
        };
        assert!(matches!(
            scan_backend_from(plugins.path(), p2p.path(), &sel),
            SyncBackend::P2p(_)
        ));
    }

    #[test]
    fn test_reload_switches_backend() {
        let (dir, store, mut state) = setup();
        load_fake_plugin(&mut state, dir.path());
        state.handle_captured("plugin-only");
        assert_eq!(remote_count(&state), 1);

        // 切到 Off：插件被卸载，后续捕获不再推送
        state.handle_reload(SyncBackend::Off);
        assert!(state.runtimes.is_empty());
        state.handle_captured("after-off");
        assert_eq!(store.count_clipboard().unwrap(), 0);
    }

    #[test]
    fn test_config_change_triggers_reload() {
        let (dir, _store, mut state) = setup();
        // 配置放在临时目录：只验「mtime 变化 → 重载」这条链，
        // backend 一律走 Off（enabled = false），不碰真实 plugins/p2p 目录。
        let cfg = dir.path().join("clipboard_sync.toml");
        state.config_mtime = None;

        // 文件从无到有：必须重载（用户第一次打开设置页就是这种情况）
        std::fs::write(&cfg, "enabled = false\n").unwrap();
        assert!(
            state.reload_if_config_changed_at(&cfg),
            "配置首次出现应重载"
        );

        // 内容没动：不重载。这个方法每 200ms 调一次，必须廉价且无副作用。
        assert!(
            !state.reload_if_config_changed_at(&cfg),
            "mtime 未变不应重载"
        );

        // 改内容后把 mtime 推到未来（避免同秒/粗粒度文件系统漏检）
        std::fs::write(&cfg, "enabled = false\nplugin_id = \"com.b.sync\"\n").unwrap();
        let f = std::fs::File::options().write(true).open(&cfg).unwrap();
        f.set_modified(std::time::SystemTime::now() + Duration::from_secs(2))
            .unwrap();
        assert!(state.reload_if_config_changed_at(&cfg), "mtime 变化应重载");
        assert_eq!(state.config_mtime, config_mtime_of(&cfg));
    }

    // ---- P2P 端到端（本机 loopback，真实握手/加密/收发）----
    /// 起一对已互相配对的节点：B 监听、A 主动连。
    ///
    /// 走真实的 `xime-p2p` 路径（Noise IK 握手 + 加密帧），只有端口是顺延的，
    /// 因此不依赖固定 47821（本机 daemon 很可能正占着它）。
    fn p2p_pair() -> (tempfile::TempDir, SyncState, SyncState) {
        use xime_p2p::{Node, NodeConfig, PairedPeer, PeerStore};

        let tmp = tempfile::tempdir().unwrap();
        let a_dir = tmp.path().join("a");
        let b_dir = tmp.path().join("b");
        std::fs::create_dir_all(&a_dir).unwrap();
        std::fs::create_dir_all(&b_dir).unwrap();
        let ida = xime_p2p::load_or_create_identity(&a_dir).unwrap();
        let idb = xime_p2p::load_or_create_identity(&b_dir).unwrap();

        // B 先起：A 的配对表里要写 B 的实际监听端口（可能已顺延）。
        let mut b_store = PeerStore::load(b_dir.join("peers.json")).unwrap();
        b_store
            .upsert(PairedPeer::from_public(&ida.public("A"), None))
            .unwrap();
        let node_b = Node::start(NodeConfig {
            identity: idb.clone(),
            store_path: b_dir.join("peers.json"),
            listen: true,
            discover: false,
            port: xime_p2p::TCP_PORT,
            connect_timeout: Duration::from_secs(3),
        })
        .unwrap();

        let mut a_store = PeerStore::load(a_dir.join("peers.json")).unwrap();
        a_store
            .upsert(PairedPeer::from_public(
                &idb.public("B"),
                Some(format!("127.0.0.1:{}", node_b.local_port())),
            ))
            .unwrap();
        // A 不监听、不广播：测试期间不往局域网发任何包。
        let node_a = Node::start(NodeConfig {
            identity: ida,
            store_path: a_dir.join("peers.json"),
            listen: false,
            discover: false,
            port: 0,
            connect_timeout: Duration::from_secs(3),
        })
        .unwrap();

        let store_a = Arc::new(ClipboardStore::open(a_dir.join("clipboard.db")).unwrap());
        let store_b = Arc::new(ClipboardStore::open(b_dir.join("clipboard.db")).unwrap());
        let state = |store: Arc<ClipboardStore>, node| SyncState {
            store,
            runtimes: HashMap::new(),
            node: Some(node),
            config_mtime: None,
            peer_connected: false,
            last_pushed_hash: None,
            last_local_text: None,
        };
        (tmp, state(store_a, node_a), state(store_b, node_b))
    }

    /// 轮询到条件成立或超时（`done` 拿到的是刚 poll 过的状态）。
    fn wait_until(
        state: &mut SyncState,
        timeout: Duration,
        done: impl Fn(&SyncState) -> bool,
    ) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            state.poll_events();
            if done(state) {
                return true;
            }
            if std::time::Instant::now() > deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn test_p2p_push_reaches_peer_store() {
        let (_tmp, mut a, mut b) = p2p_pair();

        // 先等连接建立：Node 在无连接时会丢掉 set_clipboard 的指令。
        assert!(
            wait_until(&mut a, Duration::from_secs(10), |s| s.peer_connected),
            "A 端未在超时内连上 B"
        );

        a.handle_captured("p2p hello");
        assert!(
            wait_until(&mut b, Duration::from_secs(10), |s| {
                s.store.count_clipboard().unwrap() > 0
            }),
            "B 端未在超时内收到剪贴板"
        );
        let items = b.store.list_clipboard(10).unwrap();
        assert_eq!(items[0].text, "p2p hello");
        // 回声抑制：A 自己推出去的内容不应写回本地
        assert_eq!(
            a.store.count_clipboard().unwrap(),
            0,
            "A 不应收录自己的推送"
        );
    }

    #[test]
    fn test_p2p_ignores_echo_of_own_push() {
        let (_tmp, mut a, mut b) = p2p_pair();
        assert!(
            wait_until(&mut a, Duration::from_secs(10), |s| s.peer_connected),
            "A 端未在超时内连上 B"
        );
        a.handle_captured("echo me");
        // 手动把「自己推的」profile 喂给 A：hash 与 last_pushed_hash 相同 → 丢弃
        let hash = text_hash("echo me");
        a.handle_remote_profile(Profile::from_text("echo me", Some("peer".into())));
        assert_eq!(a.store.count_clipboard().unwrap(), 0);
        assert_eq!(a.last_pushed_hash.as_deref(), Some(hash.as_str()));
        // 对端收到的是同一条
        assert!(wait_until(&mut b, Duration::from_secs(10), |s| {
            s.store.count_clipboard().unwrap() > 0
        }));
    }
}
