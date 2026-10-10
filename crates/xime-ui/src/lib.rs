pub mod candidate;
pub mod iced_view;
pub mod menu;
pub mod theme;
pub mod voice;

pub use candidate::CandidateItem;
pub use candidate::CandidateList;
pub use candidate::MoveDirection;
pub use candidate::PageInfo;
pub use iced_view::IcedSurface;
pub use menu::{
    grid_cell_width, grid_panel_height, grid_tab_height_total, grid_tab_top, grid_tab_width,
    list_panel_height, list_row_y, menu_button_hit, menu_card_rect, menu_panel_height,
    pager_button_rect, pager_label_rect, panel_back_rect, panel_hit, truncate_text, PanelGrid,
    PanelHit, PanelList, PanelListItem, PanelPage, CANDIDATE_HEIGHT, EMPTY_TEXT, GRID_CELL_GAP,
    GRID_CELL_HEIGHT, GRID_CELL_WIDTH_MIN, GRID_PER_PAGE, GRID_PER_ROW, GRID_ROWS,
    GRID_TABS_PER_ROW, GRID_TAB_GAP, GRID_TAB_HEIGHT, LIST_DISPLAY_MAX_CHARS, LIST_FOOTER_HEIGHT,
    LIST_PAGE_BUTTON_HEIGHT, LIST_PAGE_BUTTON_WIDTH, LIST_PAGE_LABEL_WIDTH, LIST_ROWS_PER_PAGE,
    MENU_BUTTON_WIDTH, PANEL_BACK_HEIGHT, PANEL_BACK_WIDTH, PANEL_BOTTOM_MARGIN, PANEL_CONTENT_GAP,
    PANEL_HEADER_HEIGHT, PANEL_H_INSET, PANEL_ITEM_HEIGHT, PANEL_MENU_COLUMNS, PANEL_MENU_COL_GAP,
    PANEL_MENU_TOP, PANEL_MIN_WIDTH, PANEL_ROW_GAP, QUICK_SEND_CODE_COL_WIDTH, RECENT_EMPTY_TEXT,
    RECENT_LABEL,
};
pub use theme::PanelTheme;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to create pixmap: {0}")]
    PixmapCreationFailed(String),

    #[error("Failed to render: {0}")]
    RenderFailed(String),
}
