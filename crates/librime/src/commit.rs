use crate::get_api;
use std::ffi::CStr;

pub struct Commit {
    inner: librime_sys2::RimeCommit,
}

impl Commit {
    pub(crate) fn new(inner: librime_sys2::RimeCommit) -> Self {
        Self { inner }
    }

    pub fn text(&self) -> &str {
        if self.inner.text.is_null() {
            ""
        } else {
            unsafe { CStr::from_ptr(self.inner.text).to_str().unwrap() }
        }
    }
}

impl Drop for Commit {
    fn drop(&mut self) {
        unsafe {
            let api = get_api();
            if !api.is_null() {
                if let Some(free_commit) = (*api).free_commit {
                    free_commit(&mut self.inner);
                }
            }
        }
    }
}
