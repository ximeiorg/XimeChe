use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use tracing::{debug, error, info, warn};
use xime_clipboard::store::ClipboardStore;
use xime_config::XimeConfig;
use xime_tray::{InputMode, TrayManager};
use xime_ui::PanelTheme;
use xime_ui::{PanelGrid, PanelHit, PanelList, PanelListItem, PanelPage, PANEL_MIN_WIDTH};
use xime_wayland::{connect_im_from_fd, connect_im_to_env, ImBackend};
use xime_xkb::XkbContext;
use xime_xkb::{keysym_to_letter, Keysym, ModifierState};

use crate::clipboard_sync::{scan_backend, SyncMessage};
use crate::{symbols, DaemonCommand, PluginHost, RimeEngine};

/// 面板路由状态（对齐 XimeYao：菜单为默认页，子页共用面板区）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum PanelState {
    #[default]
    Closed,
    Open(PanelPage),
}

/// 面板共享数据（daemon 持有权威状态；show 时整份注入 im 层）。
#[derive(Default)]
struct PanelData {
    list: PanelList,
    grid: PanelGrid,
    /// 网格数据源（表情/符号分类全量，refresh_grid_cells 按 tab/page 取窗）。
    grid_source: Vec<(String, Vec<String>)>,
}

/// 快捷发送编码注入状态（对齐 Android quick-send-demo 的"编码命中"语义）。
///
/// Rime 候选存在时：快捷发送条目**追加**在 Rime 当页候选之后（数字键超出
/// Rime 候选数的部分由宿主接管）；Rime 无候选时：完全接管（高亮/翻行/回车）。
#[derive(Debug, Default)]
struct QuickSendInject {
    /// 匹配到的 (code, text) 条目。
    items: Vec<(String, String)>,
    /// Rime 当页候选数（决定数字键归属）。
    rime_count: usize,
    /// Rime 无候选时（接管模式）的本地高亮。
    highlighted: usize,
}

impl QuickSendInject {
    fn active(&self) -> bool {
        !self.items.is_empty()
    }

    fn clear(&mut self) {
        self.items.clear();
        self.rime_count = 0;
        self.highlighted = 0;
    }

    /// 数字键位置（0 起）是否命中快捷发送条目；返回条目索引。
    fn digit_hit(&self, pos: usize) -> Option<usize> {
        pos.checked_sub(self.rime_count)
            .filter(|&i| i < self.items.len())
    }
}

/// 快捷发送编码前缀匹配（跳过无编码条目），最多 `limit` 条。
fn match_quick_send_codes(
    entries: &[(String, String)],
    raw_input: &str,
    limit: usize,
) -> Vec<(String, String)> {
    if raw_input.is_empty() {
        return Vec::new();
    }
    entries
        .iter()
        .filter(|(code, _)| code.starts_with(raw_input))
        .take(limit)
        .cloned()
        .collect()
}

/// 从配置构建渲染主题（亮/暗模式 + 字号/圆角/高亮色）。
fn build_theme(config: &XimeConfig, dark: bool) -> PanelTheme {
    PanelTheme::for_mode(
        dark,
        config.style.font_size,
        config.style.corner_radius,
        config.get_primary_color(),
    )
}

/// 决定按键是否转发给应用（系统快捷键优先 + 孤儿释放抑制）。
/// - 带 Ctrl/Alt/Super 的组合键**一律**转发：它们是应用/桌面的快捷键，输入法
///   只能"借用"（如 Ctrl+Space、Ctrl+Alt+V，那些在上游已单独消费），不能吞。
///   Rime 的 emacs 键绑定（`Control+a → Home` 等，`when: composing`）会消费这类
///   按键，若照"被消费就不转发"处理，组合态下 Ctrl+A/C/V 就全废了。
/// - 其余按键：Rime 消费的按下（`press_consumed`）不转发，其释放事件也不转发，
///   避免孤儿释放；未被消费的按下/释放正常转发。
fn should_forward_key(
    pressed: bool,
    result: bool,
    consumed_presses: &std::collections::HashSet<u32>,
    key: u32,
    system_shortcut: bool,
) -> bool {
    if system_shortcut {
        return true;
    }
    let press_consumed = result && pressed;
    let release_of_consumed = !pressed && consumed_presses.contains(&key);
    !press_consumed && !release_of_consumed
}

/// 是否带 Ctrl/Alt/Super 的应用快捷键修饰（AltGr 是 Mod5，不计入 alt）。
fn is_system_shortcut(modifiers: &ModifierState) -> bool {
    modifiers.ctrl || modifiers.alt || modifiers.super_key
}

/// 按住重复状态。
///
/// `zwp_input_method_keyboard_grab_v1/v2` 在 grab 期间**不再由合成器重复按键**：
/// 合成器只发 `repeat_info`（rate/delay），重复必须由输入法自己重投「按下」
/// 事件。不实现的话，按住方向键只会移动一格——2026-11-14 用户报的 bug。
struct KeyRepeat {
    key: u32,
    serial: u32,
    /// 首次按下时的合成器时间戳；重复事件按经过的毫秒数递增。
    time: u32,
    started: Instant,
    next: Instant,
    interval: Duration,
}

impl KeyRepeat {
    /// 合成一次重复按下的事件。`time` 用首按时间 + 经过毫秒，避免所有重复
    /// 事件共用同一个时间戳（部分应用会按 time 做双击/长按判定）。
    fn synth_event(&self) -> xime_wayland::KeyEvent {
        xime_wayland::KeyEvent {
            serial: self.serial,
            time: self
                .time
                .wrapping_add(self.started.elapsed().as_millis() as u32),
            key: self.key,
            pressed: true,
            // 兜底定时器合成的是普通按下；Repeated 只在合成器自己发时出现。
            repeated: false,
        }
    }
}

/// `repeat_info.rate`（次/秒）换算成重复间隔；rate <= 0 表示禁用重复。
fn repeat_interval(rate: i32) -> Option<Duration> {
    if rate <= 0 {
        return None;
    }
    // 上限 200Hz：异常大的值会变成忙循环，且 200Hz 已快过任何真实键盘。
    let per_second = rate.min(200) as u64;
    Some(Duration::from_millis((1000 / per_second).max(1)))
}

/// `repeat_info.delay`（毫秒）限幅，异常值不应让长按永远不开始重复。
fn repeat_delay(delay: i32) -> Duration {
    Duration::from_millis(delay.clamp(1, 5000) as u64)
}

/// 根据真实按键事件维护重复状态。
///
/// - 释放：只清同一键的重复（按住 Right 时松开 Shift 不应停止光标移动）；
/// - 修饰键按下：不重复、也不打断正在重复的键（Shift+方向键选择要连续）；
/// - 被输入法消费的按下：不重复（Ctrl+Space 启停、面板导航、语音热键等
///   重复投递会变成连续触发）。方向键在无组合时不被消费，正常重复。
fn update_key_repeat(
    current: &mut Option<KeyRepeat>,
    event: xime_wayland::KeyEvent,
    sym: Keysym,
    consumed: bool,
    rate: i32,
    delay: i32,
    now: Instant,
) {
    if !event.pressed {
        if current.as_ref().is_some_and(|r| r.key == event.key) {
            *current = None;
        }
        return;
    }
    if xime_xkb::is_modifier_keysym(sym.raw()) {
        return;
    }
    let Some(interval) = repeat_interval(rate).filter(|_| !consumed) else {
        *current = None;
        return;
    };
    *current = Some(KeyRepeat {
        key: event.key,
        serial: event.serial,
        time: event.time,
        started: now,
        next: now + repeat_delay(delay),
        interval,
    });
}

/// 空格兜底判定（对齐 XimeYao）：组合态 + 无候选时，空格不上屏首选
/// （没有首选可选）而应直接上屏编码本身。
///
/// 返回 Some(编码) = 拦截空格并上屏该文本；None = 交给 Rime 正常处理。
/// 带 Ctrl/Alt/Shift/Super 的空格（启停 IM / 全半角切换 / 窗口快捷键）
/// 一律不拦。
fn space_fallback_input(
    sym: u32,
    modifiers: &ModifierState,
    raw_input: Option<&str>,
    num_candidates: usize,
) -> Option<String> {
    if sym != 0x20 || modifiers.ctrl || modifiers.alt || modifiers.shift || modifiers.super_key {
        return None;
    }
    let input = raw_input?;
    if input.is_empty() || num_candidates > 0 {
        return None;
    }
    Some(input.to_string())
}

/// Rime ascii_mode → 托盘显示模式（中/英）。
fn tray_mode_for(is_ascii: bool) -> InputMode {
    if is_ascii {
        InputMode::English
    } else {
        InputMode::Chinese
    }
}

/// 语音输入开关快捷键：Ctrl+Alt+V（大小写不敏感；Super 修饰不触发）。
/// 硬编码绑定——悬浮层状态行文案 `VOICE_HOTKEY_LABEL` 与此同源；配置化
/// 等 libximecore hotkeys 缺键继承语义修复后一并做（见 PROGRESS 遗留）。
fn is_voice_toggle_hotkey(sym_raw: u32, modifiers: &ModifierState) -> bool {
    modifiers.ctrl
        && modifiers.alt
        && !modifiers.super_key
        && keysym_to_letter(sym_raw).is_some_and(|l| l == 'v')
}

/// 最近一次候选窗内容（菜单开/关后重绘用，主题以当前值为准）。
type CandidateCache = (Vec<xime_ui::CandidateItem>, usize);

/// 语音悬浮层重绘节拍（~30fps；电平是流数据，事件之间也要滚动/衰减）。
const VOICE_FRAME_INTERVAL: Duration = Duration::from_millis(33);

/// 语音悬浮层的 daemon 侧状态（数据权威在 speech 模块：电平快照经
/// levels_snapshot() 取，这里只存绘制所需的文本/相位与重绘节拍）。
#[derive(Default)]
struct VoiceUiState {
    /// 悬浮层是否在屏（v1 后端才可能为 true；v2 回落候选栏反馈）。
    visible: bool,
    /// 模型装载中（状态行提示装载，频谱静止）。
    loading: bool,
    /// 当前 partial 文本（Partial 事件写入，重绘时读取）。
    text: String,
    /// 上次悬浮层重绘时刻（节流用）。
    last_frame: Option<Instant>,
}

/// 断连自愈的节奏控制：
/// - 首次断连后 1 秒即尝试（给可能在途的 launcher fd 让路）；
/// - 之后的自愈动作间隔固定 25 秒——KWin 对 IM 客户端有崩溃保护：20 秒内
///   崩溃计数到 5 就 stopInputMethod 永久停用（src/inputmethod.cpp），
///   计数器在无崩溃 20 秒后清零，因此自愈杀 launcher 的间隔必须 >20s，
///   保证 KWin 计数永远数不到 2。
const HEAL_QUICK_RETRY: Duration = Duration::from_secs(1);
const HEAL_KILL_SPACING: Duration = Duration::from_secs(25);

struct HealScheduler {
    fire_at: Option<Instant>,
    last_kill: Option<Instant>,
}

impl HealScheduler {
    fn new() -> Self {
        Self {
            fire_at: None,
            last_kill: None,
        }
    }

    /// 连接断开：安排下一次自愈时间。
    fn on_disconnect(&mut self, now: Instant) {
        let quick = now + HEAL_QUICK_RETRY;
        let spaced = self.last_kill.map_or(quick, |t| t + HEAL_KILL_SPACING);
        self.fire_at = Some(quick.max(spaced));
    }

    /// 成功连上（launcher fd 或直连）：清掉待触发动作，保留 last_kill
    /// （跨重连的 25 秒间隔约束仍然有效）。
    fn on_connected(&mut self) {
        self.fire_at = None;
    }

    fn due(&self, now: Instant) -> bool {
        self.fire_at.is_some_and(|t| now >= t)
    }

    /// 触发一次自愈动作后顺延下一次（无论动作是否杀掉了进程，
    /// 都不能让自愈循环 1ms 空转重试）。
    fn step(&mut self, now: Instant) {
        self.fire_at = Some(now + HEAL_KILL_SPACING);
    }

    /// 真的终结了 launcher：记录时刻，让后续断连的重试时间不得早于
    /// 本次 +25 秒（KWin 崩溃计数清零窗口）。
    fn mark_killed(&mut self, now: Instant) {
        self.last_kill = Some(now);
        self.fire_at = Some(now + HEAL_KILL_SPACING);
    }
}

/// 检测 xime-launcher 是否在运行，用于区分 KWin 会话与 standalone 会话。
///
/// KWin 会话下 launcher 常驻（KWin 拉起并保持），此时 daemon 必须等它传
/// fd，绝不能抢先直连——否则会出现两个 IM 客户端抢 keyboard grab
/// （见主循环处 2026-10-26 的说明）。
fn xime_launcher_running() -> bool {
    // pgrep -x 精确匹配进程名，退出码 0 = 存在
    matches!(
        std::process::Command::new("pgrep")
            .args(["-x", "xime-launcher"])
            .stdout(std::process::Stdio::null())
            .status(),
        Ok(status) if status.success()
    )
}

/// 终结滞留的 xime-launcher 进程。KWin 只在 IM 客户端 CrashExit 时重拉
/// launcher（干净退出不重拉），新 launcher 会重新调 OpenWaylandSocket
/// 传入新 fd，整链恢复。返回是否真的终结了进程。
fn kill_lingering_launcher() -> bool {
    match std::process::Command::new("pkill")
        .args(["-TERM", "-x", "xime-launcher"])
        .status()
    {
        Ok(status) if status.success() => {
            warn!(
                "Self-heal: killed lingering xime-launcher, KWin should respawn it with a fresh fd"
            );
            true
        }
        Ok(_) => {
            // pkill 退出码 1 = 没有匹配进程（launcher 已死或 standalone 会话）
            debug!("Self-heal: no lingering xime-launcher process found");
            false
        }
        Err(e) => {
            warn!("Self-heal: failed to run pkill: {}", e);
            false
        }
    }
}

/// 干净退出：给 non-blocking 文件日志一点排空时间，然后 _exit 跳过
/// C++ 静态析构。librime 的静态 Service 析构在 exit() 路径会段错误
/// （coredump 2026-10-04 ×3：CleanupAllSessions → ConcreteEngine 释放
/// Translator 时崩），把本应 exit(0) 的正常退出变成 CrashExit。
fn clean_exit() -> ! {
    std::thread::sleep(Duration::from_millis(150));
    unsafe { libc::_exit(0) }
}

/// 退出前拆除 IM 连接，让合成器立刻回收 keyboard grab。
///
/// 2026-10-26 实锤：`clean_exit()` 直接 `_exit(0)` 跳过所有析构，Wayland
/// 连接不会走正常关闭流程。KWin 侧若没在客户端消失时及时回收 grab，键盘
/// 事件会持续被路由到一个已死客户端的通道——表现就是**系统级快捷键全部
/// 失灵**（Alt+Tab、Super、Ctrl+C 一概无反应）。
///
/// `zwp_input_method_v1` 没有 release grab 请求，客户端无法主动释放；能做
/// 的是显式拆对象 + flush + 关闭连接，让 KWin 走"客户端断开"路径立刻回收。
fn shutdown_im_connection(conn: &mut Option<Box<dyn ImBackend>>) {
    if let Some(mut c) = conn.take() {
        if let Err(e) = c.shutdown() {
            warn!("Shutdown: IM teardown failed: {}", e);
        }
        // flush 已包含在 shutdown 内；这里再兜一次，确保断开前请求送达。
        if let Err(e) = c.flush() {
            warn!("Shutdown: IM flush failed: {}", e);
        }
        drop(c);
    }
}

pub struct WaylandLoop {
    command_rx: Receiver<DaemonCommand>,
    tray: Arc<TrayManager>,
    rt_handle: tokio::runtime::Handle,
    /// 最近一次候选内容缓存（菜单开/关后重绘用）。
    candidate_cache: std::sync::Mutex<Option<CandidateCache>>,
    /// 剪贴板/快捷发送存储。
    clipboard: Arc<ClipboardStore>,
    /// 剪贴板同步桥命令通道。
    sync_tx: std::sync::mpsc::Sender<SyncMessage>,
}

impl WaylandLoop {
    pub fn new(
        command_rx: Receiver<DaemonCommand>,
        tray: Arc<TrayManager>,
        rt_handle: tokio::runtime::Handle,
        clipboard: Arc<ClipboardStore>,
        sync_tx: std::sync::mpsc::Sender<SyncMessage>,
    ) -> Self {
        Self {
            command_rx,
            tray,
            rt_handle,
            candidate_cache: std::sync::Mutex::new(None),
            clipboard,
            sync_tx,
        }
    }

    pub fn run(self) {
        info!("Wayland loop thread started");
        crate::speech::init();

        let mut conn: Option<Box<dyn ImBackend>> = None;
        let mut xkb: Option<XkbContext> = None;
        let mut rime = RimeEngine::new();
        let mut plugin_host = PluginHost::new();
        let mut xime_config = XimeConfig::load();
        let _last_key_root_binding = xime_config.get_last_key_root_binding();
        let primary_color = xime_config.get_primary_color();
        self.rt_handle.block_on(async {
            self.tray.set_primary_color(primary_color).await;
        });
        // 渲染主题：配置样式 + 亮/暗模式（暗色由 portal 监听任务推送）。
        let mut dark_mode = false;
        let mut theme = build_theme(&xime_config, dark_mode);
        debug!(
            "Loaded hotkeys: show_key={}, primary_color={:?}",
            xime_config.wubi_radicals.hotkeys.show_key, primary_color
        );

        let mut candidate_window_visible = false;
        let mut panel = PanelData::default();
        let mut quick_send = QuickSendInject::default();
        let mut panel_state = PanelState::Closed;
        let mut last_panel_width: u32 = 0;
        let mut consumed_presses: std::collections::HashSet<u32> = std::collections::HashSet::new();
        // 按住重复：grab 期间合成器不自己重复，由主循环按 repeat_info 重投。
        let mut key_repeat: Option<KeyRepeat> = None;
        let mut last_input_keysym: Option<u32> = None;
        let mut ctrl_root_visible = false;
        let mut last_ascii_mode = false;
        let mut last_active = false;
        // 注意：**不要在这里维护「上次转发的修饰键状态」**。
        // KWin 会为多个窗口同时保留多个 zwp_input_method_context_v1，
        // 任何全局单值缓存都会让「按下」发给 A、「松开」发给 B，A 那侧
        // 永久残留 Ctrl 按下态（鼠标点击不会触发按键，永远纠正不回来）。
        // 详见 DECISIONS.md「禁止转发 modifiers」。
        // 输入法启停开关（Ctrl+Space 切换）：停用态按键直接转发，不做任何处理。
        let mut im_enabled = true;
        // 暗色模式切换后待重绘标记（渲染需在 conn 作用域内进行）
        let mut pending_theme_redraw = false;
        // 断连自愈调度：见 HealScheduler。
        let mut heal = HealScheduler::new();
        // 语音悬浮层状态（v1 后端支持时替代候选栏反馈）。
        let mut voice = VoiceUiState::default();

        // 剪贴板同步桥初始化：按 clipboard_sync.toml 选择后端（插件/P2P）并拉取一次
        let _ = self
            .sync_tx
            .send(SyncMessage::Reload(Box::new(scan_backend())));
        let _ = self.sync_tx.send(SyncMessage::Pull);

        // 无 launcher 的会话（GNOME 等）：直接连接 $WAYLAND_DISPLAY 使用 v2 协议。
        //
        // 2026-10-26 修正：原假设「KWin 下普通 socket 不暴露 IM 协议，此步
        // 必然失败」在 KWin 6.7.4 上**不成立**——普通 socket 同样暴露
        // zwp_input_method_v1，直连会成功。此时 launcher 随后送来 fd，daemon
        // 就有了两条连接、两个 IM 对象同时向 KWin 注册，双方争抢 keyboard
        // grab，表现为 ACTIVATE/DEACTIVATE 成对快速抖动、按键被吞，用户侧
        // 就是"输入法完全失效"。而 zwp_input_method_v1 没有 destroy 请求，
        // 客户端无法主动注销旧对象，只能靠断开整条连接。
        //
        // 因此：只要 launcher 在运行（KWin 会话），就不做直连，改由 launcher
        // 传 fd 建立唯一连接。launcher 缺席（GNOME/wlroots）时才直连。
        let launcher_running = xime_launcher_running();
        if launcher_running {
            debug!("xime-launcher is running, deferring to its fd (KWin mode)");
        } else {
            match connect_im_to_env() {
                Ok(backend) => {
                    info!("Connected directly to compositor (standalone mode)");
                    conn = Some(backend);
                }
                Err(e) => {
                    debug!(
                        "Direct connection not available (waiting for launcher fd): {}",
                        e
                    );
                    // 排个自愈时间，若 launcher 的 fd 一直不来（launcher 启动即
                    // 死等场景），到点后杀滞留 launcher 逼 KWin 重拉，而不是永久干等。
                    heal.on_disconnect(Instant::now());
                }
            }
        }

        loop {
            use std::sync::mpsc::TryRecvError;

            match self.command_rx.try_recv() {
                Ok(DaemonCommand::OpenWaylandSocket(fd, display_name)) => {
                    debug!("Connecting from fd for display {}", display_name);

                    xkb = XkbContext::new().ok();

                    // 旧连接必须显式拆除后再接管新 fd。
                    //
                    // 背景（2026-10-26 实锤）：启动时 `connect_im_to_env()`
                    // 在 KWin 6.x 上**也可能成功**——KWin 6.7.4 的普通 socket
                    // 同样暴露 zwp_input_method_v1，与"KWin 下直连必败"的旧
                    // 假设不符。此时 launcher 再送来 fd，若直接 `conn = Some(新)`
                    // 覆盖，旧连接不会 drop，其 IM 对象仍在 KWin 侧注册——
                    // KWin 眼里出现两个输入法客户端，双方争抢 keyboard grab，
                    // 表现为 ACTIVATE/DEACTIVATE 成对快速抖动、按键被吞，
                    // 用户侧就是"输入法完全失效"。
                    //
                    // 拆除顺序：先 shutdown 通知 KWin 注销 IM 对象，再 drop
                    // 释放 socket（drop 本身不保证对象销毁消息送达）。
                    if let Some(mut old) = conn.take() {
                        debug!("Dropping previous IM connection before launcher takeover");
                        if let Err(e) = old.shutdown() {
                            warn!("Previous IM connection shutdown failed: {}", e);
                        }
                        drop(old);
                        last_active = false;
                    }

                    match connect_im_from_fd(fd) {
                        Ok(backend) => {
                            info!("Connected via launcher fd (KWin mode)");
                            // 托盘菜单注入方案切换组（levers 此时可用了）。
                            let schemas = rime.available_schemas();
                            let current = rime.get_current_schema().unwrap_or_default();
                            self.rt_handle
                                .block_on(self.tray.update_schema_menu(schemas, current));
                            conn = Some(backend);
                            heal.on_connected();
                        }
                        Err(e) => {
                            error!("Failed to connect: {}", e);
                            if conn.is_none() {
                                heal.on_disconnect(Instant::now());
                            }
                        }
                    }
                }
                Ok(DaemonCommand::ToggleMode) => {
                    debug!("ToggleMode command received");
                    if rime.session().is_some() {
                        let new_ascii = rime.toggle_ascii_mode();
                        last_ascii_mode = new_ascii;
                        let tray_mode = if new_ascii {
                            InputMode::English
                        } else {
                            InputMode::Chinese
                        };
                        self.rt_handle.block_on(async {
                            self.tray.set_mode(tray_mode).await;
                        });
                        debug!("Tray updated after toggle: ascii_mode={}", new_ascii);

                        // IM 未激活时（如旧 Chromium 应用复制后不重新 enable
                        // text-input），键盘事件完全不经过 IM，Shift/Ctrl+Space
                        // 都无效；托盘点击是唯一可达的控制通道，借 KWin 的
                        // forceActivate 强制激活，让本次切换真正生效。
                        if !last_active {
                            debug!("IM inactive on toggle, requesting KWin forceActivate");
                            self.rt_handle.block_on(self.tray.force_activate_im());
                        }
                    }
                }
                Ok(DaemonCommand::ToggleSpeech) => {
                    // 托盘「语音输入」入口。键盘快捷键只在 IM 激活（键盘
                    // grab）时可达；未激活时启动会话，识别文本无处上屏
                    // （commit 需要 text-input 上下文），如实拒绝并提示。
                    if !last_active {
                        info!("Voice toggle requested while IM inactive, rejected");
                        let handle = self.rt_handle.clone();
                        handle.spawn(async move {
                            notify_desktop("语音输入", "请先点击要输入的输入框，再开始语音输入")
                                .await;
                        });
                    } else {
                        crate::speech::toggle();
                    }
                }
                Ok(DaemonCommand::Deploy) => {
                    debug!("Deploy command received, starting Rime deployment...");

                    // 与托盘左键同一套恢复逻辑：KWin 在切换窗口后不再重算
                    // IM 激活态（refreshActive 只由客户端 text-input
                    // enable/disable 触发），IM 未激活时按键根本不经过 daemon，
                    // 此时"重新部署"只能靠菜单抢焦点这一副作用恢复。这里显式
                    // 请求 KWin forceActivate，不依赖上述副作用。
                    if !last_active {
                        debug!("IM inactive on deploy, requesting KWin forceActivate");
                        self.rt_handle.block_on(self.tray.force_activate_im());
                    }

                    let result = rime.redeploy_with_result();
                    let (summary, body) = match result {
                        librime::DeployResult::Success => {
                            ("部署完成".to_string(), "Rime 配置已重新加载".to_string())
                        }
                        librime::DeployResult::Failure => (
                            "部署失败".to_string(),
                            "Rime 部署返回失败，请检查 xime.log 或配置文件".to_string(),
                        ),
                    };
                    let handle = self.rt_handle.clone();
                    handle.spawn(async move {
                        notify_desktop(&summary, &body).await;
                    });
                }
                Ok(DaemonCommand::ReloadStyle) => {
                    debug!("ReloadStyle command received, reloading xime config...");
                    let new_config = XimeConfig::load();
                    let new_color = new_config.get_primary_color();
                    self.rt_handle.block_on(async {
                        self.tray.set_primary_color(new_color).await;
                    });
                    xime_config = new_config;
                    // log_level 热生效：设置程序保存任意配置都会触发
                    // ReloadStyle，用户改完日志级别无需重启 daemon。
                    crate::apply_log_level(&xime_config);
                    theme = build_theme(&xime_config, dark_mode);

                    debug!("Style config reloaded, new primary_color={:?}", new_color);
                }
                Ok(DaemonCommand::DarkMode(dark)) => {
                    debug!("DarkMode command received: dark={}", dark);
                    dark_mode = dark;
                    theme = build_theme(&xime_config, dark_mode);
                    // 候选栏可见时标记重绘（渲染需在 conn 作用域内进行）
                    pending_theme_redraw = true;
                }
                Ok(DaemonCommand::ReloadPlugins) => {
                    debug!("ReloadPlugins command received, reloading plugins...");
                    plugin_host.reload();
                    // 同步桥：按最新配置重选后端（插件/P2P）并立即拉取一次
                    let _ = self
                        .sync_tx
                        .send(SyncMessage::Reload(Box::new(scan_backend())));
                    let _ = self.sync_tx.send(SyncMessage::Pull);
                }
                Ok(DaemonCommand::SelectSchema(schema_id, result_tx)) => {
                    debug!("SelectSchema command received: {}", schema_id);
                    let ok = rime.select_schema(&schema_id);
                    let _ = result_tx.send(ok);
                    debug!("SelectSchema result: {}", ok);
                    if ok {
                        // 托盘菜单 ✓ 跟随（列表不变，只换选中项）。
                        let schemas = rime.available_schemas();
                        self.rt_handle
                            .block_on(self.tray.update_schema_menu(schemas, schema_id));
                        // 新方案的中英状态可能与切换前不同（切换后回方案默认），
                        // 托盘中/英跟随实际状态，不能停留在切换前的显示。
                        self.sync_tray_after_session_rebuild(&rime, &mut last_ascii_mode);
                    }
                }
                Ok(DaemonCommand::ListDictEntries(dict, query, result_tx)) => {
                    debug!("ListDictEntries command received: {dict} query={query:?}");
                    // 关会话 → 导出（userdb 独占）→ 重建；期间按键会丢 composition，
                    // 设置页词典浏览与打字互斥（对齐 XimeYao 词典写路径的语义）。
                    let result = rime
                        .with_user_dict_closed(|| crate::user_dict::list_entries(&dict, &query));
                    let _ = result_tx.send(result);
                    self.sync_tray_after_session_rebuild(&rime, &mut last_ascii_mode);
                }
                Ok(DaemonCommand::UserDictOp(op, result_tx)) => {
                    debug!("UserDictOp command received: {op:?}");
                    let result = rime.with_user_dict_closed(|| op.run());
                    let _ = result_tx.send(result);
                    self.sync_tray_after_session_rebuild(&rime, &mut last_ascii_mode);
                }
                Ok(DaemonCommand::Shutdown) => {
                    // 必须整进程退出：DBus 主循环不感知该命令；clean_exit 保
                    // 证退出码为 0（librime 静态析构段错误会让 exit() 变成
                    // CrashExit，触发 KWin 崩溃保护计数）。
                    //
                    // 但退出前必须先拆 IM 连接并 flush：_exit 跳过析构，
                    // 连接不会正常关闭，KWin 侧的 keyboard grab 可能残留导致
                    // 系统级快捷键全部失灵（2026-10-26 实锤）。
                    info!("Shutdown requested, tearing down IM connection then exiting");
                    shutdown_im_connection(&mut conn);
                    clean_exit();
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    // 命令通道是 daemon 的生命线（fd/托盘/部署全走这里），
                    // 断了只能整进程退出；返回 run() 会让线程静默死亡，
                    // daemon 变成"活着但永远收不到 fd"的僵尸。
                    error!("Command channel disconnected, tearing down IM then exiting");
                    shutdown_im_connection(&mut conn);
                    clean_exit();
                }
            }

            // 断连自愈：优先尝试直连（standalone/GNOME 会话）；KWin 模式下
            // 普通 socket 不暴露 IM 协议，直连必败，改为终结滞留的 launcher
            // ——KWin 只在 IM 客户端 CrashExit 时重拉，新 launcher 重新传 fd。
            if conn.is_none() && heal.due(Instant::now()) {
                heal.step(Instant::now());
                match connect_im_to_env() {
                    Ok(backend) => {
                        info!("Self-heal: reconnected directly to compositor (standalone mode)");
                        conn = Some(backend);
                        heal.on_connected();
                    }
                    Err(_) => {
                        if kill_lingering_launcher() {
                            heal.mark_killed(Instant::now());
                        }
                    }
                }
            }

            if let Some(c) = conn.as_mut() {
                if let Err(e) = c.dispatch_events() {
                    // 升级为 error：断连原因必须落到文件日志（此前只有
                    // debug 级，INFO 文件里完全看不到死因，无法定位）。
                    error!("Wayland connection lost (dispatch error): {}", e);
                    conn = None;
                    self.rt_handle.block_on(async {
                        self.tray.set_visible(false).await;
                    });
                    last_active = false;
                    key_repeat = None;
                    heal.on_disconnect(Instant::now());
                    continue;
                }

                if let Err(e) = c.handle_unavailable() {
                    warn!("handle_unavailable error: {}", e);
                }

                let is_active = c.is_active();

                if is_active != last_active {
                    debug!("State changed: active={}", is_active);
                    // 托盘常驻：失活时不再隐藏图标（fcitx5 风格）。图标是 IM
                    // 未激活时唯一可达的控制入口（键盘事件不经过 IM），藏掉
                    // 会让用户在"卡死"时失去恢复手段。
                    if is_active {
                        self.rt_handle.block_on(async {
                            self.tray.set_visible(true).await;
                        });
                    }
                    last_active = is_active;

                    if is_active {
                        // 重新激活：清掉可能残留的面板状态（KWin 在部分应用
                        // 上不发 Deactivate——面板在切窗后挂在屏幕上，回到
                        // 可输入应用打字时它还开着）。候选栏不强行恢复：
                        // 等 Rime 组合按需重建。
                        debug!("Residual panel on re-activate, dismissing");
                        panel_state = PanelState::Closed;
                        panel = PanelData::default();
                        key_repeat = None;
                        // 兜底清滞留语音事件：失活路径的 clear_events 可能早于
                        // worker finalize 发出 Committed（收尾是异步的），不清
                        // 的话上一窗口的尾巴文本会在新窗口首次 drain 时上屏。
                        crate::speech::clear_events();
                    }

                    if !is_active {
                        // 失焦（切换窗口/输入框）时彻底清理 UI 状态：
                        // 立即隐藏候选栏/菜单面板/Ctrl 字根/语音悬浮窗口，
                        // 关闭 emoji 面板，清除按键消费记录与字根缓存。
                        // 否则残留的候选栏会一直显示在新输入框上，遮挡并吞掉
                        // 点击事件，导致新输入框无法获得焦点、输入法无法重新激活。
                        c.hide_candidate_window();
                        c.hide_panel();
                        c.hide_root_window();
                        c.hide_voice_overlay();
                        voice.visible = false;
                        voice.text.clear();
                        let _ = c.flush();
                        candidate_window_visible = false;
                        quick_send.clear();
                        panel_state = PanelState::Closed;
                        ctrl_root_visible = false;
                        last_input_keysym = None;
                        consumed_presses.clear();
                        key_repeat = None;
                        // 候选缓存同样要清：语音会话结束的 redraw 会用缓存
                        // 复活候选栏——不清的话 A 窗的候选栏会叠在新窗口上
                        //（Idle 事件可能在新窗口激活后才到达）。
                        if let Ok(mut cache) = self.candidate_cache.lock() {
                            *cache = None;
                        }
                        // 听写中失焦：停止会话（没有焦点的窗口上继续录音没有
                        // 意义）。滞留事件与电平一并清空——收尾的 Committed
                        // 不允许迟到投递到下一个焦点窗口（A 窗说的话上屏到
                        // B 窗是错乱行为）；重新激活时再兜底清一次。
                        if crate::speech::state() != crate::speech::SpeechState::Idle {
                            crate::speech::toggle();
                        }
                        crate::speech::clear_events();
                        continue;
                    }
                }

                if is_active {
                    self.handle_active_state(
                        c.as_mut(),
                        &mut xkb,
                        &mut rime,
                        &mut plugin_host,
                        &mut panel,
                        &mut quick_send,
                        &mut panel_state,
                        &mut last_panel_width,
                        &xime_config,
                        &theme,
                        &mut candidate_window_visible,
                        &mut consumed_presses,
                        &mut last_input_keysym,
                        &mut ctrl_root_visible,
                        &mut last_ascii_mode,
                        &mut im_enabled,
                        &mut voice,
                        &mut key_repeat,
                    );
                }

                // 主题切换后的候选栏重绘（仅候选栏可见且面板未展开时）
                if pending_theme_redraw {
                    pending_theme_redraw = false;
                    if candidate_window_visible && matches!(panel_state, PanelState::Closed) {
                        self.redraw_menu_candidates(
                            c.as_mut(),
                            &theme,
                            &mut candidate_window_visible,
                        );
                    }
                }
            } else {
                pending_theme_redraw = false;
            }

            thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_active_state(
        &self,
        c: &mut dyn ImBackend,
        xkb: &mut Option<XkbContext>,
        rime: &mut RimeEngine,
        plugin_host: &mut PluginHost,
        panel: &mut PanelData,
        quick_send: &mut QuickSendInject,
        panel_state: &mut PanelState,
        last_panel_width: &mut u32,
        xime_config: &XimeConfig,
        theme: &PanelTheme,
        candidate_window_visible: &mut bool,
        consumed_presses: &mut std::collections::HashSet<u32>,
        last_input_keysym: &mut Option<u32>,
        ctrl_root_visible: &mut bool,
        last_ascii_mode: &mut bool,
        im_enabled: &mut bool,
        voice: &mut VoiceUiState,
        key_repeat: &mut Option<KeyRepeat>,
    ) {
        if let Some(ref mut x) = xkb {
            if let Some((fd, size)) = c.get_keymap_pending() {
                if let Err(e) = x.set_keymap_from_owned_fd(fd, size) {
                    debug!("Keymap error: {}", e);
                }
            }

            let (depressed, latched, locked, group) = c.get_modifiers();
            x.update_modifiers(depressed, latched, locked, group);
        }

        // 指针事件：菜单按钮 / 面板入口点击
        let pointer_events = c.pop_pointer_events();
        for pe in pointer_events {
            if pe.button != 272 || !pe.pressed {
                continue; // 只处理左键按下
            }
            debug!(
                "Pointer press: x={}, y={}, on_menu={}",
                pe.x, pe.y, pe.on_menu
            );
            self.handle_pointer_press(
                c,
                plugin_host,
                rime,
                panel,
                panel_state,
                last_panel_width,
                theme,
                candidate_window_visible,
                &pe,
            );
        }

        // 语音听写事件：上屏 / 悬浮层（v1）或候选栏（v2 回落）实时反馈
        // （worker 在后台线程，主循环只消费）。
        crate::speech::drain_events(|event| {
            self.handle_speech_event(
                c,
                rime,
                plugin_host,
                event,
                theme,
                candidate_window_visible,
                voice,
            );
        });

        // 语音悬浮层动画节拍（~30fps）：电平是流数据，partial 事件之间
        // 也要滚动/衰减；Loading 阶段无事件，靠节拍维持画面。
        if voice.visible
            && voice
                .last_frame
                .is_none_or(|t| t.elapsed() >= VOICE_FRAME_INTERVAL)
        {
            self.redraw_voice_overlay(c, voice, theme);
        }

        // 应用侧 reset（浏览器地址栏 Esc 等）：协议要求清空 IM 组合状态，
        // 否则随后的 commit 会插进已被应用重置的上下文。
        if c.take_reset_pending() {
            debug!("App requested reset: clearing composition and UI");
            rime.clear_composition();
            c.clear_preedit();
            c.hide_candidate_window();
            let _ = c.flush();
            *candidate_window_visible = false;
            quick_send.clear();
            *last_input_keysym = None;
            if let Ok(mut cache) = self.candidate_cache.lock() {
                *cache = None;
            }
        }

        let (repeat_rate, repeat_delay_ms) = c.repeat_info();
        let now = Instant::now();
        let events = c.pop_key_events();
        for event in events {
            debug!(
                "Key event: keycode={}, pressed={}",
                event.key, event.pressed
            );

            if let Some(ref mut x) = xkb {
                let Some(sym) = x.key_from_keycode(event.key + 8) else {
                    continue;
                };
                self.handle_key_event(
                    c,
                    x,
                    rime,
                    plugin_host,
                    panel,
                    quick_send,
                    panel_state,
                    last_panel_width,
                    xime_config,
                    theme,
                    event,
                    sym,
                    candidate_window_visible,
                    consumed_presses,
                    last_input_keysym,
                    ctrl_root_visible,
                    last_ascii_mode,
                    im_enabled,
                    voice,
                );
                // 只在真实事件之后更新重复状态：合成事件不能把 delay 重新
                // 计时，否则长按永远等不到下一次重复。
                let consumed = consumed_presses.contains(&event.key);
                update_key_repeat(
                    key_repeat,
                    event,
                    sym,
                    consumed,
                    repeat_rate,
                    repeat_delay_ms,
                    now,
                );
            }
        }

        // 到点重投一次「按下」。grab 期间合成器只发 repeat_info，不自己重复；
        // 不补这一步，按住方向键只会移动一格。
        let repeat = key_repeat.as_ref().filter(|r| now >= r.next).and_then(|r| {
            let sym = xkb.as_ref()?.key_from_keycode(r.key + 8)?;
            Some((r.synth_event(), sym, r.interval))
        });
        if let Some((event, sym, interval)) = repeat {
            if let Some(x) = xkb.as_ref() {
                self.handle_key_event(
                    c,
                    x,
                    rime,
                    plugin_host,
                    panel,
                    quick_send,
                    panel_state,
                    last_panel_width,
                    xime_config,
                    theme,
                    event,
                    sym,
                    candidate_window_visible,
                    consumed_presses,
                    last_input_keysym,
                    ctrl_root_visible,
                    last_ascii_mode,
                    im_enabled,
                    voice,
                );
                if let Some(r) = key_repeat.as_mut() {
                    r.next = Instant::now() + interval;
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_key_event(
        &self,
        c: &mut dyn ImBackend,
        xkb: &XkbContext,
        rime: &mut RimeEngine,
        plugin_host: &mut PluginHost,
        panel: &mut PanelData,
        quick_send: &mut QuickSendInject,
        panel_state: &mut PanelState,
        last_panel_width: &mut u32,
        xime_config: &XimeConfig,
        theme: &PanelTheme,
        event: xime_wayland::KeyEvent,
        sym: Keysym,
        candidate_window_visible: &mut bool,
        consumed_presses: &mut std::collections::HashSet<u32>,
        last_input_keysym: &mut Option<u32>,
        ctrl_root_visible: &mut bool,
        last_ascii_mode: &mut bool,
        im_enabled: &mut bool,
        voice: &mut VoiceUiState,
    ) {
        let modifiers = xkb.get_modifiers();
        let release_mask = if !event.pressed {
            librime::K_RELEASE_MASK
        } else {
            0
        };
        debug!(
            "keysym={}, modifiers={}, release={}",
            sym.raw(),
            modifiers.effective,
            release_mask
        );

        // Ctrl+Space：启停输入法（fcitx 风格）。任意状态下优先处理。
        if event.pressed && modifiers.ctrl && sym.raw() == 0x20 {
            *im_enabled = !*im_enabled;
            if *im_enabled {
                debug!("Input method enabled (Ctrl+Space)");
                // 停用期间托盘显示的是"直通"占位（English），并不反映 Rime
                // 模式；重新启用必须按 Rime 实际状态无条件恢复托盘并同步
                // last_ascii_mode。否则停/启一轮后托盘永远卡在停用期的显示
                // 上——后续按键只在 ascii 变化时才刷新托盘，纠正不了它。
                if let Some(session) = rime.session() {
                    if let Ok(status) = session.status() {
                        *last_ascii_mode = status.is_ascii_mode;
                        let tray_mode = tray_mode_for(status.is_ascii_mode);
                        self.rt_handle.block_on(async {
                            self.tray.set_mode(tray_mode).await;
                        });
                    }
                }
            } else {
                debug!("Input method disabled (Ctrl+Space)");
                // 停用：丢弃组合、清空 preedit、关闭全部 UI；托盘设 English
                // 仅表示"直通"占位——Rime 的 ascii_mode 与 last_ascii_mode
                // 都不动（它们仍描述 Rime 真实状态），重新启用时统一恢复。
                rime.clear_composition();
                c.clear_preedit();
                c.hide_candidate_window();
                c.hide_panel();
                c.hide_root_window();
                c.hide_voice_overlay();
                voice.visible = false;
                voice.text.clear();
                let _ = c.flush();
                *candidate_window_visible = false;
                quick_send.clear();
                *panel_state = PanelState::Closed;
                *ctrl_root_visible = false;
                *last_input_keysym = None;
                consumed_presses.clear();
                // 听写中停用 IM：会话一并停止（直通模式下 commit 无处落），
                // 滞留事件清空（同 deactivate 路径的理由）。
                if crate::speech::state() != crate::speech::SpeechState::Idle {
                    crate::speech::toggle();
                }
                crate::speech::clear_events();
                // 停用即视同失焦：候选缓存一并清（同 deactivate 路径的理由）。
                if let Ok(mut cache) = self.candidate_cache.lock() {
                    *cache = None;
                }
                self.rt_handle.block_on(async {
                    self.tray.set_mode(InputMode::English).await;
                });
            }
            // 消费按下（释放由 consumed_presses 抑制，避免孤儿释放）
            consumed_presses.insert(event.key);
            return;
        }

        // 停用态：按键直接转发，不做任何处理（被消费按下的释放仍抑制）。
        if !*im_enabled {
            if event.pressed || !consumed_presses.contains(&event.key) {
                c.forward_key(event.serial, event.time, event.key, event.forward_state());
            } else {
                consumed_presses.remove(&event.key);
            }
            return;
        }

        // 语音输入开关（Ctrl+Alt+V）：听写的键盘入口（托盘/🎙️ 之外）。
        // 仅在 IM 激活（键盘 grab）时可达——Wayland 无全局快捷键，未激活
        // 时键事件不经过 IM，那条路径由托盘菜单覆盖（含未激活拒绝提示）。
        if event.pressed && is_voice_toggle_hotkey(sym.raw(), &modifiers) {
            debug!("Voice toggle hotkey (Ctrl+Alt+V)");
            crate::speech::toggle();
            consumed_presses.insert(event.key);
            return;
        }

        // 听写中键盘为模态：吞掉所有按键（组合/面板不与语音上屏交织），
        // Esc = 停止并收尾上屏（与快捷键/托盘/点悬浮层同一 toggle 路径）。
        if crate::speech::state() == crate::speech::SpeechState::Listening {
            if event.pressed {
                consumed_presses.insert(event.key);
                if sym.raw() == 0xFF1B {
                    debug!("Escape during dictation, stopping speech");
                    crate::speech::toggle();
                }
            } else {
                consumed_presses.remove(&event.key);
            }
            return;
        }

        let is_ctrl = sym.raw() == 0xFFE3 || sym.raw() == 0xFFE4;
        debug!(
            "is_ctrl={}, candidate_visible={}, last_key={:?}",
            is_ctrl, candidate_window_visible, last_input_keysym
        );

        // 面板打开时（对齐 XimeYao 桌面交互）：Esc 关闭；`;` 上屏分号（面板保持）；
        // 其余可打印/编辑键收起面板继续输入（修饰键/Ctrl 不收起、不拦截）。
        if event.pressed && matches!(panel_state, PanelState::Open(_)) {
            let raw = sym.raw();
            let is_modifier = matches!(raw, 0xFFE1 | 0xFFE2 | 0xFFE9 | 0xFFEA | 0xFFE3 | 0xFFE4);
            let printable_or_edit = (0x20..0x7F).contains(&raw)
                || matches!(raw, 0xFF08 | 0xFF0D | 0xFF8D | 0xFF1B)
                || matches!(raw, 0xFF51..=0xFF54);
            if printable_or_edit && !is_modifier {
                if raw == 0xFF1B {
                    // Escape：直接关闭面板，恢复候选栏。
                    self.close_panel(c, panel, panel_state, theme, candidate_window_visible);
                    consumed_presses.insert(event.key);
                    return;
                }
                if raw == 0x3B {
                    // 分号：上屏分号（面板保持打开，可连续输入）。
                    c.commit_string(";");
                    let _ = c.flush();
                    plugin_host.emit_text_committed(";");
                    consumed_presses.insert(event.key);
                    return;
                }
                // 键入收起：关闭面板，按键继续走正常输入路径。
                self.close_panel(c, panel, panel_state, theme, candidate_window_visible);
            }
        }

        // `;`（中文态、无组合输入）：打开表情页（对齐 XimeYao 的分号触发入口）。
        if event.pressed
            && sym.raw() == 0x3B
            && matches!(panel_state, PanelState::Closed)
            && rime
                .session()
                .and_then(|s| s.status().ok())
                .is_some_and(|st| !st.is_ascii_mode && !st.is_composing)
        {
            // 兜底：触发前重载插件（daemon 早于插件安装启动时也能用）。
            plugin_host.reload();
            if plugin_host.emoji_plugin_count() > 0 {
                self.open_panel_page(
                    c,
                    plugin_host,
                    panel,
                    panel_state,
                    PanelPage::Emoji,
                    theme,
                    candidate_window_visible,
                );
                consumed_presses.insert(event.key);
                return;
            }
        }

        // 快捷发送编码注入：数字键/导航键按当前展示状态接管或落穿
        if quick_send.active()
            && self.handle_quick_send_key(
                c,
                rime,
                plugin_host,
                quick_send,
                &event,
                sym,
                theme,
                last_panel_width,
                consumed_presses,
                candidate_window_visible,
            )
        {
            return;
        }

        // 字根窗只是"按住 Ctrl 时顺带显示"的一层 UI：Ctrl 键本身必须继续往下
        // 走并转发给应用，否则应用收不到 Ctrl 按下，后续 Ctrl+C/V/A 全部失效。
        if *candidate_window_visible && is_ctrl {
            self.handle_ctrl_key(
                c,
                xime_config,
                theme,
                &event,
                sym,
                modifiers,
                candidate_window_visible,
                last_input_keysym,
                ctrl_root_visible,
                rime,
            );
        }

        if let Some(session) = rime.session() {
            let result = session.process_key(
                sym.raw() as i32,
                modifiers.effective as i32 | release_mask as i32,
            );
            debug!("Rime result: {:?}", result);

            // 空格兜底：Rime 处理后组合仍在且无候选（confirm 落空/未消费），
            // 直接上屏编码，保证空格始终有产出（对齐 XimeYao）。
            // get_input/RimeGetContext 都是全量拷贝+堆分配（每条候选各一次），
            // 兜底只对空格有意义——非空格键不做这两次拷贝。
            if sym.raw() == 0x20 {
                let raw_input = session.get_input().map(str::to_string);
                let num_candidates = session.context().map_or(0, |ctx| ctx.menu().num_candidates);
                if let Some(raw) = space_fallback_input(
                    sym.raw(),
                    &modifiers,
                    raw_input.as_deref(),
                    num_candidates,
                ) {
                    c.commit_string(&raw);
                    let _ = c.flush();
                    plugin_host.emit_text_committed(&raw);
                    session.clear_composition();
                    c.clear_preedit();
                    let _ = c.flush();
                    consumed_presses.insert(event.key);
                    debug!("Space with no candidates: committed raw input '{raw}'");
                    return;
                }
            }

            if result && event.pressed {
                let letter = keysym_to_letter(sym.raw());
                if letter.is_some() {
                    *last_input_keysym = Some(sym.raw());
                    debug!("Recorded last input keysym={}", sym.raw());
                }
            }

            if let Ok(status) = session.status() {
                let is_ascii = status.is_ascii_mode;
                if is_ascii != *last_ascii_mode {
                    *last_ascii_mode = is_ascii;
                    let tray_mode = tray_mode_for(is_ascii);
                    self.rt_handle.block_on(async {
                        self.tray.set_mode(tray_mode).await;
                    });
                    debug!("Tray updated: ascii_mode={}", is_ascii);
                }
                debug!("ascii_mode={}, composing={}", is_ascii, status.is_composing);
            }

            // 回车键（Return / 数字键盘回车）：与其他按键一致，按 Rime 消费结果决定转发。
            // - 组合态下 Rime 吞掉回车并提交编码（result=true），不再转发，
            //   应用只收到上屏的编码文本，不会多出回车符。
            // - 空组合态下 Rime 不消费（result=false），转发给应用（正常换行/执行命令）。
            // 被吞按下的释放事件同样抑制（孤儿释放抑制，对齐 fcitx5）。
            let system_shortcut = is_system_shortcut(&modifiers);
            if should_forward_key(
                event.pressed,
                result,
                consumed_presses,
                event.key,
                system_shortcut,
            ) {
                // 只转发 `key`，**不再转发 `modifiers`**。
                //
                // 2026-10-26 曾加上 forward_modifiers（为修终端的 Ctrl+C），
                // 结果把系统级 Ctrl 搞死了：KWin 同时为多个窗口各保留一个
                // zwp_input_method_context_v1，而「上次转发的修饰态」只能是
                // 一个全局值——Ctrl 的按下发给窗口 A、松开发给窗口 B，A 那侧
                // 永久残留「Ctrl 按住」，之后所有点击都变成 Ctrl+点击、所有
                // 按键都变成 Ctrl+键。鼠标操作不产生按键事件，所以永远纠正
                // 不回来，只能重启 daemon。
                //
                // 修饰态是**每 context 独立**的状态，在拿到按 context 隔离的
                // 方案之前不要碰它。代价：终端里 Ctrl+C/Ctrl+X 收不到 SIGINT
                // （已知问题，见 PROGRESS.md）。
                c.forward_key(event.serial, event.time, event.key, event.forward_state());
            }
            if result && event.pressed && !system_shortcut {
                consumed_presses.insert(event.key);
            } else {
                consumed_presses.remove(&event.key);
            }

            if let Some(commit) = session.commit() {
                let committed = commit.text();
                c.commit_string(committed);
                let _ = c.flush();
                debug!("Committed: {}", committed);
                plugin_host.emit_text_committed(committed);
            }

            if let Some(ctx) = session.context() {
                if let Some(p) = ctx.composition().preedit {
                    c.set_preedit(p, p.len() as i32);
                } else {
                    c.clear_preedit();
                }
                let _ = c.flush();

                let menu = ctx.menu();
                // 快捷发送编码匹配：原始输入 = preedit[..sel_start]
                let comp = ctx.composition();
                let raw_input = comp
                    .preedit
                    .and_then(|p| p.get(..comp.sel_start.min(p.len())))
                    .unwrap_or("");
                let matched: Vec<(String, String)> = if raw_input.is_empty() {
                    Vec::new()
                } else {
                    let entries = self
                        .clipboard
                        .list_quick_send()
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|e| !e.code.is_empty())
                        .map(|e| (e.code, e.text))
                        .collect::<Vec<_>>();
                    match_quick_send_codes(&entries, raw_input, 9)
                };
                quick_send.items = matched;
                // 页内候选数必须与展示端同一钳制：candidate_count 小于页大小时，
                // 未钳制的数会让数字键去选一个没显示出来的候选（落穿给 Rime）。
                quick_send.rime_count = menu
                    .num_candidates
                    .min(xime_config.style.candidate_count.clamp(1, 9) as usize);
                quick_send.highlighted = 0;

                if menu.num_candidates > 0 {
                    let max_candidates = xime_config.style.candidate_count.clamp(1, 9) as usize;
                    let mut candidate_items: Vec<xime_ui::CandidateItem> = menu
                        .candidates
                        .iter()
                        .take(max_candidates)
                        .enumerate()
                        .map(|(i, x)| {
                            let comment = x.comment.map(|c| c.to_string()).unwrap_or_default();
                            debug!("candidate {} text='{}' comment='{}'", i, x.text, comment);
                            xime_ui::CandidateItem {
                                text: x.text.to_string(),
                                comment,
                                index: i,
                            }
                        })
                        .collect();
                    // 快捷发送条目追加在 Rime 当页候选之后（数字键 9 位以内）
                    let room = 9usize.saturating_sub(candidate_items.len());
                    for (code, qs_text) in quick_send.items.iter().take(room) {
                        candidate_items.push(xime_ui::CandidateItem {
                            text: qs_text.clone(),
                            comment: code.clone(),
                            index: candidate_items.len(),
                        });
                    }
                    let highlighted_index =
                        menu.highlighted_candidate_index.min(max_candidates - 1);
                    debug!("highlighted_index={}", highlighted_index);
                    if let Err(e) =
                        c.show_candidate_window(&candidate_items, highlighted_index, theme)
                    {
                        debug!("Candidate window error: {}", e);
                    }
                    *last_panel_width = c.candidate_width(&candidate_items, theme);
                    // 缓存最近候选（菜单开/关后重绘）
                    if let Ok(mut cache) = self.candidate_cache.lock() {
                        *cache = Some((candidate_items.clone(), highlighted_index));
                    }
                    *candidate_window_visible = true;
                } else if !quick_send.items.is_empty() {
                    // Rime 无候选：宿主接管候选展示（快捷发送编码命中）
                    self.render_quick_send(
                        c,
                        quick_send,
                        theme,
                        last_panel_width,
                        candidate_window_visible,
                    );
                } else {
                    quick_send.clear();
                    if *candidate_window_visible {
                        c.hide_candidate_window();
                        let _ = c.flush();
                        *candidate_window_visible = false;
                        // 无候选时清空缓存，避免菜单重绘展示过期内容
                        if let Ok(mut cache) = self.candidate_cache.lock() {
                            *cache = None;
                        }
                    }
                }
            }
        }
    }

    // ── 面板（对齐 XimeYao：菜单/列表/网格子页，进页 reload 一次数据，
    // 绘帧只读内存；点击命中与绘制同一几何 panel_hit）──────────────────

    /// 打开面板页面：加载数据 → 注入 im 层 → 渲染（复用候选栏内容撑底）。
    #[allow(clippy::too_many_arguments)]
    fn open_panel_page(
        &self,
        c: &mut dyn ImBackend,
        _plugin_host: &mut PluginHost,
        panel: &mut PanelData,
        panel_state: &mut PanelState,
        page: PanelPage,
        theme: &PanelTheme,
        candidate_window_visible: &mut bool,
    ) {
        match page {
            PanelPage::Clipboard => {
                panel.list = self.load_panel_list(page);
                // 打开剪贴板面板时拉取一次远端（对齐 Android pullOnce 语义）。
                let _ = self.sync_tx.send(SyncMessage::Pull);
            }
            PanelPage::QuickSend => panel.list = self.load_panel_list(page),
            PanelPage::Emoji => {
                // 内置表情表（对齐 XimeYao：离线零配置，不依赖插件）。
                panel.grid_source = crate::emoji::GROUPS
                    .iter()
                    .map(|g| {
                        (
                            g.category.to_string(),
                            g.symbols.iter().map(|s| s.to_string()).collect(),
                        )
                    })
                    .collect();
                panel.grid.recent =
                    crate::recent_usage::load(crate::recent_usage::RecentKind::Emoji);
            }
            PanelPage::Symbol => {
                panel.grid_source = symbols::GROUPS
                    .iter()
                    .map(|g| {
                        (
                            g.category.to_string(),
                            g.symbols.iter().map(|s| s.to_string()).collect(),
                        )
                    })
                    .collect();
                panel.grid.recent =
                    crate::recent_usage::load(crate::recent_usage::RecentKind::Symbol);
            }
            PanelPage::Menu => {}
        }
        let tabs: Vec<String> = if page.is_grid_page() {
            std::iter::once(xime_ui::RECENT_LABEL.to_string())
                .chain(panel.grid_source.iter().map(|(name, _)| name.clone()))
                .collect()
        } else {
            Vec::new()
        };
        let has_pager = page.is_grid_page()
            && panel
                .grid_source
                .iter()
                .any(|(_, glyphs)| glyphs.len() > xime_ui::GRID_PER_PAGE);
        panel.grid = PanelGrid {
            source: page,
            tab: 0,
            page: 0,
            recent: panel.grid.recent.clone(),
            cells: Vec::new(),
            item_count: 0,
            tabs,
            has_pager,
        };
        Self::refresh_grid_cells(panel);
        *panel_state = PanelState::Open(page);
        self.show_panel_page(c, panel, page, theme, candidate_window_visible);
    }

    /// 按 tab/页码重填网格格子（进页、切标签、翻页后调用）。
    fn refresh_grid_cells(panel: &mut PanelData) {
        let tab = panel.grid.clamped_tab();
        let (items, total): (Vec<String>, usize) = if tab == 0 {
            let recent = panel.grid.recent.clone();
            let total = recent.len();
            (recent, total)
        } else {
            let group = panel
                .grid_source
                .get(tab - 1)
                .cloned()
                .unwrap_or_default()
                .1;
            let total = group.len();
            (group, total)
        };
        panel.grid.item_count = total;
        let start = panel.grid.clamped_page() * xime_ui::GRID_PER_PAGE;
        panel.grid.cells = (0..xime_ui::GRID_PER_PAGE)
            .map(|i| items.get(start + i).cloned())
            .collect();
    }

    /// 渲染面板当前页（注入 im 层 + 复用候选栏内容撑底）。
    fn show_panel_page(
        &self,
        c: &mut dyn ImBackend,
        panel: &PanelData,
        page: PanelPage,
        theme: &PanelTheme,
        candidate_window_visible: &mut bool,
    ) {
        if let Err(e) = c.show_panel(page, &panel.list, &panel.grid) {
            debug!("Panel show error: {}", e);
        }
        let cached = self.candidate_cache.lock().ok().and_then(|g| g.clone());
        let (candidates, highlighted) = cached.unwrap_or_else(|| (Vec::new(), 0usize));
        if let Err(e) = c.show_candidate_window(&candidates, highlighted, theme) {
            debug!("Panel render candidate window error: {}", e);
        }
        let _ = c.flush();
        *candidate_window_visible = true;
    }

    /// 关闭面板（清数据 + 恢复候选栏）。
    fn close_panel(
        &self,
        c: &mut dyn ImBackend,
        panel: &mut PanelData,
        panel_state: &mut PanelState,
        theme: &PanelTheme,
        candidate_window_visible: &mut bool,
    ) {
        *panel_state = PanelState::Closed;
        *panel = PanelData::default();
        c.hide_panel();
        self.redraw_menu_candidates(c, theme, candidate_window_visible);
    }

    /// 面板选中上屏后的收尾（对齐 XimeYao：上屏走 commit 通道 →
    /// composition 终止）：丢弃 Rime 里挂着的编码组合、收起面板、
    /// **隐藏候选栏**——输入流程已结束，候选栏不再遮挡应用内容。
    fn dismiss_panel_after_commit(
        &self,
        c: &mut dyn ImBackend,
        rime: &mut RimeEngine,
        panel: &mut PanelData,
        panel_state: &mut PanelState,
        candidate_window_visible: &mut bool,
    ) {
        if let Some(session) = rime.session() {
            session.clear_composition();
        }
        c.clear_preedit();
        *panel_state = PanelState::Closed;
        *panel = PanelData::default();
        c.hide_panel();
        c.hide_candidate_window();
        let _ = c.flush();
        *candidate_window_visible = false;
        // 清候选缓存：下次激活时从空组合开始，避免重绘出过期内容。
        if let Ok(mut cache) = self.candidate_cache.lock() {
            *cache = None;
        }
        debug!("Panel commit finished: composition cleared, candidate window hidden");
    }

    /// 加载列表子页数据（剪切板 / 快捷发送）。
    fn load_panel_list(&self, page: PanelPage) -> PanelList {
        let items: Vec<PanelListItem> = match page {
            PanelPage::Clipboard => self
                .clipboard
                .list_clipboard(200)
                .unwrap_or_default()
                .into_iter()
                .map(|e| PanelListItem {
                    text: e.text,
                    code: String::new(),
                })
                .collect(),
            PanelPage::QuickSend => self
                .clipboard
                .list_quick_send()
                .unwrap_or_default()
                .into_iter()
                .map(|e| PanelListItem {
                    text: e.text,
                    code: e.code,
                })
                .collect(),
            _ => Vec::new(),
        };
        PanelList {
            source: page,
            items,
            page: 0,
        }
    }

    /// 语音听写事件处理：Committed 上屏；状态/中间文本驱动语音悬浮层
    /// （v1 输入面板支持独立定位）或候选栏实时反馈（v2 回落）。
    #[allow(clippy::too_many_arguments)]
    fn handle_speech_event(
        &self,
        c: &mut dyn ImBackend,
        rime: &mut RimeEngine,
        plugin_host: &mut PluginHost,
        event: crate::speech::SpeechEvent,
        theme: &PanelTheme,
        candidate_window_visible: &mut bool,
        voice: &mut VoiceUiState,
    ) {
        use crate::speech::{SpeechEvent as Ev, SpeechState};
        // v1 输入面板的 set_toplevel 可把悬浮层钉在屏幕底部居中；v2 的
        // popup surface 只能跟随光标，回落 P10 的候选栏实时反馈。
        let overlay = c.supports_voice_overlay();
        match event {
            Ev::Committed(text) => {
                // 停顿断句自动上屏（对齐 XimeYao：识别文本直接落光标处）。
                c.commit_string(&text);
                let _ = c.flush();
                plugin_host.emit_text_committed(&text);
                debug!(
                    "Speech committed: {} ({} chars)",
                    text,
                    text.chars().count()
                );
            }
            Ev::Partial(text) => {
                // 听写中的实时反馈：悬浮层只存数据（重绘走 ~30fps 节拍），
                // 候选栏模式（v2）立即渲染。
                if !matches!(crate::speech::state(), SpeechState::Listening) {
                    return;
                }
                if overlay {
                    voice.text = text;
                    return;
                }
                // 听写视图是纯候选栏：清掉可能残留的面板（同 redraw）。
                c.hide_panel();
                let display = if text.trim().is_empty() {
                    "🎙️ 正在听写…".to_string()
                } else {
                    format!("🎙️ {text}")
                };
                let candidates = vec![xime_ui::CandidateItem {
                    text: display,
                    comment: String::new(),
                    index: 0,
                }];
                if let Err(e) = c.show_candidate_window(&candidates, 0, theme) {
                    debug!("Speech partial render error: {e}");
                }
                let _ = c.flush();
                *candidate_window_visible = true;
            }
            Ev::State(state) => match state {
                SpeechState::Loading => {
                    if overlay {
                        // 语音是模态会话：清掉进行中的组合/面板/候选栏，
                        // 避免候选窗与语音上屏交织；悬浮层立即可见。
                        rime.clear_composition();
                        c.clear_preedit();
                        c.hide_panel();
                        c.hide_candidate_window();
                        if let Ok(mut cache) = self.candidate_cache.lock() {
                            *cache = None;
                        }
                        *candidate_window_visible = false;
                        voice.visible = true;
                        voice.loading = true;
                        voice.text.clear();
                        self.redraw_voice_overlay(c, voice, theme);
                        return;
                    }
                    let candidates = vec![xime_ui::CandidateItem {
                        text: "🎙️ 正在装载语音引擎…".to_string(),
                        comment: String::new(),
                        index: 0,
                    }];
                    let _ = c.show_candidate_window(&candidates, 0, theme);
                    let _ = c.flush();
                    *candidate_window_visible = true;
                }
                SpeechState::Listening => {
                    if overlay {
                        voice.loading = false;
                        self.redraw_voice_overlay(c, voice, theme);
                        return;
                    }
                    // partial 事件随后就到，这里只标记可见。
                    *candidate_window_visible = true;
                }
                SpeechState::Idle => {
                    // 会话结束（用户停止或失败后）：悬浮层收起，恢复原候选栏。
                    if overlay {
                        c.hide_voice_overlay();
                        voice.visible = false;
                        voice.loading = false;
                        voice.text.clear();
                    }
                    self.redraw_menu_candidates(c, theme, candidate_window_visible);
                }
            },
            Ev::Error(message) => {
                // 失败兜底：桌面通知 + 悬浮层收起 + 恢复候选栏。
                if overlay {
                    c.hide_voice_overlay();
                    voice.visible = false;
                    voice.text.clear();
                }
                let handle = self.rt_handle.clone();
                handle.spawn(async move {
                    notify_desktop("语音输入", &message).await;
                });
                self.redraw_menu_candidates(c, theme, candidate_window_visible);
            }
        }
    }

    /// 语音悬浮层整幅重绘（固定尺寸；电平取 speech 环形缓冲快照，
    /// 节流由调用方控制——事件回调立即重绘，动画节拍 ~30fps 重绘）。
    fn redraw_voice_overlay(
        &self,
        c: &mut dyn ImBackend,
        voice: &mut VoiceUiState,
        theme: &PanelTheme,
    ) {
        let view = xime_ui::voice::VoiceView {
            loading: voice.loading,
            levels: crate::speech::levels_snapshot(),
            text: voice.text.clone(),
        };
        if let Err(e) = c.show_voice_overlay(&view, theme) {
            debug!("Voice overlay show error: {e}");
            voice.visible = false;
            return;
        }
        let _ = c.flush();
        voice.last_frame = Some(Instant::now());
    }

    /// 处理候选栏菜单按钮 / 面板点击（命中测试与绘制同源：xime_ui::panel_hit）。
    #[allow(clippy::too_many_arguments)]
    fn handle_pointer_press(
        &self,
        c: &mut dyn ImBackend,
        plugin_host: &mut PluginHost,
        rime: &mut RimeEngine,
        panel: &mut PanelData,
        panel_state: &mut PanelState,
        last_panel_width: &u32,
        theme: &PanelTheme,
        candidate_window_visible: &mut bool,
        pe: &xime_wayland::PointerEvent,
    ) {
        if pe.button != 272 || !pe.pressed {
            return; // 只处理左键按下
        }
        // 语音悬浮层点击（只读 surface）：停止听写（与快捷键/托盘同一
        // toggle 路径）。悬浮层只在听写/装载时在屏，滞留点击到 Idle 时
        // 忽略（toggle 在 Idle 是"开始"，不能被幽灵点击触发）。
        if pe.on_voice {
            if crate::speech::state() != crate::speech::SpeechState::Idle {
                crate::speech::toggle();
            }
            return;
        }
        let width = (*last_panel_width).max(PANEL_MIN_WIDTH);
        let bar = theme.bar_height();

        // 候选栏区域：菜单按钮开合。听写中点它 = 停止听写（🎙️ 结束入口）。
        // 注意用 buffer 宽度（measured 钳到 PANEL_MIN_WIDTH），与绘制端一致；
        // last_panel_width 存的是未钳制的自然宽度，直接传会在窄栏下错位。
        if pe.y < bar as i32 {
            if xime_ui::menu_button_hit(pe.x, pe.y, width, bar) {
                if crate::speech::state() == crate::speech::SpeechState::Listening {
                    crate::speech::toggle();
                    return;
                }
                match panel_state {
                    PanelState::Closed => {
                        self.open_panel_page(
                            c,
                            plugin_host,
                            panel,
                            panel_state,
                            PanelPage::Menu,
                            theme,
                            candidate_window_visible,
                        );
                    }
                    PanelState::Open(_) => {
                        self.close_panel(c, panel, panel_state, theme, candidate_window_visible);
                    }
                }
            }
            return;
        }

        // 面板区与候选栏之间的 PANEL_GAP 缝隙：无交互。
        let gap_bottom = bar + xime_ui::menu::PANEL_GAP;
        if pe.y < gap_bottom as i32 {
            return;
        }

        let page = match panel_state {
            PanelState::Open(page) => *page,
            PanelState::Closed => return,
        };
        // 面板内坐标（扣除候选栏高度 + PANEL_GAP，与绘制同源）。
        let (px, py) = (pe.x.max(0) as u32, (pe.y - gap_bottom as i32).max(0) as u32);
        let Some(hit) = xime_ui::panel_hit(page, width, &panel.list, &panel.grid, px, py) else {
            // 面板空白点击：不动作（对齐 XimeYao，不误关）。
            return;
        };
        match hit {
            PanelHit::MenuItem(card) => {
                let target = match card {
                    xime_ui::menu::MenuCard::Clipboard => Some(PanelPage::Clipboard),
                    xime_ui::menu::MenuCard::QuickSend => Some(PanelPage::QuickSend),
                    xime_ui::menu::MenuCard::Emoji => Some(PanelPage::Emoji),
                    xime_ui::menu::MenuCard::Symbol => Some(PanelPage::Symbol),
                    // 语音输入：切换听写会话（开始/停止），面板收起让位给
                    // 候选栏上的实时反馈（下载/装载/听写中的 partial 文本）。
                    xime_ui::menu::MenuCard::VoiceInput => {
                        crate::speech::toggle();
                        self.close_panel(c, panel, panel_state, theme, candidate_window_visible);
                        None
                    }
                    // 设置：启动设置程序（动作，不开子页）。
                    xime_ui::menu::MenuCard::Settings => {
                        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
                        std::process::Command::new(format!("{home}/.local/bin/xime-setup"))
                            .spawn()
                            .map_err(|e| debug!("Failed to launch xime-setup: {e}"))
                            .ok();
                        None
                    }
                };
                match target {
                    Some(target_page) => {
                        self.open_panel_page(
                            c,
                            plugin_host,
                            panel,
                            panel_state,
                            target_page,
                            theme,
                            candidate_window_visible,
                        );
                    }
                    None => {
                        // 动作类：重绘菜单（状态不变）。
                        self.show_panel_page(c, panel, page, theme, candidate_window_visible);
                    }
                }
            }
            PanelHit::Back => {
                self.open_panel_page(
                    c,
                    plugin_host,
                    panel,
                    panel_state,
                    PanelPage::Menu,
                    theme,
                    candidate_window_visible,
                );
            }
            PanelHit::ListItem(row) => {
                // 上屏即输入流程结束（对齐 XimeYao：上屏走 commit 通道，
                // composition 随之终止 → 面板收起 + 候选栏隐藏 + 原编码丢弃）。
                if let Some(item) = panel.list.item_at(row) {
                    let text = item.text.clone();
                    c.commit_string(&text);
                    let _ = c.flush();
                    debug!(
                        "List item committed: {} ({} chars)",
                        text,
                        text.chars().count()
                    );
                    plugin_host.emit_text_committed(&text);
                    self.dismiss_panel_after_commit(
                        c,
                        rime,
                        panel,
                        panel_state,
                        candidate_window_visible,
                    );
                }
            }
            PanelHit::GlyphCell(slot) => {
                // 点字形 = 上屏，与列表条目同一收尾（XimeYao GlyphCell 分支
                // 同样 collapse_panel；「最近使用」仍要记录）。
                if let Some(glyph) = panel.grid.item_at(slot).map(str::to_string) {
                    c.commit_string(&glyph);
                    let _ = c.flush();
                    debug!("Grid glyph committed: {glyph}");
                    plugin_host.emit_text_committed(&glyph);
                    let kind = if page == PanelPage::Emoji {
                        crate::recent_usage::RecentKind::Emoji
                    } else {
                        crate::recent_usage::RecentKind::Symbol
                    };
                    crate::recent_usage::push(kind, &glyph);
                    self.dismiss_panel_after_commit(
                        c,
                        rime,
                        panel,
                        panel_state,
                        candidate_window_visible,
                    );
                }
            }
            PanelHit::GlyphTab(tab) => {
                // 切分类标签：回到该类第一页并刷新网格。
                if panel.grid.select_tab(tab) {
                    Self::refresh_grid_cells(panel);
                    self.show_panel_page(c, panel, page, theme, candidate_window_visible);
                }
            }
            PanelHit::PrevPage => {
                if page.is_grid_page() {
                    if panel.grid.prev_page() {
                        Self::refresh_grid_cells(panel);
                        self.show_panel_page(c, panel, page, theme, candidate_window_visible);
                    }
                } else if panel.list.prev_page() {
                    self.show_panel_page(c, panel, page, theme, candidate_window_visible);
                }
            }
            PanelHit::NextPage => {
                if page.is_grid_page() {
                    if panel.grid.next_page() {
                        Self::refresh_grid_cells(panel);
                        self.show_panel_page(c, panel, page, theme, candidate_window_visible);
                    }
                } else if panel.list.next_page() {
                    self.show_panel_page(c, panel, page, theme, candidate_window_visible);
                }
            }
        }
    }

    fn redraw_menu_candidates(
        &self,
        c: &mut dyn ImBackend,
        theme: &PanelTheme,
        candidate_window_visible: &mut bool,
    ) {
        // 恢复"纯候选栏"视图前必须清面板：im 层的 show_candidate_window 按
        // 自身残留的 panel_page 画面板，这里不清的话任何绕过 close_panel 的
        // 渲染路径（语音反馈/激活恢复/KWin 不发 deactivate 的切窗）都会让
        // 菜单面板挂在屏幕上、打字也收不掉（幂等：无面板时无害）。
        c.hide_panel();
        let cached = self.candidate_cache.lock().ok().and_then(|g| g.clone());
        if let Some((candidates, highlighted)) = cached {
            if let Err(e) = c.show_candidate_window(&candidates, highlighted, theme) {
                debug!("Menu redraw candidate window error: {}", e);
            }
            let _ = c.flush();
            *candidate_window_visible = true;
            debug!("Candidate window redrawn after menu state change");
        } else {
            c.hide_candidate_window();
            let _ = c.flush();
            *candidate_window_visible = false;
        }
    }

    /// 按住 Ctrl 时显示上一键的字根（组合态、候选窗可见时）。
    ///
    /// 只做显示，**不消费按键**：Ctrl 按下/释放要继续走正常路径转发给应用，
    /// 否则应用收不到 Ctrl 修饰态，Ctrl+C/V/A 之类的快捷键全部失效。
    #[allow(clippy::too_many_arguments)]
    fn handle_ctrl_key(
        &self,
        c: &mut dyn ImBackend,
        xime_config: &XimeConfig,
        theme: &PanelTheme,
        event: &xime_wayland::KeyEvent,
        _sym: Keysym,
        modifiers: ModifierState,
        _candidate_window_visible: &mut bool,
        last_input_keysym: &mut Option<u32>,
        ctrl_root_visible: &mut bool,
        rime: &mut RimeEngine,
    ) {
        if event.pressed {
            let ctrl_pressed = modifiers.ctrl;
            let alt_pressed = modifiers.alt;
            let shift_pressed = modifiers.shift;
            let super_pressed = modifiers.super_key;

            if ctrl_pressed && !alt_pressed && !shift_pressed && !super_pressed {
                if let Some(last_key) = *last_input_keysym {
                    let letter = keysym_to_letter(last_key);
                    debug!("last_key={}, letter={:?}", last_key, letter);
                    if let Some(letter) = letter {
                        let schema = rime.get_current_schema().unwrap_or_default();
                        let root = xime_config.get_root_for_key(&schema, letter);
                        debug!("root for '{}' (schema={}) = {:?}", letter, schema, root);
                        if let Some(root) = root {
                            debug!("Ctrl pressed, showing root for '{}': {}", letter, root);
                            if let Err(e) = c.show_root_window(letter, &root, theme) {
                                debug!("Failed to show root window: {}", e);
                            } else {
                                *ctrl_root_visible = true;
                            }
                        }
                    }
                }
            }
        } else if *ctrl_root_visible {
            debug!("Ctrl released, restoring candidate window");
            c.hide_root_window();
            *ctrl_root_visible = false;

            if let Some(session) = rime.session() {
                if let Some(ctx) = session.context() {
                    let menu = ctx.menu();
                    if menu.num_candidates > 0 {
                        // 与主渲染路径同一钳制：字根窗期间超配置数量的候选
                        // 不能借恢复路径漏出来；恢复结果也要回写候选缓存，
                        // 否则缓存与屏幕内容分叉（后续菜单重绘用旧缓存）。
                        let max_candidates = xime_config.style.candidate_count.clamp(1, 9) as usize;
                        let candidate_items: Vec<xime_ui::CandidateItem> = menu
                            .candidates
                            .iter()
                            .take(max_candidates)
                            .enumerate()
                            .map(|(i, x)| {
                                let comment = x.comment.map(|c| c.to_string()).unwrap_or_default();
                                xime_ui::CandidateItem {
                                    text: x.text.to_string(),
                                    comment,
                                    index: i,
                                }
                            })
                            .collect();
                        let highlighted_index = menu
                            .highlighted_candidate_index
                            .min(max_candidates.saturating_sub(1));
                        if let Err(e) =
                            c.show_candidate_window(&candidate_items, highlighted_index, theme)
                        {
                            debug!("Failed to restore candidate window: {}", e);
                        }
                        if let Err(e) = c.flush() {
                            debug!("Failed to flush: {}", e);
                        }
                        if let Ok(mut cache) = self.candidate_cache.lock() {
                            *cache = Some((candidate_items, highlighted_index));
                        }
                    }
                }
            }
        }
    }

    /// 会话重建后（词典操作/切方案）按 Rime 实际状态同步托盘中英：
    /// 新会话的 ascii_mode 回到方案默认值，不同步则托盘与实际模式背离。
    fn sync_tray_after_session_rebuild(&self, rime: &RimeEngine, last_ascii_mode: &mut bool) {
        if let Some(session) = rime.session() {
            if let Ok(status) = session.status() {
                if status.is_ascii_mode != *last_ascii_mode {
                    *last_ascii_mode = status.is_ascii_mode;
                    let tray_mode = tray_mode_for(status.is_ascii_mode);
                    self.rt_handle.block_on(async {
                        self.tray.set_mode(tray_mode).await;
                    });
                    debug!(
                        "Tray updated after session rebuild: ascii_mode={}",
                        status.is_ascii_mode
                    );
                }
            }
        }
    }

    // ── 快捷发送编码注入（从 main 恢复：独立功能，与面板改造无关）──
    fn render_quick_send(
        &self,
        c: &mut dyn ImBackend,
        quick_send: &QuickSendInject,
        theme: &PanelTheme,
        last_panel_width: &mut u32,
        candidate_window_visible: &mut bool,
    ) {
        let candidate_items: Vec<xime_ui::CandidateItem> = quick_send
            .items
            .iter()
            .enumerate()
            .map(|(i, (code, text))| xime_ui::CandidateItem {
                text: text.clone(),
                comment: code.clone(),
                index: i,
            })
            .collect();
        let highlighted_index = quick_send.highlighted.min(8);
        if let Err(e) = c.show_candidate_window(&candidate_items, highlighted_index, theme) {
            debug!("Quick send render error: {}", e);
        }
        *last_panel_width = c.candidate_width(&candidate_items, theme);
        if let Ok(mut cache) = self.candidate_cache.lock() {
            *cache = Some((candidate_items, highlighted_index));
        }
        let _ = c.flush();
        *candidate_window_visible = true;
    }

    /// 提交快捷发送条目：上屏、清组合、退出注入状态。
    #[allow(clippy::too_many_arguments)]
    fn commit_quick_send(
        &self,
        c: &mut dyn ImBackend,
        rime: &mut RimeEngine,
        plugin_host: &PluginHost,
        quick_send: &mut QuickSendInject,
        index: usize,
        candidate_window_visible: &mut bool,
    ) {
        let Some((_, text)) = quick_send.items.get(index) else {
            return;
        };
        let text = text.clone();
        c.commit_string(&text);
        let _ = c.flush();
        debug!("Quick send committed: {text}");
        plugin_host.emit_text_committed(&text);
        rime.clear_composition();
        c.clear_preedit();
        quick_send.clear();
        if let Ok(mut cache) = self.candidate_cache.lock() {
            *cache = None;
        }
        c.hide_candidate_window();
        let _ = c.flush();
        *candidate_window_visible = false;
    }

    /// 注入态按键处理。返回 true 表示按键已被宿主消费。
    ///
    /// - 数字键：位置超出 Rime 当页候选数时提交对应快捷发送条目，否则落穿给 Rime
    /// - Rime 无候选（接管模式）：Return/Space 提交高亮、↑↓/←→/Tab 移动高亮、
    ///   Escape 清组合退出
    /// - 其余按键（含字母）：落穿给 Rime 正常处理
    #[allow(clippy::too_many_arguments)]
    fn handle_quick_send_key(
        &self,
        c: &mut dyn ImBackend,
        rime: &mut RimeEngine,
        plugin_host: &PluginHost,
        quick_send: &mut QuickSendInject,
        event: &xime_wayland::KeyEvent,
        sym: Keysym,
        theme: &PanelTheme,
        last_panel_width: &mut u32,
        consumed_presses: &mut std::collections::HashSet<u32>,
        candidate_window_visible: &mut bool,
    ) -> bool {
        // 我们消费的按下，其释放一并吞掉（孤儿释放抑制）
        if !event.pressed {
            return consumed_presses.remove(&event.key);
        }
        let raw = sym.raw();

        // 数字键归属判定
        let digit_pos = match raw {
            k @ (0x31..=0x39) => Some((k - 0x31) as usize),
            0x30 => Some(9),
            _ => None,
        };
        if let Some(pos) = digit_pos {
            if let Some(index) = quick_send.digit_hit(pos) {
                consumed_presses.insert(event.key);
                self.commit_quick_send(
                    c,
                    rime,
                    plugin_host,
                    quick_send,
                    index,
                    candidate_window_visible,
                );
                return true;
            }
            // Rime 候选范围内的数字：正常选择（落穿）
            return false;
        }

        // Rime 无候选：接管导航与提交键
        if quick_send.rime_count == 0 {
            match raw {
                0xFF0D | 0xFF8D | 0x20 => {
                    // Return / KP_Enter / Space：提交高亮
                    consumed_presses.insert(event.key);
                    let index = quick_send
                        .highlighted
                        .min(quick_send.items.len().saturating_sub(1));
                    self.commit_quick_send(
                        c,
                        rime,
                        plugin_host,
                        quick_send,
                        index,
                        candidate_window_visible,
                    );
                    true
                }
                0xFF1B => {
                    // Escape：清组合退出注入态
                    // 接管的按下必须记入 consumed_presses：clear() 之后释放
                    // 事件连本处理器都进不去了，不记会被当孤儿释放转发给应用。
                    consumed_presses.insert(event.key);
                    quick_send.clear();
                    rime.clear_composition();
                    c.clear_preedit();
                    c.hide_candidate_window();
                    let _ = c.flush();
                    if let Ok(mut cache) = self.candidate_cache.lock() {
                        *cache = None;
                    }
                    *candidate_window_visible = false;
                    true
                }
                0xFF52 | 0xFF51 => {
                    // Up / Left：上一个
                    consumed_presses.insert(event.key);
                    quick_send.highlighted = quick_send.highlighted.saturating_sub(1);
                    self.render_quick_send(
                        c,
                        quick_send,
                        theme,
                        last_panel_width,
                        candidate_window_visible,
                    );
                    true
                }
                0xFF54 | 0xFF53 | 0xFF09 => {
                    // Down / Right / Tab：下一个
                    consumed_presses.insert(event.key);
                    if quick_send.highlighted + 1 < quick_send.items.len() {
                        quick_send.highlighted += 1;
                    }
                    self.render_quick_send(
                        c,
                        quick_send,
                        theme,
                        last_panel_width,
                        candidate_window_visible,
                    );
                    true
                }
                _ => false,
            }
        } else {
            false
        }
    }
}

/// 发 freedesktop 桌面通知（org.freedesktop.Notifications，KDE/GNOME 标准）。
/// 对齐 XimeYao 的部署结果 toast：失败也通知（用户能看到部署按钮没白点）。
async fn notify_desktop(summary: &str, body: &str) {
    let Ok(conn) = zbus::Connection::session().await else {
        debug!("Desktop notification: no session bus");
        return;
    };
    let result = conn
        .call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(
                "xime",           // app_name
                0u32,             // replaces_id
                "input-keyboard", // app_icon（主题图标）
                summary,
                body,
                Vec::<String>::new(), // actions
                std::collections::HashMap::<String, zbus::zvariant::Value>::new(), // hints
                4000i32,              // expire_timeout (ms)
            ),
        )
        .await;
    if let Err(e) = result {
        debug!("Desktop notification failed: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_heal_first_disconnect_fires_quickly() {
        let t0 = Instant::now();
        let mut h = HealScheduler::new();
        assert!(!h.due(t0), "初始不触发");

        h.on_disconnect(t0);
        assert!(!h.due(t0));
        assert!(h.due(t0 + HEAL_QUICK_RETRY), "1 秒后应触发");
    }

    #[test]
    fn test_heal_kill_spacing_respected_across_reconnects() {
        let t0 = Instant::now();
        let mut h = HealScheduler::new();

        // t0+1s 触发自愈并杀掉 launcher → 下一次动作必须在 +25s 之后
        h.on_disconnect(t0);
        let t1 = t0 + HEAL_QUICK_RETRY;
        h.step(t1);
        h.mark_killed(t1);

        // 杀掉后 5 秒又断连：不允许提前到 +6s，仍须等满 25s 间隔
        h.on_disconnect(t1 + Duration::from_secs(5));
        assert!(!h.due(t1 + Duration::from_secs(24)));
        assert!(h.due(t1 + HEAL_KILL_SPACING));
    }

    #[test]
    fn test_heal_stale_kill_does_not_delay_fresh_disconnect() {
        let t0 = Instant::now();
        let mut h = HealScheduler::new();

        // 很久以前杀过一次：新断连只受 1s 快速重试约束
        h.step(t0);
        h.mark_killed(t0);

        let late = t0 + Duration::from_secs(3600);
        h.on_disconnect(late);
        assert!(h.due(late + HEAL_QUICK_RETRY));
    }

    #[test]
    fn test_heal_on_connected_clears_pending() {
        let t0 = Instant::now();
        let mut h = HealScheduler::new();
        h.on_disconnect(t0);
        h.on_connected();
        assert!(!h.due(t0 + HEAL_QUICK_RETRY), "连上后不应再触发");
    }

    #[test]
    fn test_space_fallback_input() {
        let none = ModifierState::default();
        // 组合态 + 无候选：空格上屏编码
        assert_eq!(
            space_fallback_input(0x20, &none, Some("wubi"), 0),
            Some("wubi".into())
        );
        // 有候选：交给 Rime confirm 首选
        assert_eq!(space_fallback_input(0x20, &none, Some("wubi"), 3), None);
        // 无组合：正常空格
        assert_eq!(space_fallback_input(0x20, &none, None, 0), None);
        assert_eq!(space_fallback_input(0x20, &none, Some(""), 0), None);
        // 带修饰键：不拦（Ctrl+Space 启停 / Shift+Space 全半角）
        let with_shift = ModifierState {
            shift: true,
            ..ModifierState::default()
        };
        assert_eq!(
            space_fallback_input(0x20, &with_shift, Some("wubi"), 0),
            None
        );
        // 非空格键不拦
        assert_eq!(space_fallback_input(0xFF0D, &none, Some("wubi"), 0), None);
    }

    #[test]
    fn test_tray_mode_for() {
        // Rime ascii_mode → 托盘：英文态显示 en，中文态显示中。
        // Ctrl+Space 停/启与切方案后的托盘恢复都依赖这个映射。
        assert_eq!(tray_mode_for(true), InputMode::English);
        assert_eq!(tray_mode_for(false), InputMode::Chinese);
    }

    #[test]
    fn test_voice_toggle_hotkey() {
        // Ctrl+Alt+V（大小写不敏感）：触发
        let combo = ModifierState {
            ctrl: true,
            alt: true,
            ..ModifierState::default()
        };
        assert!(is_voice_toggle_hotkey(0x76, &combo)); // v
        assert!(is_voice_toggle_hotkey(0x56, &combo)); // V（CapsLock 态）
                                                       // 缺修饰 / 带 Super：不触发
        assert!(!is_voice_toggle_hotkey(0x76, &ModifierState::default()));
        let ctrl_only = ModifierState {
            ctrl: true,
            ..ModifierState::default()
        };
        assert!(!is_voice_toggle_hotkey(0x76, &ctrl_only), "缺 Alt 不触发");
        let with_super = ModifierState {
            ctrl: true,
            alt: true,
            super_key: true,
            ..ModifierState::default()
        };
        assert!(
            !is_voice_toggle_hotkey(0x76, &with_super),
            "带 Super 不触发"
        );
        // 非字母 V 键：不触发
        assert!(!is_voice_toggle_hotkey(0x63, &combo)); // c
        assert!(!is_voice_toggle_hotkey(0x20, &combo)); // space
    }

    #[test]
    fn test_should_forward_key_empty() {
        let consumed = HashSet::new();
        // 空组合态按下：未消费 → 转发
        assert!(should_forward_key(true, false, &consumed, 10, false));
        // 空组合态释放：未消费 → 转发
        assert!(should_forward_key(false, false, &consumed, 10, false));
    }

    /// 系统快捷键（Ctrl/Alt/Super 组合）必须一律转发。
    ///
    /// 回归防护：终端的 Ctrl+C(SIGINT) / Ctrl+X / Ctrl+Z 依赖应用同时收到
    /// `key` 和 `modifiers`。这里守 `key` 部分；`modifiers` 由
    /// `ImBackend::forward_modifiers` 负责（v1 必须实现，v2 合成器自理）。
    #[test]
    fn test_system_shortcut_always_forwarded() {
        let consumed: HashSet<u32> = HashSet::new();
        // Rime 即使消费了（result=true），带修饰键也必须转发
        assert!(should_forward_key(true, true, &consumed, 46, true));
        assert!(should_forward_key(false, false, &consumed, 46, true));
    }

    #[test]
    fn test_should_forward_key_consumed_press() {
        let mut consumed = HashSet::new();
        // Rime 消费的按下（如组合态回车）：不转发
        assert!(!should_forward_key(true, true, &consumed, 10, false));
        consumed.insert(10);
        // 其释放事件：不转发（孤儿释放抑制）
        assert!(!should_forward_key(false, false, &consumed, 10, false));
    }

    #[test]
    fn test_should_forward_key_after_commit() {
        let consumed = HashSet::new();
        // 组合态回车提交编码后，同键的第二次按下（已清空组合）：正常转发
        assert!(should_forward_key(true, false, &consumed, 10, false));
    }

    #[test]
    fn test_should_forward_key_different_keys() {
        let mut consumed = HashSet::new();
        consumed.insert(10);
        // 10 的释放被抑制，但其他键不受影响
        assert!(!should_forward_key(false, false, &consumed, 10, false));
        assert!(should_forward_key(false, false, &consumed, 20, false));
    }

    #[test]
    fn test_should_forward_key_system_shortcut_always_forwarded() {
        let mut consumed = HashSet::new();
        // 组合态下 Rime 的 emacs 绑定（Control+a → Home 等）会消费 Ctrl 组合键，
        // 但快捷键必须优先给应用：被消费也要转发。
        assert!(should_forward_key(true, true, &consumed, 30, true));
        // 按下已转发，释放也必须转发（不能当成"被消费按下的释放"抑制掉）
        consumed.insert(30);
        assert!(should_forward_key(false, true, &consumed, 30, true));
        assert!(should_forward_key(false, false, &consumed, 30, true));
    }

    #[test]
    fn test_is_system_shortcut() {
        assert!(!is_system_shortcut(&ModifierState::default()));
        assert!(!is_system_shortcut(&ModifierState {
            shift: true,
            ..ModifierState::default()
        }));
        for m in [
            ModifierState {
                ctrl: true,
                ..ModifierState::default()
            },
            ModifierState {
                alt: true,
                ..ModifierState::default()
            },
            ModifierState {
                super_key: true,
                ..ModifierState::default()
            },
        ] {
            assert!(is_system_shortcut(&m));
        }
    }

    fn qs_entries() -> Vec<(String, String)> {
        vec![
            ("dz".into(), "地址".into()),
            ("dh".into(), "电话".into()),
            ("dz2".into(), "地址2".into()),
        ]
    }

    #[test]
    fn test_match_quick_send_codes() {
        // 空输入不匹配
        assert!(match_quick_send_codes(&qs_entries(), "", 9).is_empty());
        // 前缀匹配：dz 命中 2 条（按声明顺序）
        let m = match_quick_send_codes(&qs_entries(), "dz", 9);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0], ("dz".to_string(), "地址".to_string()));
        assert_eq!(m[1], ("dz2".to_string(), "地址2".to_string()));
        // 精确 + 更长前缀
        assert_eq!(match_quick_send_codes(&qs_entries(), "dh", 9).len(), 1);
        assert!(match_quick_send_codes(&qs_entries(), "dzz", 9).is_empty());
        // limit 截断
        assert_eq!(match_quick_send_codes(&qs_entries(), "d", 2).len(), 2);
    }

    #[test]
    fn test_quick_send_digit_hit() {
        // rime_count = 5：数字 1-5（pos 0-4）属 Rime，6+（pos 5+）属快捷发送
        let qs = QuickSendInject {
            items: vec![("dz".into(), "地址".into()), ("dh".into(), "电话".into())],
            rime_count: 5,
            highlighted: 0,
        };
        assert_eq!(qs.digit_hit(0), None);
        assert_eq!(qs.digit_hit(4), None);
        assert_eq!(qs.digit_hit(5), Some(0));
        assert_eq!(qs.digit_hit(6), Some(1));
        assert_eq!(qs.digit_hit(7), None);
        // 接管模式（rime_count = 0）：全部数字归宿主
        let qs = QuickSendInject {
            items: vec![("dz".into(), "地址".into())],
            rime_count: 0,
            highlighted: 0,
        };
        assert_eq!(qs.digit_hit(0), Some(0));
        assert_eq!(qs.digit_hit(1), None);
    }

    #[test]
    fn test_panel_grid_paging_and_tabs() {
        // 35 项分类 → 2 页（GRID_PER_PAGE=32）；翻页/切标签与 daemon 填格逻辑联动
        let mut data = PanelData {
            grid_source: vec![("数".to_string(), (0..35).map(|i| format!("s{i}")).collect())],
            ..PanelData::default()
        };
        data.grid = PanelGrid {
            source: PanelPage::Symbol,
            tab: 1,
            page: 0,
            recent: vec!["r0".to_string()],
            cells: Vec::new(),
            item_count: 0,
            tabs: vec!["最近".to_string(), "数".to_string()],
            has_pager: true,
        };
        WaylandLoop::refresh_grid_cells(&mut data);
        // 第 1 页：32 格满页
        assert_eq!(data.grid.page_count(), 2);
        assert_eq!(data.grid.item_at(0), Some("s0"));
        assert_eq!(data.grid.item_at(31), Some("s31"));
        assert_eq!(data.grid.item_at(32), None);
        // 下一页：剩 3 项
        assert!(data.grid.next_page());
        WaylandLoop::refresh_grid_cells(&mut data);
        assert_eq!(data.grid.item_at(0), Some("s32"));
        assert_eq!(data.grid.item_at(2), Some("s34"));
        assert_eq!(data.grid.item_at(3), None);
        assert!(!data.grid.next_page());
        // 切回「最近」标签：回到该标签第 0 页
        assert!(data.grid.select_tab(0));
        assert_eq!(data.grid.clamped_page(), 0);
        WaylandLoop::refresh_grid_cells(&mut data);
        assert_eq!(data.grid.item_at(0), Some("r0"));
        assert_eq!(data.grid.item_at(1), None);
        // 重复切同一标签不动作
        assert!(!data.grid.select_tab(0));
    }
    #[test]
    fn test_repeat_interval_and_delay() {
        assert_eq!(repeat_interval(25), Some(Duration::from_millis(40)));
        assert_eq!(repeat_interval(0), None, "rate=0 表示禁用");
        assert_eq!(repeat_interval(-1), None);
        // 异常大的 rate 限幅到 200Hz，避免忙循环
        assert_eq!(repeat_interval(1000), Some(Duration::from_millis(5)));
        assert_eq!(repeat_delay(600), Duration::from_millis(600));
        assert_eq!(repeat_delay(0), Duration::from_millis(1));
        assert_eq!(repeat_delay(99_999), Duration::from_millis(5000));
    }

    fn key_event(key: u32, pressed: bool) -> xime_wayland::KeyEvent {
        xime_wayland::KeyEvent {
            serial: 7,
            time: 1000,
            key,
            pressed,
            repeated: false,
        }
    }

    #[test]
    fn test_update_key_repeat_tracks_press_and_release() {
        let t0 = Instant::now();
        let mut repeat = None;
        // 0xff53 = Right
        let right = Keysym::new(0xff53);

        update_key_repeat(&mut repeat, key_event(114, true), right, false, 25, 600, t0);
        let r = repeat.as_ref().expect("方向键按下应开始重复");
        assert_eq!(r.key, 114);
        assert_eq!(r.serial, 7);
        assert_eq!(r.next, t0 + Duration::from_millis(600));
        assert_eq!(r.interval, Duration::from_millis(40));
        let synth = r.synth_event();
        assert!(synth.pressed);
        assert_eq!(synth.key, 114);
        assert!(synth.time >= 1000, "重复事件时间戳不能倒退");

        // 修饰键（Shift_L）按下：不重复、也不打断正在重复的方向键
        update_key_repeat(
            &mut repeat,
            key_event(50, true),
            Keysym::new(0xffe1),
            false,
            25,
            600,
            t0,
        );
        assert_eq!(
            repeat.as_ref().map(|r| r.key),
            Some(114),
            "Shift 不应打断方向键重复"
        );

        // 同键释放：停止
        update_key_repeat(
            &mut repeat,
            key_event(114, false),
            right,
            false,
            25,
            600,
            t0,
        );
        assert!(repeat.is_none());

        // 被消费的按下（Ctrl+Space 等）：不重复
        update_key_repeat(
            &mut repeat,
            key_event(65, true),
            Keysym::new(0x20),
            true,
            25,
            600,
            t0,
        );
        assert!(repeat.is_none());

        // rate=0：关闭重复
        update_key_repeat(&mut repeat, key_event(114, true), right, false, 0, 600, t0);
        assert!(repeat.is_none());
    }

    #[test]
    fn test_update_key_repeat_switches_keys() {
        let t0 = Instant::now();
        let mut repeat = None;
        let right = Keysym::new(0xff53);
        let left = Keysym::new(0xff51);

        update_key_repeat(&mut repeat, key_event(114, true), right, false, 25, 600, t0);
        update_key_repeat(&mut repeat, key_event(113, true), left, false, 25, 600, t0);
        assert_eq!(
            repeat.as_ref().map(|r| r.key),
            Some(113),
            "新按下的键接管重复"
        );

        // 旧键的释放不能把新键的重复清掉（滚动按两个方向键时会发生）
        update_key_repeat(
            &mut repeat,
            key_event(114, false),
            right,
            false,
            25,
            600,
            t0,
        );
        assert_eq!(repeat.as_ref().map(|r| r.key), Some(113));
    }
}
