pub mod error;
pub mod traits;
pub mod session;
pub mod context;
pub mod commit;
pub mod status;
pub mod key;

pub use key::{KeyCode, Modifier, K_SHIFT_MASK, K_CONTROL_MASK, K_ALT_MASK, K_RELEASE_MASK, XK_SHIFT_L};

use once_cell::sync::Lazy;
use std::sync::Mutex;

use librime_sys2::rime_get_api;

struct RimeApiWrapper(*mut librime_sys2::RimeApi);
unsafe impl Send for RimeApiWrapper {}
unsafe impl Sync for RimeApiWrapper {}

static RIME_API: Lazy<RimeApiWrapper> = Lazy::new(|| {
    RimeApiWrapper(unsafe { rime_get_api() })
});

static DEPLOY_RESULT: Lazy<Mutex<Option<DeployResult>>> = Lazy::new(|| Mutex::new(None));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeployResult {
    Success,
    Failure,
}

macro_rules! rime_api_call {
    ($f:tt, $($arg:tt)*) => {
        unsafe {
            let api = $crate::get_api();
            if api.is_null() {
                return Err($crate::error::Error::ApiNotInitialized);
            }
            let func = (*api).$f;
            if func.is_none() {
                return Err($crate::error::Error::FunctionNotAvailable(stringify!($f)));
            }
            func.unwrap()($($arg)*)
        }
    };
    ($f:tt) => {
        unsafe {
            let api = $crate::get_api();
            if api.is_null() {
                return Err($crate::error::Error::ApiNotInitialized);
            }
            let func = (*api).$f;
            if func.is_none() {
                return Err($crate::error::Error::FunctionNotAvailable(stringify!($f)));
            }
            func.unwrap()()
        }
    };
}


pub fn get_api() -> *mut librime_sys2::RimeApi {
    RIME_API.0
}

pub fn setup(traits: &mut traits::Traits) {
    unsafe {
        let api = get_api();
        if !api.is_null() {
            if let Some(setup) = (*api).setup {
                setup(&mut traits.inner);
            }
        }
    }
}

pub fn initialize(traits: &mut traits::Traits) -> error::Result<()> {
    unsafe {
        let api = get_api();
        if api.is_null() {
            return Err(error::Error::ApiNotInitialized);
        }
        if let Some(initialize) = (*api).initialize {
            initialize(&mut traits.inner);
        }
        if let Some(set_notification_handler) = (*api).set_notification_handler {
            set_notification_handler(Some(notification_handler), std::ptr::null_mut());
        }
    }
    Ok(())
}

pub fn finalize() {
    unsafe {
        let api = get_api();
        if !api.is_null() {
            if let Some(finalize) = (*api).finalize {
                finalize();
            }
        }
    }
}

pub fn start_maintenance(full_check: bool) -> error::Result<()> {
    let result = rime_api_call!(start_maintenance, full_check as i32);
    if result == 0 {
        Err(error::Error::StartMaintenance)
    } else {
        Ok(())
    }
}

pub fn join_maintenance_thread() {
    unsafe {
        let api = get_api();
        if !api.is_null() {
            if let Some(join) = (*api).join_maintenance_thread {
                join();
            }
        }
    }
}

pub fn is_maintenance_mode() -> bool {
    unsafe {
        let api = get_api();
        if api.is_null() {
            return false;
        }
        if let Some(is_maintenance) = (*api).is_maintenance_mode {
            is_maintenance() != 0
        } else {
            false
        }
    }
}

pub fn full_deploy_and_wait() -> DeployResult {
    *DEPLOY_RESULT.lock().unwrap() = None;
    if start_maintenance(true).is_err() {
        return DeployResult::Failure;
    }
    join_maintenance_thread();
    if let Some(result) = *DEPLOY_RESULT.lock().unwrap() {
        result
    } else {
        DeployResult::Failure
    }
}

pub fn sync_user_data() -> error::Result<()> {
    let result = rime_api_call!(sync_user_data);
    if result == 0 {
        Err(error::Error::SyncUserData)
    } else {
        Ok(())
    }
}

extern "C" fn notification_handler(
    _context: *mut std::ffi::c_void,
    _session_id: librime_sys2::RimeSessionId,
    message_type: *const std::ffi::c_char,
    message_value: *const std::ffi::c_char,
) {
    use std::ffi::CStr;
    unsafe {
        let msg_type = CStr::from_ptr(message_type).to_string_lossy();
        let msg_value = CStr::from_ptr(message_value).to_string_lossy();
        
        if msg_type == "deploy" {
            let mut result = DEPLOY_RESULT.lock().unwrap();
            match msg_value.as_ref() {
                "success" => *result = Some(DeployResult::Success),
                "failure" => *result = Some(DeployResult::Failure),
                _ => {}
            }
        }
    }
}

pub fn create_session() -> error::Result<session::Session> {
    let session_id = rime_api_call!(create_session);
    let session = session::Session::new(session_id);
    if !session.find_session() {
        Err(error::Error::CreateSession)
    } else {
        Ok(session)
    }
}