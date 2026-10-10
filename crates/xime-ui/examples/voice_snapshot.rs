//! 语音悬浮层渲染快照（目检用）：亮/暗两版输出 PNG 到 target/。
//! 运行：cargo run -p xime-ui --example voice_snapshot

use xime_ui::voice::{self, VoiceView};
use xime_ui::{IcedSurface, PanelTheme};

fn main() {
    for (name, dark) in [("voice_overlay_light", false), ("voice_overlay_dark", true)] {
        let theme = PanelTheme::for_mode(dark, 14.0, 8.0, (0x8F, 0x73, 0xE2));
        let mut surface = IcedSurface::new();
        let mut pixmap = tiny_skia11::Pixmap::new(voice::VOICE_WIDTH, voice::VOICE_HEIGHT).unwrap();

        // 合成一段"说话中"的电平序列（正弦包络，模拟频谱滚动）
        let levels: Vec<f32> = (0..voice::BAR_COUNT)
            .map(|i| {
                let t = i as f32 / voice::BAR_COUNT as f32;
                let env = (t * std::f32::consts::PI).sin();
                (env * (0.35 + 0.5 * (t * 9.0).sin().abs())).clamp(0.0, 1.0)
            })
            .collect();
        let view = VoiceView {
            loading: false,
            levels,
            text: "今天下午三点开会讨论输入法".to_string(),
        };
        surface.draw_voice_overlay(pixmap.data_mut(), &view, &theme);
        let path = format!("target/{name}.png");
        pixmap.save_png(&path).unwrap();
        println!("saved {path}");

        // 装载中状态（空电平）
        let mut pixmap = tiny_skia11::Pixmap::new(voice::VOICE_WIDTH, voice::VOICE_HEIGHT).unwrap();
        let loading = VoiceView {
            loading: true,
            levels: vec![0.0; voice::BAR_COUNT],
            text: String::new(),
        };
        surface.draw_voice_overlay(pixmap.data_mut(), &loading, &theme);
        let path = format!("target/{name}_loading.png");
        pixmap.save_png(&path).unwrap();
        println!("saved {path}");
    }
}
