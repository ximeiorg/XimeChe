use zbus::{interface, object_server::SignalEmitter};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::Sender;
use tiny_skia::{Pixmap, Paint, PathBuilder, FillRule, Transform, Shader, Color};
use cosmic_text::{FontSystem, SwashCache, Buffer, Metrics, Attrs, Color as CosmicColor, Shaping, Family};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Chinese,
    English,
}

fn build_rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> tiny_skia::Path {
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish().unwrap()
}

fn blend_glyph(pixmap_data: &mut [u8], x: i32, y: i32, w: i32, h: i32, color: CosmicColor, width: usize, height: usize) {
    for gy in 0..h as usize {
        for gx in 0..w as usize {
            let px = x as usize + gx;
            let py = y as usize + gy;
            if px >= width || py >= height {
                continue;
            }
            let offset = py * width * 4 + px * 4;
            if offset + 3 >= pixmap_data.len() {
                continue;
            }
            let alpha = color.a() as f32 / 255.0;
            if alpha > 0.01 {
                // tiny-skia uses RGBA format
                let bg_r = pixmap_data[offset] as f32;
                let bg_g = pixmap_data[offset + 1] as f32;
                let bg_b = pixmap_data[offset + 2] as f32;

                let fg_r = color.r() as f32;
                let fg_g = color.g() as f32;
                let fg_b = color.b() as f32;

                let blend_r = (bg_r * (1.0 - alpha) + fg_r * alpha).min(255.0) as u8;
                let blend_g = (bg_g * (1.0 - alpha) + fg_g * alpha).min(255.0) as u8;
                let blend_b = (bg_b * (1.0 - alpha) + fg_b * alpha).min(255.0) as u8;

                pixmap_data[offset] = blend_r;
                pixmap_data[offset + 1] = blend_g;
                pixmap_data[offset + 2] = blend_b;
                pixmap_data[offset + 3] = 0xFF;
            }
        }
    }
}

fn render_text_icon(text: &str, bg_color: Color, size: i32) -> Vec<u8> {
    let width = size as u32;
    let height = size as u32;
    
    let mut pixmap = Pixmap::new(width, height).unwrap();
    
    let radius = 3.0;
    let rect = build_rounded_rect(0.0, 0.0, width as f32, height as f32, radius);
    
    let paint = Paint {
        shader: Shader::SolidColor(bg_color),
        ..Default::default()
    };
    pixmap.fill_path(&rect, &paint, FillRule::Winding, Transform::identity(), None);
    
    let mut font_system = FontSystem::new();
    let mut swash_cache = SwashCache::new();
    
    let font_size = size as f32 * 0.6;
    let metrics = Metrics::new(font_size, font_size * 1.2);
    let attrs = Attrs::new().family(Family::SansSerif);
    
    let text_color = CosmicColor::rgba(255, 255, 255, 255);
    
    let mut buffer = Buffer::new(&mut font_system, metrics);
    buffer.set_text(&mut font_system, text, attrs, Shaping::Advanced);
    
    let mut text_width = 0.0f32;
    for run in buffer.layout_runs() {
        for glyph in run.glyphs {
            text_width = text_width.max(glyph.x + glyph.w);
        }
    }
    
    let x_offset = ((width as f32 - text_width) / 2.0) as i32;
    let y_offset = ((height as f32 - font_size) / 2.0) as i32;
    
    let pixmap_width = pixmap.width() as usize;
    let pixmap_height = pixmap.height() as usize;
    
    buffer.draw(
        &mut font_system,
        &mut swash_cache,
        text_color,
        |x, y, w, h, color| {
            blend_glyph(pixmap.data_mut(), x as i32 + x_offset, y as i32 + y_offset, w as i32, h as i32, color, pixmap_width, pixmap_height);
        },
    );
    
    let data = pixmap.data();
    // Convert RGBA to ARGB (StatusNotifierItem expects ARGB32)
    let mut argb_data = Vec::with_capacity(data.len());
    for i in (0..data.len()).step_by(4) {
        argb_data.push(data[i + 3]); // A
        argb_data.push(data[i]);     // R (tiny-skia RGBA: index 0 is R)
        argb_data.push(data[i + 1]); // G
        argb_data.push(data[i + 2]); // B
    }
    
    argb_data
}

pub struct StatusNotifierItem {
    mode: Arc<Mutex<InputMode>>,
    visible: Arc<Mutex<bool>>,
    primary_color: Arc<Mutex<(u8, u8, u8)>>,
    toggle_tx: Option<Sender<()>>,
}

impl StatusNotifierItem {
    pub fn new() -> Self {
        Self {
            mode: Arc::new(Mutex::new(InputMode::Chinese)),
            visible: Arc::new(Mutex::new(false)),
            primary_color: Arc::new(Mutex::new((0x8F, 0x73, 0xE2))),
            toggle_tx: None,
        }
    }
    
    pub fn with_toggle_channel(toggle_tx: Sender<()>) -> Self {
        Self {
            mode: Arc::new(Mutex::new(InputMode::Chinese)),
            visible: Arc::new(Mutex::new(false)),
            primary_color: Arc::new(Mutex::new((0x8F, 0x73, 0xE2))),
            toggle_tx: Some(toggle_tx),
        }
    }
    
    pub fn set_mode(&self, mode: InputMode) {
        if let Ok(mut m) = self.mode.lock() {
            *m = mode;
        }
    }
    
    pub fn get_mode(&self) -> InputMode {
        self.mode.lock().map(|m| *m).unwrap_or(InputMode::Chinese)
    }
    
    pub fn set_visible(&self, visible: bool) {
        if let Ok(mut v) = self.visible.lock() {
            *v = visible;
        }
    }
    
    pub fn is_visible(&self) -> bool {
        self.visible.lock().map(|v| *v).unwrap_or(false)
    }
    
    pub fn set_primary_color(&self, color: (u8, u8, u8)) {
        if let Ok(mut c) = self.primary_color.lock() {
            *c = color;
        }
    }
    
    pub fn get_primary_color(&self) -> (u8, u8, u8) {
        self.primary_color.lock().map(|c| *c).unwrap_or((0x8F, 0x73, 0xE2))
    }
}

#[interface(name = "org.kde.StatusNotifierItem")]
impl StatusNotifierItem {
    #[zbus(signal)]
    async fn new_icon(signal_emitter: &SignalEmitter<'_>) -> zbus::Result<()> {}
    
    #[zbus(signal)]
    async fn new_tool_tip(signal_emitter: &SignalEmitter<'_>) -> zbus::Result<()> {}
    
    #[zbus(signal)]
    async fn new_status(signal_emitter: &SignalEmitter<'_>, status: &str) -> zbus::Result<()> {}
    
    fn scroll(&self, _delta: i32, _orientation: &str) {}
    
    async fn activate(&self, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>, _x: i32, _y: i32) {
        if let Some(tx) = &self.toggle_tx {
            let _ = tx.send(()).await;
        }
        // Don't toggle mode here - let daemon handle it based on Rime state
        eprintln!("DEBUG: Tray icon clicked, sending toggle request to daemon");
    }
    
    fn secondary_activate(&self, _x: i32, _y: i32) {}
    
    #[zbus(property)]
    fn category(&self) -> &str {
        "SystemServices"
    }
    
    #[zbus(property)]
    fn id(&self) -> &str {
        "xime"
    }
    
    #[zbus(property)]
    fn title(&self) -> &str {
        "XIME Input Method"
    }
    
    #[zbus(property)]
    fn status(&self) -> &str {
        if self.is_visible() {
            "Active"
        } else {
            "Passive"
        }
    }
    
    #[zbus(property)]
    fn window_id(&self) -> i32 {
        0
    }
    
    #[zbus(property)]
    fn icon_name(&self) -> String {
        String::new()
    }
    
    #[zbus(property)]
    fn icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        let mode = self.get_mode();
        let primary_color = self.get_primary_color();
        let (text, bg_color) = match mode {
            InputMode::Chinese => ("ZH", Color::from_rgba8(primary_color.0, primary_color.1, primary_color.2, 255)),
            InputMode::English => ("EN", Color::from_rgba8(0x60, 0x60, 0x60, 255)),
        };
        
        let sizes = [16, 22, 32];
        sizes.iter().map(|size| {
            let data = render_text_icon(text, bg_color, *size);
            (*size, *size, data)
        }).collect()
    }
    
    #[zbus(property)]
    fn overlay_icon_name(&self) -> &str {
        ""
    }
    
    #[zbus(property)]
    fn overlay_icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        Vec::new()
    }
    
    #[zbus(property)]
    fn attention_icon_name(&self) -> &str {
        ""
    }
    
    #[zbus(property)]
    fn attention_icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        Vec::new()
    }
    
    #[zbus(property)]
    fn attention_movie_name(&self) -> &str {
        ""
    }
    
    #[zbus(property)]
    fn tooltip(&self) -> (String, Vec<(i32, i32, Vec<u8>)>, String, String) {
        let title = match self.get_mode() {
            InputMode::Chinese => "中文输入",
            InputMode::English => "英文输入",
        };
        (String::new(), Vec::new(), title.to_string(), String::new())
    }
    
    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }
    
    #[zbus(property)]
    fn menu(&self) -> zbus::zvariant::ObjectPath<'static> {
        zbus::zvariant::ObjectPath::try_from("/MenuBar").unwrap()
    }
    
    #[zbus(property)]
    fn icon_theme_path(&self) -> &str {
        ""
    }
}

impl StatusNotifierItem {
    fn toggle_mode(&self) {
        let new_mode = match self.get_mode() {
            InputMode::Chinese => InputMode::English,
            InputMode::English => InputMode::Chinese,
        };
        self.set_mode(new_mode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_primary_color() {
        let sni = StatusNotifierItem::new();
        let color = sni.get_primary_color();
        assert_eq!(color, (0x8F, 0x73, 0xE2));
    }

    #[test]
    fn test_set_primary_color() {
        let sni = StatusNotifierItem::new();
        
        sni.set_primary_color((0x1A, 0x73, 0xE8));
        let color = sni.get_primary_color();
        assert_eq!(color, (26, 115, 232));
        
        sni.set_primary_color((0xC6, 0x28, 0x28));
        let color = sni.get_primary_color();
        assert_eq!(color, (198, 40, 40));
    }

    #[test]
    fn test_primary_color_coral_red() {
        let sni = StatusNotifierItem::new();
        sni.set_primary_color((0xC6, 0x28, 0x28));
        assert_eq!(sni.get_primary_color(), (198, 40, 40));
    }

    #[test]
    fn test_primary_color_ocean_blue() {
        let sni = StatusNotifierItem::new();
        sni.set_primary_color((0x1A, 0x73, 0xE8));
        assert_eq!(sni.get_primary_color(), (26, 115, 232));
    }

    #[test]
    fn test_primary_color_slate_gray() {
        let sni = StatusNotifierItem::new();
        sni.set_primary_color((0x42, 0x42, 0x42));
        assert_eq!(sni.get_primary_color(), (66, 66, 66));
    }

    #[test]
    fn test_input_mode_default() {
        let sni = StatusNotifierItem::new();
        assert_eq!(sni.get_mode(), InputMode::Chinese);
    }

    #[test]
    fn test_input_mode_toggle() {
        let sni = StatusNotifierItem::new();
        assert_eq!(sni.get_mode(), InputMode::Chinese);
        
        sni.set_mode(InputMode::English);
        assert_eq!(sni.get_mode(), InputMode::English);
        
        sni.set_mode(InputMode::Chinese);
        assert_eq!(sni.get_mode(), InputMode::Chinese);
    }

    #[test]
    fn test_visibility() {
        let sni = StatusNotifierItem::new();
        assert!(!sni.is_visible());
        
        sni.set_visible(true);
        assert!(sni.is_visible());
        
        sni.set_visible(false);
        assert!(!sni.is_visible());
    }
}