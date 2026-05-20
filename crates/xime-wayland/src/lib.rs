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
