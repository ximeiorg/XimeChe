//! 基于 iced 的候选栏/菜单/字根 UI 渲染（离屏）。
//!
//! 用 iced 的 widget 树（Element）描述 UI，经 `iced_tiny_skia`
//! 离屏渲染到 SHM buffer。布局/圆角/文本/图标交给 iced，避免手绘。
//! 样式来自 [`PanelTheme`]（配置 + 亮/暗模式），不再硬编码。

use iced_tiny_skia::core::widget::tree::Tree;
use iced_tiny_skia::core::Renderer as CoreRenderer;
use iced_tiny_skia::core::{layout, Element};
use iced_tiny_skia::core::{mouse, renderer::Style, Color, Font, Pixels, Rectangle, Size, Theme};
use iced_tiny_skia::graphics::Viewport;
use iced_tiny_skia::Renderer;
use iced_widget::{container, row, text, Space, Svg};
// 与 iced_tiny_skia 同版本的 tiny-skia（0.11），避免版本冲突
type Pixmap = tiny_skia11::Pixmap;
type Mask = tiny_skia11::Mask;

use crate::menu::{
    list_row_y, PanelGrid, PanelList, PanelPage, CANDIDATE_BAR_H_PADDING, EMPTY_TEXT,
    GRID_CELL_GAP, GRID_CELL_HEIGHT, GRID_HEIGHT, GRID_PER_ROW, GRID_ROWS, GRID_TAB_GAP,
    GRID_TAB_HEIGHT, LIST_DISPLAY_MAX_CHARS, LIST_PAGE_BUTTON_HEIGHT, LIST_PAGE_BUTTON_WIDTH,
    LIST_PAGE_LABEL_WIDTH, LIST_ROWS_PER_PAGE, MENU_BUTTON_WIDTH, PANEL_CONTENT_GAP, PANEL_GAP,
    PANEL_HEADER_HEIGHT, PANEL_H_INSET, PANEL_ITEM_HEIGHT, PANEL_MENU_COL_GAP, PANEL_MENU_TOP,
    PANEL_ROW_GAP, RECENT_EMPTY_TEXT,
};
use crate::theme::PanelTheme;
use crate::CandidateItem;

const MENU_SVG: &[u8] = include_bytes!("../resources/menu.svg");

/// 离屏渲染器（内部持有 iced Renderer + 树状态）。
pub struct IcedSurface {
    renderer: Renderer,
    tree: Tree,
}

impl IcedSurface {
    pub fn new() -> Self {
        let renderer = Renderer::new(Font::default(), Pixels::from(14.0));
        let tree = Tree::empty();
        Self { renderer, tree }
    }

    /// 渲染任意 Element 到 BGRA buffer（通用入口）。
    pub fn render<M: 'static>(
        &mut self,
        element: &mut Element<'_, M, Theme, Renderer>,
        pixels: &mut [u8],
        width: u32,
        height: u32,
    ) {
        let mut pixmap = Pixmap::new(width, height).expect("pixmap");

        // 清空渲染层（iced_tiny_skia 的 layers 不会自动清空，
        // 正常 iced 应用由 window 调用 reset；这里必须手动 reset，
        // 否则上一次渲染的内容会叠加重复绘制）
        self.renderer.reset(Rectangle {
            x: 0.0,
            y: 0.0,
            width: width as f32,
            height: height as f32,
        });

        // diff：让 tree 状态与 widget 结构匹配（container 等需要 state）
        self.tree.diff(element.as_widget());

        // layout
        let widget = element.as_widget_mut();
        let limits = layout::Limits::new(Size::ZERO, Size::new(width as f32, height as f32));
        let node = widget.layout(&mut self.tree, &self.renderer, &limits);

        // draw
        let layout = layout::Layout::new(&node);
        let viewport = Rectangle {
            x: 0.0,
            y: 0.0,
            width: width as f32,
            height: height as f32,
        };
        let style = Style {
            text_color: Color::BLACK,
        };
        let cursor = mouse::Cursor::default();
        widget.draw(
            &self.tree,
            &mut self.renderer,
            &Theme::Dark,
            &style,
            layout,
            cursor,
            &viewport,
        );

        // 渲染到 pixmap
        let viewport =
            Viewport::with_physical_size(iced_tiny_skia::core::Size::new(width, height), 1.0);
        let mut clip_mask = Mask::new(width, height).expect("mask");
        let damage = vec![Rectangle {
            x: 0.0,
            y: 0.0,
            width: width as f32,
            height: height as f32,
        }];
        let bg = Color::TRANSPARENT;
        let mut pixmap_mut = pixmap.as_mut();
        self.renderer
            .draw(&mut pixmap_mut, &mut clip_mask, &viewport, &damage, bg);

        // 直接按序拷贝：tiny-skia 的 pixmap 内存序是 BGRA（小端 u32
        // 0xAABBGGRR），正好是 wl_shm ARGB8888 需要的字节序。
        // 2026-10-03 事故：此处曾按 RGBA→BGRA 再交换一次，把整幅画面
        // 的 R/B 翻反——文字黑白无感、主题色被配置掩盖，只有彩色 emoji
        // 暴露（黄脸变蓝脸）。
        let data = pixmap.data();
        pixels[..data.len()].copy_from_slice(data);
    }

    /// 测量 Element 自然尺寸（内容自适应宽度）。
    pub fn measure<M: 'static>(&mut self, element: &mut Element<'_, M, Theme, Renderer>) -> Size {
        self.tree.diff(element.as_widget());
        let widget = element.as_widget_mut();
        let limits = layout::Limits::new(Size::ZERO, Size::new(10000.0, 10000.0));
        let node = widget.layout(&mut self.tree, &self.renderer, &limits);
        node.bounds().size()
    }

    /// 候选栏自然宽度（内容 + 菜单按钮）。
    pub fn measure_candidates(&mut self, candidates: &[CandidateItem], theme: &PanelTheme) -> u32 {
        let mut view = candidate_bar_view(candidates, 0, theme);
        let size = self.measure(&mut view);
        size.width.ceil() as u32
    }

    /// 绘制候选栏（面板关闭）。
    pub fn draw_candidates(
        &mut self,
        pixels: &mut [u8],
        width: u32,
        height: u32,
        candidates: &[CandidateItem],
        highlighted_index: usize,
        theme: &PanelTheme,
    ) {
        self.draw_panel(
            pixels,
            width,
            height,
            candidates,
            highlighted_index,
            theme,
            None,
            &PanelList::default(),
            &PanelGrid::default(),
        );
    }

    /// 绘制完整面板（候选栏 + 菜单按钮 + 可选展开页面：菜单卡片/列表/网格）。
    ///
    /// `page`/`list`/`grid` 由 daemon 持有（进页时 reload 一次数据，绘帧只读内存，
    /// 对齐 XimeYao 的面板数据纪律）。
    #[allow(clippy::too_many_arguments)]
    pub fn draw_panel(
        &mut self,
        pixels: &mut [u8],
        width: u32,
        height: u32,
        candidates: &[CandidateItem],
        highlighted_index: usize,
        theme: &PanelTheme,
        page: Option<PanelPage>,
        list: &PanelList,
        grid: &PanelGrid,
    ) {
        // 1. 自绘圆角背景 + 边框（SDF 精确，绕开 tiny-skia 曲线光栅化偏差）
        paint_rounded_panel(
            pixels,
            width,
            height,
            theme.corner_radius,
            2.0,
            theme.border,
            theme.bg,
        );

        // 1.5 面板区底色（对齐 XimeYao：候选栏之下空 PANEL_GAP，面板块用
        // 独立于候选栏主题的浅灰底，底部两角随面板圆角收口）。
        if page.is_some() {
            let panel_top = theme.bar_height() + PANEL_GAP;
            paint_panel_surface(pixels, width, height, panel_top, theme);
        }

        // 2. iced 渲染内容（透明背景）到临时 buffer
        let mut content = vec![0u8; (width * height * 4) as usize];
        let mut view = build_panel_view(candidates, highlighted_index, theme, page, list, grid);
        self.render(&mut view, &mut content, width, height);

        // 3. 内容合成到背景
        blend_over(pixels, &content);
    }

    /// 字根窗口自然宽度。
    pub fn measure_root(&mut self, key: char, root: &str, theme: &PanelTheme) -> u32 {
        let mut view = root_view(key, root, theme);
        let size = self.measure(&mut view);
        size.width.ceil() as u32
    }

    /// 绘制字根窗口。
    pub fn draw_root(
        &mut self,
        pixels: &mut [u8],
        width: u32,
        height: u32,
        key: char,
        root: &str,
        theme: &PanelTheme,
    ) {
        // 1. 自绘圆角背景 + 边框（SDF 精确）
        paint_rounded_panel(
            pixels,
            width,
            height,
            theme.corner_radius,
            2.0,
            theme.border,
            theme.bg,
        );

        // 2. iced 渲染内容（透明背景）到临时 buffer
        let mut content = vec![0u8; (width * height * 4) as usize];
        let mut view = root_view(key, root, theme);
        self.render(&mut view, &mut content, width, height);

        // 3. 内容合成到背景
        blend_over(pixels, &content);
    }
}

impl Default for IcedSurface {
    fn default() -> Self {
        Self::new()
    }
}

/// 圆角矩形有符号距离场（负 = 内部）。
fn rounded_rect_sdf(x: f32, y: f32, w: f32, h: f32, r: f32, px: f32, py: f32) -> f32 {
    let half_w = w / 2.0;
    let half_h = h / 2.0;
    let qx = (px - (x + half_w)).abs() - (half_w - r);
    let qy = (py - (y + half_h)).abs() - (half_h - r);
    let dx = qx.max(0.0);
    let dy = qy.max(0.0);
    let d = (dx * dx + dy * dy).sqrt();
    d - r + qx.max(qy).min(0.0)
}

/// 圆角矩形抗锯齿覆盖因子（0..=1）。
fn rounded_alpha(sdf: f32) -> f32 {
    (0.5 - sdf).clamp(0.0, 1.0)
}

/// 面板区底色（亮/暗；对齐 XimeYao 硬编码浅灰，暗色用等价深灰）。
fn panel_surface_color(theme: &PanelTheme) -> Color {
    let [r, g, b] = if theme.dark {
        crate::menu::PANEL_SURFACE_BG_DARK
    } else {
        crate::menu::PANEL_SURFACE_BG_LIGHT
    };
    Color::from_rgb8(r, g, b)
}

/// 在 buffer 下部画面板区底色块（对齐 XimeYao：独立圆角卡片，四角
/// 圆角 = corner_radius，与候选栏之间由 PANEL_GAP 露出底色缝隙）。
fn paint_panel_surface(pixels: &mut [u8], width: u32, height: u32, top: u32, theme: &PanelTheme) {
    let (fw, fh) = (width as f32, (height - top) as f32);
    let radius = theme.corner_radius;
    let (cr, cg, cb) = {
        let c = panel_surface_color(theme);
        (c.r * 255.0, c.g * 255.0, c.b * 255.0)
    };
    let w = width as usize;
    for y in top as usize..height as usize {
        for x in 0..w {
            let d = rounded_rect_sdf(
                0.0,
                0.0,
                fw,
                fh,
                radius,
                x as f32 + 0.5,
                (y - top as usize) as f32 + 0.5,
            );
            let a = rounded_alpha(d);
            if a <= 0.0 {
                continue;
            }
            let idx = (y * w + x) * 4;
            let inv = 1.0 - a;
            pixels[idx] = (cb * a + pixels[idx] as f32 * inv) as u8;
            pixels[idx + 1] = (cg * a + pixels[idx + 1] as f32 * inv) as u8;
            pixels[idx + 2] = (cr * a + pixels[idx + 2] as f32 * inv) as u8;
            pixels[idx + 3] = 255;
        }
    }
}

/// 在 BGRA buffer 上绘制圆角矩形背景 + 边框（SDF 精确，绕开 tiny-skia 曲线偏差）。
fn paint_rounded_panel(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    radius: f32,
    border_width: f32,
    border_color: Color,
    fill_color: Color,
) {
    let (fw, fh) = (width as f32, height as f32);
    let (fr, fg, fb) = (
        fill_color.r * 255.0,
        fill_color.g * 255.0,
        fill_color.b * 255.0,
    );
    let (br, bg, bb) = (
        border_color.r * 255.0,
        border_color.g * 255.0,
        border_color.b * 255.0,
    );
    let w = width as usize;
    for y in 0..height as usize {
        for x in 0..width as usize {
            let d = rounded_rect_sdf(0.0, 0.0, fw, fh, radius, x as f32 + 0.5, y as f32 + 0.5);
            // 边框区域：d ∈ [-border_width, 0]
            let border_a = rounded_alpha(d);
            let fill_a = rounded_alpha(d + border_width);
            let idx = (y * w + x) * 4;
            // 合成：边框在上层，填充在下层（目标为透明背景）
            let mut r_ = fill_a * fr;
            let mut g_ = fill_a * fg;
            let mut b_ = fill_a * fb;
            let mut a_ = fill_a;
            let ba = (border_a - fill_a).max(0.0);
            if ba > 0.0 {
                // 边框叠加在填充上
                let oa = a_;
                r_ = r_ * (1.0 - ba) + br * ba;
                g_ = g_ * (1.0 - ba) + bg * ba;
                b_ = b_ * (1.0 - ba) + bb * ba;
                a_ = oa * (1.0 - ba) + ba;
            }
            pixels[idx] = b_.clamp(0.0, 255.0) as u8;
            pixels[idx + 1] = g_.clamp(0.0, 255.0) as u8;
            pixels[idx + 2] = r_.clamp(0.0, 255.0) as u8;
            pixels[idx + 3] = (a_ * 255.0).clamp(0.0, 255.0) as u8;
        }
    }
}

/// 将内容 buffer 合成到目标 buffer。
///
/// 两边都是**预乘 alpha** 语义：src 来自 tiny-skia（输出即预乘 BGRA），
/// dst 由 paint_rounded_panel 写入（fill_a 已乘进通道），最终 SHM
/// ARGB8888 也由合成器按预乘解释——因此必须用 premultiplied-over：
/// `out = src + dst*(1-sa)`，不再除 out_a（旧实现按直通 alpha 公式
/// 处理预乘数据，AA 边缘被二次乘 alpha，文字边缘偏暗发硬）。
fn blend_over(dst: &mut [u8], src: &[u8]) {
    for i in (0..src.len()).step_by(4) {
        let sa = src[i + 3] as f32 / 255.0;
        if sa >= 1.0 {
            // 完全覆盖：直接替换（省 3 次乘法，不透明像素占绝大多数）
            dst[i] = src[i];
            dst[i + 1] = src[i + 1];
            dst[i + 2] = src[i + 2];
            dst[i + 3] = src[i + 3];
            continue;
        }
        if sa <= 0.0 {
            continue;
        }
        let keep = 1.0 - sa;
        for c in 0..3 {
            let v = src[i + c] as f32 + dst[i + c] as f32 * keep;
            dst[i + c] = v.clamp(0.0, 255.0) as u8;
        }
        let out_a = sa + (dst[i + 3] as f32 / 255.0) * keep;
        dst[i + 3] = (out_a * 255.0).clamp(0.0, 255.0) as u8;
    }
}

/// 候选内容行（不带按钮，Shrink）。
fn candidate_items<'a>(
    candidates: &'a [CandidateItem],
    highlighted_index: usize,
    theme: &'a PanelTheme,
) -> Element<'a, (), Theme, Renderer> {
    let mut content = row![].spacing(6);
    let comment_size = (theme.font_size - 2.0).max(10.0);
    for (idx, candidate) in candidates.iter().enumerate() {
        let is_hl = idx == highlighted_index;
        let label = format!("{}. {}", idx + 1, candidate.text);
        let t = text(label).size(theme.font_size).color(if is_hl {
            Color::WHITE
        } else {
            theme.text_main
        });
        let comment = if !candidate.comment.is_empty() {
            text(candidate.comment.clone())
                .size(comment_size)
                .color(theme.text_comment)
        } else {
            text("").size(comment_size)
        };
        let item = row![t, comment]
            .spacing(4)
            .align_y(iced_widget::core::alignment::Vertical::Center);

        let radius = theme.highlight_radius();
        if is_hl {
            content =
                content.push(
                    container(item)
                        .padding([4, 8])
                        .style(move |_| container::Style {
                            background: Some(iced_widget::core::Background::Color(theme.primary)),
                            border: iced_widget::core::border::Border {
                                radius: iced_widget::core::border::Radius::from(radius),
                                ..Default::default()
                            },
                            ..Default::default()
                        }),
                );
        } else {
            // 与选中同构（container + padding），保证字形垂直位置一致
            content = content.push(container(item).padding([4, 8]));
        }
    }
    content.into()
}

/// 候选栏内容 + 按钮（Shrink，含左右 padding，用于宽度测量）。
///
/// 必须与渲染版 `candidate_bar` 的 padding 一致，否则渲染时
/// 按钮会被 padding 挤压（剩余空间不足导致按钮/图标变小）。
fn candidate_bar_view<'a>(
    candidates: &'a [CandidateItem],
    highlighted_index: usize,
    theme: &'a PanelTheme,
) -> Element<'a, (), Theme, Renderer> {
    container(row![
        candidate_items(candidates, highlighted_index, theme),
        menu_button(false, theme),
    ])
    .padding([0, CANDIDATE_BAR_H_PADDING as u16])
    .into()
}

/// 菜单按钮（九宫格 SVG 图标，active 时高亮）。
fn menu_button(active: bool, theme: &PanelTheme) -> Element<'static, (), Theme, Renderer> {
    let icon = Svg::new(iced_widget::core::svg::Handle::from_memory(MENU_SVG))
        .width(32)
        .height(32);
    let (r, g, b) = (
        (theme.primary.r * 255.0) as u8,
        (theme.primary.g * 255.0) as u8,
        (theme.primary.b * 255.0) as u8,
    );
    container(icon)
        .width(MENU_BUTTON_WIDTH)
        .height(theme.bar_height())
        .align_x(iced_widget::core::alignment::Horizontal::Center)
        .align_y(iced_widget::core::alignment::Vertical::Center)
        .style(move |_| container::Style {
            background: if active {
                Some(iced_widget::core::Background::Color(Color::from_rgba8(
                    r, g, b, 0.13,
                )))
            } else {
                None
            },
            ..Default::default()
        })
        .into()
}

/// 完整面板：候选栏 + 可选展开页面（菜单卡片 / 列表 / 网格）。
fn build_panel_view<'a>(
    candidates: &'a [CandidateItem],
    highlighted_index: usize,
    theme: &'a PanelTheme,
    page: Option<PanelPage>,
    list: &PanelList,
    grid: &PanelGrid,
) -> Element<'a, (), Theme, Renderer> {
    // 布局纪律（画得出来必须点得到）：子页各区块的纵向位置全部按
    // menu.rs 的命中公式排布——header(36) → 分隔线(占内容间距首像素)
    // → 空余间距 → 内容 → …，区块之间用显式 Space 补 PANEL_CONTENT_GAP，
    // 不依赖 iced 流式布局的累积高度。
    let bar = candidate_bar(candidates, highlighted_index, theme);
    let gap_after_divider = PANEL_CONTENT_GAP - 1;
    let content: Element<'a, (), Theme, Renderer> = match page {
        None => bar,
        Some(PanelPage::Menu) => {
            // 菜单页（对齐 XimeYao）：无标题栏；卡片自 PANEL_MENU_TOP 起，
            // 品牌条 = 卡片区 + 间距；总高与 menu_panel_height() 一致。
            iced_widget::column![
                bar,
                Space::new().height(PANEL_GAP),
                menu_cards_page(theme),
                Space::new().height(PANEL_CONTENT_GAP),
                brand_footer(theme),
            ]
            .into()
        }
        Some(sub) if sub.is_list_page() => {
            // y：header 0..36 → divider 36..37 → Space → 行区 44..256 →
            // Space → 翻页条 list_footer_y()..+32 → 底边距。
            iced_widget::column![
                bar,
                Space::new().height(PANEL_GAP),
                page_header(sub, theme),
                panel_divider(theme),
                Space::new().height(gap_after_divider),
                list_rows_page(sub, list, theme),
                Space::new().height(PANEL_CONTENT_GAP),
                pager_bar(
                    &format!("共 {} 条", list.items.len()),
                    list.clamped_page() + 1,
                    list.page_count(),
                    list.has_prev_page(),
                    list.has_next_page(),
                    theme,
                ),
            ]
            .into()
        }
        Some(sub) => {
            // y：header → divider → Space → 网格 grid_top()..+GRID_HEIGHT →
            // Space → 翻页条 grid_footer_y()..（**布局恒定**：只要面板数据
            // 需要翻页就常驻这一行——切到单页分类时不能省，否则标签栏上移
            // 与命中公式错位；单页时 pager_bar 自己只画计数不画按钮，
            // 对齐 XimeYao draw_footer）→ Space → 标签栏 → 底边距。
            let mut col = iced_widget::column![
                bar,
                Space::new().height(PANEL_GAP),
                page_header(sub, theme),
                panel_divider(theme),
                Space::new().height(gap_after_divider),
            ];
            col = col.push(grid_rows_page(grid, theme));
            if grid.has_pager {
                col = col.push(Space::new().height(PANEL_CONTENT_GAP));
                col = col.push(pager_bar(
                    &format!("共 {} 个", grid.item_count),
                    grid.clamped_page() + 1,
                    grid.page_count(),
                    grid.has_prev_page(),
                    grid.has_next_page(),
                    theme,
                ));
            }
            col = col.push(Space::new().height(PANEL_CONTENT_GAP));
            col = col.push(tab_bar(grid, theme));
            col.into()
        }
    };

    // 背景圆角/边框由 paint_rounded_panel 自绘（SDF 精确），这里透明
    container(content)
        .width(iced_widget::core::Length::Fill)
        .height(iced_widget::core::Length::Fill)
        .style(|_| container::Style::default())
        .into()
}

/// 中性行背景色（主题前景按 alpha 混入**面板底色**——卡片/格子都画在
/// 面板块上，对齐 XimeYao 的 fg@0.06 混合基底）。
fn neutral_row_bg(theme: &PanelTheme, alpha: f32) -> Color {
    let to8 = |c: f32| (c * 255.0).round() as u8;
    let blend = |fg: u8, bg: u8| (fg as f32 * alpha + bg as f32 * (1.0 - alpha)) as u8;
    let surface = panel_surface_color(theme);
    let (fg, bg) = (theme.text_main, surface);
    Color::from_rgb8(
        blend(to8(fg.r), to8(bg.r)),
        blend(to8(fg.g), to8(bg.g)),
        blend(to8(fg.b), to8(bg.b)),
    )
}

/// 子页标题栏（对齐 XimeYao）：标题**粗体居中**，左侧「← 菜单」为纯次色文字。
/// 标题栏下的分隔线由 build_panel_view 排布（占内容间距的首个 1px，总高不变）。
fn page_header<'a>(page: PanelPage, theme: &'a PanelTheme) -> Element<'a, (), Theme, Renderer> {
    let bold = Font {
        weight: iced_widget::core::font::Weight::Bold,
        ..Font::default()
    };
    let title_layer = container(
        text(page.title())
            .size(theme.font_size + 1.0)
            .font(bold)
            .color(theme.text_main),
    )
    .width(iced_widget::core::Length::Fill)
    .height(PANEL_HEADER_HEIGHT)
    .align_x(iced_widget::core::alignment::Horizontal::Center)
    .align_y(iced_widget::core::alignment::Vertical::Center);
    let back_layer = container(
        text("← 菜单")
            .size(theme.font_size)
            .color(theme.text_comment),
    )
    .width(iced_widget::core::Length::Fill)
    .height(PANEL_HEADER_HEIGHT)
    .padding([0, PANEL_H_INSET as u16])
    .align_x(iced_widget::core::alignment::Horizontal::Left)
    .align_y(iced_widget::core::alignment::Vertical::Center);
    iced_widget::stack![title_layer, back_layer].into()
}

/// 菜单页：2 列 × 3 行卡片（几何见 menu_card_rect，图标 + 文字）。
fn menu_cards_page(theme: &PanelTheme) -> Element<'static, (), Theme, Renderer> {
    let mut col = iced_widget::column![].spacing(PANEL_ROW_GAP as f32);
    for row_idx in 0..3usize {
        let mut row_widget = iced_widget::row![].spacing(PANEL_MENU_COL_GAP as f32);
        for col_idx in 0..2usize {
            let index = row_idx * 2 + col_idx;
            let cell: Element<'static, (), Theme, Renderer> = match crate::menu::MenuCard::at(index)
            {
                Some(card) => menu_card_cell(card, theme),
                None => Space::new()
                    .width(iced_widget::core::Length::Fill)
                    .height(PANEL_ITEM_HEIGHT)
                    .into(),
            };
            row_widget = row_widget.push(cell);
        }
        col = col.push(
            row_widget
                .width(iced_widget::core::Length::Fill)
                .height(PANEL_ITEM_HEIGHT),
        );
    }
    container(col)
        .padding([PANEL_MENU_TOP as u16, PANEL_H_INSET as u16])
        .width(iced_widget::core::Length::Fill)
        .into()
}

/// 菜单卡片：图标 chip + 文字（对齐 XimeYao 📋🚀😀🔣🎙️⚙️）。
fn menu_card_cell(
    card: crate::menu::MenuCard,
    theme: &PanelTheme,
) -> Element<'static, (), Theme, Renderer> {
    let colors = [
        Color::from_rgb8(0x2E, 0xA0, 0x7D),
        Color::from_rgb8(0x1A, 0x73, 0xE8),
        Color::from_rgb8(0x8F, 0x73, 0xE2),
        Color::from_rgb8(0xE5, 0x8F, 0x2A),
        Color::from_rgb8(0xD5, 0x4B, 0x4B),
        Color::from_rgb8(0x64, 0x74, 0x88),
    ];
    let idx = crate::menu::MenuCard::ALL
        .iter()
        .position(|c| *c == card)
        .unwrap_or(0);
    let color = colors[idx];
    let card_bg = neutral_row_bg(theme, 0.06);
    let icon = container(
        text(card.icon().to_string())
            .size(theme.font_size + 2.0)
            .color(color),
    )
    .width(24)
    .height(24)
    .align_x(iced_widget::core::alignment::Horizontal::Center)
    .align_y(iced_widget::core::alignment::Vertical::Center);
    let item = row![
        icon,
        text(card.label().to_string())
            .size(theme.font_size)
            .color(theme.text_main),
        Space::new().width(iced_widget::core::Length::Fill),
    ]
    .spacing(10)
    .align_y(iced_widget::core::alignment::Vertical::Center);
    container(item)
        .width(iced_widget::core::Length::Fill)
        .height(PANEL_ITEM_HEIGHT)
        .padding([0, 10])
        .align_y(iced_widget::core::alignment::Vertical::Center)
        .style(move |_| container::Style {
            // 卡片底 = 前景 6%（对齐 XimeYao 的中性卡底，暗色主题自动适配）。
            background: Some(iced_widget::core::Background::Color(card_bg)),
            border: iced_widget::core::border::Border {
                radius: iced_widget::core::border::Radius::from(8.0),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

/// 菜单页底部品牌条（XimeYao 的 footer 入口条位；字号小一档）。
fn brand_footer(theme: &PanelTheme) -> Element<'static, (), Theme, Renderer> {
    container(
        text("曦码·澈输入法")
            .size(theme.font_size - 2.0)
            .color(theme.text_comment),
    )
    .width(iced_widget::core::Length::Fill)
    .height(PANEL_ITEM_HEIGHT)
    .align_x(iced_widget::core::alignment::Horizontal::Center)
    .align_y(iced_widget::core::alignment::Vertical::Center)
    .into()
}

/// 列表条目展示文本（对齐 XimeYao list_display_text）：控制字符折空格，
/// 超长截断加省略号——面板单行显示，绝不折行。
fn list_display_text(text: &str) -> String {
    let mut out = String::new();
    let mut overflowed = false;
    for (i, ch) in text.chars().enumerate() {
        if i >= LIST_DISPLAY_MAX_CHARS {
            overflowed = true;
            break;
        }
        out.push(if ch.is_control() { ' ' } else { ch });
    }
    if overflowed {
        out.push('…');
    }
    out
}

/// 列表子页（对齐 XimeYao）：6 行卡片式条目（圆角卡底、编码列在左、
/// 正文左对齐单行）；空态主文案 + 补充提示整体居中于内容区。
fn list_rows_page<'a>(
    page: PanelPage,
    list: &PanelList,
    theme: &'a PanelTheme,
) -> Element<'a, (), Theme, Renderer> {
    let small = theme.font_size;
    // 行区高度 = 6 行 + 5 个行距（末行后没有行距；list_row_y 是面板内 y，
    // 含 header 偏移，需扣掉）。
    let content_height =
        list_row_y(crate::menu::LIST_ROWS_PER_PAGE) - list_row_y(0) - PANEL_ROW_GAP;
    if list.items.is_empty() {
        let mut col = iced_widget::column![container(
            text(page.list_empty_text())
                .size(small)
                .color(theme.text_comment)
        )
        .width(iced_widget::core::Length::Fill)
        .align_x(iced_widget::core::alignment::Horizontal::Center),]
        .spacing(4);
        if let Some(hint) = page.list_empty_hint() {
            col = col.push(
                container(
                    text(hint.to_string())
                        .size(small - 2.0)
                        .color(theme.text_comment),
                )
                .width(iced_widget::core::Length::Fill)
                .align_x(iced_widget::core::alignment::Horizontal::Center),
            );
        }
        return container(col)
            .width(iced_widget::core::Length::Fill)
            .height(content_height)
            .align_y(iced_widget::core::alignment::Vertical::Center)
            .into();
    }
    let card_bg = neutral_row_bg(theme, 0.06);
    let code_col = list.has_codes();
    let mut rows = iced_widget::column![].spacing(PANEL_ROW_GAP as f32);
    for i in 0..LIST_ROWS_PER_PAGE {
        let row_widget: Element<'static, (), Theme, Renderer> = match list.item_at(i) {
            Some(item) => {
                // 编码列固定在正文左侧（对齐 XimeYao：左 8px 起、定宽 64，
                // 翻页时正文起点不随编码长度跳动）。
                let content: Element<'static, (), Theme, Renderer> = if code_col {
                    row![
                        container(
                            text(item.code.clone())
                                .size(small)
                                .color(theme.text_comment)
                        )
                        .width(crate::menu::QUICK_SEND_CODE_COL_WIDTH),
                        text(list_display_text(&item.text))
                            .size(small)
                            .color(theme.text_main),
                    ]
                    .spacing(crate::menu::QUICK_SEND_CODE_COL_GAP as f32)
                    .align_y(iced_widget::core::alignment::Vertical::Center)
                    .into()
                } else {
                    text(list_display_text(&item.text))
                        .size(small)
                        .color(theme.text_main)
                        .into()
                };
                container(
                    row![content, Space::new().width(iced_widget::core::Length::Fill)]
                        .align_y(iced_widget::core::alignment::Vertical::Center),
                )
                .width(iced_widget::core::Length::Fill)
                .height(PANEL_ITEM_HEIGHT)
                .padding([0, 8])
                .align_y(iced_widget::core::alignment::Vertical::Center)
                .style(move |_| container::Style {
                    background: Some(iced_widget::core::Background::Color(card_bg)),
                    border: iced_widget::core::border::Border {
                        radius: iced_widget::core::border::Radius::from(8.0),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into()
            }
            None => Space::new()
                .width(iced_widget::core::Length::Fill)
                .height(PANEL_ITEM_HEIGHT)
                .into(),
        };
        rows = rows.push(row_widget);
    }
    container(rows)
        .padding([0, PANEL_H_INSET as u16])
        .width(iced_widget::core::Length::Fill)
        .into()
}

/// 底部翻页条（对齐 XimeYao）：左「共 N 条/个」、pages>1 时中「第 x/y 页」、
/// 右侧上一页/下一页按钮（60x24 卡底；禁用不画底、文字次色）；按钮文字小一档。
fn pager_bar(
    count_text: &str,
    current: usize,
    total: usize,
    has_prev: bool,
    has_next: bool,
    theme: &PanelTheme,
) -> Element<'static, (), Theme, Renderer> {
    let small = theme.font_size;
    let card_bg = neutral_row_bg(theme, 0.06);
    let btn = |label: &'static str, enabled: bool| -> Element<'static, (), Theme, Renderer> {
        let fg = if enabled {
            theme.text_main
        } else {
            theme.text_comment
        };
        let mut btn = container(text(label.to_string()).size(small - 2.0).color(fg))
            .width(LIST_PAGE_BUTTON_WIDTH)
            .height(LIST_PAGE_BUTTON_HEIGHT)
            .align_x(iced_widget::core::alignment::Horizontal::Center)
            .align_y(iced_widget::core::alignment::Vertical::Center);
        if enabled {
            btn = btn.style(move |_| container::Style {
                background: Some(iced_widget::core::Background::Color(card_bg)),
                border: iced_widget::core::border::Border {
                    radius: iced_widget::core::border::Radius::from(8.0),
                    ..Default::default()
                },
                ..Default::default()
            });
        }
        btn.into()
    };
    // 结构（对齐 XimeYao 几何）：左 count 占满剩余宽，右端依次是
    // 页码 label（72）→ 上一页 → 下一页；**单页时整段右侧不画**
    // （XimeYao draw_footer 的 pages > 1 分支），只留左侧计数。
    if total > 1 {
        let mut right = iced_widget::row![].spacing(PANEL_MENU_COL_GAP as f32);
        right = right.push(
            container(
                text(format!("第 {current}/{total} 页"))
                    .size(small - 2.0)
                    .color(theme.text_comment),
            )
            .width(LIST_PAGE_LABEL_WIDTH)
            .align_x(iced_widget::core::alignment::Horizontal::Left),
        );
        right = right.push(btn("上一页", has_prev));
        right = right.push(btn("下一页", has_next));
        return container(
            row![
                text(count_text.to_string())
                    .size(small - 2.0)
                    .color(theme.text_comment),
                Space::new().width(iced_widget::core::Length::Fill),
                right
            ]
            .align_y(iced_widget::core::alignment::Vertical::Center),
        )
        .width(iced_widget::core::Length::Fill)
        .height(PANEL_ITEM_HEIGHT)
        .padding([0, PANEL_H_INSET as u16])
        .align_y(iced_widget::core::alignment::Vertical::Center)
        .into();
    }
    container(
        text(count_text.to_string())
            .size(small - 2.0)
            .color(theme.text_comment),
    )
    .width(iced_widget::core::Length::Fill)
    .height(PANEL_ITEM_HEIGHT)
    .padding([0, PANEL_H_INSET as u16])
    .align_y(iced_widget::core::alignment::Vertical::Center)
    .into()
}

/// 网格子页（对齐 XimeYao）：8 列圆角卡片格子，表情字号 +4、符号 +2；
/// 空槽不画；空标签在网格区中央给提示（暂无最近使用/暂无内容）。
fn grid_rows_page(grid: &PanelGrid, theme: &PanelTheme) -> Element<'static, (), Theme, Renderer> {
    let card_bg = neutral_row_bg(theme, 0.06);
    let is_emoji = grid.source == PanelPage::Emoji;
    let glyph_size = if is_emoji {
        theme.font_size + 4.0
    } else {
        theme.font_size + 2.0
    };
    let items = grid.items_on_page();
    if items == 0 {
        let empty = if grid.tab == 0 {
            RECENT_EMPTY_TEXT
        } else {
            EMPTY_TEXT
        };
        return container(
            text(empty.to_string())
                .size(theme.font_size - 2.0)
                .color(theme.text_comment),
        )
        .width(iced_widget::core::Length::Fill)
        .height(GRID_HEIGHT)
        .align_x(iced_widget::core::alignment::Horizontal::Center)
        .align_y(iced_widget::core::alignment::Vertical::Center)
        .into();
    }
    let mut col = iced_widget::column![].spacing(GRID_CELL_GAP as f32);
    for r in 0..GRID_ROWS {
        let mut row_widget = iced_widget::row![].spacing(GRID_CELL_GAP as f32);
        for c in 0..GRID_PER_ROW {
            let slot = r * GRID_PER_ROW + c;
            let cell: Element<'static, (), Theme, Renderer> = match grid.item_at(slot) {
                Some(glyph) => container(
                    text(glyph.to_string())
                        .size(glyph_size)
                        .color(theme.text_main),
                )
                .width(iced_widget::core::Length::Fill)
                .height(GRID_CELL_HEIGHT)
                .align_x(iced_widget::core::alignment::Horizontal::Center)
                .align_y(iced_widget::core::alignment::Vertical::Center)
                .style(move |_| container::Style {
                    background: Some(iced_widget::core::Background::Color(card_bg)),
                    border: iced_widget::core::border::Border {
                        radius: iced_widget::core::border::Radius::from(8.0),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into(),
                None => Space::new()
                    .width(iced_widget::core::Length::Fill)
                    .height(GRID_CELL_HEIGHT)
                    .into(),
            };
            row_widget = row_widget.push(cell);
        }
        col = col.push(row_widget.width(iced_widget::core::Length::Fill));
    }
    container(col)
        .padding([0, PANEL_H_INSET as u16])
        .width(iced_widget::core::Length::Fill)
        .into()
}

/// 底部分类标签栏（表情 1 行 / 符号 2 行；当前标签主色高亮，贴面板底边）。
fn tab_bar(grid: &PanelGrid, theme: &PanelTheme) -> Element<'static, (), Theme, Renderer> {
    let tabs = grid.tab_count();
    let current = grid.clamped_tab();
    let mut rows = iced_widget::column![].spacing(GRID_TAB_GAP as f32);
    let row_count = tabs.div_ceil(crate::menu::GRID_TABS_PER_ROW);
    for r in 0..row_count {
        let mut row_widget = iced_widget::row![].spacing(0);
        let count_in_row = if row_count == 1 {
            tabs
        } else {
            crate::menu::GRID_TABS_PER_ROW
        };
        for c in 0..count_in_row {
            let i = r * crate::menu::GRID_TABS_PER_ROW + c;
            let cell: Element<'static, (), Theme, Renderer> = match grid.tabs.get(i) {
                Some(label) => {
                    // 对齐 XimeYao：选中 = 高亮底（主色 15%）+ 主文字色；
                    // 其余 = 卡片底（fg 6%）+ 次文字色；字号小一档。
                    let active = i == current;
                    let (bg, fg) = if active {
                        (
                            Color::from_rgba8(
                                (theme.primary.r * 255.0) as u8,
                                (theme.primary.g * 255.0) as u8,
                                (theme.primary.b * 255.0) as u8,
                                0.15,
                            ),
                            theme.text_main,
                        )
                    } else {
                        (neutral_row_bg(theme, 0.06), theme.text_comment)
                    };
                    container(text(label.clone()).size(theme.font_size - 2.0).color(fg))
                        .width(iced_widget::core::Length::Fill)
                        .height(GRID_TAB_HEIGHT)
                        .align_x(iced_widget::core::alignment::Horizontal::Center)
                        .align_y(iced_widget::core::alignment::Vertical::Center)
                        .style(move |_| container::Style {
                            background: Some(iced_widget::core::Background::Color(bg)),
                            border: iced_widget::core::border::Border {
                                radius: iced_widget::core::border::Radius::from(8.0),
                                ..Default::default()
                            },
                            ..Default::default()
                        })
                        .into()
                }
                None => Space::new()
                    .width(iced_widget::core::Length::Fill)
                    .height(GRID_TAB_HEIGHT)
                    .into(),
            };
            row_widget = row_widget.push(cell);
        }
        rows = rows.push(row_widget.width(iced_widget::core::Length::Fill));
    }
    container(rows)
        .padding([0, PANEL_H_INSET as u16])
        .width(iced_widget::core::Length::Fill)
        .into()
}

/// 标题栏底部分隔线（fg@6%，对齐 XimeYao 的 line_brush）。
fn panel_divider<'a>(theme: &'a PanelTheme) -> Element<'a, (), Theme, Renderer> {
    let line_bg = neutral_row_bg(theme, 0.06);
    container(Space::new())
        .width(iced_widget::core::Length::Fill)
        .height(1)
        .style(move |_| container::Style {
            background: Some(iced_widget::core::Background::Color(line_bg)),
            ..Default::default()
        })
        .into()
}

/// 候选栏行（渲染用：容器撑满 buffer 宽，内容左对齐 + 按钮右对齐）。
fn candidate_bar<'a>(
    candidates: &'a [CandidateItem],
    highlighted_index: usize,
    theme: &'a PanelTheme,
) -> Element<'a, (), Theme, Renderer> {
    container(
        row![
            candidate_items(candidates, highlighted_index, theme),
            Space::new().width(iced_widget::core::Length::Fill),
            menu_button(false, theme),
        ]
        .align_y(iced_widget::core::alignment::Vertical::Center),
    )
    .width(iced_widget::core::Length::Fill)
    .height(theme.bar_height())
    .padding([0, CANDIDATE_BAR_H_PADDING as u16])
    .align_y(iced_widget::core::alignment::Vertical::Center)
    .into()
}

/// 字根窗口：`[key] root`。
fn root_view<'a>(
    key: char,
    root: &'a str,
    theme: &'a PanelTheme,
) -> Element<'a, (), Theme, Renderer> {
    let key_box = container(
        text(key.to_string())
            .size(theme.font_size + 6.0)
            .color(Color::WHITE),
    )
    .width(32)
    .height(32)
    .align_x(iced_widget::core::alignment::Horizontal::Center)
    .align_y(iced_widget::core::alignment::Vertical::Center)
    .style(move |_| container::Style {
        background: Some(iced_widget::core::Background::Color(theme.primary)),
        border: iced_widget::core::border::Border {
            radius: iced_widget::core::border::Radius::from(4.0),
            ..Default::default()
        },
        ..Default::default()
    });
    let root_text = text(root.to_string())
        .size(theme.font_size + 2.0)
        .color(theme.text_main);
    // 背景圆角/边框由 paint_rounded_panel 自绘（SDF 精确），这里透明
    container(
        row![key_box, root_text]
            .spacing(8)
            .align_y(iced_widget::core::alignment::Vertical::Center),
    )
    .padding([4, 12])
    .align_y(iced_widget::core::alignment::Vertical::Center)
    .into()
}

/// 供测试/验证用：无操作消息类型。
pub type IcedElement<'a> = Element<'a, (), Theme, Renderer>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CandidateItem;

    /// blend_over 必须按预乘语义合成：半透明蓝（预乘 src）盖在不透明红上，
    /// 结果 = src + dst*(1-sa)，alpha 通道单独 over。直通公式会把预乘的
    /// src 通道再乘一次 sa，AA 边缘偏暗。
    #[test]
    fn blend_over_is_premultiplied() {
        // src：半透明纯蓝，a=128（预乘后 B=255*128/255≈128，字节存 128）
        let sa = 128.0 / 255.0;
        let src = [0u8, 0, (255.0 * sa) as u8, 128];
        // dst：不透明纯红（预乘 = 直通，因为 a=255）
        let mut dst = [255u8, 0, 0, 255];
        blend_over(&mut dst, &src);
        // out.B = 128 + 0*(1-sa) = 128（预乘）
        // out.R = 0 + 255*(1-sa) = 127
        // out.A = sa + 1*(1-sa) = 1 → 255
        // （f32 截断允许 ±1 误差）
        let sa = sa as f32;
        assert!(
            (dst[0] as i32 - (255.0f32 * (1.0 - sa)) as i32).abs() <= 1,
            "R 通道: {}",
            dst[0]
        );
        assert_eq!(dst[1], 0);
        assert!(
            (dst[2] as i32 - (255.0f32 * sa) as i32).abs() <= 1,
            "B 通道: {}",
            dst[2]
        );
        assert_eq!(dst[3], 255);
    }

    /// 全透明 src 不得改动 dst；全不透明 src 必须整体替换。
    #[test]
    fn blend_over_endpoints() {
        let mut dst = [10u8, 20, 30, 40];
        blend_over(&mut dst, &[0, 0, 0, 0]);
        assert_eq!(dst, [10, 20, 30, 40]);

        blend_over(&mut dst, &[1, 2, 3, 255]);
        assert_eq!(dst, [1, 2, 3, 255]);
    }

    fn sample_candidates() -> Vec<CandidateItem> {
        vec![
            CandidateItem {
                text: "式".into(),
                comment: "aa".into(),
                index: 0,
            },
            CandidateItem {
                text: "是".into(),
                comment: "bb".into(),
                index: 1,
            },
            CandidateItem {
                text: "时".into(),
                comment: "cc".into(),
                index: 2,
            },
        ]
    }

    fn test_theme() -> PanelTheme {
        PanelTheme::light(16.0, 6.0, (0x8F, 0x73, 0xE2))
    }

    /// 测量宽度应合理（内容自适应，不会撑满极限尺寸）。
    #[test]
    fn test_measure_candidates_width_reasonable() {
        let candidates = sample_candidates();
        let theme = test_theme();
        let mut surface = IcedSurface::new();
        let w = surface.measure_candidates(&candidates, &theme);
        assert!(
            (80..=800).contains(&w),
            "candidate width should be reasonable, got: {}",
            w
        );
    }

    /// 四角应透明（圆角裁剪生效），边缘中部应有背景色。
    #[test]
    fn test_panel_corners_rounded() {
        let candidates = sample_candidates();
        let theme = test_theme();
        let mut surface = IcedSurface::new();
        let w = surface.measure_candidates(&candidates, &theme);
        let h = theme.bar_height();
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        surface.draw_candidates(&mut pixels, w, h, &candidates, 0, &theme);

        // 角部 1px 内应透明（圆角），检查 (0,0) 与 (w-1, h-1)
        let at = |x: u32, y: u32| pixels[((y * w + x) * 4 + 3) as usize];
        assert_eq!(at(0, 0), 0, "top-left corner should be transparent");
        assert_eq!(at(w - 1, 0), 0, "top-right corner should be transparent");
        assert_eq!(at(0, h - 1), 0, "bottom-left corner should be transparent");
        assert_eq!(
            at(w - 1, h - 1),
            0,
            "bottom-right corner should be transparent"
        );

        // 上边缘中部应有内容（圆角弧线之外）
        assert!(at(w / 2, 1) != 0, "top edge middle should have background");
    }

    /// 连续多次渲染面板，菜单按钮图标不应叠加重复（复现"打一次多一个图标"）。
    #[test]
    fn test_repeated_draw_no_icon_doubling() {
        let candidates = sample_candidates();
        let theme = test_theme();
        let mut surface = IcedSurface::new();
        let w = surface.measure_candidates(&candidates, &theme);
        let h = theme.bar_height() + crate::menu_panel_height();

        let mut frames: Vec<Vec<u8>> = Vec::new();
        for _ in 0..3 {
            let mut pixels = vec![0u8; (w * h * 4) as usize];
            surface.draw_panel(
                &mut pixels,
                w,
                h,
                &candidates,
                0,
                &theme,
                Some(crate::PanelPage::Menu),
                &crate::PanelList::default(),
                &crate::PanelGrid::default(),
            );
            frames.push(pixels);
        }
        let a = &frames[0];
        let b = &frames[1];
        let mut minx = w;
        let mut maxx = 0;
        let mut miny = h;
        let mut maxy = 0;
        let mut count = 0;
        for y in 0..h {
            for x in 0..w {
                let ia = ((y * w + x) * 4) as usize;
                if a[ia..ia + 4] != b[ia..ia + 4] {
                    count += 1;
                    minx = minx.min(x);
                    maxx = maxx.max(x);
                    miny = miny.min(y);
                    maxy = maxy.max(y);
                }
            }
        }
        assert_eq!(
            count, 0,
            "frames differ: {} px, bbox x:[{},{}] y:[{},{}]",
            count, minx, maxx, miny, maxy
        );
    }

    /// 亮/暗主题渲染都应正常完成（暗色背景非透明）。
    #[test]
    fn test_dark_theme_renders() {
        let candidates = sample_candidates();
        let theme = PanelTheme::dark(16.0, 6.0, (0x8F, 0x73, 0xE2));
        let mut surface = IcedSurface::new();
        let w = surface.measure_candidates(&candidates, &theme);
        let h = theme.bar_height();
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        surface.draw_candidates(&mut pixels, w, h, &candidates, 0, &theme);
        let at = |x: u32, y: u32| pixels[((y * w + x) * 4 + 3) as usize];
        assert!(at(w / 2, 1) != 0, "dark theme should paint background");
    }
}
