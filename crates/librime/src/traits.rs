use std::ffi::{CStr, CString};
use std::os::raw::c_int;

pub struct Traits {
    pub(crate) inner: librime_sys2::RimeTraits,
    resources: Vec<CString>,
}

impl Traits {
    pub fn new() -> Self {
        librime_sys2::rime_struct!(traits: librime_sys2::RimeTraits);
        Self {
            inner: traits,
            resources: Vec::new(),
        }
    }

    pub fn set_shared_data_dir(&mut self, path: &str) -> &mut Self {
        let cstr = CString::new(path).unwrap();
        self.inner.shared_data_dir = cstr.as_ptr();
        self.resources.push(cstr);
        self
    }

    pub fn set_user_data_dir(&mut self, path: &str) -> &mut Self {
        let cstr = CString::new(path).unwrap();
        self.inner.user_data_dir = cstr.as_ptr();
        self.resources.push(cstr);
        self
    }

    pub fn set_distribution_name(&mut self, name: &str) -> &mut Self {
        let cstr = CString::new(name).unwrap();
        self.inner.distribution_name = cstr.as_ptr();
        self.resources.push(cstr);
        self
    }

    pub fn set_distribution_code_name(&mut self, name: &str) -> &mut Self {
        let cstr = CString::new(name).unwrap();
        self.inner.distribution_code_name = cstr.as_ptr();
        self.resources.push(cstr);
        self
    }

    pub fn set_distribution_version(&mut self, version: &str) -> &mut Self {
        let cstr = CString::new(version).unwrap();
        self.inner.distribution_version = cstr.as_ptr();
        self.resources.push(cstr);
        self
    }

    pub fn set_app_name(&mut self, name: &str) -> &mut Self {
        let cstr = CString::new(name).unwrap();
        self.inner.app_name = cstr.as_ptr();
        self.resources.push(cstr);
        self
    }

    pub fn set_min_log_level(&mut self, level: u8) -> &mut Self {
        self.inner.min_log_level = level as c_int;
        self
    }

    pub fn set_log_dir(&mut self, path: &str) -> &mut Self {
        let cstr = CString::new(path).unwrap();
        self.inner.log_dir = cstr.as_ptr();
        self.resources.push(cstr);
        self
    }

    pub fn set_prebuilt_data_dir(&mut self, path: &str) -> &mut Self {
        let cstr = CString::new(path).unwrap();
        self.inner.prebuilt_data_dir = cstr.as_ptr();
        self.resources.push(cstr);
        self
    }

    pub fn set_staging_dir(&mut self, path: &str) -> &mut Self {
        let cstr = CString::new(path).unwrap();
        self.inner.staging_dir = cstr.as_ptr();
        self.resources.push(cstr);
        self
    }
}

impl Default for Traits {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Traits {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let shared = unsafe {
            if self.inner.shared_data_dir.is_null() {
                "<null>".to_string()
            } else {
                CStr::from_ptr(self.inner.shared_data_dir).to_string_lossy().into_owned()
            }
        };
        let user = unsafe {
            if self.inner.user_data_dir.is_null() {
                "<null>".to_string()
            } else {
                CStr::from_ptr(self.inner.user_data_dir).to_string_lossy().into_owned()
            }
        };
        f.debug_struct("Traits")
            .field("shared_data_dir", &shared)
            .field("user_data_dir", &user)
            .finish()
    }
}