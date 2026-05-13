use cosmic_text::{FontSystem, SwashCache, Buffer, Metrics, Attrs, Color, Shaping, Family};
use tiny_skia::{Pixmap, Paint, PathBuilder, FillRule};

const SHADOW_OFFSET_X: f32 = 2.0;
const SHADOW_OFFSET_Y: f32 = 4.0;

pub struct CandidateRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
}

impl CandidateRenderer {
    pub fn new() -> Self {
        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        Self { font_system, swash_cache }
    }

    pub fn calculate_width(&mut self, candidates: &[String]) -> u32 {
        if candidates.is_empty() {
            return 100;
        }

        let mut total_width: f32 = 15.0;
        for (idx, candidate) in candidates.iter().enumerate() {
            let text = format!("{}. {}", idx + 1, candidate);
            total_width += self.measure_text_width(&text) + 20.0;
        }
        total_width += 15.0;

        (total_width.ceil() as u32).max(100)
    }

    pub fn draw_candidates(&mut self, pixels: &mut [u8], width: u32, height: u32, candidates: &[String]) {
        if candidates.is_empty() {
            return;
        }

        let mut pixmap = Pixmap::new(width, height).expect("Failed to create pixmap");

        self.draw_shadow(&mut pixmap, width, height);
        self.draw_background(&mut pixmap, width, height);
        self.draw_highlight(&mut pixmap, width, height, candidates);
        self.draw_text(&mut pixmap, width, height, candidates);

        pixels.copy_from_slice(pixmap.data());
    }

    fn draw_shadow(&self, pixmap: &mut Pixmap, width: u32, height: u32) {
        let corner_radius = 8.0f32;

        let shadow_rect = build_rounded_rect(
            SHADOW_OFFSET_X,
            SHADOW_OFFSET_Y,
            width as f32,
            height as f32,
            corner_radius,
        );

        let mut paint = Paint::default();
        paint.set_color_rgba8(0x00, 0x00, 0x00, 0x28);
        paint.anti_alias = true;

        pixmap.fill_path(
            &shadow_rect,
            &paint,
            FillRule::Winding,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    fn draw_background(&self, pixmap: &mut Pixmap, width: u32, height: u32) {
        let corner_radius = 8.0f32;
        let border_width = 2.0f32;

        let rounded_rect = build_rounded_rect(0.0, 0.0, width as f32, height as f32, corner_radius);

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

        let mut border_paint = Paint::default();
        border_paint.set_color_rgba8(0xE0, 0xE0, 0xE0, 0xFF);
        border_paint.anti_alias = true;

        let stroke = tiny_skia::Stroke { width: border_width, ..Default::default() };
        pixmap.stroke_path(
            &rounded_rect,
            &border_paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    fn draw_highlight(&mut self, pixmap: &mut Pixmap, _width: u32, height: u32, candidates: &[String]) {
        if candidates.is_empty() {
            return;
        }

        let first_text = format!("1. {}", candidates[0]);
        let text_width = self.measure_text_width(&first_text);

        let hl_x = 8.0f32;
        let hl_width = text_width + 16.0f32;
        let hl_height = height as f32 - 8.0f32;
        let hl_y = 4.0f32;
        let hl_radius = 4.0f32;

        let rounded_rect = build_rounded_rect(hl_x, hl_y, hl_width, hl_height, hl_radius);

        let mut paint = Paint::default();
        paint.set_color_rgba8(0x8F, 0x73, 0xE2, 0xFF);
        paint.anti_alias = true;

        pixmap.fill_path(
            &rounded_rect,
            &paint,
            FillRule::Winding,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    fn draw_text(&mut self, pixmap: &mut Pixmap, width: u32, height: u32, candidates: &[String]) {
        let font_size = 16.0f32;
        let line_height = 20.0f32;
        let metrics = Metrics::new(font_size, line_height);
        let attrs = Attrs::new().family(Family::SansSerif);

        let normal_color = Color::rgba(0x33, 0x33, 0x33, 0xFF);
        let highlight_color = Color::rgba(0xFF, 0xFF, 0xFF, 0xFF);

        let pixmap_width = pixmap.width() as usize;
        let pixmap_height = pixmap.height() as usize;

        let text_area_height = line_height;
        let y_offset = ((height as f32 - text_area_height) / 2.0).max(0.0) as i32;

        let mut x_offset: f32 = 15.0;

        for (idx, candidate) in candidates.iter().enumerate() {
            let is_first = idx == 0;
            let text_color = if is_first { highlight_color } else { normal_color };

            let text = format!("{}. {}", idx + 1, candidate);
            let mut buffer = Buffer::new(&mut self.font_system, metrics);
            buffer.set_text(&mut self.font_system, &text, attrs.clone(), Shaping::Advanced);

            let x_start = x_offset as i32;

            let pixmap_data = pixmap.data_mut();

            buffer.draw(
                &mut self.font_system,
                &mut self.swash_cache,
                text_color,
                |x, y, w, h, color| {
                    blend_glyph(pixmap_data, x as i32 + x_start, y + y_offset, w as i32, h as i32, color, pixmap_width, pixmap_height);
                },
            );

            x_offset += self.measure_text_width(&text) + 20.0;
        }
    }

    fn measure_text_width(&mut self, text: &str) -> f32 {
        let metrics = Metrics::new(18.0, 22.0);
        let attrs = Attrs::new().family(Family::SansSerif);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_text(&mut self.font_system, text, attrs, Shaping::Advanced);

        let mut max_x = 0.0f32;
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                max_x = max_x.max(glyph.x + glyph.w);
            }
        }
        max_x
    }
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
                let bg_b = pixmap_data[offset] as f32;
                let bg_g = pixmap_data[offset + 1] as f32;
                let bg_r = pixmap_data[offset + 2] as f32;

                let fg_r = color.r() as f32;
                let fg_g = color.g() as f32;
                let fg_b = color.b() as f32;

                let blend_b = (bg_b * (1.0 - alpha) + fg_b * alpha).min(255.0) as u8;
                let blend_g = (bg_g * (1.0 - alpha) + fg_g * alpha).min(255.0) as u8;
                let blend_r = (bg_r * (1.0 - alpha) + fg_r * alpha).min(255.0) as u8;

                pixmap_data[offset] = blend_b;
                pixmap_data[offset + 1] = blend_g;
                pixmap_data[offset + 2] = blend_r;
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

impl Default for CandidateRenderer {
    fn default() -> Self {
        Self::new()
    }
}

pub fn draw_candidates_to_buffer(pixels: &mut [u8], width: u32, height: u32, candidates: &[String]) {
    let mut renderer = CandidateRenderer::new();
    renderer.draw_candidates(pixels, width, height, candidates);
}

pub fn calculate_candidate_width(candidates: &[String]) -> u32 {
    let mut renderer = CandidateRenderer::new();
    renderer.calculate_width(candidates)
}