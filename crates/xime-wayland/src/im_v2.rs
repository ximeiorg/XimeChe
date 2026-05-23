use wayland_client;
use wayland_client::globals::GlobalListContents;
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::protocol::*;
use wayland_client::{globals::registry_queue_init, Connection, EventQueue};
use wayland_client::{Dispatch, Proxy, QueueHandle};

pub mod __interfaces {
    use wayland_client::protocol::__interfaces::*;
    wayland_scanner::generate_interfaces!("protocols/input-method-unstable-v2.xml");
}

use self::__interfaces::*;

wayland_scanner::generate_client_code!("protocols/input-method-unstable-v2.xml");

pub use zwp_input_method_manager_v2::ZwpInputMethodManagerV2;
pub use zwp_input_method_v2::Event as ImEvent;
pub use zwp_input_method_v2::ZwpInputMethodV2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMethodState {
    #[default]
    Inactive,
    Active,
}

#[derive(Debug, Clone, Default)]
pub struct InputMethodData {
    pub serial: u32,
    pub state: InputMethodState,
    pub surrounding_text: Option<String>,
    pub surrounding_cursor: u32,
    pub surrounding_anchor: u32,
    pub content_hint: u32,
    pub content_purpose: u32,
}

impl Dispatch<WlRegistry, GlobalListContents> for InputMethodData {
    fn event(
        _state: &mut InputMethodData,
        _proxy: &WlRegistry,
        _event: <WlRegistry as Proxy>::Event,
        _data: &GlobalListContents,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodData>,
    ) {
    }
}

impl Dispatch<WlSeat, InputMethodData> for InputMethodData {
    fn event(
        _state: &mut InputMethodData,
        _proxy: &WlSeat,
        _event: <WlSeat as Proxy>::Event,
        _data: &InputMethodData,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodData>,
    ) {
    }
}

impl Dispatch<ZwpInputMethodManagerV2, InputMethodData> for InputMethodData {
    fn event(
        _state: &mut InputMethodData,
        _proxy: &ZwpInputMethodManagerV2,
        _event: <ZwpInputMethodManagerV2 as Proxy>::Event,
        _data: &InputMethodData,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodData>,
    ) {
    }
}

impl Dispatch<ZwpInputMethodV2, InputMethodData> for InputMethodData {
    fn event(
        state: &mut InputMethodData,
        _proxy: &ZwpInputMethodV2,
        event: <ZwpInputMethodV2 as Proxy>::Event,
        _data: &InputMethodData,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodData>,
    ) {
        match event {
            ImEvent::Activate => {
                state.state = InputMethodState::Active;
            }
            ImEvent::Deactivate => {
                state.state = InputMethodState::Inactive;
            }
            ImEvent::SurroundingText {
                text,
                cursor,
                anchor,
            } => {
                state.surrounding_text = Some(text);
                state.surrounding_cursor = cursor;
                state.surrounding_anchor = anchor;
            }
            ImEvent::ContentType { hint, purpose } => {
                state.content_hint = hint;
                state.content_purpose = purpose;
            }
            ImEvent::Done { serial } => {
                state.serial = serial;
            }
            _ => {}
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to connect to Wayland display: {0}")]
    ConnectFailed(String),

    #[error("Failed to get global: {0}")]
    GlobalNotFound(String),

    #[error("Failed to bind interface: {0}")]
    BindFailed(String),

    #[error("No input method manager available")]
    NoInputMethodManager,

    #[error("No seat available")]
    NoSeat,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct WaylandConnection {
    connection: Connection,
    event_queue: EventQueue<InputMethodData>,
    state: InputMethodData,
    seat: Option<WlSeat>,
    input_method_manager: Option<ZwpInputMethodManagerV2>,
    pub input_method: Option<ZwpInputMethodV2>,
}

impl WaylandConnection {
    pub fn connect() -> Result<Self> {
        let connection =
            Connection::connect_to_env().map_err(|e| Error::ConnectFailed(e.to_string()))?;

        let (globals, event_queue) =
            registry_queue_init(&connection).map_err(|e| Error::ConnectFailed(e.to_string()))?;

        let qh = event_queue.handle();
        let state = InputMethodData::default();

        let seat: Option<WlSeat> = globals.bind(&qh, 1..=8, InputMethodData::default()).ok();

        let input_method_manager: Option<ZwpInputMethodManagerV2> =
            globals.bind(&qh, 1..=1, InputMethodData::default()).ok();

        Ok(Self {
            connection,
            event_queue,
            state,
            seat,
            input_method_manager,
            input_method: None,
        })
    }

    pub fn get_seat(&self) -> Result<&WlSeat> {
        self.seat.as_ref().ok_or(Error::NoSeat)
    }

    pub fn get_input_method_manager(&self) -> Result<&ZwpInputMethodManagerV2> {
        self.input_method_manager
            .as_ref()
            .ok_or(Error::NoInputMethodManager)
    }

    pub fn create_input_method(&mut self) -> Result<&ZwpInputMethodV2> {
        let seat = self.get_seat()?;
        let manager = self.get_input_method_manager()?;
        let qh = self.event_queue.handle();

        let input_method = manager.get_input_method(seat, &qh, InputMethodData::default());
        self.input_method = Some(input_method);

        self.sync_roundtrip()?;

        self.input_method
            .as_ref()
            .ok_or(Error::BindFailed("Input method not created".to_string()))
    }

    pub fn dispatch_events(&mut self) -> Result<()> {
        // Blocking dispatch - wait for at least one event
        self.event_queue
            .roundtrip(&mut self.state)
            .map_err(|e| Error::Io(std::io::Error::other(e)))?;
        Ok(())
    }

    pub fn dispatch_pending(&mut self) -> Result<()> {
        self.event_queue
            .dispatch_pending(&mut self.state)
            .map_err(|e| Error::Io(std::io::Error::other(e)))?;
        Ok(())
    }

    pub fn sync_roundtrip(&mut self) -> Result<()> {
        self.event_queue
            .roundtrip(&mut self.state)
            .map_err(|e| Error::Io(std::io::Error::other(e)))?;
        Ok(())
    }

    pub fn get_state(&self) -> &InputMethodData {
        &self.state
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn event_queue(&mut self) -> &mut EventQueue<InputMethodData> {
        &mut self.event_queue
    }
}
