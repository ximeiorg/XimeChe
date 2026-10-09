//! 候选栏菜单面板的纯逻辑：页面路由、几何、命中测试、列表/网格数据结构。
//!
//! 完全对齐 XimeYao（Windows 版）候选栏面板的版式与交互（其 `ui/panel.rs`）：
//! - **菜单页**：2 列 × 3 行 6 张卡片（📋 剪切板 / 🚀 快捷发送 / 😀 表情 /
//!   🔣 符号 / 🎙️ 语音输入 / ⚙️ 设置）+ 底部品牌条；设置与语音不是页面——
//!   点了就执行/提示，不开子页；
//! - **列表子页**（剪切板 / 快捷发送）：标题栏（← 菜单 + 标题）+ 6 行条目 +
//!   底部翻页条，空态有主文案（快捷发送附设置程序入口提示）；
//! - **网格子页**（表情 / 符号）：标题栏 + 8 列 × 4 行网格 +（符号页）翻页条 +
//!   **底部分类标签栏**（表情 1 行、符号 2 行 9+9）；「最近使用」是第 0 个标签；
//! - 绘制与命中调同一套几何纯函数（画得出来必须点得到）。
//!
//! 渲染由 `iced_view` 承担（iced 离屏绘制），本模块不涉及绘制。

// ── 菜单按钮（候选栏最右侧固定区域）──────────────────────────────

/// 菜单按钮区域宽度。
pub const MENU_BUTTON_WIDTH: u32 = 36;

/// 候选栏左右水平内边距（iced_view 的 candidate_bar 容器 padding [0, 12]）。
/// 菜单按钮贴内边距右缘，命中测试与绘制必须共用本常量。
pub const CANDIDATE_BAR_H_PADDING: u32 = 12;

/// 候选栏高度（与 daemon 一致）。
pub const CANDIDATE_HEIGHT: u32 = 36;

// ── 面板几何常量（1:1 对齐 XimeYao panel.rs 的 DIP 数值）─────────

/// 菜单按钮是否包含坐标（候选栏区域，surface 局部坐标）。
///
/// 渲染端按钮贴 buffer 右缘、容器带 [0, 12] 水平内边距，实际绘制区间是
/// `[W-12-36, W-12)`——命中区间必须与此一致（画得出来必须点得到）。
pub fn menu_button_hit(x: i32, y: i32, panel_width: u32, bar_height: u32) -> bool {
    let end = panel_width as i32 - CANDIDATE_BAR_H_PADDING as i32;
    let start = end - MENU_BUTTON_WIDTH as i32;
    x >= start && x < end && y >= 0 && y < bar_height as i32
}

/// 面板标题栏高（列表/网格子页顶部：← 菜单 + 标题）。
pub const PANEL_HEADER_HEIGHT: u32 = 36;
/// 菜单卡片 / 列表行 / 翻页条行高。
pub const PANEL_ITEM_HEIGHT: u32 = 32;
/// 行距。
pub const PANEL_ROW_GAP: u32 = 4;
/// 内容块之间的间距。
pub const PANEL_CONTENT_GAP: u32 = 8;
/// 面板底边距。
pub const PANEL_BOTTOM_MARGIN: u32 = 8;
/// 菜单卡片列数。
pub const PANEL_MENU_COLUMNS: usize = 2;
/// 菜单卡片列间距。
pub const PANEL_MENU_COL_GAP: u32 = 8;
/// 面板左右内边距。
pub const PANEL_H_INSET: u32 = 10;
/// 面板最小宽度（宽度跟随候选栏，但不小于此值）。
pub const PANEL_MIN_WIDTH: u32 = 320;
/// 「← 菜单」返回按钮尺寸。
pub const PANEL_BACK_WIDTH: u32 = 64;
pub const PANEL_BACK_HEIGHT: u32 = 24;
/// 菜单页顶部留白（菜单页无标题栏，内容直接从顶部开始）。
pub const PANEL_MENU_TOP: u32 = 10;
/// 面板区与候选栏之间的间距（对齐 XimeYao PANEL_GAP：候选栏之下先空 4px，
/// 面板底色块才起画；命中坐标系同步下移，见 daemon 的指针事件换算）。
pub const PANEL_GAP: u32 = 4;
/// 面板底色（对齐 XimeYao 硬编码的浅灰 (0.92,0.92,0.94)@0.96；
/// 暗色主题用对应深灰，保持同样的层级感）。
pub const PANEL_SURFACE_BG_LIGHT: [u8; 3] = [235, 235, 240];
pub const PANEL_SURFACE_BG_DARK: [u8; 3] = [38, 38, 42];

// ── 列表子页（剪切板 / 快捷发送）────────────────────────────────

/// 列表每页行数。
pub const LIST_ROWS_PER_PAGE: usize = 6;
/// 行内文本展示上限（字符数，超出截断加 …）。
pub const LIST_DISPLAY_MAX_CHARS: usize = 40;
/// 翻页条行高（= PANEL_ITEM_HEIGHT）。
pub const LIST_FOOTER_HEIGHT: u32 = PANEL_ITEM_HEIGHT;
/// 翻页按钮宽 / 高 / 页码标签宽。
pub const LIST_PAGE_BUTTON_WIDTH: u32 = 60;
pub const LIST_PAGE_BUTTON_HEIGHT: u32 = 24;
pub const LIST_PAGE_LABEL_WIDTH: u32 = 72;
/// 快捷发送页编码列宽 / 与正文的间距。
pub const QUICK_SEND_CODE_COL_WIDTH: u32 = 64;
pub const QUICK_SEND_CODE_COL_GAP: u32 = 8;

/// 列表子页面板高度：标题栏 + 间距 + 6 行条目 + 间距 + 翻页条 + 底边距。
pub fn list_panel_height() -> u32 {
    PANEL_HEADER_HEIGHT
        + PANEL_CONTENT_GAP
        + LIST_ROWS_PER_PAGE as u32 * (PANEL_ITEM_HEIGHT + PANEL_ROW_GAP)
        - PANEL_ROW_GAP
        + PANEL_CONTENT_GAP
        + LIST_FOOTER_HEIGHT
        + PANEL_BOTTOM_MARGIN
}

/// 列表子页第 i 行的 y（面板内坐标）。
pub fn list_row_y(i: usize) -> u32 {
    PANEL_HEADER_HEIGHT + PANEL_CONTENT_GAP + i as u32 * (PANEL_ITEM_HEIGHT + PANEL_ROW_GAP)
}

/// 列表子页翻页条的 y（行区之下空一个内容间距；绘制与命中同源）。
pub fn list_footer_y() -> u32 {
    list_row_y(LIST_ROWS_PER_PAGE) - PANEL_ROW_GAP + PANEL_CONTENT_GAP
}

// ── 网格子页（表情 / 符号）──────────────────────────────────────

/// 网格每行 8 格（安卓 `EmojiData.layoutColumns = 8`）。
pub const GRID_PER_ROW: usize = 8;
/// 网格固定 4 行：一页 32 格。
pub const GRID_ROWS: usize = 4;
/// 一页容量（也是「最近使用」上限）。
pub const GRID_PER_PAGE: usize = GRID_PER_ROW * GRID_ROWS;
/// 标签栏一行放几个标签（符号页 18 个标签 → 两行 9 + 9）。
pub const GRID_TABS_PER_ROW: usize = 9;
/// 标签行高 / 行距。
pub const GRID_TAB_HEIGHT: u32 = 26;
pub const GRID_TAB_GAP: u32 = 4;
/// 格子高 / 格间距 / 格宽下限。
pub const GRID_CELL_HEIGHT: u32 = 36;
pub const GRID_CELL_GAP: u32 = 4;
pub const GRID_CELL_WIDTH_MIN: u32 = 26;
/// 「最近使用」标签文案。
pub const RECENT_LABEL: &str = "最近";
/// 「最近使用」为空时的提示。
pub const RECENT_EMPTY_TEXT: &str = "暂无最近使用";
/// 内置分类为空时的兜底提示（内置表有单测保证非空，这里只是绘制兜底）。
pub const EMPTY_TEXT: &str = "暂无内容";
/// 网格页网格区高度（4 行格子 + 行间距）。
pub const GRID_HEIGHT: u32 = GRID_ROWS as u32 * (GRID_CELL_HEIGHT + GRID_CELL_GAP) - GRID_CELL_GAP;

/// 网格顶边（面板内坐标）：标题栏下空一个内容间距。
pub fn grid_top() -> u32 {
    PANEL_HEADER_HEIGHT + PANEL_CONTENT_GAP
}

/// 标签栏需要几行（1 = 表情 7 标签；2 = 符号 18 标签）。
pub fn grid_tab_rows(tab_count: usize) -> usize {
    tab_count.div_ceil(GRID_TABS_PER_ROW)
}

/// 标签栏总高。
pub fn grid_tab_height_total(tab_count: usize) -> u32 {
    let rows = grid_tab_rows(tab_count) as u32;
    rows * GRID_TAB_HEIGHT + rows.saturating_sub(1) * GRID_TAB_GAP
}

/// 网格页翻页条的 y（只有需要翻页的页才有这一行）。
pub fn grid_footer_y() -> u32 {
    grid_top() + GRID_HEIGHT + PANEL_CONTENT_GAP
}

/// 标签栏顶边：翻页条（若有）之下。
pub fn grid_tab_top(has_pager: bool) -> u32 {
    let above = if has_pager {
        grid_footer_y() + LIST_FOOTER_HEIGHT
    } else {
        grid_top() + GRID_HEIGHT
    };
    above + PANEL_CONTENT_GAP
}

/// 网格页面板高度：标题栏 + 间距 + 网格 +（翻页条 + 间距）+ 标签栏 + 底边距。
pub fn grid_panel_height(tab_count: usize, has_pager: bool) -> u32 {
    grid_tab_top(has_pager) + grid_tab_height_total(tab_count) + PANEL_BOTTOM_MARGIN
}

/// 格子列宽：8 列 + 格间距铺满面板可用宽度（不封顶，宽面板不留大块空白）。
pub fn grid_cell_width(panel_width: u32) -> u32 {
    let usable = panel_width.saturating_sub(2 * PANEL_H_INSET);
    ((usable - (GRID_PER_ROW as u32 - 1) * GRID_CELL_GAP) / GRID_PER_ROW as u32)
        .max(GRID_CELL_WIDTH_MIN)
}

/// 标签栏单栏宽：单行时按标签数均分，多行时固定每行 GRID_TABS_PER_ROW 个。
pub fn grid_tab_width(tab_count: usize, panel_width: u32) -> u32 {
    let usable = panel_width.saturating_sub(2 * PANEL_H_INSET).max(1);
    let columns = if grid_tab_rows(tab_count) == 1 {
        tab_count.max(1)
    } else {
        GRID_TABS_PER_ROW
    };
    usable / columns as u32
}

// ── 页面路由 ────────────────────────────────────────────────────

/// 面板页面（候选栏下方面板承载的页面，菜单页为默认首页）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelPage {
    /// 菜单（默认页，2 列功能入口）。
    #[default]
    Menu,
    /// 剪切板。
    Clipboard,
    /// 快捷发送。
    QuickSend,
    /// 表情。
    Emoji,
    /// 符号。
    Symbol,
}

impl PanelPage {
    /// 子页标题（菜单页无标题栏，不使用）。
    pub fn title(self) -> &'static str {
        match self {
            Self::Menu => "菜单",
            Self::Clipboard => "剪切板",
            Self::QuickSend => "快捷发送",
            Self::Emoji => "表情",
            Self::Symbol => "符号",
        }
    }

    /// 是否为「条目列表」子页（进入时读一次数据，绘制只读内存）。
    pub fn is_list_page(self) -> bool {
        matches!(self, Self::Clipboard | Self::QuickSend)
    }

    /// 是否为「网格」子页。
    pub fn is_grid_page(self) -> bool {
        matches!(self, Self::Emoji | Self::Symbol)
    }

    /// 列表子页空态主文案。
    pub fn list_empty_text(self) -> &'static str {
        match self {
            Self::Clipboard => "暂无剪贴板记录",
            Self::QuickSend => "暂无快捷发送内容",
            _ => "暂无内容",
        }
    }

    /// 列表子页空态补充说明（可选）。
    pub fn list_empty_hint(self) -> Option<&'static str> {
        match self {
            // 快捷发送的录入入口在设置程序（面板只负责消费与上屏）。
            Self::QuickSend => Some("在设置程序的「剪贴板 → 快捷发送」里添加短语"),
            _ => None,
        }
    }
}

// ── 菜单卡片（菜单页 2 列 × 3 行 6 张）──────────────────────────

/// 菜单页功能 id（点击卡片后由 daemon 路由：页面 / 动作）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuCard {
    Clipboard,
    QuickSend,
    Emoji,
    Symbol,
    /// 语音输入：卡片占位（引擎未接，点击提示暂未开放）。
    VoiceInput,
    /// 打开设置程序（动作，不是页面）。
    Settings,
}

impl MenuCard {
    pub fn icon(self) -> &'static str {
        match self {
            Self::Clipboard => "📋",
            Self::QuickSend => "🚀",
            Self::Emoji => "😀",
            Self::Symbol => "🔣",
            Self::VoiceInput => "🎙️",
            Self::Settings => "⚙️",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Clipboard => "剪切板",
            Self::QuickSend => "快捷发送",
            Self::Emoji => "表情",
            Self::Symbol => "符号",
            Self::VoiceInput => "语音输入",
            Self::Settings => "设置",
        }
    }

    /// 菜单页第 i 张卡片（行优先，2 列）。
    pub fn at(index: usize) -> Option<Self> {
        Some(match index {
            0 => Self::Clipboard,
            1 => Self::QuickSend,
            2 => Self::Emoji,
            3 => Self::Symbol,
            4 => Self::VoiceInput,
            5 => Self::Settings,
            _ => return None,
        })
    }

    pub const ALL: [MenuCard; 6] = [
        Self::Clipboard,
        Self::QuickSend,
        Self::Emoji,
        Self::Symbol,
        Self::VoiceInput,
        Self::Settings,
    ];
}

/// 菜单卡片矩形（面板内坐标，行优先 2 列）。
pub fn menu_card_rect(index: usize, panel_width: u32) -> (u32, u32, u32, u32) {
    let col_w = (panel_width.saturating_sub(2 * PANEL_H_INSET)
        - (PANEL_MENU_COLUMNS as u32 - 1) * PANEL_MENU_COL_GAP)
        / PANEL_MENU_COLUMNS as u32;
    let col = index % PANEL_MENU_COLUMNS;
    let row = index / PANEL_MENU_COLUMNS;
    let x = PANEL_H_INSET + col as u32 * (col_w + PANEL_MENU_COL_GAP);
    let y = PANEL_MENU_TOP + row as u32 * (PANEL_ITEM_HEIGHT + PANEL_ROW_GAP);
    (x, y, x + col_w, y + PANEL_ITEM_HEIGHT)
}

/// 菜单页总高：3 行卡片 + 间距 + 顶部留白 + 间距 + 底部品牌条 + 底边距。
pub fn menu_panel_height() -> u32 {
    let rows = (MenuCard::ALL.len() as u32).div_ceil(PANEL_MENU_COLUMNS as u32);
    PANEL_MENU_TOP + rows * (PANEL_ITEM_HEIGHT + PANEL_ROW_GAP) - PANEL_ROW_GAP
        + PANEL_CONTENT_GAP
        + PANEL_ITEM_HEIGHT
        + PANEL_BOTTOM_MARGIN
}

/// 「← 菜单」返回按钮矩形（子页标题栏内，面板内坐标）。
pub fn panel_back_rect() -> (u32, u32, u32, u32) {
    let y = (PANEL_HEADER_HEIGHT - PANEL_BACK_HEIGHT) / 2;
    (
        PANEL_H_INSET,
        y,
        PANEL_H_INSET + PANEL_BACK_WIDTH,
        y + PANEL_BACK_HEIGHT,
    )
}

// ── 列表子页数据 ────────────────────────────────────────────────

/// 列表子页的一行：全文 + 触发编码（剪切板历史无编码，快捷发送条目可带编码）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PanelListItem {
    /// 条目全文（上屏/复制用的是它，展示时截断）。
    pub text: String,
    /// 触发编码（快捷发送专有；剪切板历史恒为空串）。
    pub code: String,
}

/// 列表子页数据：进入该页时读一次（clipboard.db），绘制只读内存。
#[derive(Debug, Clone, Default)]
pub struct PanelList {
    /// 数据来源页面。
    pub source: PanelPage,
    /// 条目（最新在前；快捷发送为置顶优先）。
    pub items: Vec<PanelListItem>,
    /// 当前页码（0 起）。
    pub page: usize,
}

impl PanelList {
    /// 总页数（无条目时按 1 页显示空态）。
    pub fn page_count(&self) -> usize {
        if self.items.is_empty() {
            1
        } else {
            self.items.len().div_ceil(LIST_ROWS_PER_PAGE)
        }
    }

    /// 夹回范围内的当前页。
    pub fn clamped_page(&self) -> usize {
        self.page.min(self.page_count().saturating_sub(1))
    }

    /// 当前页可见行数。
    pub fn rows_on_page(&self) -> usize {
        let start = self.clamped_page() * LIST_ROWS_PER_PAGE;
        self.items
            .len()
            .saturating_sub(start)
            .min(LIST_ROWS_PER_PAGE)
    }

    /// 当前页第 row 行的条目。
    pub fn item_at(&self, row: usize) -> Option<&PanelListItem> {
        self.items
            .get(self.clamped_page() * LIST_ROWS_PER_PAGE + row)
    }

    /// 是否有条目带触发编码（决定快捷发送页是否留出编码列）。
    pub fn has_codes(&self) -> bool {
        self.items.iter().any(|item| !item.code.is_empty())
    }

    pub fn has_prev_page(&self) -> bool {
        self.clamped_page() > 0
    }

    pub fn has_next_page(&self) -> bool {
        self.clamped_page() + 1 < self.page_count()
    }

    /// 翻页；已在首/末页返回 false。
    pub fn prev_page(&mut self) -> bool {
        if self.has_prev_page() {
            self.page = self.clamped_page() - 1;
            true
        } else {
            false
        }
    }

    pub fn next_page(&mut self) -> bool {
        if self.has_next_page() {
            self.page += 1;
            true
        } else {
            false
        }
    }
}

// ── 网格子页数据 ────────────────────────────────────────────────

/// 网格子页（表情 / 符号）的状态：当前标签、当前页、最近使用记录。
/// 进入该页时读一次（recent_usage.json + 分类表），绘帧只读内存。
#[derive(Debug, Clone, Default)]
pub struct PanelGrid {
    /// 数据来源页面。
    pub source: PanelPage,
    /// 当前标签（0 = 最近使用）。
    pub tab: usize,
    /// 当前页（0 起；在当前标签内翻页）。
    pub page: usize,
    /// 最近使用记录（进入页面时读一次，点一次更新一次）。
    pub recent: Vec<String>,
    /// 当前页的格子内容（进页/翻页时由 daemon 按分类数据填好；None = 空槽）。
    pub cells: Vec<Option<String>>,
    /// 当前标签的条目总数（翻页条「共 N 个」）。
    pub item_count: usize,
    /// 标签文案表（含第 0 个「最近」；daemon 进页时按数据源填好）。
    pub tabs: Vec<String>,
    /// 该页是否需要翻页条（符号页是；表情每类一页则否）。
    pub has_pager: bool,
}

impl PanelGrid {
    /// 数据来源（非网格页为 None）。
    pub fn kind(&self) -> Option<PanelPage> {
        self.source.is_grid_page().then_some(self.source)
    }

    /// 这份数据是否属于当前页面。
    pub fn is_live(&self, page: PanelPage) -> bool {
        self.source == page
    }

    /// 标签总数。
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// 夹回范围内的当前标签。
    pub fn clamped_tab(&self) -> usize {
        self.tab.min(self.tab_count().saturating_sub(1))
    }

    /// 当前标签的页数（空标签也画一页空态）。
    pub fn page_count(&self) -> usize {
        if self.item_count == 0 {
            1
        } else {
            self.item_count.div_ceil(GRID_PER_PAGE)
        }
    }

    /// 夹回范围内的当前页。
    pub fn clamped_page(&self) -> usize {
        self.page.min(self.page_count().saturating_sub(1))
    }

    /// 当前页有内容的格子数。
    pub fn items_on_page(&self) -> usize {
        self.cells.iter().filter(|c| c.is_some()).count()
    }

    /// 当前页第 `slot` 格的字形（空槽 / 越界为 None：绘制与命中共用）。
    pub fn item_at(&self, slot: usize) -> Option<&str> {
        self.cells.get(slot).and_then(|c| c.as_deref())
    }

    /// 切到第 `tab` 个分类标签（回到该类第一页）；重复点当前标签不动作。
    pub fn select_tab(&mut self, tab: usize) -> bool {
        if tab < self.tab_count() && tab != self.tab {
            self.tab = tab;
            self.page = 0;
            true
        } else {
            false
        }
    }

    pub fn has_prev_page(&self) -> bool {
        self.clamped_page() > 0
    }

    pub fn has_next_page(&self) -> bool {
        self.clamped_page() + 1 < self.page_count()
    }

    pub fn prev_page(&mut self) -> bool {
        if self.has_prev_page() {
            self.page = self.clamped_page() - 1;
            true
        } else {
            false
        }
    }

    pub fn next_page(&mut self) -> bool {
        if self.has_next_page() {
            self.page += 1;
            true
        } else {
            false
        }
    }
}

// ── 命中测试（绘制与命中同一几何）───────────────────────────────

fn rect_contains(r: (u32, u32, u32, u32), x: u32, y: u32) -> bool {
    x >= r.0 && x < r.2 && y >= r.1 && y < r.3
}

/// 面板内命中的交互元素。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelHit {
    /// 命中菜单页第 i 张卡片。
    MenuItem(MenuCard),
    /// 命中 "← 菜单" 返回按钮。
    Back,
    /// 命中列表子页当前页第 i 行。
    ListItem(usize),
    /// 命中网格子页第 i 个分类标签。
    GlyphTab(usize),
    /// 命中网格子页当前页第 i 格（格内下标，行优先）。
    GlyphCell(usize),
    /// 命中「上一页」（列表与网格子页共用底部翻页条）。
    PrevPage,
    /// 命中「下一页」。
    NextPage,
}

/// 翻页条两个按钮的矩形（页脚行内，右侧对齐：下一页在最右，上一页在其左）。
/// `index`：0 = 下一页、1 = 上一页（与 XimeYao footer_page_button_rect 同序）。
pub fn pager_button_rect(panel_width: u32, footer_y: u32, index: u32) -> (u32, u32, u32, u32) {
    let next_x = panel_width.saturating_sub(PANEL_H_INSET + LIST_PAGE_BUTTON_WIDTH);
    let prev_x = next_x.saturating_sub(LIST_PAGE_BUTTON_WIDTH + 8);
    let x = if index == 0 { next_x } else { prev_x };
    let y = footer_y + (LIST_FOOTER_HEIGHT - LIST_PAGE_BUTTON_HEIGHT) / 2;
    (
        x,
        y,
        x + LIST_PAGE_BUTTON_WIDTH,
        y + LIST_PAGE_BUTTON_HEIGHT,
    )
}

/// 翻页条页码标签矩形（两按钮之间）。
pub fn pager_label_rect(panel_width: u32, footer_y: u32) -> (u32, u32, u32, u32) {
    let (_, _, next_x, _) = pager_button_rect(panel_width, footer_y, 0);
    let (prev_x, _, _, _) = pager_button_rect(panel_width, footer_y, 1);
    let y = footer_y + (LIST_FOOTER_HEIGHT - LIST_PAGE_BUTTON_HEIGHT) / 2;
    (
        prev_x + LIST_PAGE_BUTTON_WIDTH,
        y,
        next_x,
        y + LIST_PAGE_BUTTON_HEIGHT,
    )
}

/// 面板命中测试（绘制几何与点击共用的唯一布局来源）。
/// `x`/`y` 为面板内坐标（相对面板左上角）；候选栏上的坐标由调用方先行扣除。
pub fn panel_hit(
    page: PanelPage,
    panel_width: u32,
    list: &PanelList,
    grid: &PanelGrid,
    x: u32,
    y: u32,
) -> Option<PanelHit> {
    if x >= panel_width {
        return None;
    }
    match page {
        PanelPage::Menu => MenuCard::ALL
            .iter()
            .enumerate()
            .find(|(i, _)| {
                let (x0, y0, x1, y1) = menu_card_rect(*i, panel_width);
                rect_contains((x0, y0, x1, y1), x, y)
            })
            .map(|(i, card)| {
                let _ = i;
                PanelHit::MenuItem(*card)
            }),
        page if page.is_grid_page() => {
            // 返回按钮先判：它和网格在纵向不重叠，但先判更省事、意图也更清楚。
            if !grid.is_live(page) {
                return None;
            }
            let (bx0, by0, bx1, by1) = panel_back_rect();
            if rect_contains((bx0, by0, bx1, by1), x, y) {
                return Some(PanelHit::Back);
            }
            let live = grid.is_live(page);
            let tabs = if live { grid.tab_count() } else { 0 };
            let has_pager = live && grid.has_pager;
            for i in 0..tabs {
                let width = grid_tab_width(tabs, panel_width);
                let row = i / GRID_TABS_PER_ROW;
                let col = i % GRID_TABS_PER_ROW;
                let left = PANEL_H_INSET + col as u32 * width;
                let top = grid_tab_top(has_pager) + row as u32 * (GRID_TAB_HEIGHT + GRID_TAB_GAP);
                if rect_contains((left, top, left + width, top + GRID_TAB_HEIGHT), x, y) {
                    return Some(PanelHit::GlyphTab(i));
                }
            }
            let items = if live { grid.items_on_page() } else { 0 };
            let cell_width = grid_cell_width(panel_width);
            for i in 0..items {
                let col = i % GRID_PER_ROW;
                let row = i / GRID_PER_ROW;
                let left = PANEL_H_INSET + col as u32 * (cell_width + GRID_CELL_GAP);
                let top = grid_top() + row as u32 * (GRID_CELL_HEIGHT + GRID_CELL_GAP);
                if rect_contains((left, top, left + cell_width, top + GRID_CELL_HEIGHT), x, y) {
                    return Some(PanelHit::GlyphCell(i));
                }
            }
            if live && has_pager && grid.page_count() > 1 {
                let footer_y = grid_footer_y();
                if grid.has_prev_page()
                    && rect_contains(pager_button_rect(panel_width, footer_y, 1), x, y)
                {
                    return Some(PanelHit::PrevPage);
                }
                if grid.has_next_page()
                    && rect_contains(pager_button_rect(panel_width, footer_y, 0), x, y)
                {
                    return Some(PanelHit::NextPage);
                }
            }
            None
        }
        page if page.is_list_page() => {
            let rows = if list.source == page {
                list.rows_on_page()
            } else {
                0
            };
            for row in 0..rows {
                let y0 = list_row_y(row);
                if rect_contains((0, y0, panel_width, y0 + PANEL_ITEM_HEIGHT), x, y) {
                    return Some(PanelHit::ListItem(row));
                }
            }
            let live = list.source == page;
            if live && list.page_count() > 1 {
                let footer_y = list_row_y(LIST_ROWS_PER_PAGE) + PANEL_CONTENT_GAP;
                if list.has_prev_page()
                    && rect_contains(pager_button_rect(panel_width, footer_y, 1), x, y)
                {
                    return Some(PanelHit::PrevPage);
                }
                if list.has_next_page()
                    && rect_contains(pager_button_rect(panel_width, footer_y, 0), x, y)
                {
                    return Some(PanelHit::NextPage);
                }
            }
            // 子页空白区域命中「← 菜单」之外都算无操作（对齐 XimeYao）。
            let (bx0, by0, bx1, by1) = panel_back_rect();
            if rect_contains((bx0, by0, bx1, by1), x, y) {
                return Some(PanelHit::Back);
            }
            None
        }
        _ => None,
    }
}

/// 文本渲染宽度估算（16px 字号），用于列表行截断（保留给渲染层用）。
pub fn text_estimate_width(text: &str) -> u32 {
    text.chars()
        .map(|c| if c.is_ascii() { 8 } else { 16 })
        .sum()
}

/// 截断到展示宽度（超出加 …；按字符数上限先截一次，再按宽度兜底）。
pub fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 菜单按钮命中区间 = 绘制区间：buffer 右缘内缩 [0,12] 容器内边距，
    /// 再向左取 36px 按钮宽。2026-10-07 事故：命中公式曾假设 [W-36, W)，
    /// 实际画在 [W-48, W-12)——右 1/3 永远点不中，窄栏（W 钳到 320 而命中
    /// 传未钳制宽）时整个按钮不可点。
    #[test]
    fn menu_button_hit_matches_drawn_geometry() {
        let w = 320u32;
        let bar = CANDIDATE_HEIGHT;
        // 绘制区间 [W-48, W-12) = [272, 308)
        assert!(!menu_button_hit(271, 10, w, bar));
        assert!(menu_button_hit(272, 10, w, bar));
        assert!(menu_button_hit(307, 10, w, bar));
        assert!(!menu_button_hit(308, 10, w, bar));
        assert!(!menu_button_hit(319, 10, w, bar), "右内边距不是按钮");
        // 纵向：只认候选栏高度以内
        assert!(menu_button_hit(280, 0, w, bar));
        assert!(!menu_button_hit(280, bar as i32, w, bar));
    }

    /// 绘制堆叠必须与命中公式同源（2026-10-03 事故：iced 流式布局累积高度
    /// 偏离命中公式 ~15px，表情/符号页标签全部点不中）。
    #[test]
    fn list_layout_stacking_matches_hit_geometry() {
        // 绘制顺序：header → divider(1) → 空余间距(GAP-1) → 行区 → GAP → 翻页条。
        // 行区 = 6 行 + 5 个行距（末行后没有行距，故扣回 ROW_GAP）。
        let stacked = PANEL_HEADER_HEIGHT
            + 1
            + (PANEL_CONTENT_GAP - 1)
            + (list_row_y(LIST_ROWS_PER_PAGE) - list_row_y(0) - PANEL_ROW_GAP)
            + PANEL_CONTENT_GAP
            + LIST_FOOTER_HEIGHT;
        assert_eq!(
            stacked + PANEL_BOTTOM_MARGIN,
            list_panel_height(),
            "列表页绘制堆叠高度必须等于命中公式的面板高度"
        );
    }

    #[test]
    fn grid_layout_stacking_matches_hit_geometry() {
        for (tabs, has_pager) in [(2usize, false), (18usize, true)] {
            let mut stacked =
                PANEL_HEADER_HEIGHT + 1 + (PANEL_CONTENT_GAP - 1) + GRID_HEIGHT + PANEL_CONTENT_GAP;
            if has_pager {
                stacked += LIST_FOOTER_HEIGHT + PANEL_CONTENT_GAP;
            }
            stacked += grid_tab_height_total(tabs);
            assert_eq!(
                stacked + PANEL_BOTTOM_MARGIN,
                grid_panel_height(tabs, has_pager),
                "网格页 tabs={tabs} pager={has_pager} 堆叠高度偏离命中公式"
            );
        }
    }

    #[test]
    fn menu_page_geometry_and_cards() {
        let h = menu_panel_height();
        // 3 行卡片 32 + 行距 4（末行扣回）+ 顶部 10 + 间距 8 + 品牌条 32 + 底 8。
        assert_eq!(h, 10 + 3 * 36 - 4 + 8 + 32 + 8);
        let (x0, y0, x1, _) = menu_card_rect(0, 320);
        assert_eq!((x0, y0), (10, 10));
        assert_eq!(x1 - x0, (320 - 20 - 8) / 2);
        let (x0, _, _, _) = menu_card_rect(1, 320);
        assert!(x0 > 150, "第 2 列在右侧");
        let (_, y0, _, _) = menu_card_rect(2, 320);
        assert_eq!(y0, 10 + 36, "第 2 行从首行下开始");
        assert_eq!(MenuCard::at(4), Some(MenuCard::VoiceInput));
        assert_eq!(MenuCard::at(6), None);
    }

    #[test]
    fn list_page_geometry_and_paging() {
        let mut list = PanelList {
            source: PanelPage::Clipboard,
            items: (0..8)
                .map(|i| PanelListItem {
                    text: format!("t{i}"),
                    code: String::new(),
                })
                .collect(),
            page: 0,
        };
        assert_eq!(list.page_count(), 2);
        assert_eq!(list.rows_on_page(), 6);
        assert!(list.has_next_page());
        assert!(!list.has_prev_page());
        assert!(list.next_page());
        assert_eq!(list.rows_on_page(), 2);
        assert!(list.has_prev_page() && !list.has_next_page());
        // 第 1 页第 0 行 = 第 7 个条目。
        assert_eq!(list.item_at(0).unwrap().text, "t6");
        assert!(list.prev_page());

        // 高度：标题 36 + 8 + 6*(32+4)-4 + 8 + 32 + 8。
        assert_eq!(list_panel_height(), 36 + 8 + 6 * 36 - 4 + 8 + 32 + 8);
    }

    #[test]
    fn panel_hit_menu_and_list() {
        let list = PanelList::default();
        let grid = PanelGrid::default();
        // 菜单页第 0 张卡片中心。
        let (x0, y0, x1, y1) = menu_card_rect(0, 320);
        assert_eq!(
            panel_hit(
                PanelPage::Menu,
                320,
                &list,
                &grid,
                (x0 + x1) / 2,
                (y0 + y1) / 2
            ),
            Some(PanelHit::MenuItem(MenuCard::Clipboard))
        );
        // 空列表页：行不可点，但返回按钮可点。
        let hits = panel_hit(PanelPage::Clipboard, 320, &list, &grid, 40, 24);
        assert_eq!(hits, Some(PanelHit::Back));
        assert_eq!(
            panel_hit(PanelPage::Clipboard, 320, &list, &grid, 200, 100),
            None
        );
    }

    #[test]
    fn panel_hit_list_rows_and_pager() {
        let mut list = PanelList {
            source: PanelPage::QuickSend,
            items: (0..8)
                .map(|i| PanelListItem {
                    text: format!("t{i}"),
                    code: format!("c{i}"),
                })
                .collect(),
            page: 0,
        };
        let grid = PanelGrid::default();
        // 第 0 行中心。
        let row_y = list_row_y(0) + PANEL_ITEM_HEIGHT / 2;
        assert_eq!(
            panel_hit(PanelPage::QuickSend, 320, &list, &grid, 160, row_y),
            Some(PanelHit::ListItem(0))
        );
        // 翻页：下一页按钮（首页有下一页）。
        let footer_y = list_row_y(LIST_ROWS_PER_PAGE) + PANEL_CONTENT_GAP;
        let (bx, by, _, by1) = pager_button_rect(320, footer_y, 0);
        assert_eq!(
            panel_hit(
                PanelPage::QuickSend,
                320,
                &list,
                &grid,
                bx + 5,
                by + (by1 - by) / 2
            ),
            Some(PanelHit::NextPage)
        );
        assert!(list.next_page());
        // 末页下一页不可命中。
        assert_eq!(
            panel_hit(
                PanelPage::QuickSend,
                320,
                &list,
                &grid,
                bx + 5,
                by + (by1 - by) / 2
            ),
            None
        );
        // 上一页可命中。
        let (px, py, _, py1) = pager_button_rect(320, footer_y, 1);
        assert_eq!(
            panel_hit(
                PanelPage::QuickSend,
                320,
                &list,
                &grid,
                px + 5,
                py + (py1 - py) / 2
            ),
            Some(PanelHit::PrevPage)
        );
    }

    #[test]
    fn grid_geometry_matches_ximeyao() {
        // 表情 7+1=8 个标签 → 1 行；符号 18+1=19 → 3 行？不：XimeYao 符号 18 标签
        // 两行 9+9，最近使用算进 tab_count → 19 → 3 行。对齐实现：标签数由
        // daemon 填（含最近使用），这里只验几何函数本身。
        assert_eq!(grid_tab_rows(8), 1);
        assert_eq!(grid_tab_rows(19), 3);
        assert_eq!(grid_tab_height_total(8), 26);
        assert_eq!(grid_tab_height_total(19), 26 * 3 + 4 * 2);
        // 格宽：320 - 20 内边距 - 7*4 间距 = 272 → 34/格。
        assert_eq!(grid_cell_width(320), 34);
        assert_eq!(grid_top(), 44);
        assert_eq!(grid_footer_y(), 44 + GRID_HEIGHT + 8);
    }

    #[test]
    fn grid_state_paging_and_clamps() {
        let mut grid = PanelGrid {
            source: PanelPage::Symbol,
            tab: 0,
            page: 5,
            recent: Vec::new(),
            cells: vec![],
            item_count: 40,
            tabs: vec!["最近".into()],
            has_pager: true,
        };
        assert_eq!(grid.page_count(), 2);
        assert_eq!(grid.clamped_page(), 1);
        assert!(grid.has_prev_page() && !grid.has_next_page());
        grid.page = 0;
        assert!(grid.next_page());
        assert_eq!(grid.page, 1);
        // 非本页数据 is_live=false → 命中落空。
        let list = PanelList::default();
        assert_eq!(
            panel_hit(PanelPage::Emoji, 320, &list, &grid, 160, 60),
            None
        );
    }
}
