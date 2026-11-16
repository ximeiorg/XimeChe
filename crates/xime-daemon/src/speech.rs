//! 语音听写（占位实现）。
//!
//! 原先基于 `xime-speech`（sherpa-onnx）做流式 ASR。项目已决定统一改用
//! candle（`xime-infer`）做本地推理，sherpa-onnx 依赖已移除；在 candle 版
//! 识别器接入前，本模块保留既有对外 API（DBus 状态、设置页、托盘、候选栏
//! 🎙️ 都还按这个契约接线），但 `toggle` 只记一条日志，不再采集音频。
//!
//! 后续接入 candle ASR 时只替换本文件实现，wayland/daemon/setup 的接线不动。

// 这些事件/状态由 candle ASR 接入后使用；当前占位实现不会构造它们。
#![allow(dead_code)]

use tracing::{info, warn};

/// 频谱条数（候选栏语音反馈与 `xime-ui::voice::BAR_COUNT` 必须一致）。
pub const LEVEL_BARS: usize = 24;

/// worker 发给主循环的事件（保留原契约，candle 实现接入后复用）。
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
    /// 设置页状态字符串（对齐 `SpeechServerStatus.state` 语义）。
    pub fn as_str(&self) -> &'static str {
        match self {
            SpeechState::Idle => "idle",
            SpeechState::Loading => "loading",
            SpeechState::Listening => "listening",
        }
    }
}

/// 初始化：占位实现不装载模型、不占麦克风。
pub fn init() {
    info!("Speech: sherpa-onnx 已移除，语音识别等待 candle 实现；当前为占位实现");
}

/// 启停听写：占位实现只记日志（保留托盘/候选栏/快捷键的调用点）。
pub fn toggle() {
    warn!("Speech: 语音识别暂不可用（sherpa-onnx 已移除，candle ASR 尚未接入）");
}

/// 当前永远是 `Idle`。
pub fn state() -> SpeechState {
    SpeechState::Idle
}

/// 清空滞留事件与电平（占位实现无事件）。
pub fn clear_events() {}

/// 取走事件（占位实现永不产生事件）。
pub fn drain_events(_on_event: impl FnMut(SpeechEvent)) {}

/// 悬浮层频谱快照：全零条（视觉上静音）。
pub fn levels_snapshot() -> Vec<f32> {
    vec![0.0; LEVEL_BARS]
}

/// 模型操作占位（设置页仍可点击，但不会下载/删除任何东西）。
pub fn download_model(_id: &str) {}

pub fn delete_model(_id: &str) {}

pub fn select_model(_id: &str) {}

pub fn test_start() {}

pub fn test_stop() {}

/// 设置页状态快照（字段对齐 `xime-setup-lib` 的 `SpeechServerStatus`）。
///
/// 返回空模型列表 + 一句错误说明，页面上会显示「模型未下载」而不是崩溃。
pub fn status_json() -> String {
    serde_json::json!({
        "state": "idle",
        "model_id": "",
        "model_name": "",
        "model_ready": false,
        "provider": "未启用（sherpa-onnx 已移除）",
        "text": "",
        "error": "语音识别已改用 candle 方案，当前版本尚未接入",
        "download": null,
        "models_rev": 0,
        "models": [],
    })
    .to_string()
}
