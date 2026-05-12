pub mod candidate;
pub mod renderer;

pub use candidate::CandidateList;
pub use candidate::MoveDirection;
pub use candidate::PageInfo;
pub use renderer::CandidateRenderer;
pub use renderer::draw_candidates_to_buffer;

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