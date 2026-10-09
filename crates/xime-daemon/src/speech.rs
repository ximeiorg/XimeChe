//! 语音听写（P10：核心链路 + 设置页数据源）。
//!
//! 分工（对齐 xime-speech 的设计注释）：纯推理在 [`xime_speech`]（sherpa-onnx
//! 流式 zipformer，端点检测内建）；本模块负责宿主侧四件事——
//!
//! 1. **模型管理（daemon 是权威数据源）**：下载（ModelScope tar.bz2）、删除、
//!    选中（持久化 `~/.config/xime/speech.json`）都由本模块执行；目录
//!    `~/.config/xime/models/<id>/` 与设置程序 models_dir 同一约定。候选栏
//!    🎙️ 不做下载——未就绪时提示用户去设置程序「语音转文本」页处理；
//! 2. **状态快照**：[`status_json`] 组装设置页轮询的全部数据（引擎状态 +
//!    模型列表 + 下载进度），DBus `GetSpeechStatus` 直读；
//! 3. **音频采集**：PulseAudio 简单 API（16kHz 单声道 s16ne；KDE 的
//!    PipeWire 经 pipewire-pulse 兼容），dlopen 运行时绑定，构建零依赖；
//! 4. **会话线程**：单个 worker 独占 [`StreamingRecognizer`]（非 Send 共享
//!    语义），命令进 / 事件出；主循环只消费事件做上屏与候选栏反馈。
//!
//! 交互（对齐 XimeYao 设置页说明的设计意图）：点候选栏 🎙️ 开始听写，
//! 停顿时自动上屏（端点检测断句），再点 🎙️ 结束。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use tracing::{debug, error, info};
use xime_speech::{AsrModelProfile, AsrModelRegistry, SpeechConfig, StreamingRecognizer};

// ── PulseAudio 简单 API 的内嵌 dlopen 绑定 ──────────────────────────
// 只用三个函数（new/read/free），运行时加载 libpulse-simple.so.0——KDE 的
// PipeWire 经 pipewire-pulse 兼容此 API；构建零系统依赖，缺失时给出可读错误。

/// pa_sample_spec（simple.h）：format(u32) + rate(u32) + channels(u8)，C 对齐。
#[repr(C)]
struct PaSampleSpec {
    format: u32,
    rate: u32,
    channels: u8,
}

/// PA_SAMPLE_S16NE（本机字节序 s16）。
const PA_SAMPLE_S16NE: u32 = 3;
/// PA_STREAM_RECORD。
const PA_STREAM_RECORD: u32 = 2;

/// libpulse-simple 运行时绑定（占用即独占录音流，Drop 释放）。
struct PulseCapture {
    _lib: libloading::Library,
    handle: *mut std::ffi::c_void,
    read_fn: unsafe extern "C" fn(*mut std::ffi::c_void, *mut u8, usize, *mut i32) -> i32,
    error: i32,
}

impl PulseCapture {
    /// 打开默认录音源（16kHz mono s16ne）。
    fn new() -> Result<Self, String> {
        type PaSimpleNew = unsafe extern "C" fn(
            *const std::ffi::c_char,
            *const std::ffi::c_char,
            u32,
            *const std::ffi::c_char,
            *const std::ffi::c_char,
            *const PaSampleSpec,
            *const std::ffi::c_void,
            *const std::ffi::c_void,
            *mut i32,
        ) -> *mut std::ffi::c_void;
        type PaSimpleFree = unsafe extern "C" fn(*mut std::ffi::c_void);
        type PaSimpleRead =
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut u8, usize, *mut i32) -> i32;

        unsafe {
            let lib = libloading::Library::new("libpulse-simple.so.0").map_err(|e| {
                format!("未找到 libpulse-simple（需要 pipewire-pulse 或 pulseaudio）：{e}")
            })?;
            let new_fn: libloading::Symbol<PaSimpleNew> = lib
                .get(b"pa_simple_new")
                .map_err(|e| format!("libpulse-simple 缺少 pa_simple_new：{e}"))?;
            let read_fn: libloading::Symbol<PaSimpleRead> = lib
                .get(b"pa_simple_read")
                .map_err(|e| format!("libpulse-simple 缺少 pa_simple_read：{e}"))?;
            let _free_fn: libloading::Symbol<PaSimpleFree> =
                lib.get(b"pa_simple_free").map_err(|e| format!("{e}"))?;

            let spec = PaSampleSpec {
                format: PA_SAMPLE_S16NE,
                rate: SAMPLE_RATE,
                channels: CHANNELS as u8,
            };
            let cstr = |s: &str| std::ffi::CString::new(s).unwrap_or_default();
            let handle = new_fn(
                std::ptr::null(), // server = 默认
                cstr("xime-daemon").as_ptr(),
                PA_STREAM_RECORD,
                std::ptr::null(), // 设备 = 默认
                cstr("xime-dictation").as_ptr(),
                &spec,
                std::ptr::null(), // channel map
                std::ptr::null(), // buffer attr
                &mut 0,
            );
            if handle.is_null() {
                return Err("麦克风打开失败：请检查输入设备与 pipewire-pulse 服务".into());
            }
            // Symbol 借用 lib；函数指针拷出后 lib 一起存进结构体。
            let read_fn: PaSimpleRead = *read_fn;
            Ok(Self {
                _lib: lib,
                handle,
                read_fn,
                error: 0,
            })
        }
    }

    /// 读取一块 s16 样本（字节缓冲，调用方转 i16）。
    fn read(&mut self, buf: &mut [u8]) -> Result<(), String> {
        unsafe {
            if (self.read_fn)(self.handle, buf.as_mut_ptr(), buf.len(), &mut self.error) < 0 {
                return Err("录音读取失败（麦克风被占用或已断开？）".into());
            }
        }
        Ok(())
    }
}

impl Drop for PulseCapture {
    fn drop(&mut self) {
        type PaSimpleFree = unsafe extern "C" fn(*mut std::ffi::c_void);
        unsafe {
            if let Ok(free_fn) = self._lib.get::<PaSimpleFree>(b"pa_simple_free") {
                free_fn(self.handle);
            }
        }
    }
}

// ── 命令 / 事件 ─────────────────────────────────────────────────────

/// 发给 worker 的命令。
#[derive(Debug)]
pub enum SpeechCommand {
    /// 🎙️ 点击：Idle → 开始听写；Listening → 结束上屏。
    Toggle,
    /// 设置页请求下载模型（独立线程执行，进度经事件回）。
    DownloadModel(String),
    /// 设置页请求删除模型目录（听写中且是选中模型时拒绝）。
    DeleteModel(String),
    /// 设置页切换选中模型（持久化 + 下次会话生效）。
    SelectModel(String),
}

/// worker 发给主循环的事件（主循环负责上屏 / 候选栏反馈 / 桌面通知）。
#[derive(Debug, Clone)]
pub enum SpeechEvent {
    /// 状态迁移（驱动候选栏样式与托盘反馈）。
    State(SpeechState),
    /// 听写中的中间文本（候选栏实时显示）。
    Partial(String),
    /// 一句识别完成（端点断句或停止收尾），主循环 commit_string 上屏。
    Committed(String),
    /// 失败（模型未就绪/装载/采集），文案已面向用户。
    Error(String),
}

/// 听写状态机。
#[derive(Debug, Clone, PartialEq)]
pub enum SpeechState {
    Idle,
    /// 模型装载中（首次 ~秒级）。
    Loading,
    Listening,
}

impl SpeechState {
    /// 设置页状态字符串（对齐 SpeechServerStatus.state 语义）。
    pub fn as_str(&self) -> &'static str {
        match self {
            SpeechState::Idle => "idle",
            SpeechState::Loading => "loading",
            SpeechState::Listening => "listening",
        }
    }
}

/// 采集参数：16kHz 单声道，每块 1024 样本 ≈ 64ms。
const SAMPLE_RATE: u32 = 16_000;
const CHANNELS: u16 = 1;
const BLOCK_SAMPLES: usize = 1024;

/// 选中模型持久化（会话开始时读取；测试重定向，与 recent_usage 同一教训）。
fn speech_config_path() -> PathBuf {
    #[cfg(test)]
    let path = std::env::temp_dir().join(format!(
        "xime-speech-config-test-{}.json",
        std::process::id()
    ));
    #[cfg(not(test))]
    let path = {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
        Path::new(&home)
            .join(".config")
            .join(xime_config::app_metadata().config_dir_name)
            .join("speech.json")
    };
    path
}

/// 读选中的模型 id（未设置/损坏 = 空串 → 会话回退默认模型）。
fn read_selected_id() -> String {
    std::fs::read_to_string(speech_config_path())
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| v.get("model").and_then(|m| m.as_str()).map(str::to_string))
        .unwrap_or_default()
}

/// 持久化选中的模型 id。
fn write_selected_id(id: &str) -> anyhow::Result<()> {
    let path = speech_config_path();
    xime_config::atomic_write(
        &path,
        serde_json::json!({ "model": id }).to_string().as_bytes(),
    )?;
    Ok(())
}

/// 模型数据根（与设置程序 models_dir 同一约定：~/.config/xime/models）。
fn models_root() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    Path::new(&home)
        .join(".config")
        .join(xime_config::app_metadata().config_dir_name)
        .join("models")
}

#[cfg(test)]
fn models_root_for_test() -> PathBuf {
    std::env::temp_dir().join(format!("xime-speech-test-{}", std::process::id()))
}

/// 模型目录是否就绪：profile 要求的四个文件全部存在且非空。
fn is_model_ready(profile: &AsrModelProfile, dir: &Path) -> bool {
    [
        profile.encoder_file.as_str(),
        profile.decoder_file.as_str(),
        profile.joiner_file.as_str(),
        profile.tokens_file.as_str(),
    ]
    .iter()
    .all(|name| {
        dir.join(name).is_file() && std::fs::metadata(dir.join(name)).is_ok_and(|m| m.len() > 0)
    })
}

/// 下载 tar.bz2（流式，进度经 `on_progress` 上报 0.0~1.0）。
fn download_archive(
    profile: &AsrModelProfile,
    dest: &Path,
    on_progress: &dyn Fn(f32),
) -> anyhow::Result<()> {
    use std::io::{Read, Write};
    info!(
        "Downloading ASR model '{}' from {}",
        profile.id, profile.download_url
    );
    // ModelScope 会 403 掉无 UA 的客户端，带一个正常 UA。
    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) xime-input-method")
        .build()?;
    let mut resp = client.get(&profile.download_url).send()?;
    if !resp.status().is_success() {
        anyhow::bail!("模型下载失败：HTTP {}", resp.status());
    }
    let total = resp.content_length().unwrap_or(0);
    let mut file = std::fs::File::create(dest)?;
    let mut downloaded: u64 = 0;
    let mut last_reported = 0.0f32;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = resp.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        file.write_all(&buffer[..n])?;
        downloaded += n as u64;
        if total > 0 {
            let progress = downloaded as f32 / total as f32;
            // 每 5% 上报一次，避免事件风暴。
            if progress - last_reported >= 0.05 {
                last_reported = progress;
                on_progress(progress);
            }
        }
    }
    file.flush()?;
    on_progress(1.0);
    info!("ASR model downloaded: {downloaded} bytes");
    Ok(())
}

/// 解压 tar.bz2 并把 profile 要求的四个文件归位到 `model_dir`（发布包内
/// 通常带一层日期目录，递归扫描按文件名找、按角色拷贝到目标根）。
fn extract_archive(
    archive: &Path,
    profile: &AsrModelProfile,
    model_dir: &Path,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(model_dir)?;
    let bz = bzip2::read::BzDecoder::new(std::fs::File::open(archive)?);
    let mut tar = tar::Archive::new(bz);
    let work = model_dir.join("_extract");
    std::fs::create_dir_all(&work)?;
    tar.unpack(&work)?;

    for file_name in [
        &profile.encoder_file,
        &profile.decoder_file,
        &profile.joiner_file,
        &profile.tokens_file,
    ] {
        let target = model_dir.join(file_name);
        let found = find_file_recursive(&work, file_name)?;
        let Some(found) = found else {
            anyhow::bail!("压缩包里找不到 {}", file_name);
        };
        std::fs::copy(&found, &target)?;
    }
    std::fs::remove_dir_all(&work)?;
    Ok(())
}

/// 递归找文件名精确匹配的第一个文件。
fn find_file_recursive(root: &Path, name: &str) -> anyhow::Result<Option<PathBuf>> {
    let mut out = None;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().is_some_and(|n| n == name) {
                out = Some(path);
                break;
            }
        }
        if out.is_some() {
            break;
        }
    }
    Ok(out)
}

/// 下载 + 解压 + 校验，返回就绪的模型目录。
fn ensure_model(
    profile: &AsrModelProfile,
    root: &Path,
    on_progress: &dyn Fn(f32),
) -> anyhow::Result<PathBuf> {
    let dir = root.join(&profile.id);
    if is_model_ready(profile, &dir) {
        return Ok(dir);
    }
    std::fs::create_dir_all(&dir)?;
    let archive = dir.join("_download.tar.bz2");
    let result = (|| -> anyhow::Result<()> {
        download_archive(profile, &archive, on_progress)?;
        extract_archive(&archive, profile, &dir)?;
        std::fs::remove_file(&archive).ok();
        if !is_model_ready(profile, &dir) {
            anyhow::bail!("解压后模型文件不齐");
        }
        Ok(())
    })();
    if let Err(e) = result {
        std::fs::remove_dir_all(&dir).ok();
        return Err(e);
    }
    info!("ASR model '{}' ready at {}", profile.id, dir.display());
    Ok(dir)
}

// ── 全局桥（DBus 线程与 wayland 主循环共享）─────────────────────────

/// daemon 内部状态视图（worker/下载线程是写者，DBus/主循环是读者）。
#[derive(Default)]
struct SpeechView {
    state: Option<SpeechState>,
    /// 听写中的 partial / 最近一次识别文本（试听回显）。
    text: String,
    last_error: Option<String>,
    /// 正在下载的模型与进度（设置页进度条）。
    download: Option<(String, f32)>,
    selected_id: String,
}

static CMD_TX: OnceLock<Sender<SpeechCommand>> = OnceLock::new();
static EVENT_RX: OnceLock<Mutex<Receiver<SpeechEvent>>> = OnceLock::new();
static VIEW: OnceLock<Mutex<SpeechView>> = OnceLock::new();
/// 模型集合变化计数（下载/删除完成的独立线程也能原子自增）。
static MODELS_REV: AtomicU64 = AtomicU64::new(0);

fn view() -> &'static Mutex<SpeechView> {
    VIEW.get_or_init(|| Mutex::new(SpeechView::default()))
}

/// 启动 worker 线程（daemon 启动时调用一次）。
pub fn init() {
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<SpeechCommand>();
    let (event_tx, event_rx) = std::sync::mpsc::channel::<SpeechEvent>();
    std::thread::Builder::new()
        .name("xime-speech".into())
        .spawn(move || worker_loop(cmd_rx, event_tx))
        .expect("spawn speech worker");
    let _ = CMD_TX.set(cmd_tx);
    let _ = EVENT_RX.set(Mutex::new(event_rx));
    let selected = read_selected_id();
    view().lock().unwrap_or_else(|p| p.into_inner()).selected_id = selected;
    info!("Speech bridge initialized");
}

/// worker/下载线程统一事件出口：先写视图（DBus 快照的权威来源，
/// 不依赖主循环是否在 drain），再进事件通道（主循环做上屏/候选栏反馈）。
fn emit(event: SpeechEvent, event_tx: &Sender<SpeechEvent>) {
    {
        let mut view = view().lock().unwrap_or_else(|p| p.into_inner());
        match &event {
            SpeechEvent::State(state) => {
                view.state = Some(state.clone());
                if *state != SpeechState::Listening {
                    view.text.clear();
                }
            }
            SpeechEvent::Partial(text) => view.text = text.clone(),
            SpeechEvent::Committed(text) => view.text = text.clone(),
            SpeechEvent::Error(e) => {
                view.last_error = Some(e.clone());
                view.download = None;
            }
        }
    }
    let _ = event_tx.send(event);
}

fn send_cmd(cmd: SpeechCommand) {
    match CMD_TX.get() {
        Some(tx) => {
            if tx.send(cmd).is_err() {
                error!("speech worker gone");
            }
        }
        None => error!("speech bridge not initialized"),
    }
}

/// 🎙️ 切换（不阻塞，结果经事件回）。
pub fn toggle() {
    send_cmd(SpeechCommand::Toggle);
}

/// 设置页请求下载模型（独立线程执行，进度写视图）。
pub fn download_model(id: &str) {
    send_cmd(SpeechCommand::DownloadModel(id.to_string()));
}

/// 设置页请求删除模型。
pub fn delete_model(id: &str) {
    send_cmd(SpeechCommand::DeleteModel(id.to_string()));
}

/// 设置页切换选中模型（持久化，下次会话生效）。
pub fn select_model(id: &str) {
    send_cmd(SpeechCommand::SelectModel(id.to_string()));
}

/// 设置页请求开始试听（= 候选栏 🎙️ 同一条会话）。
pub fn test_start() {
    toggle();
}

pub fn test_stop() {
    toggle();
}

/// 主循环每轮拉取事件（副作用：上屏/候选栏/通知都在调用方做）。
/// 视图已在 worker 的 emit 时更新——输入法未激活（setup 试听场景）
/// 主循环不 drain，快照依然实时。
pub fn drain_events(mut on_event: impl FnMut(SpeechEvent)) {
    let Some(rx) = EVENT_RX.get() else {
        return;
    };
    let rx = rx.lock().unwrap_or_else(|p| p.into_inner());
    loop {
        match rx.try_recv() {
            Ok(event) => on_event(event),
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => break,
        }
    }
}

/// 当前引擎状态（候选栏 🎙️ 决策用）。
pub fn state() -> SpeechState {
    view()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .state
        .clone()
        .unwrap_or(SpeechState::Idle)
}

// ── 设置页状态快照（DBus GetSpeechStatus 的载荷）────────────────────

/// 一个可管理模型的 JSON 镜像（字段名与 libximecore speech_models 的
/// SpeechModelEntry 逐字对齐，设置端 serde 反序列化）。
#[derive(serde::Serialize)]
struct SpeechModelJson {
    id: String,
    name: String,
    description: String,
    size: String,
    downloaded: bool,
    selected: bool,
    recommended: bool,
}

/// 引擎状态快照（字段名与 SpeechServerStatus 逐字对齐）。
#[derive(serde::Serialize)]
struct SpeechStatusJson {
    state: String,
    model_id: String,
    model_name: String,
    model_ready: bool,
    provider: String,
    text: String,
    error: Option<String>,
    download: Option<(String, f32)>,
    models_rev: u64,
    models: Vec<SpeechModelJson>,
}

/// 组装设置页轮询快照（DBus GetSpeechStatus 直读；纯文件系统 + 内存视图）。
pub fn status_json() -> String {
    let view = view().lock().unwrap_or_else(|p| p.into_inner());
    let selected_id = if view.selected_id.is_empty() {
        AsrModelRegistry::default_profile().id
    } else {
        view.selected_id.clone()
    };
    let root = models_root();
    let models: Vec<SpeechModelJson> = AsrModelRegistry::profiles()
        .iter()
        .map(|p| SpeechModelJson {
            id: p.id.clone(),
            name: p.name.clone(),
            description: p.description.clone(),
            size: p.size.clone(),
            downloaded: is_model_ready(p, &root.join(&p.id)),
            selected: p.id == selected_id,
            recommended: p.id == AsrModelRegistry::recommended_id(),
        })
        .collect();
    let default_profile = AsrModelRegistry::default_profile();
    let model_ready = is_model_ready(&default_profile, &root.join(&selected_id));
    let state = view.state.clone().unwrap_or(SpeechState::Idle);
    let status = SpeechStatusJson {
        state: state.as_str().to_string(),
        model_id: selected_id.clone(),
        model_name: AsrModelRegistry::profile_or_default(&selected_id).name,
        model_ready: if selected_id == default_profile.id {
            is_model_ready(&default_profile, &root.join(&default_profile.id))
        } else {
            model_ready
        },
        provider: format!("cpu（{} 线程）", SpeechConfig::default().num_threads),
        text: view.text.clone(),
        error: view.last_error.clone(),
        download: view.download.clone(),
        models_rev: MODELS_REV.load(Ordering::Relaxed),
        models,
    };
    serde_json::to_string(&status).unwrap_or_default()
}

// ── worker ──────────────────────────────────────────────────────────

/// 模型 id 清洗：DBus 入参直接拼 `models_root()/id` 路径，先滤掉路径
/// 穿越（对照 user_dict 的 sanitize_dict_name；会话总线限同用户，但
/// 出错/被入侵的设置端不该能删 $HOME）。
fn sanitize_model_id(id: &str) -> Option<String> {
    let t = id.trim();
    if t.is_empty() || t.contains('/') || t.contains('\\') || t.contains("..") {
        return None;
    }
    Some(t.to_string())
}

/// worker 主循环：Idle 时阻塞等命令；下载/删除/选择即时处理，
/// Toggle 跑完整场听写会话。
fn worker_loop(cmd_rx: Receiver<SpeechCommand>, event_tx: Sender<SpeechEvent>) {
    loop {
        let Ok(cmd) = cmd_rx.recv() else {
            return; // 主循环退出
        };
        match cmd {
            SpeechCommand::Toggle => {
                // 会话内命令由 run_listening_session 的循环消费；
                // 这里清掉滞留的 Toggle 避免误停，其余命令（下载/删除/选择）
                // 回灌队列——不能静默丢弃（否则设置页的操作"没发生"且无
                // 任何错误提示）。
                while let Ok(c) = cmd_rx.try_recv() {
                    if !matches!(c, SpeechCommand::Toggle) {
                        send_cmd(c);
                    }
                }
                let selected = read_selected_id();
                let profile = AsrModelRegistry::profile_or_default(&selected);
                let dir = models_root().join(&profile.id);
                if !is_model_ready(&profile, &dir) {
                    // 引导去设置程序（候选栏 🎙️ 不做下载）。
                    emit(
                        SpeechEvent::Error(
                            "语音模型未下载，请打开设置程序的「语音转文本」页下载".into(),
                        ),
                        &event_tx,
                    );
                    continue;
                }
                emit(SpeechEvent::State(SpeechState::Loading), &event_tx);
                match run_listening_session(&profile, &cmd_rx, &event_tx) {
                    Ok(()) => info!("Speech session ended normally"),
                    Err(e) => {
                        error!("Speech session failed: {e:#}");
                        emit(SpeechEvent::Error(format!("{e:#}")), &event_tx);
                    }
                }
                emit(SpeechEvent::State(SpeechState::Idle), &event_tx);
            }
            SpeechCommand::DownloadModel(id) => match sanitize_model_id(&id) {
                Some(id) => download_model_async(&id, event_tx.clone()),
                None => emit(SpeechEvent::Error("模型 id 非法".into()), &event_tx),
            },
            SpeechCommand::DeleteModel(id) => match sanitize_model_id(&id) {
                Some(id) => handle_delete_model(&id, event_tx.clone()),
                None => emit(SpeechEvent::Error("模型 id 非法".into()), &event_tx),
            },
            SpeechCommand::SelectModel(id) => match sanitize_model_id(&id) {
                Some(id) => handle_select_model(&id, event_tx.clone()),
                None => emit(SpeechEvent::Error("模型 id 非法".into()), &event_tx),
            },
        }
    }
}

/// 下载在独立线程执行：134MB 期间 Toggle/其他操作不被阻塞。
/// 进度直接写视图（DBus 轮询可见），完成/失败发事件刷新。
fn download_model_async(id: &str, _event_tx: Sender<SpeechEvent>) {
    let id = id.to_string();
    let profile = AsrModelRegistry::profile_or_default(&id);
    // 已就绪 / 已在下载：不重复。检查与占位在同一把锁内完成——
    // 否则两次请求可同时穿过检查窗口，并发写同一临时文件互相截断。
    {
        let mut view = view().lock().unwrap_or_else(|p| p.into_inner());
        if is_model_ready(&profile, &models_root().join(&profile.id))
            || matches!(&view.download, Some((downloading, _)) if *downloading == profile.id)
        {
            return;
        }
        view.download = Some((profile.id.clone(), 0.0));
        view.last_error = None;
    }
    std::thread::Builder::new()
        .name("xime-speech-download".into())
        .spawn(move || {
            let result = ensure_model(&profile, &models_root(), &|p: f32| {
                let mut view = view().lock().unwrap_or_else(|p| p.into_inner());
                view.download = Some((profile.id.clone(), p));
            });
            match result {
                Ok(_) => {
                    MODELS_REV.fetch_add(1, Ordering::Relaxed);
                    let mut view = view().lock().unwrap_or_else(|p| p.into_inner());
                    view.download = None;
                    info!("Model '{}' download finished", profile.id);
                }
                Err(e) => {
                    error!("Model '{}' download failed: {e:#}", profile.id);
                    let mut view = view().lock().unwrap_or_else(|p| p.into_inner());
                    view.download = None;
                    view.last_error = Some(format!("模型「{}」下载失败：{e:#}", profile.name));
                }
            }
        })
        .expect("spawn speech download");
}

/// 删除模型目录；听写中的选中模型拒绝（会话正占着推理器）。
fn handle_delete_model(id: &str, event_tx: Sender<SpeechEvent>) {
    let selected = read_selected_id();
    // Loading 也要拒绝：模型装载与目录删除并发会装载失败且选中模型被清。
    if id == selected && state() != SpeechState::Idle {
        emit(
            SpeechEvent::Error("正在使用该模型，先停止后再删除".into()),
            &event_tx,
        );
        return;
    }
    let dir = models_root().join(id);
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => {
            MODELS_REV.fetch_add(1, Ordering::Relaxed);
            info!("Model '{id}' deleted");
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // 本来就没装：视为成功（设置页状态会刷新）。
            MODELS_REV.fetch_add(1, Ordering::Relaxed);
        }
        Err(e) => {
            emit(SpeechEvent::Error(format!("删除模型失败：{e}")), &event_tx);
        }
    }
}

/// 切换选中模型：持久化；听写中的切换在下次会话生效。
fn handle_select_model(id: &str, event_tx: Sender<SpeechEvent>) {
    let profile = AsrModelRegistry::profile_or_default(id);
    if !is_model_ready(&profile, &models_root().join(&profile.id)) {
        emit(
            SpeechEvent::Error(format!("模型「{}」未下载，先下载再选择", profile.name)),
            &event_tx,
        );
        return;
    }
    match write_selected_id(id) {
        Ok(()) => {
            view().lock().unwrap_or_else(|p| p.into_inner()).selected_id = id.to_string();
            info!("Speech model selected: {id}");
        }
        Err(e) => {
            emit(
                SpeechEvent::Error(format!("保存模型选择失败：{e}")),
                &event_tx,
            );
        }
    }
}

/// 一次听写会话：装载（模型必须已就绪，worker 已挡）→ 采集识别 → Stop 收尾。
fn run_listening_session(
    profile: &AsrModelProfile,
    cmd_rx: &Receiver<SpeechCommand>,
    event_tx: &Sender<SpeechEvent>,
) -> anyhow::Result<()> {
    let dir = models_root().join(&profile.id);

    // 装载（首次秒级）。
    let mut recognizer = StreamingRecognizer::open(profile, &dir, &SpeechConfig::default())
        .map_err(|e| anyhow::anyhow!("语音引擎装载失败：{e:?}"))?;

    // 采集（PulseAudio simple record：16k mono s16ne，dlopen 运行时绑定）。
    let mut capture = PulseCapture::new().map_err(|e| anyhow::anyhow!(e))?;

    emit(SpeechEvent::State(SpeechState::Listening), event_tx);
    let mut raw = vec![0u8; BLOCK_SAMPLES * 2];
    loop {
        // 停止命令优先检查（read 是阻塞点，块间隔 ~64ms 检查一次）。
        match cmd_rx.try_recv() {
            Ok(SpeechCommand::Toggle) => break,
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => break,
            // 会话内的模型管理命令同样视为停止信号（听写优先让位），
            // 但命令本身要回灌队列，让 worker 循环在会话结束后处理。
            Ok(
                cmd @ (SpeechCommand::DownloadModel(_)
                | SpeechCommand::DeleteModel(_)
                | SpeechCommand::SelectModel(_)),
            ) => {
                send_cmd(cmd);
                break;
            }
        }

        capture.read(&mut raw).map_err(|e| anyhow::anyhow!(e))?;
        // s16ne = 本机字节序 i16。
        let block: Vec<i16> = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| i16::from_ne_bytes(*pair))
            .collect();
        recognizer.accept_pcm16(SAMPLE_RATE as i32, &block);

        emit(SpeechEvent::Partial(recognizer.partial_text()), event_tx);

        // 停顿自动上屏：端点检测命中 → 当句提交、继续听下一句。
        if recognizer.is_endpoint() {
            let text = recognizer.partial_text();
            if !text.trim().is_empty() {
                emit(SpeechEvent::Committed(text), event_tx);
            }
            recognizer.reset();
            emit(SpeechEvent::Partial(String::new()), event_tx);
        }

        std::thread::sleep(Duration::from_millis(1));
    }

    // 收尾：finalize 冲出未断句的尾巴。
    drop(capture);
    let tail = recognizer.finalize();
    if !tail.trim().is_empty() {
        emit(SpeechEvent::Committed(tail), event_tx);
    }
    debug!("Speech capture stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_files_check_shape() {
        let profile = AsrModelRegistry::default_profile();
        assert!(!profile.encoder_file.is_empty());
        assert!(!profile.tokens_file.is_empty());
    }

    #[test]
    fn is_model_ready_rejects_incomplete_dir() {
        let profile = AsrModelRegistry::default_profile();
        let dir = models_root_for_test().join("incomplete");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!is_model_ready(&profile, &dir), "空目录不就绪");
        std::fs::write(dir.join(&profile.encoder_file), b"x").unwrap();
        assert!(!is_model_ready(&profile, &dir), "只差一个文件也不就绪");
        std::fs::remove_dir_all(models_root_for_test()).ok();
    }

    #[test]
    fn selected_id_persists_roundtrip() {
        write_selected_id("zipformer-zh-int8").unwrap();
        assert_eq!(read_selected_id(), "zipformer-zh-int8");
        std::fs::remove_file(speech_config_path()).ok();
    }

    #[test]
    fn status_json_shape_matches_setup_mirror() {
        // 字段名必须与 libximecore speech_models 的 SpeechServerStatus /
        // SpeechModelEntry 逐字对齐（设置端 serde 反序列化依赖）。
        let json = status_json();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        for key in [
            "state",
            "model_id",
            "model_name",
            "model_ready",
            "provider",
            "text",
            "error",
            "download",
            "models_rev",
            "models",
        ] {
            assert!(v.get(key).is_some(), "缺少字段 {key}");
        }
        if let Some(models) = v.get("models").and_then(|m| m.as_array()) {
            if let Some(first) = models.first() {
                for key in [
                    "id",
                    "name",
                    "description",
                    "size",
                    "downloaded",
                    "selected",
                    "recommended",
                ] {
                    assert!(first.get(key).is_some(), "models[] 缺少字段 {key}");
                }
            }
        }
    }
}
