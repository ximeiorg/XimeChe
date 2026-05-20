use std::ffi::NulError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Rime API not initialized")]
    ApiNotInitialized,
    #[error("Rime function '{0}' not available")]
    FunctionNotAvailable(&'static str),
    #[error("Failed to create session")]
    CreateSession,
    #[error("Failed to start maintenance")]
    StartMaintenance,
    #[error("Failed to sync user data")]
    SyncUserData,
    #[error("Failed to get context")]
    GetContext,
    #[error("Failed to get status")]
    GetStatus,
    #[error("Failed to get commit")]
    GetCommit,
    #[error("Failed to select schema")]
    SelectSchema,
    #[error("Failed to close session")]
    CloseSession,
    #[error("Failed to simulate key sequence")]
    SimulateKeySequence,
    #[error("Invalid UTF-8 string")]
    InvalidUtf8,
    #[error("String contains null byte")]
    NulByte(#[from] NulError),
}

pub type Result<T> = std::result::Result<T, Error>;
