pub mod candidate;
pub mod iced_view;
pub mod menu;

pub use candidate::CandidateItem;
pub use candidate::CandidateList;
pub use candidate::MoveDirection;
pub use candidate::PageInfo;
pub use iced_view::IcedSurface;
pub use menu::{
    menu_button_hit, menu_item_hit, menu_panel_height, expanded_height, CANDIDATE_HEIGHT,
    MENU_BUTTON_WIDTH, MENU_ITEM_HEIGHT, MENU_COLUMNS, MenuAction,
};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to create pixmap: {0}")]
    PixmapCreationFailed(String),

    #[error("Failed to render: {0}")]
    RenderFailed(String),
}
