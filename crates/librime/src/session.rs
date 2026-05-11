use crate::context::Context;
use crate::commit::Commit;
use crate::status::Status;
use crate::error::{Error, Result};
use crate::get_api;

use std::ffi::CString;

pub struct Session {
    pub(crate) session_id: librime_sys::RimeSessionId,
    closed: bool,
}

impl Session {
    pub(crate) fn new(session_id: librime_sys::RimeSessionId) -> Self {
        Self {
            session_id,
            closed: false,
        }
    }

    pub fn find_session(&self) -> bool {
        unsafe {
            let api = get_api();
            if api.is_null() {
                return false;
            }
            if let Some(find_session) = (*api).find_session {
                find_session(self.session_id) != 0
            } else {
                false
            }
        }
    }

    pub fn select_schema(&self, schema_id: &str) -> Result<()> {
        let cstr = CString::new(schema_id)?;
        unsafe {
            let api = get_api();
            if api.is_null() {
                return Err(Error::ApiNotInitialized);
            }
            if let Some(select_schema) = (*api).select_schema {
                if select_schema(self.session_id, cstr.as_ptr()) == 0 {
                    return Err(Error::SelectSchema);
                }
            }
        }
        Ok(())
    }

    pub fn process_key(&self, key_code: i32, modifiers: i32) -> bool {
        unsafe {
            let api = get_api();
            if api.is_null() {
                return false;
            }
            if let Some(process_key) = (*api).process_key {
                process_key(self.session_id, key_code, modifiers) != 0
            } else {
                false
            }
        }
    }

    pub fn context(&self) -> Option<Context> {
        librime_sys::rime_struct!(ctx: librime_sys::RimeContext);
        unsafe {
            let api = get_api();
            if api.is_null() {
                return None;
            }
            if let Some(get_context) = (*api).get_context {
                if get_context(self.session_id, &mut ctx) == 0 {
                    return None;
                }
                Some(Context::new(ctx))
            } else {
                None
            }
        }
    }

    pub fn commit(&self) -> Option<Commit> {
        librime_sys::rime_struct!(commit: librime_sys::RimeCommit);
        unsafe {
            let api = get_api();
            if api.is_null() {
                return None;
            }
            if let Some(get_commit) = (*api).get_commit {
                if get_commit(self.session_id, &mut commit) == 0 {
                    return None;
                }
                Some(Commit::new(commit))
            } else {
                None
            }
        }
    }

    pub fn status(&self) -> Result<Status> {
        librime_sys::rime_struct!(status: librime_sys::RimeStatus);
        unsafe {
            let api = get_api();
            if api.is_null() {
                return Err(Error::ApiNotInitialized);
            }
            if let Some(get_status) = (*api).get_status {
                if get_status(self.session_id, &mut status) == 0 {
                    return Err(Error::GetStatus);
                }
                Ok(Status::new(status))
            } else {
                Err(Error::FunctionNotAvailable("get_status"))
            }
        }
    }

    pub fn simulate_key_sequence(&self, sequence: &str) -> Result<()> {
        let cstr = CString::new(sequence)?;
        unsafe {
            let api = get_api();
            if api.is_null() {
                return Err(Error::ApiNotInitialized);
            }
            if let Some(simulate_key_sequence) = (*api).simulate_key_sequence {
                if simulate_key_sequence(self.session_id, cstr.as_ptr()) == 0 {
                    return Err(Error::SimulateKeySequence);
                }
            }
        }
        Ok(())
    }

    pub fn get_input(&self) -> Option<&str> {
        unsafe {
            let api = get_api();
            if api.is_null() {
                return None;
            }
            if let Some(get_input) = (*api).get_input {
                let ptr = get_input(self.session_id);
                if ptr.is_null() {
                    None
                } else {
                    Some(std::ffi::CStr::from_ptr(ptr).to_str().unwrap())
                }
            } else {
                None
            }
        }
    }

    pub fn close(&mut self) -> Result<()> {
        if self.closed {
            return Ok(());
        }
        unsafe {
            let api = get_api();
            if api.is_null() {
                return Err(Error::ApiNotInitialized);
            }
            if let Some(destroy_session) = (*api).destroy_session {
                if destroy_session(self.session_id) == 0 {
                    return Err(Error::CloseSession);
                }
                self.closed = true;
            }
        }
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if !self.closed {
            let _ = self.close();
        }
    }
}