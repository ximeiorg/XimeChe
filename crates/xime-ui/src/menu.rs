//! 候选栏菜单面板的纯逻辑：常量、命中测试、菜单项定义。
//!
//! 菜单按钮是候选栏最右侧固定区域（36x36），绘制九宫格 SVG 图标；
//! 点击后候选栏向下展开：候选栏与菜单同处一个圆角容器内，
//! 菜单双列显示（表情/符号/剪切板/快捷发送）。
//! 渲染由 `iced_view` 承担（iced 离屏绘制），本模块不涉及绘制。
/// 菜单按钮区域宽度。
pub const MENU_BUTTON_WIDTH: u32 = 36;

/// 候选栏高度（与 daemon 一致）。
pub const CANDIDATE_HEIGHT: u32 = 36;

/// 菜单行高。
pub const MENU_ITEM_HEIGHT: u32 = 44;

/// 菜单列数。
pub const MENU_COLUMNS: usize = 2;

/// 内容面板（表情/符号网格）列数上限。
pub const CONTENT_COLUMNS_MAX: usize = 10;

/// 内容面板网格行数（固定，空位留白）。
pub const CONTENT_ROWS: usize = 3;

/// 内容面板单元格边长下限。
pub const CONTENT_ITEM_SIZE: u32 = 36;

/// 内容面板网格间距。
pub const CONTENT_GAP: u32 = 6;

/// 内容面板最大期望宽度（超出则减少列数）。
pub const CONTENT_MAX_WIDTH: u32 = 660;

/// 文本渲染宽度估算（16px 字号），用于内容网格单元格定宽。
///
/// 组合标记/变体选择符/键帽等零宽字符按 0 计；估算偏保守（偏大），
/// 避免单元格不够宽导致换行。
pub fn content_text_width(text: &str) -> u32 {
    text.chars()
        .map(|c| match c {
            '\u{0300}'..='\u{036F}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{20E3}'
            | '\u{1F3FB}'..='\u{1F3FF}' => 0,
            c if c.is_ascii() => 10,
            _ => 17,
        })
        .sum()
}

/// 内容网格单元格边长：最宽项 + 内边距，下限 CONTENT_ITEM_SIZE。
pub fn content_cell_width(widest: u32) -> u32 {
    (widest + 16).max(CONTENT_ITEM_SIZE)
}

/// 内容网格列数：在 CONTENT_MAX_WIDTH 内尽量多列（4..=10）。
pub fn content_columns_for(cell_width: u32) -> usize {
    let cols = (CONTENT_MAX_WIDTH + CONTENT_GAP) / (cell_width + CONTENT_GAP);
    cols.clamp(4, CONTENT_COLUMNS_MAX as u32) as usize
}

/// 内容面板渲染宽度。
pub fn content_panel_width(cell_width: u32, columns: usize) -> u32 {
    columns as u32 * cell_width + (columns as u32 - 1) * CONTENT_GAP
}

/// 内容面板网格项（表情/符号）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridItem {
    pub text: String,
    pub comment: String,
}

/// 面板路由视图：候选栏下方展开区的当前内容。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum PanelView {
    /// 未展开（仅候选栏）。
    #[default]
    Closed,
    /// 菜单网格（active_index 高亮）。
    Menu(Option<usize>),
    /// 内容网格（表情/符号），highlighted 为页内索引。
    Content {
        items: Vec<GridItem>,
        highlighted: Option<usize>,
    },
}

/// 展开区高度（视当前视图而定）。
pub fn panel_height_for(view: &PanelView) -> u32 {
    match view {
        PanelView::Closed => 0,
        PanelView::Menu(_) => menu_panel_height(),
        PanelView::Content { .. } => content_panel_height(),
    }
}

/// 内容面板每页容量（网格容量）。
pub fn content_capacity(columns: usize) -> usize {
    columns * CONTENT_ROWS
}

/// 内容面板总高度（固定行数网格 + 行间距）。
pub fn content_panel_height() -> u32 {
    CONTENT_ROWS as u32 * (CONTENT_ITEM_SIZE + CONTENT_GAP) - CONTENT_GAP
}

/// 菜单功能入口。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Emoji,
    Symbols,
    Clipboard,
    QuickSend,
}

impl MenuAction {
    pub fn label(self) -> &'static str {
        match self {
            MenuAction::Emoji => "表情",
            MenuAction::Symbols => "符号",
            MenuAction::Clipboard => "剪切板",
            MenuAction::QuickSend => "快捷发送",
        }
    }

    /// 网格中的行（0 起）。
    pub fn row(self) -> usize {
        self.index() / MENU_COLUMNS
    }

    /// 网格中的列（0 起）。
    pub fn col(self) -> usize {
        self.index() % MENU_COLUMNS
    }

    /// 面板内第几个入口（0 起）。
    pub fn index(self) -> usize {
        match self {
            MenuAction::Emoji => 0,
            MenuAction::Symbols => 1,
            MenuAction::Clipboard => 2,
            MenuAction::QuickSend => 3,
        }
    }

    /// 功能是否已实现（未实现的入口置灰，点击无效）。
    pub fn is_available(self) -> bool {
        matches!(self, MenuAction::Emoji | MenuAction::Symbols)
    }

    pub const ALL: [MenuAction; 4] = [
        MenuAction::Emoji,
        MenuAction::Symbols,
        MenuAction::Clipboard,
        MenuAction::QuickSend,
    ];
}

/// 菜单面板总高度（双列，2 行）。
pub fn menu_panel_height() -> u32 {
    MENU_ITEM_HEIGHT * (MenuAction::ALL.len() as u32 / MENU_COLUMNS as u32)
}

/// 展开后容器总高度（候选栏 + 菜单）。
pub fn expanded_height() -> u32 {
    CANDIDATE_HEIGHT + menu_panel_height()
}

/// 菜单按钮是否包含坐标（候选栏区域，surface 局部坐标）。
pub fn menu_button_hit(x: i32, y: i32, panel_width: u32) -> bool {
    let start = panel_width as i32 - MENU_BUTTON_WIDTH as i32;
    x >= start && x < panel_width as i32 && y >= 0 && y < CANDIDATE_HEIGHT as i32
}

/// 菜单面板中某坐标命中的入口。
///
/// 面板在候选栏下方展开：候选栏 y ∈ [0, 36)，面板 y ∈ [36, 36+panel_height)。
/// 面板为双列网格，单元格宽 = 容器宽/2，行高 = MENU_ITEM_HEIGHT。
pub fn menu_item_hit(x: i32, y: i32, container_width: u32) -> Option<MenuAction> {
    let panel_start = CANDIDATE_HEIGHT as i32;
    let panel_end = panel_start + menu_panel_height() as i32;
    if y < panel_start || y >= panel_end {
        return None;
    }
    if x < 0 || x >= container_width as i32 {
        return None;
    }
    let row = (y - panel_start) as usize / MENU_ITEM_HEIGHT as usize;
    let col = x as usize * MENU_COLUMNS / container_width as usize;
    let idx = row * MENU_COLUMNS + col;
    MenuAction::ALL.get(idx).copied()
}

/// 内容面板网格中某坐标命中的项（页内索引）。
///
/// 网格在候选栏下方展开：y ∈ [36, 36+content_panel_height)，固定 CONTENT_ROWS 行、
/// 指定列数，单元格宽 = 容器宽/列数，行高 = CONTENT_ITEM_SIZE + CONTENT_GAP。
pub fn content_item_hit(
    x: i32,
    y: i32,
    panel_width: u32,
    columns: usize,
    item_count: usize,
) -> Option<usize> {
    let panel_start = CANDIDATE_HEIGHT as i32;
    let panel_end = panel_start + content_panel_height() as i32;
    if y < panel_start || y >= panel_end {
        return None;
    }
    if x < 0 || x >= panel_width as i32 {
        return None;
    }
    let row = (y - panel_start) as usize / (CONTENT_ITEM_SIZE + CONTENT_GAP) as usize;
    let col = x as usize * columns / panel_width as usize;
    let idx = row * columns + col;
    (idx < item_count).then_some(idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_text_width() {
        // 单 CJK/符号字符
        assert_eq!(content_text_width("。"), 17);
        // ASCII 字符较窄
        assert_eq!(content_text_width("abc"), 30);
        // 组合序列（如 0️⃣）零宽字符不计
        assert_eq!(content_text_width("0️⃣"), 10);
        // 空字符串
        assert_eq!(content_text_width(""), 0);
    }

    #[test]
    fn test_content_grid_sizing() {
        // 窄内容：单元格取下限 36，10 列，宽 414
        let cell = content_cell_width(content_text_width("。"));
        assert_eq!(cell, 36);
        let cols = content_columns_for(cell);
        assert_eq!(cols, 10);
        assert_eq!(content_panel_width(cell, cols), 414);
        assert_eq!(content_capacity(cols), 30);

        // 颜文字（如 (ﾟ∀ﾟ)）：单元格加宽、列数减少、宽度增大
        let kaomoji = "(ﾟ∀ﾟ)";
        let cell = content_cell_width(content_text_width(kaomoji));
        assert!(cell >= 36);
        let cols = content_columns_for(cell);
        assert!(cols < 10, "wide items should reduce columns");
        assert!(content_panel_width(cell, cols) <= CONTENT_MAX_WIDTH + cell);
        assert!(content_panel_width(cell, cols) > 414);

        // 列数下限 4
        let cols = content_columns_for(200);
        assert_eq!(cols, 4);
    }

    #[test]
    fn test_content_panel_geometry() {
        // 3 行 × (36+6) - 6 = 120
        assert_eq!(content_panel_height(), 120);
    }

    #[test]
    fn test_content_item_hit() {
        let w = 414u32;
        let cols = 10usize;
        // 第 1 行第 1 列
        assert_eq!(content_item_hit(0, 36, w, cols, 30), Some(0));
        // 第 1 行第 10 列
        assert_eq!(content_item_hit(w as i32 - 1, 36, w, cols, 30), Some(9));
        // 第 2 行（y = 36 + 42）
        assert_eq!(content_item_hit(0, 78, w, cols, 30), Some(10));
        // 第 3 行（y = 36 + 2*42 = 120）
        assert_eq!(content_item_hit(0, 120, w, cols, 30), Some(20));
        assert_eq!(content_item_hit(0, 155, w, cols, 30), Some(20));
        // 超出网格范围
        assert_eq!(content_item_hit(0, 35, w, cols, 30), None);
        assert_eq!(content_item_hit(0, 36 + 120, w, cols, 30), None);
        assert_eq!(content_item_hit(-1, 36, w, cols, 30), None);
        // 超出实际项数
        assert_eq!(content_item_hit(0, 36, w, cols, 0), None);
        assert_eq!(content_item_hit(0, 36, w, cols, 5), Some(0));
        assert_eq!(content_item_hit(w as i32 - 1, 36, w, cols, 5), None);
    }

    #[test]
    fn test_panel_height_for() {
        assert_eq!(panel_height_for(&PanelView::Closed), 0);
        assert_eq!(
            panel_height_for(&PanelView::Menu(None)),
            menu_panel_height()
        );
        assert_eq!(
            panel_height_for(&PanelView::Content {
                items: vec![],
                highlighted: None,
            }),
            content_panel_height()
        );
    }
}
