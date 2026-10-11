//! 语音输入悬浮层模型与几何（屏幕底部居中，半透明 + 频谱条）。
//!
//! 定位：KWin 经 `zwp_input_panel_surface_v1.set_toplevel(output,
//! center_bottom)` 把 surface 钉在屏幕底部居中（v1 输入面板协议唯一的
//! 位置枚举；keyboard surface 只在 text-input 激活时显示，与语音会话的
//! 存活条件天然一致）。绘制分三层（见 [`crate::IcedSurface::
//! draw_voice_overlay`]）：半透明圆角背景（SDF + fill alpha 进预乘通道）
//! → 频谱条（预乘实心矩形逐像素 over）→ iced 文本内容（premultiplied
//! -over 合成）。
//!
//! 电平数据由 daemon 的 speech 模块维护：每 ~64ms 一条 RMS，
//! [`BAR_COUNT`] 条 ≈ 1.5s 历史（与 `speech::LEVEL_BARS` 必须一致）。

use iced_tiny_skia::core::Color;

/// 悬浮层固定尺寸（宽高恒定，避免逐帧重绘时 surface 反复变尺寸）。
pub const VOICE_WIDTH: u32 = 440;
pub const VOICE_HEIGHT: u32 = 72;

/// 频谱条数（与 daemon `speech::LEVEL_BARS` 对齐）。
pub const BAR_COUNT: usize = 24;

/// 布局几何（悬浮层内坐标，绘制与单测共用同一套常量）：
/// `[16 pad][图标 32][→ 频谱区 60..228 ←][14][文本区 242..424][16 pad]`
pub const VOICE_PAD: f32 = 16.0;
pub const ICON_ZONE_WIDTH: f32 = 32.0;
pub const BAR_ZONE_X: f32 = 60.0;
pub const BAR_ZONE_WIDTH: f32 = 168.0;
pub const BAR_GAP: f32 = 3.0;
pub const BAR_MAX_HEIGHT: f32 = 40.0;
/// 频谱区与文本区的间距。
pub const TEXT_ZONE_GAP: f32 = 14.0;

/// 面板整体不透明度（"整体半透明"；与 theme.bg 自身的 0.96~0.98 相乘）。
pub const VOICE_BG_ALPHA: f32 = 0.78;
/// 频谱条不透明度（活元素，比背景更实）。
pub const BAR_ALPHA: f32 = 0.95;

/// 识别文本行最大字符数（超出截断加 …；频谱右侧文本区放不下长句）。
pub const TEXT_MAX_CHARS: usize = 24;

/// 快捷键提示（与 daemon 按键路径的硬编码绑定 Ctrl+Alt+V 保持一致）。
pub const VOICE_HOTKEY_LABEL: &str = "Ctrl+Alt+V";

/// 悬浮层内容快照（daemon 组装，绘制只读）。
#[derive(Debug, Clone, Default)]
pub struct VoiceView {
    /// true = 模型装载中（频谱区静止，状态行提示装载）。
    pub loading: bool,
    /// 电平 0..1，从左到右（最新在右）。
    pub levels: Vec<f32>,
    /// 当前识别文本（partial）。
    pub text: String,
}

/// 文本区起始 x（iced 行内 spacer 宽度由此推导，单一事实来源）。
pub fn text_zone_x() -> f32 {
    BAR_ZONE_X + BAR_ZONE_WIDTH + TEXT_ZONE_GAP
}

/// 第 i 根条形的 (x, w)。
pub fn voice_bar_rect(i: usize) -> (f32, f32) {
    let step = BAR_ZONE_WIDTH / BAR_COUNT as f32;
    let w = (step - BAR_GAP).max(1.0);
    (BAR_ZONE_X + step * i as f32, w)
}

/// 电平 → 条形高度（全静音给 2px 底线，界面不死板）。
pub fn voice_bar_height(level: f32) -> f32 {
    const MIN_H: f32 = 2.0;
    MIN_H + level.clamp(0.0, 1.0) * (BAR_MAX_HEIGHT - MIN_H)
}

/// 条形顶部 y（在悬浮层内垂直居中，镜像条形围绕中线）。
pub fn voice_bar_top(height: f32) -> f32 {
    (VOICE_HEIGHT as f32 - height) / 2.0
}

/// 频谱条颜色（主题高亮色，略降不透明度）。
pub fn bar_color(theme: &crate::PanelTheme) -> Color {
    Color {
        a: BAR_ALPHA,
        ..theme.primary
    }
}

impl VoiceView {
    /// 频谱区左侧图标。
    pub fn icon(&self) -> &'static str {
        "🎙️"
    }

    /// 状态行小字（文本区第二行）。
    pub fn status_text(&self) -> String {
        if self.loading {
            "正在装载语音引擎…".to_string()
        } else {
            format!("{} 结束", VOICE_HOTKEY_LABEL)
        }
    }

    /// 识别文本行（空文本给占位提示；超长截断）。
    pub fn display_text(&self) -> String {
        if self.text.trim().is_empty() {
            return "开始说话…".to_string();
        }
        let mut out: String = self.text.chars().take(TEXT_MAX_CHARS).collect();
        if self.text.chars().count() > TEXT_MAX_CHARS {
            out.push('…');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_geometry_tiles_zone_without_overlap() {
        for i in 0..BAR_COUNT {
            let (x, w) = voice_bar_rect(i);
            assert!(x >= BAR_ZONE_X);
            assert!(w >= 1.0, "条形宽度不足 1px（BAR_GAP 过大？）");
            let (next_x, _) = voice_bar_rect(i + 1);
            assert!(x + w <= next_x + f32::EPSILON, "条形 {i} 与下一条重叠");
        }
        let (last_x, last_w) = voice_bar_rect(BAR_COUNT - 1);
        assert!(
            last_x + last_w <= BAR_ZONE_X + BAR_ZONE_WIDTH + f32::EPSILON,
            "最后一根条形超出频谱区"
        );
    }

    #[test]
    fn bar_height_maps_level_and_bars_stay_centered() {
        assert!((voice_bar_height(0.0) - 2.0).abs() < 1e-6);
        assert!((voice_bar_height(1.0) - BAR_MAX_HEIGHT).abs() < 1e-6);
        // 超界钳制
        assert!((voice_bar_height(2.0) - BAR_MAX_HEIGHT).abs() < 1e-6);
        assert!((voice_bar_height(-1.0) - 2.0).abs() < 1e-6);
        // 镜像条形围绕垂直中线
        let h = voice_bar_height(0.6);
        assert!((voice_bar_top(h) + h / 2.0 - VOICE_HEIGHT as f32 / 2.0).abs() < 1e-6);
    }

    #[test]
    fn text_zone_starts_after_bars() {
        // iced 行布局（图标 + spacer + 文本）与像素层频谱区必须同源：
        // 文本区起点 = 频谱区终点 + 间距。
        assert!((text_zone_x() - (BAR_ZONE_X + BAR_ZONE_WIDTH + TEXT_ZONE_GAP)).abs() < 1e-6);
        assert!(text_zone_x() < VOICE_WIDTH as f32 - VOICE_PAD);
    }

    #[test]
    fn view_texts_loading_listening_and_placeholder() {
        let mut view = VoiceView::default();
        assert_eq!(view.display_text(), "开始说话…");
        assert!(view.status_text().contains(VOICE_HOTKEY_LABEL));
        view.loading = true;
        assert!(view.status_text().contains("装载"));
        view.text = "你好世界".to_string();
        assert_eq!(view.display_text(), "你好世界");
        view.text = "字".repeat(TEXT_MAX_CHARS + 5);
        let shown = view.display_text();
        assert!(shown.ends_with('…'));
        assert_eq!(shown.chars().count(), TEXT_MAX_CHARS + 1);
    }
}
