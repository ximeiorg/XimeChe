pub mod im_v2;
pub mod im_v1;

pub use im_v2::{
    WaylandConnection,
    InputMethodState,
    InputMethodData,
    Error as ErrorV2,
    Result as ResultV2,
    ZwpInputMethodV2,
    ZwpInputMethodManagerV2,
};

pub use im_v1::{
    WaylandConnectionV1,
    InputMethodV1State,
    InputMethodV1Data,
    KeyEvent,
    Error as ErrorV1,
    Result as ResultV1,
};