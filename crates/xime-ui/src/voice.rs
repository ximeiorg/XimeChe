//! 语音输入悬浮面板模型与几何（屏幕底部居中，圆角大卡片 + 频谱 + 实时文字）。
//!
//! 定位与后端无关：v1 走 `zwp_input_panel_surface_v1.set_toplevel(center_bottom)`，
//! v2（KWin 6 / COSMIC / GNOME）没有 input panel，改走 `zwlr_layer_shell_v1`
//! 的 overlay 层 + bottom anchor —— 两边最终都是「屏幕底部居中、固定尺寸、
//! 不占工作区」的浮层。
//!
//! 版式对齐商业输入法的听写面板：顶部一行状态（左状态 / 右热键提示）、
//! 中间一行识别文本（主视觉）、底部一条居中频谱。绘制分三层（见
//! [`crate::IcedSurface::draw_voice_overlay`]）：半透明圆角背景（SDF + fill
//! alpha 进预乘通道）→ 频谱条（预乘实心矩形逐像素 over）→ iced 文本内容
//! （premultiplied-over 合成）。
//!
//! 电平数据由 daemon 的 speech 模块维护：每 ~120ms 一条 RMS，
//! [`BAR_COUNT`] 条 ≈ 2.9s 历史（与 `speech::LEVEL_BARS` 必须一致）。

use std::time::Duration;

use iced_tiny_skia::core::Color;

/// 悬浮面板固定尺寸（宽高恒定，避免逐帧重绘时 surface 反复变尺寸）。
pub const VOICE_WIDTH: u32 = 560;
pub const VOICE_HEIGHT: u32 = 128;

/// 面板底边到屏幕底边的距离（逻辑像素）。
///
/// 商业输入法的语音面板都不贴底：贴底会和任务栏、Dock 抢位置。160 在
/// 1080p 上落在屏幕下三分之一，符合「中间偏下」的视觉习惯。
pub const VOICE_MARGIN_BOTTOM: i32 = 160;

/// 电平样本数（与 daemon `speech::LEVEL_BARS` 对齐；每 ~120ms 一条）。
pub const BAR_COUNT: usize = 24;

/// 实际绘制的条形数量：样本线性插值成两倍密度。
///
/// 24 根 10px 宽的粗条像柱状图；插值成 48 根 4px 细条才像语音波形。
/// 电平来源（daemon 的环形缓冲）不必跟着变——插值只发生在绘制层。
pub const BAR_DRAW_COUNT: usize = 48;

// ---- 版式几何（绘制与单测共用同一套常量）----

/// 卡片内边距。
pub const VOICE_PAD: f32 = 22.0;
/// 卡片圆角（比候选栏大：语音面板是独立大卡片，圆角太小像候选条）。
pub const VOICE_CORNER_RADIUS: f32 = 22.0;

/// 顶部状态行高度。
pub const STATUS_ROW_HEIGHT: f32 = 20.0;
/// 状态行与主文本的间距。
pub const TEXT_GAP: f32 = 10.0;
/// 主文本字号增量（相对 `theme.font_size`）。
pub const TEXT_SIZE_BONUS: f32 = 6.0;

/// 主文本区最大字符数（超出截断加 …）。
pub const TEXT_MAX_CHARS: usize = 24;

/// 状态行里异常文案的最大字符数（面板宽度固定，再长就画不下了）。
pub const STATUS_WARNING_CHARS: usize = 26;

/// 按字符截断（中文安全），超出补 …。
fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

/// 频谱区宽度（水平居中）。
pub const BAR_ZONE_WIDTH: f32 = 320.0;
/// 频谱区顶部 y（底部一条，给上方文字留出整块空间）。
pub const BAR_ZONE_TOP: f32 = 88.0;
/// 条形间距。
pub const BAR_GAP: f32 = 2.0;
/// 条形最大高度（满电平）。
pub const BAR_MAX_HEIGHT: f32 = 30.0;

/// 面板整体不透明度（「整体半透明」；与 theme.bg 自身的 0.96~0.98 相乘）。
pub const VOICE_BG_ALPHA: f32 = 0.82;
/// 频谱条不透明度（活元素，比背景更实）。
pub const BAR_ALPHA: f32 = 0.95;

/// 热键提示（与 daemon 按键路径的硬编码绑定「按住右 Ctrl」保持一致；
/// Ctrl+Alt+V 仍作为备选组合保留）。
pub const VOICE_HOTKEY_LABEL: &str = "右 Ctrl";

/// 面板阶段（决定状态行文案与频谱是否动）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VoicePhase {
    /// 听写中（默认：面板存在即意味着会话进行中）。
    #[default]
    Listening,
    /// 模型装载中（首次触发，频谱静止）。
    Loading,
    /// 松手后等最终结果。
    Transcribing,
    /// 一次性的提示（错误 / 没听清），下次会话覆盖。
    Notice,
}

/// 悬浮面板内容快照（daemon 组装，绘制只读）。
#[derive(Debug, Clone, Default)]
pub struct VoiceView {
    pub phase: VoicePhase,
    /// 电平 0..1，从左到右（最新在右）。
    pub levels: Vec<f32>,
    /// 当前识别文本（partial）。
    pub text: String,
    /// 本次听写已持续时长（Listening 才有意义；0 = 不显示）。
    pub elapsed: Duration,
    /// 本次听写已经上屏的字数。
    ///
    /// 放在状态行里是为了让「还在录」和「还在识别」能分开看：波形一直在动只
    /// 说明麦克风有输入，字数在涨才说明识别在推进。
    pub committed_chars: usize,
    /// 识别异常提示（解码持续失败时由 daemon 传入）。
    ///
    /// 有值时状态行改显示这个：解码报错如果只进日志，用户看到的就是「说半天
    /// 一个字不出」而无从判断问题出在录音还是识别。
    pub warning: Option<String>,
}

/// 频谱区起始 x（水平居中）。
pub fn bar_zone_x() -> f32 {
    (VOICE_WIDTH as f32 - BAR_ZONE_WIDTH) / 2.0
}

/// 第 i 根条形的 (x, w)。
pub fn voice_bar_rect(i: usize) -> (f32, f32) {
    let step = BAR_ZONE_WIDTH / BAR_DRAW_COUNT as f32;
    let w = (step - BAR_GAP).max(1.0);
    (bar_zone_x() + step * i as f32, w)
}

/// 把 `levels` 线性插值到 [`BAR_DRAW_COUNT`] 根，供绘制用。
///
/// 输入可以是空、1 条或任意长度：样本不足时按 0 补齐（左侧旧、右侧新，
/// 与 daemon 环形缓冲同向）。
pub fn resample_levels(levels: &[f32]) -> Vec<f32> {
    if levels.is_empty() {
        return vec![0.0; BAR_DRAW_COUNT];
    }
    if levels.len() == 1 {
        return vec![levels[0].clamp(0.0, 1.0); BAR_DRAW_COUNT];
    }
    let last = (levels.len() - 1) as f32;
    (0..BAR_DRAW_COUNT)
        .map(|i| {
            let t = i as f32 / (BAR_DRAW_COUNT - 1) as f32 * last;
            let lo = t.floor() as usize;
            let hi = (lo + 1).min(levels.len() - 1);
            let frac = t - lo as f32;
            let v = levels[lo] * (1.0 - frac) + levels[hi] * frac;
            v.clamp(0.0, 1.0)
        })
        .collect()
}

/// 录音呼吸点半径（Listening 阶段在状态行左侧脉动）。
pub const PULSE_RADIUS: f32 = 4.0;

/// 呼吸周期（秒）。1.8s 接近静息呼吸，太快会像报警。
pub const PULSE_PERIOD_SECS: f32 = 1.8;

/// 呼吸相位 0..=1（`elapsed` 秒处）。绘制层据此调呼吸点透明度。
pub fn pulse_alpha(elapsed: Duration) -> f32 {
    let t = elapsed.as_secs_f32() / PULSE_PERIOD_SECS;
    let phase = (t * std::f32::consts::TAU).sin() * 0.5 + 0.5;
    0.35 + 0.65 * phase
}

/// 电平 → 条形高度（全静音给 2px 底线，界面不死板）。
pub fn voice_bar_height(level: f32) -> f32 {
    const MIN_H: f32 = 2.0;
    MIN_H + level.clamp(0.0, 1.0) * (BAR_MAX_HEIGHT - MIN_H)
}

/// 条形顶部 y（在频谱区内垂直居中，镜像条形围绕中线）。
pub fn voice_bar_top(height: f32) -> f32 {
    BAR_ZONE_TOP + (BAR_MAX_HEIGHT - height) / 2.0
}

/// 频谱条颜色（主题高亮色，略降不透明度）。
pub fn bar_color(theme: &crate::PanelTheme) -> Color {
    Color {
        a: BAR_ALPHA,
        ..theme.primary
    }
}

/// `mm:ss` 时长文案（超过 1 小时只显示分钟，听写场景够用）。
pub fn format_elapsed(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

impl VoiceView {
    /// 顶部左侧状态文案。
    pub fn status_text(&self) -> String {
        // 异常优先：识别出错时最该看见的是原因，而不是「正在听写」。
        if let Some(warning) = self.warning.as_deref().filter(|w| !w.trim().is_empty()) {
            return format!("识别异常：{}", truncate_chars(warning, STATUS_WARNING_CHARS));
        }
        match self.phase {
            VoicePhase::Loading => "正在装载语音引擎…".to_string(),
            VoicePhase::Listening => {
                let base = if self.text.trim().is_empty() {
                    "正在听候…"
                } else {
                    "正在听写"
                };
                match self.committed_chars {
                    0 => base.to_string(),
                    n => format!("{base} · 已上屏 {n} 字"),
                }
            }
            VoicePhase::Transcribing => "正在识别…".to_string(),
            VoicePhase::Notice => self.text.clone(),
        }
    }

    /// 顶部右侧提示：热键 + 时长（时长 > 0 才显示）。
    pub fn hint_text(&self) -> String {
        match self.phase {
            VoicePhase::Loading => "首次使用需装载模型".to_string(),
            // 按住说话：热键是 PTT（松开即上屏），不是按一下开始按一下停。
            VoicePhase::Listening => {
                let hotkey = format!("松开 {} 上屏", VOICE_HOTKEY_LABEL);
                if self.elapsed.as_secs() > 0 {
                    format!("{}  ·  {}", format_elapsed(self.elapsed), hotkey)
                } else {
                    hotkey
                }
            }
            VoicePhase::Transcribing => "请稍候".to_string(),
            VoicePhase::Notice => format!("按 {} 重试", VOICE_HOTKEY_LABEL),
        }
    }

    /// 主文本行（空文本给占位提示；超长截断）。
    pub fn display_text(&self) -> String {
        if self.phase == VoicePhase::Loading {
            // 装载阶段没有文本，主行说清「按住说话」这件事。
            return "准备好后按住说话…".to_string();
        }
        if self.text.trim().is_empty() {
            return "按住说话…".to_string();
        }
        let mut out: String = self.text.chars().take(TEXT_MAX_CHARS).collect();
        if self.text.chars().count() > TEXT_MAX_CHARS {
            out.push('…');
        }
        out
    }

    /// 频谱是否该动（装载/识别阶段电平无意义，保持静止）。
    pub fn bars_active(&self) -> bool {
        matches!(self.phase, VoicePhase::Listening)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_geometry_tiles_zone_without_overlap() {
        for i in 0..BAR_COUNT {
            let (x, w) = voice_bar_rect(i);
            assert!(x >= bar_zone_x());
            assert!(w >= 1.0, "条形宽度不足 1px（BAR_GAP 过大？）");
            let (next_x, _) = voice_bar_rect(i + 1);
            assert!(x + w <= next_x + f32::EPSILON, "条形 {i} 与下一条重叠");
        }
        let (last_x, last_w) = voice_bar_rect(BAR_COUNT - 1);
        assert!(
            last_x + last_w <= bar_zone_x() + BAR_ZONE_WIDTH + f32::EPSILON,
            "最后一根条形超出频谱区"
        );
    }

    #[test]
    fn bar_zone_is_horizontally_centered() {
        // 面板必须居中：左右留白相等（合成器只负责把 surface 摆中间，
        // 面板内的频谱自己也得居中，否则视觉上偏一边）。
        let left = bar_zone_x();
        let right = VOICE_WIDTH as f32 - (bar_zone_x() + BAR_ZONE_WIDTH);
        assert!((left - right).abs() < 1e-6, "频谱区未居中");
        assert!(left >= VOICE_PAD, "频谱区贴边了");
    }

    #[test]
    fn bars_and_text_do_not_overlap() {
        // 频谱顶（含满电平）必须低于主文本底：文字画在像素层频谱之上时
        // 若重叠会糊成一团。
        let text_bottom = VOICE_PAD + STATUS_ROW_HEIGHT + TEXT_GAP + (14.0 + TEXT_SIZE_BONUS) * 1.4;
        let bars_top = voice_bar_top(voice_bar_height(1.0));
        assert!(bars_top > text_bottom, "频谱与主文本重叠");
        assert!(
            voice_bar_top(voice_bar_height(1.0)) + BAR_MAX_HEIGHT <= VOICE_HEIGHT as f32,
            "频谱超出面板"
        );
    }

    #[test]
    fn bar_height_maps_level_and_bars_stay_centered() {
        assert!((voice_bar_height(0.0) - 2.0).abs() < 1e-6);
        assert!((voice_bar_height(1.0) - BAR_MAX_HEIGHT).abs() < 1e-6);
        // 超界钳制
        assert!((voice_bar_height(2.0) - BAR_MAX_HEIGHT).abs() < 1e-6);
        assert!((voice_bar_height(-1.0) - 2.0).abs() < 1e-6);
        // 镜像条形围绕频谱区中线
        let h = voice_bar_height(0.6);
        let mid = BAR_ZONE_TOP + BAR_MAX_HEIGHT / 2.0;
        assert!((voice_bar_top(h) + h / 2.0 - mid).abs() < 1e-6);
    }

    #[test]
    fn view_texts_listening_loading_and_placeholder() {
        let mut view = VoiceView::default();
        // 热键是按住说话（PTT），占位与提示都要说清「按住 / 松开」，
        // 否则用户会按一下就开始等自动上屏。
        assert_eq!(view.display_text(), "按住说话…");
        assert!(view.hint_text().contains("松开"));
        assert!(view.hint_text().contains(VOICE_HOTKEY_LABEL));
        assert!(view.bars_active());

        view.phase = VoicePhase::Loading;
        assert!(view.status_text().contains("装载"));
        assert!(!view.bars_active(), "装载阶段频谱不该动");

        view.phase = VoicePhase::Listening;
        view.text = "你好世界".to_string();
        assert_eq!(view.display_text(), "你好世界");
        assert_eq!(view.status_text(), "正在听写");
        view.text = "字".repeat(TEXT_MAX_CHARS + 5);
        let shown = view.display_text();
        assert!(shown.ends_with('…'));
        assert_eq!(shown.chars().count(), TEXT_MAX_CHARS + 1);
    }

    #[test]
    fn hint_shows_elapsed_only_while_listening() {
        let mut view = VoiceView::default();
        assert!(!view.hint_text().contains(':'), "0 秒不显示时长");
        view.elapsed = Duration::from_secs(83);
        assert!(view.hint_text().contains("01:23"));
        view.phase = VoicePhase::Transcribing;
        assert!(!view.hint_text().contains("01:23"));
    }

    #[test]
    fn status_text_reports_progress_and_warning() {
        let mut view = VoiceView {
            phase: VoicePhase::Listening,
            text: "你好".to_string(),
            ..Default::default()
        };
        assert_eq!(view.status_text(), "正在听写");
        view.committed_chars = 12;
        assert_eq!(view.status_text(), "正在听写 · 已上屏 12 字");

        view.warning = Some("识别失败：显存不足".to_string());
        assert!(view.status_text().starts_with("识别异常：识别失败"));

        // 长警告按字符截断，不会撑破固定宽度的状态行。
        view.warning = Some("错".repeat(80));
        assert!(view.status_text().chars().count() <= STATUS_WARNING_CHARS + 6);

        // 纯空白的 warning 等于没有。
        view.warning = Some("   ".to_string());
        assert_eq!(view.status_text(), "正在听写 · 已上屏 12 字");
    }

    #[test]
    fn notice_phase_replaces_text() {
        let view = VoiceView {
            phase: VoicePhase::Notice,
            text: "没有听清，再说一次".to_string(),
            ..Default::default()
        };
        assert_eq!(view.status_text(), "没有听清，再说一次");
        assert!(view.hint_text().contains(VOICE_HOTKEY_LABEL));
    }
}
