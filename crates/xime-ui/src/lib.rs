pub mod candidate;

pub use candidate::CandidateList;
pub use candidate::MoveDirection;
pub use candidate::PageInfo;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to create window: {0}")]
    WindowCreationFailed(String),

    #[error("Failed to render: {0}")]
    RenderFailed(String),

    #[error("UI error: {0}")]
    UiError(String),
}