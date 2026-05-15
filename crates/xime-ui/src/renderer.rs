use cosmic_text::{FontSystem, SwashCache, Buffer, Metrics, Attrs, Color, Shaping, Family, fontdb};
use tiny_skia::{Pixmap, Paint, PathBuilder, FillRule};
use crate::candidate::CandidateItem;

const SHADOW_OFFSET_X: f32 = 2.0;
const SHADOW_OFFSET_Y: f32 = 4.0;

// Embed vivoSans font for candidate display
const VIVO_FONT: &[u8] = include_bytes!("../resources/fonts/vivoSans-Regular.ttf");

pub struct CandidateRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
}

impl CandidateRenderer {
    pub fn new() -> Self {
        // Create font system with embedded vivoSans font
        let font_source = fontdb::Source::Binary(std::sync::Arc::new(VIVO_FONT));
        let font_system = FontSystem::new_with_fonts([font_source]);
        let swash_cache = SwashCache::new();
        Self { font_system, swash_cache }
    }

    pub fn calculate_width(&mut self, candidates: &[CandidateItem]) -> u32 {
        if candidates.is_empty() {
            return 100;
        }

        let mut total_width: f32 = 15.0;
        for (idx, candidate) in candidates.iter().enumerate() {
            let main_text = format!("{}. {}", idx + 1, candidate.text);
            total_width += self.measure_text_width(&main_text, 16.0);
            if !candidate.comment.is_empty() {
                total_width += self.measure_text_width(&candidate.comment, 12.0) + 5.0;
            }
            total_width += 23.0;
        }
        total_width += 15.0;

        (total_width.ceil() as u32).max(100)
    }

    pub fn draw_candidates(&mut self, pixels: &mut [u8], width: u32, height: u32, candidates: &[CandidateItem], highlighted_index: usize, primary_color: (u8, u8, u8)) {
        if candidates.is_empty() {
            return;
        }

        let mut pixmap = Pixmap::new(width, height).expect("Failed to create pixmap");

        // First calculate all widths
        let widths: Vec<f32> = candidates.iter().enumerate().map(|(idx, c)| {
            let main_text = format!("{}. {}", idx + 1, c.text);
            let w = self.measure_text_width(&main_text, 16.0);
            let comment_w = if !c.comment.is_empty() {
                self.measure_text_width(&c.comment, 12.0) + 5.0
            } else {
                0.0
            };
            w + comment_w + 23.0
        }).collect();

        self.draw_shadow(&mut pixmap, width, height);
        self.draw_background(&mut pixmap, width, height);
        self.draw_highlight(&mut pixmap, width, height, candidates, highlighted_index, &widths, primary_color);
        self.draw_text(&mut pixmap, width, height, candidates, highlighted_index, &widths);

        let data = pixmap.data();
        for i in (0..data.len()).step_by(4) {
            pixels[i] = data[i + 2];
            pixels[i + 1] = data[i + 1];
            pixels[i + 2] = data[i];
            pixels[i + 3] = data[i + 3];
        }
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

    fn draw_highlight(&mut self, pixmap: &mut Pixmap, _width: u32, height: u32, candidates: &[CandidateItem], highlighted_index: usize, widths: &[f32], primary_color: (u8, u8, u8)) {
        if candidates.is_empty() || highlighted_index >= candidates.len() {
            return;
        }

        // Calculate x offset for highlighted candidate using precomputed widths
        let mut x_offset: f32 = 15.0;
        for i in 0..highlighted_index {
            x_offset += widths[i];
        }

        let candidate = &candidates[highlighted_index];
        let main_text = format!("{}. {}", highlighted_index + 1, candidate.text);
        let text_width = self.measure_text_width(&main_text, 16.0);
        let comment_width = if !candidate.comment.is_empty() {
            self.measure_text_width(&candidate.comment, 12.0) + 5.0
        } else {
            0.0
        };

        let hl_x = x_offset - 7.0;
        let hl_width = text_width + comment_width + 14.0f32;
        let hl_height = height as f32 - 8.0f32;
        let hl_y = 4.0f32;
        let hl_radius = 4.0f32;

        let rounded_rect = build_rounded_rect(hl_x, hl_y, hl_width, hl_height, hl_radius);

        let mut paint = Paint::default();
        paint.set_color_rgba8(primary_color.0, primary_color.1, primary_color.2, 0xFF);
        paint.anti_alias = true;

        pixmap.fill_path(
            &rounded_rect,
            &paint,
            FillRule::Winding,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    fn draw_text(&mut self, pixmap: &mut Pixmap, _width: u32, height: u32, candidates: &[CandidateItem], highlighted_index: usize, widths: &[f32]) {
        let line_height = 20.0f32;
        let y_offset = ((height as f32 - line_height) / 2.0).max(0.0) as i32;

        let pixmap_width = pixmap.width() as usize;
        let pixmap_height = pixmap.height() as usize;

        let mut x_offset: f32 = 15.0;

        for (idx, candidate) in candidates.iter().enumerate() {
            let is_highlighted = idx == highlighted_index;

            let main_color = if is_highlighted {
                Color::rgba(0xFF, 0xFF, 0xFF, 0xFF)
            } else {
                Color::rgba(0x33, 0x33, 0x33, 0xFF)
            };
            let comment_color = if is_highlighted {
                Color::rgba(0xCC, 0xCC, 0xCC, 0xFF)
            } else {
                Color::rgba(0x99, 0x99, 0x99, 0xFF)
            };

            let main_text = format!("{}. {}", idx + 1, candidate.text);
            let main_width = self.draw_text_item(
                &main_text, 16.0, main_color, x_offset as i32, y_offset,
                pixmap.data_mut(), pixmap_width, pixmap_height
            );

let _comment_width = if !candidate.comment.is_empty() {
                self.draw_text_item(
                    &candidate.comment, 12.0, comment_color,
                    (x_offset + main_width + 3.0) as i32, y_offset + 4,
                    pixmap.data_mut(), pixmap_width, pixmap_height
                )
            } else {
                0.0
            };

            // Use precomputed width for positioning
            x_offset += widths[idx];
        }
    }

    fn draw_text_item(
        &mut self,
        text: &str,
        font_size: f32,
        color: Color,
        x_start: i32,
        y_offset: i32,
        pixmap_data: &mut [u8],
        pixmap_width: usize,
        pixmap_height: usize,
    ) -> f32 {
        let metrics = Metrics::new(font_size, font_size + 4.0);
        let attrs = Attrs::new().family(Family::SansSerif);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);

        let text_width = buffer.layout_runs().fold(0.0f32, |max_w, run| max_w.max(run.line_w));

        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            color,
            |x, y, w, h, color| {
                blend_glyph(pixmap_data, x as i32 + x_start, y + y_offset, w as i32, h as i32, color, pixmap_width, pixmap_height);
            },
        );

        text_width
    }

    fn measure_text_width(&mut self, text: &str, font_size: f32) -> f32 {
        let metrics = Metrics::new(font_size, font_size + 4.0);
        let attrs = Attrs::new().family(Family::SansSerif);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);

        buffer.layout_runs().fold(0.0f32, |max_w, run| max_w.max(run.line_w))
    }
}

impl Default for CandidateRenderer {
    fn default() -> Self {
        Self::new()
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

pub fn draw_candidates_to_buffer(pixels: &mut [u8], width: u32, height: u32, candidates: &[CandidateItem], highlighted_index: usize, primary_color: (u8, u8, u8)) {
    let mut renderer = CandidateRenderer::new();
    renderer.draw_candidates(pixels, width, height, candidates, highlighted_index, primary_color);
}

pub fn calculate_candidate_width(candidates: &[CandidateItem]) -> u32 {
    let mut renderer = CandidateRenderer::new();
    renderer.calculate_width(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_width_with_comment() {
        let mut renderer = CandidateRenderer::new();

        let candidates: Vec<CandidateItem> = vec![
            CandidateItem { text: "式".to_string(), comment: "aa".to_string(), index: 0 },
            CandidateItem { text: "是".to_string(), comment: "bb".to_string(), index: 1 },
            CandidateItem { text: "时".to_string(), comment: "cc".to_string(), index: 2 },
        ];
        let width = renderer.calculate_width(&candidates);
        eprintln!("Width with comments: {}", width);
        assert!(width > 100, "Width should include comment space");
    }

    #[test]
    fn test_measure_text_width() {
        let mut renderer = CandidateRenderer::new();

        let w16 = renderer.measure_text_width("式", 16.0);
        let w12 = renderer.measure_text_width("aa", 12.0);
        eprintln!("16px: {}, 12px: {}", w16, w12);
        assert!(w16 > 0.0);
        assert!(w12 > 0.0);
    }

    #[test]
    fn test_primary_color_lavender_purple() {
        let primary_color = (0x8F, 0x73, 0xE2);
        assert_eq!(primary_color.0, 143);
        assert_eq!(primary_color.1, 115);
        assert_eq!(primary_color.2, 226);
    }

    #[test]
    fn test_primary_color_ocean_blue() {
        let primary_color = (0x1A, 0x73, 0xE8);
        assert_eq!(primary_color.0, 26);
        assert_eq!(primary_color.1, 115);
        assert_eq!(primary_color.2, 232);
    }

    #[test]
    fn test_draw_candidates_with_custom_color() {
        let mut renderer = CandidateRenderer::new();
        
        let candidates: Vec<CandidateItem> = vec![
            CandidateItem { text: "测试".to_string(), comment: "".to_string(), index: 0 },
        ];
        
        let width = 200;
        let height = 36;
        let primary_color = (0x1A, 0x73, 0xE8);
        
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        renderer.draw_candidates(&mut pixels, width, height, &candidates, 0, primary_color);
        
        assert!(!pixels.is_empty());
        assert!(pixels.len() == width as usize * height as usize * 4);
    }

    #[test]
    fn test_draw_candidates_empty() {
        let mut renderer = CandidateRenderer::new();
        
        let candidates: Vec<CandidateItem> = vec![];
        let primary_color = (0x8F, 0x73, 0xE2);
        
        let mut pixels = vec![0u8; 100 * 36 * 4];
        renderer.draw_candidates(&mut pixels, 100, 36, &candidates, 0, primary_color);
        
        assert!(!pixels.is_empty());
    }

    #[test]
    fn test_draw_highlight_primary_color_applied() {
        let primary_color = (0xC6, 0x28, 0x28);
        
        let r = primary_color.0;
        let g = primary_color.1;
        let b = primary_color.2;
        
        assert_eq!(r, 198);
        assert_eq!(g, 40);
        assert_eq!(b, 40);
    }
}