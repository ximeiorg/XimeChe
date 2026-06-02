use cosmic_text::{fontdb, Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap};

const CORNER_RADIUS: f32 = 8.0;
const KEY_BG_CORNER_RADIUS: f32 = 4.0;

// Embed ChaiPUA font for root display
const CHAI_FONT: &[u8] = include_bytes!("../resources/fonts/ChaiPUA-0.2.7-snow.ttf");

pub struct RootRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
}

impl RootRenderer {
    pub fn new() -> Self {
        // Create font system with embedded ChaiPUA font
        let font_source = fontdb::Source::Binary(std::sync::Arc::new(CHAI_FONT));
        let font_system = FontSystem::new_with_fonts([font_source]);
        let swash_cache = SwashCache::new();
        Self {
            font_system,
            swash_cache,
        }
    }

    /// Calculate width for single key root display
    /// Format: [key] root_text
    pub fn calculate_width(key: char, root: &str, _primary_color: (u8, u8, u8)) -> u32 {
        let key_text = key.to_string();
        let root_text = root;

        let metrics = Metrics::new(16.0, 20.0);
        let attrs = Attrs::new().family(Family::Name("ChaiPUA-0.2.7"));

        let font_source = fontdb::Source::Binary(std::sync::Arc::new(CHAI_FONT));
        let mut font_system = FontSystem::new_with_fonts([font_source]);

        let mut key_buffer = Buffer::new(&mut font_system, metrics);
        key_buffer.set_text(&key_text, &attrs, Shaping::Advanced, None);
        key_buffer.shape_until_scroll(&mut font_system, false);
        let key_width = key_buffer
            .layout_runs()
            .fold(0.0f32, |max_w, run| max_w.max(run.line_w));

        let mut root_buffer = Buffer::new(&mut font_system, metrics);
        root_buffer.set_text(root_text, &attrs, Shaping::Advanced, None);
        root_buffer.shape_until_scroll(&mut font_system, false);
        let root_width = root_buffer
            .layout_runs()
            .fold(0.0f32, |max_w, run| max_w.max(run.line_w));

        let key_bg_width = key_width + 16.0;
        let total_width = 12.0 + key_bg_width + 8.0 + root_width + 12.0;
        (total_width.ceil() as u32).max(80)
    }

    /// Draw single key root to buffer
    /// Shows [key] with primary colored background/border, followed by root text
    pub fn draw_root(
        pixels: &mut [u8],
        width: u32,
        height: u32,
        key: char,
        root: &str,
        primary_color: (u8, u8, u8),
    ) {
        let mut renderer = RootRenderer::new();
        renderer.draw_root_internal(pixels, width, height, key, root, primary_color);
    }

    fn draw_root_internal(
        &mut self,
        pixels: &mut [u8],
        width: u32,
        height: u32,
        key: char,
        root: &str,
        primary_color: (u8, u8, u8),
    ) {
        let mut pixmap = Pixmap::new(width, height).expect("Failed to create pixmap");

        self.draw_background(&mut pixmap, width, height);
        self.draw_text(&mut pixmap, width, height, key, root, primary_color);

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
        border_paint.set_color_rgba8(0xE0, 0xE0, 0xE0, 0xFF);
        border_paint.anti_alias = true;

        let stroke = tiny_skia::Stroke {
            width: 2.0,
            ..Default::default()
        };
        pixmap.stroke_path(
            &rounded_rect,
            &border_paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    fn draw_text(
        &mut self,
        pixmap: &mut Pixmap,
        _width: u32,
        height: u32,
        key: char,
        root: &str,
        primary_color: (u8, u8, u8),
    ) {
        let (r, g, b) = primary_color;
        let metrics = Metrics::new(16.0, 20.0);
        let attrs = Attrs::new().family(Family::Name("ChaiPUA-0.2.7"));

        let key_text = key.to_string();
        let mut key_buffer = Buffer::new(&mut self.font_system, metrics);
        key_buffer.set_text(&key_text, &attrs, Shaping::Advanced, None);
        key_buffer.shape_until_scroll(&mut self.font_system, false);
        let key_text_width = key_buffer
            .layout_runs()
            .fold(0.0f32, |max_w, run| max_w.max(run.line_w));

        let mut root_buffer = Buffer::new(&mut self.font_system, metrics);
        root_buffer.set_text(root, &attrs, Shaping::Advanced, None);
        root_buffer.shape_until_scroll(&mut self.font_system, false);
        let _root_text_width = root_buffer
            .layout_runs()
            .fold(0.0f32, |max_w, run| max_w.max(run.line_w));

        let key_bg_width = key_text_width + 16.0;
        let key_bg_height = 24.0;

        let x_start = 12.0;
        let key_bg_x = x_start;
        let key_bg_y = (height as f32 - key_bg_height) / 2.0;

        let key_bg_rect = build_rounded_rect(
            key_bg_x,
            key_bg_y,
            key_bg_width,
            key_bg_height,
            KEY_BG_CORNER_RADIUS,
        );

        let mut bg_paint = Paint::default();
        bg_paint.set_color_rgba8(r, g, b, 0x30);
        bg_paint.anti_alias = true;
        pixmap.fill_path(
            &key_bg_rect,
            &bg_paint,
            FillRule::Winding,
            tiny_skia::Transform::identity(),
            None,
        );

        let mut border_paint = Paint::default();
        border_paint.set_color_rgba8(r, g, b, 0xFF);
        border_paint.anti_alias = true;
        let stroke = tiny_skia::Stroke {
            width: 1.5,
            ..Default::default()
        };
        pixmap.stroke_path(
            &key_bg_rect,
            &border_paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );

        let pixmap_width = pixmap.width() as usize;
        let pixmap_height = pixmap.height() as usize;
        let pixmap_data = pixmap.data_mut();

        let key_x_offset = (key_bg_x + (key_bg_width - key_text_width) / 2.0) as i32;
        let key_y_offset = (key_bg_y + (key_bg_height - 20.0) / 2.0) as i32;

        let key_color = Color::rgba(r, g, b, 0xFF);
        key_buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            key_color,
            |x, y, w, h, color| {
                blend_glyph(
                    pixmap_data,
                    x + key_x_offset,
                    y + key_y_offset,
                    w as i32,
                    h as i32,
                    color,
                    pixmap_width,
                    pixmap_height,
                );
            },
        );

        let root_x_start = x_start + key_bg_width + 8.0;
        let root_y_offset = ((height as f32 - 20.0) / 2.0).max(0.0) as i32;

        let root_color = Color::rgba(0x33, 0x33, 0x33, 0xFF);
        root_buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            root_color,
            |x, y, w, h, color| {
                blend_glyph(
                    pixmap_data,
                    x + root_x_start as i32,
                    y + root_y_offset,
                    w as i32,
                    h as i32,
                    color,
                    pixmap_width,
                    pixmap_height,
                );
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
pub fn draw_root_to_buffer(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    key: char,
    root: &str,
    primary_color: (u8, u8, u8),
) {
    RootRenderer::draw_root(pixels, width, height, key, root, primary_color);
}

/// Calculate width for root display
pub fn calculate_root_width(key: char, root: &str, primary_color: (u8, u8, u8)) -> u32 {
    RootRenderer::calculate_width(key, root, primary_color)
}

#[allow(clippy::too_many_arguments)]
fn blend_glyph(
    pixmap_data: &mut [u8],
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: Color,
    width: usize,
    height: usize,
) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_root_width_basic() {
        let width = RootRenderer::calculate_width('a', "工匚戈艹廿龷七弋戈", (0x8F, 0x73, 0xE2));
        assert!(width >= 80, "Width should be at least 80, got {}", width);
    }

    #[test]
    fn test_calculate_root_width_minimum() {
        let width = RootRenderer::calculate_width('a', "", (0x8F, 0x73, 0xE2));
        assert!(
            width >= 80,
            "Width should be at least 80 for empty root, got {}",
            width
        );
    }

    #[test]
    fn test_calculate_root_width_different_colors() {
        let width1 = RootRenderer::calculate_width('g', "王龶五一戋", (0x8F, 0x73, 0xE2));
        let width2 = RootRenderer::calculate_width('g', "王龶五一戋", (0x1A, 0x73, 0xE8));
        assert_eq!(width1, width2, "Width should not depend on primary color");
    }

    #[test]
    fn test_calculate_root_width_single_char() {
        let width_single = RootRenderer::calculate_width('a', "工", (0x8F, 0x73, 0xE2));
        assert!(
            width_single >= 80,
            "Single char root width should be >= 80, got {}",
            width_single
        );
    }

    #[test]
    fn test_calculate_root_width_long_root() {
        let width = RootRenderer::calculate_width(
            'a',
            "工匚戈艹廿龷七弋戈艹廿龷七弋戈艹廿龷",
            (0x8F, 0x73, 0xE2),
        );
        let width_short = RootRenderer::calculate_width('a', "工", (0x8F, 0x73, 0xE2));
        assert!(
            width >= width_short,
            "Longer root should not be narrower, got width={}, width_short={}",
            width,
            width_short
        );
    }

    #[test]
    fn test_blend_glyph_partial_inside() {
        let mut pixels = vec![0u8; 10 * 10 * 4];
        // Partially inside (right edge)
        blend_glyph(&mut pixels, 8, 8, 5, 5, Color::rgba(255, 0, 0, 255), 10, 10);
        // Pixel at (8,8) should be modified (inside bounds)
        let offset = (8 * 10 * 4) + (8 * 4);
        assert!(pixels[offset] > 0 || pixels[offset + 1] > 0 || pixels[offset + 2] > 0);
    }

    #[test]
    fn test_blend_glyph_fully_transparent() {
        let mut pixels = vec![0u8; 10 * 10 * 4];
        let original = pixels.clone();
        // Fully transparent glyph
        blend_glyph(&mut pixels, 0, 0, 5, 5, Color::rgba(255, 0, 0, 0), 10, 10);
        assert_eq!(
            pixels, original,
            "Transparent glyph should not modify pixels"
        );
    }

    #[test]
    fn test_draw_root_to_buffer_creates_output() {
        let width = 200;
        let height = 36;
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        draw_root_to_buffer(
            &mut pixels,
            width,
            height,
            'a',
            "工匚戈艹廿龷七弋戈",
            (0x8F, 0x73, 0xE2),
        );
        // Buffer should have been modified
        assert!(
            pixels.iter().any(|&b| b != 0),
            "Buffer should have non-zero pixels"
        );
        assert_eq!(pixels.len(), width as usize * height as usize * 4);
    }

    #[test]
    fn test_draw_root_to_buffer_different_keys() {
        let width = 200;
        let height = 36;
        let mut pixels_a = vec![0u8; width as usize * height as usize * 4];
        let mut pixels_b = vec![0u8; width as usize * height as usize * 4];
        draw_root_to_buffer(
            &mut pixels_a,
            width,
            height,
            'a',
            "工匚戈",
            (0x8F, 0x73, 0xE2),
        );
        draw_root_to_buffer(
            &mut pixels_b,
            width,
            height,
            'b',
            "子孑孓",
            (0x8F, 0x73, 0xE2),
        );
        // Different keys with different roots should produce different buffers
        assert!(
            pixels_a != pixels_b,
            "Different roots should produce different buffers"
        );
    }

    #[test]
    fn test_draw_root_to_buffer_with_ocean_blue() {
        let width = 200;
        let height = 36;
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        draw_root_to_buffer(
            &mut pixels,
            width,
            height,
            'a',
            "工匚戈",
            (0x1A, 0x73, 0xE8),
        );
        assert!(pixels.iter().any(|&b| b != 0));
    }

    #[test]
    fn test_build_rounded_rect_valid() {
        let path = build_rounded_rect(0.0, 0.0, 100.0, 36.0, 8.0);
        assert!(!path.points().is_empty(), "Rounded rect should have points");
    }

    #[test]
    fn test_build_rounded_rect_zero_radius() {
        let path = build_rounded_rect(0.0, 0.0, 50.0, 20.0, 0.0);
        assert!(!path.points().is_empty());
    }

    #[test]
    fn test_build_rounded_rect_minimal() {
        let path = build_rounded_rect(0.0, 0.0, 10.0, 10.0, 2.0);
        assert!(!path.points().is_empty());
    }
}
