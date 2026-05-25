pub mod candidate;
pub mod renderer;
pub mod root_display;

pub use candidate::CandidateItem;
pub use candidate::CandidateList;
pub use candidate::MoveDirection;
pub use candidate::PageInfo;
pub use renderer::calculate_candidate_width;
pub use renderer::draw_candidates_to_buffer;
pub use renderer::CandidateRenderer;
pub use root_display::calculate_root_width;
pub use root_display::draw_root_to_buffer;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to create pixmap: {0}")]
    PixmapCreationFailed(String),

    #[error("Failed to render: {0}")]
    RenderFailed(String),

    #[error("UI error: {0}")]
    UiError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_pixmap_creation() {
        let err = Error::PixmapCreationFailed("out of memory".to_string());
        assert_eq!(format!("{}", err), "Failed to create pixmap: out of memory");
    }

    #[test]
    fn test_error_render_failed() {
        let err = Error::RenderFailed("font missing".to_string());
        assert_eq!(format!("{}", err), "Failed to render: font missing");
    }

    #[test]
    fn test_error_ui_error() {
        let err = Error::UiError("invalid state".to_string());
        assert_eq!(format!("{}", err), "UI error: invalid state");
    }

    #[test]
    fn test_error_debug() {
        let err = Error::UiError("test".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("UiError"));
    }

    #[test]
    fn test_candidate_item_default() {
        let item = CandidateItem::default();
        assert_eq!(item.text, "");
        assert_eq!(item.comment, "");
        assert_eq!(item.index, 0);
    }

    #[test]
    fn test_candidate_item_custom() {
        let item = CandidateItem {
            text: "测试".to_string(),
            comment: "ceshi".to_string(),
            index: 1,
        };
        assert_eq!(item.text, "测试");
        assert_eq!(item.comment, "ceshi");
        assert_eq!(item.index, 1);
    }

    #[test]
    fn test_move_direction_equality() {
        assert_eq!(MoveDirection::Up, MoveDirection::Up);
        assert_eq!(MoveDirection::Down, MoveDirection::Down);
        assert_ne!(MoveDirection::Up, MoveDirection::Down);
    }

    #[test]
    fn test_page_info_debug() {
        let info = PageInfo {
            page_size: 5,
            current_page: 0,
            total_pages: 1,
            is_last_page: true,
            highlighted_index: 0,
            select_keys: "12345".to_string(),
        };
        let debug = format!("{:?}", info);
        assert!(debug.contains("page_size"));
        assert!(debug.contains("select_keys"));
    }

    #[test]
    fn test_candidate_item_debug() {
        let item = CandidateItem {
            text: "你好".to_string(),
            comment: "".to_string(),
            index: 0,
        };
        let debug = format!("{:?}", item);
        assert!(debug.contains("你好"));
    }
}
