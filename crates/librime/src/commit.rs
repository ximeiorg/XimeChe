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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commit_new_with_null_text() {
        // Create a RimeCommit with null text pointer (simulating empty commit)
        let inner = librime_sys2::RimeCommit {
            data_size: std::mem::size_of::<librime_sys2::RimeCommit>() as std::os::raw::c_int,
            text: std::ptr::null_mut(),
        };
        let commit = Commit::new(inner);
        assert_eq!(commit.text(), "");
    }
}
