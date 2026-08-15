pub mod candidate;
pub mod iced_view;
pub mod menu;

pub use candidate::CandidateItem;
pub use candidate::CandidateList;
pub use candidate::MoveDirection;
pub use candidate::PageInfo;
pub use iced_view::IcedSurface;
pub use menu::{
    expanded_height, menu_button_hit, menu_item_hit, menu_panel_height, MenuAction,
    CANDIDATE_HEIGHT, MENU_BUTTON_WIDTH, MENU_COLUMNS, MENU_ITEM_HEIGHT,
};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to create pixmap: {0}")]
    PixmapCreationFailed(String),

    #[error("Failed to render: {0}")]
    RenderFailed(String),
}
