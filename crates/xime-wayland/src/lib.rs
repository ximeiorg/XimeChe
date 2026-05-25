pub mod im_v1;
pub mod im_v2;

pub use im_v2::{
    Error as ErrorV2, InputMethodData, InputMethodState, Result as ResultV2, WaylandConnection,
    ZwpInputMethodManagerV2, ZwpInputMethodV2,
};

pub use im_v1::{
    Error as ErrorV1, InputMethodV1Data, InputMethodV1State, KeyEvent, Result as ResultV1,
    WaylandConnectionV1,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_method_v1_state_default() {
        let state = InputMethodV1State::default();
        assert_eq!(state, InputMethodV1State::Inactive);
    }

    #[test]
    fn test_input_method_v1_state_equality() {
        assert_eq!(InputMethodV1State::Inactive, InputMethodV1State::Inactive);
        assert_eq!(InputMethodV1State::Active, InputMethodV1State::Active);
        assert_ne!(InputMethodV1State::Inactive, InputMethodV1State::Active);
    }

    #[test]
    fn test_input_method_v1_state_debug() {
        let debug = format!("{:?}", InputMethodV1State::Active);
        assert_eq!(debug, "Active");
    }

    #[test]
    fn test_input_method_v2_state_default() {
        let state = InputMethodState::default();
        assert_eq!(state, InputMethodState::Inactive);
    }

    #[test]
    fn test_input_method_v2_state_equality() {
        assert_eq!(InputMethodState::Inactive, InputMethodState::Inactive);
        assert_eq!(InputMethodState::Active, InputMethodState::Active);
        assert_ne!(InputMethodState::Inactive, InputMethodState::Active);
    }

    #[test]
    fn test_input_method_v2_state_debug() {
        let debug = format!("{:?}", InputMethodState::Active);
        assert_eq!(debug, "Active");
    }

    #[test]
    fn test_input_method_v1_data_default() {
        let data = InputMethodV1Data::default();
        assert_eq!(data.state, InputMethodV1State::Inactive);
        assert_eq!(data.serial, 0);
        assert!(data.surrounding_text.is_none());
    }

    #[test]
    fn test_input_method_v2_data_default() {
        let data = InputMethodData::default();
        assert_eq!(data.state, InputMethodState::Inactive);
        assert_eq!(data.serial, 0);
        assert!(data.surrounding_text.is_none());
    }

    #[test]
    fn test_key_event_creation() {
        let event = KeyEvent {
            serial: 42,
            time: 1000,
            key: 30,
            pressed: true,
        };
        assert_eq!(event.serial, 42);
        assert_eq!(event.time, 1000);
        assert_eq!(event.key, 30);
        assert!(event.pressed);
    }

    #[test]
    fn test_key_event_release() {
        let event = KeyEvent {
            serial: 43,
            time: 1001,
            key: 30,
            pressed: false,
        };
        assert!(!event.pressed);
    }

    #[test]
    fn test_error_v1_display() {
        let err = ErrorV1::NoInputMethod;
        assert_eq!(format!("{}", err), "No input method available");
    }

    #[test]
    fn test_error_v1_no_seat() {
        let err = ErrorV1::NoSeat;
        assert_eq!(format!("{}", err), "No seat available");
    }

    #[test]
    fn test_error_v1_no_compositor() {
        let err = ErrorV1::NoCompositor;
        assert_eq!(format!("{}", err), "No compositor available");
    }

    #[test]
    fn test_error_v2_display() {
        let err = ErrorV2::NoSeat;
        assert_eq!(format!("{}", err), "No seat available");
    }

    #[test]
    fn test_error_v2_no_input_method_manager() {
        let err = ErrorV2::NoInputMethodManager;
        assert_eq!(format!("{}", err), "No input method manager available");
    }

    #[test]
    fn test_input_method_v1_data_clone() {
        let data = InputMethodV1Data::default();
        let cloned = data.clone();
        assert_eq!(cloned.state, InputMethodV1State::Inactive);
    }
}
