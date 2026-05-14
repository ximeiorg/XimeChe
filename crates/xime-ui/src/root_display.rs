use cosmic_text::{FontSystem, SwashCache, Buffer, Metrics, Attrs, Color, Shaping, Family};
use tiny_skia::{Pixmap, Paint, PathBuilder, FillRule};

const CORNER_RADIUS: f32 = 8.0;

pub struct RootRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
}

impl RootRenderer {
    pub fn new() -> Self {
        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        Self { font_system, swash_cache }
    }

    /// Calculate width for single key root display
    /// Format: "a: 工匚戈艹廿龷七弋戈"
    pub fn calculate_width(key: char, root: &str) -> u32 {
        let text = format!("{}: {}", key, root);
        let metrics = Metrics::new(16.0, 20.0);
        let attrs = Attrs::new().family(Family::SansSerif);
        let mut font_system = FontSystem::new();
        let mut buffer = Buffer::new(&mut font_system, metrics);
        buffer.set_text(&text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);

        let text_width = buffer.layout_runs().fold(0.0f32, |max_w, run| max_w.max(run.line_w));
        let total_width = 20.0 + text_width + 20.0;  // left padding + text + right padding
        (total_width.ceil() as u32).max(60)
    }

    /// Draw single key root to buffer
    /// Shows "a: 工匚戈艹廿龷七弋戈" in a small popup
    pub fn draw_root(pixels: &mut [u8], width: u32, height: u32, key: char, root: &str) {
        let mut renderer = RootRenderer::new();
        renderer.draw_root_internal(pixels, width, height, key, root);
    }

    fn draw_root_internal(&mut self, pixels: &mut [u8], width: u32, height: u32, key: char, root: &str) {
        let mut pixmap = Pixmap::new(width, height).expect("Failed to create pixmap");

        self.draw_background(&mut pixmap, width, height);
        self.draw_text(&mut pixmap, width, height, key, root);

        // Convert RGBA to BGRA (Wayland ARGB8888 format)
        let data = pixmap.data();
        for i in (0..data.len()).step_by(4) {
            pixels[i] = data[i + 2];
            pixels[i + 1] = data[i + 1];
            pixels[i + 2] = data[i];
            pixels[i + 3] = data[i + 3];
        }
    }

    fn draw_background(&self, pixmap: &mut Pixmap, width: u32, height: u32) {
        let rounded_rect = build_rounded_rect(0.0, 0.0, width as f32, height as f32, CORNER_RADIUS);

        // Fill background
        let mut paint = Paint::default();
        paint.set_color_rgba8(0xFA, 0xFA, 0xFA, 0xFF);
        paint.anti_alias = true;

        pixmap.fill_path(
            &rounded_rect,
            &paint,
            FillRule::Winding,
            tiny_skia::Transform::identity(),
            None,
        );

        // Draw border
        let mut border_paint = Paint::default();
        border_paint.set_color_rgba8(0x8F, 0x73, 0xE2, 0xFF);
        border_paint.anti_alias = true;

        let stroke = tiny_skia::Stroke { width: 2.0, ..Default::default() };
        pixmap.stroke_path(
            &rounded_rect,
            &border_paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    fn draw_text(&mut self, pixmap: &mut Pixmap, width: u32, height: u32, key: char, root: &str) {
        let metrics = Metrics::new(16.0, 20.0);
        let attrs = Attrs::new().family(Family::SansSerif);
        
        let text = format!("{}: {}", key, root);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_text(&text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);

        let text_width = buffer.layout_runs().fold(0.0f32, |max_w, run| max_w.max(run.line_w));
        
        // Center the text horizontally
        let x_offset = ((width as f32 - text_width) / 2.0).max(10.0) as i32;
        let y_offset = ((height as f32 - 20.0) / 2.0).max(0.0) as i32;

        let pixmap_width = pixmap.width() as usize;
        let pixmap_height = pixmap.height() as usize;
        let pixmap_data = pixmap.data_mut();

        let color = Color::rgba(0x33, 0x33, 0x33, 0xFF);

        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            color,
            |x, y, w, h, color| {
                blend_glyph(pixmap_data, x as i32 + x_offset, y + y_offset, w as i32, h as i32, color, pixmap_width, pixmap_height);
            },
        );
    }
}

impl Default for RootRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// Draw root display to buffer
pub fn draw_root_to_buffer(pixels: &mut [u8], width: u32, height: u32, key: char, root: &str) {
    RootRenderer::draw_root(pixels, width, height, key, root);
}

/// Calculate width for root display
pub fn calculate_root_width(key: char, root: &str) -> u32 {
    RootRenderer::calculate_width(key, root)
}

fn blend_glyph(pixmap_data: &mut [u8], x: i32, y: i32, w: i32, h: i32, color: Color, width: usize, height: usize) {
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