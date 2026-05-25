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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_api_not_initialized() {
        let err = Error::ApiNotInitialized;
        assert_eq!(format!("{}", err), "Rime API not initialized");
    }

    #[test]
    fn test_error_display_function_not_available() {
        let err = Error::FunctionNotAvailable("process_key");
        assert_eq!(
            format!("{}", err),
            "Rime function 'process_key' not available"
        );
    }

    #[test]
    fn test_error_display_create_session() {
        let err = Error::CreateSession;
        assert_eq!(format!("{}", err), "Failed to create session");
    }

    #[test]
    fn test_error_display_start_maintenance() {
        let err = Error::StartMaintenance;
        assert_eq!(format!("{}", err), "Failed to start maintenance");
    }

    #[test]
    fn test_error_display_sync_user_data() {
        let err = Error::SyncUserData;
        assert_eq!(format!("{}", err), "Failed to sync user data");
    }

    #[test]
    fn test_error_display_get_context() {
        let err = Error::GetContext;
        assert_eq!(format!("{}", err), "Failed to get context");
    }

    #[test]
    fn test_error_display_get_status() {
        let err = Error::GetStatus;
        assert_eq!(format!("{}", err), "Failed to get status");
    }

    #[test]
    fn test_error_display_get_commit() {
        let err = Error::GetCommit;
        assert_eq!(format!("{}", err), "Failed to get commit");
    }

    #[test]
    fn test_error_display_select_schema() {
        let err = Error::SelectSchema;
        assert_eq!(format!("{}", err), "Failed to select schema");
    }

    #[test]
    fn test_error_display_close_session() {
        let err = Error::CloseSession;
        assert_eq!(format!("{}", err), "Failed to close session");
    }

    #[test]
    fn test_error_display_simulate_key_sequence() {
        let err = Error::SimulateKeySequence;
        assert_eq!(format!("{}", err), "Failed to simulate key sequence");
    }

    #[test]
    fn test_error_display_invalid_utf8() {
        let err = Error::InvalidUtf8;
        assert_eq!(format!("{}", err), "Invalid UTF-8 string");
    }

    #[test]
    fn test_error_from_nul_error() {
        // Create a NulError by trying to create a CString with interior null
        let err_inner = std::ffi::CString::new("hello\0world").unwrap_err();
        let err: Error = err_inner.into();
        assert!(matches!(err, Error::NulByte(_)));
    }

    #[test]
    fn test_error_debug() {
        let err = Error::ApiNotInitialized;
        let debug = format!("{:?}", err);
        assert!(debug.contains("ApiNotInitialized"));
    }
}
