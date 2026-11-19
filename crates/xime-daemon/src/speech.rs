//! 语音听写：宿主侧接线，识别引擎在**独立进程** `xime-speechd` 里。
//!
//! 为什么不把模型放进输入法进程：语音模型动辄 1–2 GB（Qwen3-ASR-0.6B 与
//! Fun-ASR-Nano 都是 1.9 GB），常驻等于永不还内存；推理崩了不能连累打字；
//! 麦克风授权也该落在真正录音的那个进程上。
//! 这三条是 libximecore `xime-speech-service` 的设计前提，本模块顺着接管：
//!
//! ```text
//! wayland 线程 ──命令──► worker 线程 ──JSON 行/socket──► xime-speechd
//!      ▲                    │                          ├ 麦克风（cpal）
//!      └──── SpeechEvent ───┘                          └ 语音模型（candle）
//! ```
//!
//! 模型族由 speechd 按目录里的 `config.json` 自己判定，本模块不关心是哪一个：
//! Qwen3-ASR 与 Fun-ASR-Nano 都走同一套 `ListenStart` / `ListenStop`。
//! 区别只在**流式能力**——Qwen3-ASR 边录边出字，Fun-ASR（非因果编码器 + LFR
//! 跨帧）只能整段识别，录音期间 `committed` 一直是空的，松手后一次性上屏。
//!
//! 交互（对齐商业输入法的语音键）：
//!
//! - **按住右 Ctrl 说话，松开上屏**（`Ctrl+Alt+V` 也认），只在 IM 激活
//!   （键盘 grab）时可达，因为 release 事件只有 grab 期间才会送到我们；
//!   右 Ctrl 要「独自按住」约 250ms 才起播，期间按别的键即当组合键撤销；
//! - 托盘与 DBus `ToggleSpeech` 是 toggle（按一下开始、再按一下结束），
//!   供未激活时由桌面全局快捷键调用；
//! - 支持流式的模型（Qwen3-ASR）：录音中每轮重解码，`status.committed` 是已经
//!   定稿、可以立刻上屏的增量，`status.partial` 是还会变的预览（驱动悬浮层）。
//!   定稿部分永不回改，所以这里按增量 `commit_string`，松手只补最后的尾巴；
//! - 非流式模型（Fun-ASR-Nano）：录音期间两者都是空的，面板按 `Transcribing`
//!   显示「正在识别…」，松手后 `status.text` 一次性走同一个增量路径上屏。

use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use tracing::{debug, info, warn};
use xime_speech_service::protocol::{Request, Status};
use xime_speech_service::transport::Client;

/// 频谱条数（候选栏语音反馈与 `xime-ui::voice::BAR_COUNT` 必须一致）。
pub const LEVEL_BARS: usize = 24;

/// 监听期间拉状态的间隔。电平是流数据，慢于 150ms 频谱会一顿一顿；
/// 频率再高则每次都要解析一整份模型列表，不值。
const POLL_INTERVAL: Duration = Duration::from_millis(120);

/// 拉起 `xime-speechd` 后等 socket 就绪的上限。首次启动不装载模型，很快。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

/// 停止录音到出结果的兜底上限（首次会现场装载模型，CPU 上可能十几秒）。
const TRANSCRIBE_TIMEOUT: Duration = Duration::from_secs(90);

/// worker 发给主循环的事件。
#[derive(Debug, Clone)]
pub enum SpeechEvent {
    /// 状态迁移（驱动悬浮层/候选栏与托盘反馈）。
    State(SpeechState),
    /// 未定稿的预览文本（整段替换）。流式听写中每轮可能更新，
    /// wayland 侧画成悬浮层/候选栏的实时反馈；空串表示清空预览。
    Partial(String),
    /// 一句识别完成，主循环 `commit_string` 上屏。
    Committed(String),
    /// 失败（服务未运行/模型未就绪/录音失败），文案已面向用户。
    Error(String),
}

/// 听写状态机。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeechState {
    Idle,
    /// 模型装载中（首次几秒到十几秒）。
    Loading,
    /// 正在录音。
    Listening,
    /// 已停止录音，等待整段识别结果。
    Transcribing,
}

impl SpeechState {
    /// 设置页状态字符串（对齐 `SpeechServerStatus.state` 语义）。
    pub fn as_str(&self) -> &'static str {
        match self {
            SpeechState::Idle => "idle",
            SpeechState::Loading => "loading",
            SpeechState::Listening => "listening",
            SpeechState::Transcribing => "transcribing",
        }
    }
}

enum SpeechCommand {
    Start,
    Stop,
    Cancel,
}

/// worker 与主循环共享的状态（主循环只读，worker 只写）。
struct Shared {
    state: Mutex<SpeechState>,
    events: Mutex<Vec<SpeechEvent>>,
    /// 最近 `LEVEL_BARS` 个电平样本（0.0-1.0，已做快起慢落）。
    levels: Mutex<VecDeque<f32>>,
    /// 最近一次 `status` 快照（设置页 `GetSpeechStatus` 与错误提示读它）。
    last_status: Mutex<Option<Status>>,
    tx: Mutex<Option<Sender<SpeechCommand>>>,
    started: Mutex<bool>,
    /// 听写进展（面板状态行读它，用来区分「在录」和「在识别」）。
    progress: Mutex<Progress>,
}

/// 听写进展快照。
#[derive(Debug, Clone, Default)]
pub struct Progress {
    /// 本次听写已上屏的字数：在涨说明识别在推进，不涨说明没识别出东西。
    pub committed_chars: usize,
    /// 最近一次解码失败的原因（成功一轮就清空）。
    pub warning: Option<String>,
}

static SHARED: OnceLock<Shared> = OnceLock::new();

impl Shared {
    fn new() -> Self {
        Self {
            state: Mutex::new(SpeechState::Idle),
            events: Mutex::new(Vec::new()),
            levels: Mutex::new(std::iter::repeat_n(0.0, LEVEL_BARS).collect()),
            last_status: Mutex::new(None),
            tx: Mutex::new(None),
            started: Mutex::new(false),
            progress: Mutex::new(Progress::default()),
        }
    }
}

fn shared() -> &'static Shared {
    SHARED.get_or_init(Shared::new)
}

/// 初始化：起 worker 线程（不连 socket——真正开始听写时才拉起服务）。
pub fn init() {
    let shared = shared();
    let mut started = shared.started.lock().unwrap();
    if *started {
        return;
    }
    *started = true;
    drop(started);

    let (tx, rx) = mpsc::channel();
    *shared.tx.lock().unwrap() = Some(tx);
    if let Err(e) = std::thread::Builder::new()
        .name("xime-speech".to_string())
        .spawn(move || worker(rx, shared))
    {
        warn!("Speech: 启动 worker 失败：{e}");
    }
    info!("Speech: 听写 worker 已就绪（引擎在 xime-speechd）");
}

/// 按住说话：开始录音。已在听时忽略（按键 auto-repeat 会连发按下）。
pub fn press() {
    send(SpeechCommand::Start);
}

/// 松手结束：停止录音并识别上屏。
pub fn release() {
    send(SpeechCommand::Stop);
}

/// 托盘 / 全局快捷键：按一下开始，再按一下结束。
pub fn toggle() {
    match state() {
        SpeechState::Idle => send(SpeechCommand::Start),
        SpeechState::Listening => send(SpeechCommand::Stop),
        // 装载/识别中再按一次没有可做的：不打断已经录到的语音。
        SpeechState::Loading | SpeechState::Transcribing => {
            debug!("Speech: toggle ignored while {}", state().as_str());
        }
    }
}

/// 取消当前会话（Esc / 失活）：停止录音但**不上屏**。
pub fn cancel() {
    send(SpeechCommand::Cancel);
}

/// 当前状态。
pub fn state() -> SpeechState {
    *shared().state.lock().unwrap()
}

/// 清空滞留事件与电平（失活/停用时调用）。
pub fn clear_events() {
    shared().events.lock().unwrap().clear();
    let mut levels = shared().levels.lock().unwrap();
    *levels = std::iter::repeat_n(0.0, LEVEL_BARS).collect();
}

/// 取走事件。
pub fn drain_events(mut on_event: impl FnMut(SpeechEvent)) {
    let events: Vec<SpeechEvent> = {
        let mut guard = shared().events.lock().unwrap();
        if guard.is_empty() {
            return;
        }
        std::mem::take(&mut *guard)
    };
    for event in events {
        on_event(event);
    }
}

/// 悬浮层频谱快照（长度恒为 [`LEVEL_BARS`]）。
pub fn levels_snapshot() -> Vec<f32> {
    let levels = shared().levels.lock().unwrap();
    levels.iter().copied().collect()
}

/// 面板用的听写进展快照（已上屏字数 + 识别异常）。
pub fn progress() -> Progress {
    shared().progress.lock().unwrap().clone()
}

/// 设置页状态快照（JSON，字段对齐 `xime-setup-lib` 的 `SpeechServerStatus`）。
///
/// 服务没起来时给一份 idle 空表：设置页显示「没有模型」而不是轮询超时。
pub fn status_json() -> String {
    let status = shared()
        .last_status
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(Status::state_idle);
    serde_json::to_string(&status).unwrap_or_else(|_| "{}".to_string())
}

fn send(command: SpeechCommand) {
    let shared = shared();
    let guard = shared.tx.lock().unwrap();
    if let Some(tx) = guard.as_ref() {
        if tx.send(command).is_err() {
            warn!("Speech: worker 已退出，命令被丢弃");
        }
    }
}

fn set_state(shared: &Shared, next: SpeechState) {
    {
        let mut guard = shared.state.lock().unwrap();
        if *guard == next {
            return;
        }
        *guard = next;
    }
    // 会话结束时清掉进展：否则下一次听写的第一帧还挂着上次的错误或字数。
    if next == SpeechState::Idle {
        *shared.progress.lock().unwrap() = Progress::default();
    }
    push_event(shared, SpeechEvent::State(next));
}

fn push_event(shared: &Shared, event: SpeechEvent) {
    let mut events = shared.events.lock().unwrap();
    // 主循环正常情况下每轮都取走；上限只是防「读侧卡住」把内存涨起来。
    if events.len() >= 64 {
        events.remove(0);
    }
    events.push(event);
}

fn push_level(shared: &Shared, level: u8) {
    let mut levels = shared.levels.lock().unwrap();
    let current = (level as f32 / 100.0).clamp(0.0, 1.0);
    let previous = levels.back().copied().unwrap_or(0.0);
    // 快起慢落：起音立刻跟上，回落做一阶低通，视觉上像真实电平表。
    let next = if current > previous {
        current
    } else {
        previous * 0.7 + current * 0.3
    };
    levels.push_back(next);
    while levels.len() > LEVEL_BARS {
        levels.pop_front();
    }
}

/// 等待一次识别结果时的记账。
struct Pending {
    discard: bool,
    started: Instant,
}

fn worker(rx: Receiver<SpeechCommand>, shared: &'static Shared) {
    let mut session = Session::default();

    loop {
        while let Ok(command) = rx.try_recv() {
            match command {
                SpeechCommand::Start => session.start(shared),
                SpeechCommand::Stop => session.stop(shared, false),
                SpeechCommand::Cancel => session.stop(shared, true),
            }
        }
        session.poll(shared);
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// worker 独占的会话状态（`Client` 非 Clone，全部留在 worker 线程里）。
#[derive(Default)]
struct Session {
    client: Option<Client>,
    listening: bool,
    /// 已停止、等识别结果。
    pending: Option<Pending>,
    /// 已经上屏的定稿字符数。流式识别里 `committed` 单调变长，只补差量。
    committed_chars: usize,
    /// 已渲染的预览修订号（没变就不重画）。
    partial_rev_seen: u64,
}

impl Session {
    fn start(&mut self, shared: &Shared) {
        // 已在听或正在识别时不重复开始：按键 auto-repeat 会连发按下，
        // 而重复 `listen_start` 会让服务把刚录到的那段丢掉重录。
        if self.listening || self.pending.is_some() {
            return;
        }
        if self.client.is_none() {
            match connect_or_spawn() {
                Ok(client) => self.client = Some(client),
                Err(error) => {
                    fail(shared, error);
                    return;
                }
            }
        }
        let Some(client) = self.client.as_mut() else {
            return;
        };
        match client.call(&Request::ListenStart) {
            Ok(response) if response.ok => {
                self.listening = true;
                // 新一轮听写：上屏水位与预览修订都归零。
                self.committed_chars = 0;
                self.partial_rev_seen = 0;
                let ready = response
                    .status
                    .as_ref()
                    .map(|status| status.model_ready)
                    .unwrap_or(true);
                if let Some(status) = response.status {
                    *shared.last_status.lock().unwrap() = Some(status);
                }
                // 首次使用会在后台现场装载模型（秒级到十几秒）。先报
                // Loading，悬浮层显示「正在装载语音引擎」，不至于让人以为
                // 按了没反应——录音本身已经开始，装载完就能识别。
                if !ready {
                    set_state(shared, SpeechState::Loading);
                }
                set_state(shared, SpeechState::Listening);
            }
            Ok(response) => fail(
                shared,
                response.error.unwrap_or_else(|| "开始录音失败".to_string()),
            ),
            Err(error) => {
                self.client = None;
                fail(shared, error);
            }
        }
    }

    fn stop(&mut self, shared: &Shared, discard: bool) {
        if !self.listening || self.pending.is_some() {
            return;
        }
        let Some(client) = self.client.as_mut() else {
            self.listening = false;
            set_state(shared, SpeechState::Idle);
            return;
        };
        match client.call(&Request::ListenStop) {
            Ok(response) if response.ok => {
                self.listening = false;
                self.pending = Some(Pending {
                    discard,
                    started: Instant::now(),
                });
                if let Some(status) = response.status {
                    *shared.last_status.lock().unwrap() = Some(status);
                }
                set_state(shared, SpeechState::Transcribing);
            }
            Ok(response) => {
                self.listening = false;
                fail(
                    shared,
                    response.error.unwrap_or_else(|| "结束录音失败".to_string()),
                );
            }
            Err(error) => {
                self.client = None;
                self.listening = false;
                fail(shared, error);
            }
        }
    }

    fn poll(&mut self, shared: &Shared) {
        if !self.listening && self.pending.is_none() {
            return;
        }
        let Some(client) = self.client.as_mut() else {
            return;
        };
        let response = match client.call(&Request::Status) {
            Ok(response) => response,
            Err(error) => {
                // 服务没了（被杀/崩溃）：清理会话，别让悬浮层一直挂着。
                self.client = None;
                self.listening = false;
                self.pending = None;
                fail(shared, format!("语音服务断开：{error}"));
                return;
            }
        };
        let Some(status) = response.status else {
            return;
        };

        if self.listening {
            push_level(shared, status.level);
            // 流式听写：定稿部分立刻上屏（`committed` 单调变长，只补差量），
            // 未定稿部分驱动悬浮层/候选栏的实时反馈。
            let committed_chars = status.committed.chars().count();
            {
                // 面板靠这两个值区分「在录」和「在识别」：波形动 = 麦克风有
                // 输入，字数涨 = 识别在推进，warning 有值 = 解码确实报错了。
                let mut progress = shared.progress.lock().unwrap();
                progress.committed_chars = committed_chars;
                progress.warning = status.error.clone();
            }
            if committed_chars > self.committed_chars {
                let fresh: String = status
                    .committed
                    .chars()
                    .skip(self.committed_chars)
                    .collect();
                self.committed_chars = committed_chars;
                if !fresh.is_empty() {
                    push_event(shared, SpeechEvent::Committed(fresh));
                }
            }
            if status.partial_rev != self.partial_rev_seen {
                self.partial_rev_seen = status.partial_rev;
                push_event(shared, SpeechEvent::Partial(status.partial.clone()));
            }
        }

        if let Some(pending) = self.pending.as_mut() {
            let finished = matches!(status.state.as_str(), "idle" | "error");
            let timed_out = pending.started.elapsed() > TRANSCRIBE_TIMEOUT;
            if !finished && !timed_out {
                *shared.last_status.lock().unwrap() = Some(status);
                return;
            }
            let discard = pending.discard;
            self.pending = None;
            *shared.last_status.lock().unwrap() = Some(status.clone());
            if discard {
                // 取消：预览丢掉；已经定稿并上屏的部分保留——那部分模型已经
                // 确定，符合「取消的是还没说完的那半句」的直觉。
                push_event(shared, SpeechEvent::Partial(String::new()));
                set_state(shared, SpeechState::Idle);
            } else if timed_out {
                fail(shared, "识别超时，模型可能还没装载完".to_string());
            } else if let Some(error) = status.error {
                fail(shared, error);
            } else {
                // 流式已经把定稿部分逐段上过屏了，这里只补最后的尾巴，
                // 避免整段重打一遍（重复落字比漏字更难接受）。
                let total = status.text.chars().count();
                if total > self.committed_chars {
                    let rest: String = status.text.chars().skip(self.committed_chars).collect();
                    if !rest.trim().is_empty() {
                        push_event(shared, SpeechEvent::Committed(rest));
                        self.committed_chars = total;
                    }
                }
                push_event(shared, SpeechEvent::Partial(String::new()));
                if self.committed_chars == 0 {
                    fail(shared, "没有听清，再说一次".to_string());
                } else {
                    set_state(shared, SpeechState::Idle);
                }
            }
            return;
        }

        // 录音中服务自己停了（麦克风被拔/采集失败）：如实结束会话。
        if status.state != "listening" {
            self.listening = false;
            fail(
                shared,
                status.error.unwrap_or_else(|| "录音意外中断".to_string()),
            );
            return;
        }
        *shared.last_status.lock().unwrap() = Some(status);
    }
}

fn fail(shared: &Shared, message: String) {
    warn!("Speech: {message}");
    push_event(shared, SpeechEvent::Error(message));
    set_state(shared, SpeechState::Idle);
}

fn connect_or_spawn() -> Result<Client, String> {
    let socket = socket_path();
    let endpoint = socket.display().to_string();
    if let Ok(client) = Client::connect(&endpoint) {
        return Ok(client);
    }

    let binary = speechd_binary()
        .ok_or_else(|| "找不到 xime-speechd（语音识别需要它，重跑 dev-install.sh）".to_string())?;
    let models = models_dir();
    info!(
        "Speech: 拉起 {}（socket={endpoint} models={}）",
        binary.display(),
        models.display()
    );

    let mut command = Command::new(&binary);
    command
        .arg("--socket")
        .arg(&socket)
        .arg("--models")
        .arg(&models)
        // 不传 --device：speechd 启动时自己探测后端。写死 cpu 会让编好的 cuda
        // 版退化成 2 秒窗口（流式只能做到句级）。
        .arg("--idle-secs")
        .arg("300")
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    // stderr 落到日志：服务起不来时（缺模型、没有麦克风）这里才有原因。
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(config_dir().join("xime-speechd.log"))
    {
        Ok(file) => command.stderr(Stdio::from(file)),
        Err(_) => command.stderr(Stdio::null()),
    };
    command
        .spawn()
        .map_err(|e| format!("启动 xime-speechd 失败：{e}"))?;

    let deadline = Instant::now() + CONNECT_TIMEOUT;
    while Instant::now() < deadline {
        if let Ok(client) = Client::connect(&endpoint) {
            return Ok(client);
        }
        std::thread::sleep(Duration::from_millis(120));
    }
    Err(format!(
        "语音服务没起来（{}），看 {} 里的原因",
        endpoint,
        config_dir().join("xime-speechd.log").display()
    ))
}

/// `xime-speechd` 与 daemon 同目录安装（`~/.local/bin` 或 `/usr/bin`），
/// 找不到再退回 PATH。
fn speechd_binary() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("xime-speechd");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join("xime-speechd"))
        .find(|path| path.is_file())
}

fn config_dir() -> PathBuf {
    xime_config::app_db::default_db_path()
        .parent()
        .map(|dir| dir.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn socket_path() -> PathBuf {
    config_dir().join("speech.sock")
}

/// 模型根目录：`xime-speech-service` 约定「每个子目录 = 一个模型」。
fn models_dir() -> PathBuf {
    config_dir().join("models")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 电平快照长度恒定：悬浮层按固定条数取样本，长度抖动会让条形错位。
    #[test]
    fn level_buffer_keeps_fixed_length() {
        let shared = Shared::new();
        assert_eq!(shared.levels.lock().unwrap().len(), LEVEL_BARS);
        for level in [0u8, 30, 100, 50] {
            push_level(&shared, level);
        }
        assert_eq!(shared.levels.lock().unwrap().len(), LEVEL_BARS);
    }

    /// 起音立刻满格、回落缓慢（快起慢落）。
    #[test]
    fn level_attack_is_immediate_and_release_is_smoothed() {
        let shared = Shared::new();
        push_level(&shared, 100);
        let peak = *shared.levels.lock().unwrap().back().unwrap();
        assert!((peak - 1.0).abs() < 1e-6, "起音应立刻到 1.0，实际 {peak}");

        push_level(&shared, 0);
        let after = *shared.levels.lock().unwrap().back().unwrap();
        assert!(
            after > 0.5 && after < 1.0,
            "回落应是衰减而不是清零：{after}"
        );
    }

    /// 事件上限：读侧卡住时不能无限增长。
    #[test]
    fn event_queue_is_bounded() {
        let shared = Shared::new();
        for i in 0..100 {
            push_event(&shared, SpeechEvent::Partial(i.to_string()));
        }
        assert_eq!(shared.events.lock().unwrap().len(), 64);
    }

    /// 状态只迁移一次：重复设置同一状态不重复发事件。
    #[test]
    fn duplicate_state_does_not_emit() {
        let shared = Shared::new();
        set_state(&shared, SpeechState::Listening);
        set_state(&shared, SpeechState::Listening);
        let events = shared.events.lock().unwrap();
        assert_eq!(events.len(), 1, "同一状态应只发一次事件");
    }

    /// 解析 socket / 模型目录落在同一配置根下（与 xime.db 同级）。
    #[test]
    fn paths_share_config_root() {
        assert_eq!(socket_path().parent(), models_dir().parent());
    }

    #[test]
    fn state_strings_match_setup_contract() {
        assert_eq!(SpeechState::Idle.as_str(), "idle");
        assert_eq!(SpeechState::Loading.as_str(), "loading");
        assert_eq!(SpeechState::Listening.as_str(), "listening");
        assert_eq!(SpeechState::Transcribing.as_str(), "transcribing");
    }
}
