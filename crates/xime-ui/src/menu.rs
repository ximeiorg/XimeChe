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

