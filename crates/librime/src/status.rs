use crate::get_api;
use std::ffi::CStr;

pub struct Status {
    inner: librime_sys2::RimeStatus,
    pub is_disabled: bool,
    pub is_composing: bool,
    pub is_ascii_mode: bool,
    pub is_full_shape: bool,
    pub is_simplified: bool,
    pub is_traditional: bool,
    pub is_ascii_punct: bool,
}

impl Status {
    pub(crate) fn new(inner: librime_sys2::RimeStatus) -> Self {
        Self {
            inner,
            is_disabled: inner.is_disabled != 0,
            is_composing: inner.is_composing != 0,
            is_ascii_mode: inner.is_ascii_mode != 0,
            is_full_shape: inner.is_full_shape != 0,
            is_simplified: inner.is_simplified != 0,
            is_traditional: inner.is_traditional != 0,
            is_ascii_punct: inner.is_ascii_punct != 0,
        }
    }

    pub fn schema_id(&self) -> &str {
        if self.inner.schema_id.is_null() {
            ""
        } else {
            unsafe { CStr::from_ptr(self.inner.schema_id).to_str().unwrap() }
        }
    }

    pub fn schema_name(&self) -> &str {
        if self.inner.schema_name.is_null() {
            ""
        } else {
            unsafe { CStr::from_ptr(self.inner.schema_name).to_str().unwrap() }
        }
    }
}

impl Drop for Status {
    fn drop(&mut self) {
        unsafe {
            let api = get_api();
            if !api.is_null() {
                if let Some(free_status) = (*api).free_status {
                    free_status(&mut self.inner);
                }
            }
        }
    }
}
