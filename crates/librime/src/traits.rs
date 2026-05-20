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
                CStr::from_ptr(self.inner.shared_data_dir)
                    .to_string_lossy()
                    .into_owned()
            }
        };
        let user = unsafe {
            if self.inner.user_data_dir.is_null() {
                "<null>".to_string()
            } else {
                CStr::from_ptr(self.inner.user_data_dir)
                    .to_string_lossy()
                    .into_owned()
            }
        };
        f.debug_struct("Traits")
            .field("shared_data_dir", &shared)
            .field("user_data_dir", &user)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_traits_new() {
        let traits = Traits::new();
        assert!(traits.inner.shared_data_dir.is_null());
        assert!(traits.inner.user_data_dir.is_null());
    }

    #[test]
    fn test_traits_default() {
        let traits = Traits::default();
        assert!(traits.inner.shared_data_dir.is_null());
        assert!(traits.inner.user_data_dir.is_null());
    }

    #[test]
    fn test_traits_set_shared_data_dir() {
        let mut traits = Traits::new();
        traits.set_shared_data_dir("/usr/share/rime-data");
        assert!(!traits.inner.shared_data_dir.is_null());
        let path = unsafe {
            CStr::from_ptr(traits.inner.shared_data_dir)
                .to_str()
                .unwrap()
        };
        assert_eq!(path, "/usr/share/rime-data");
    }

    #[test]
    fn test_traits_set_user_data_dir() {
        let mut traits = Traits::new();
        traits.set_user_data_dir("/home/user/.config/rime");
        assert!(!traits.inner.user_data_dir.is_null());
        let path = unsafe { CStr::from_ptr(traits.inner.user_data_dir).to_str().unwrap() };
        assert_eq!(path, "/home/user/.config/rime");
    }

    #[test]
    fn test_traits_set_distribution_name() {
        let mut traits = Traits::new();
        traits.set_distribution_name("Xime");
        assert!(!traits.inner.distribution_name.is_null());
        let name = unsafe {
            CStr::from_ptr(traits.inner.distribution_name)
                .to_str()
                .unwrap()
        };
        assert_eq!(name, "Xime");
    }

    #[test]
    fn test_traits_set_distribution_code_name() {
        let mut traits = Traits::new();
        traits.set_distribution_code_name("xime");
        assert!(!traits.inner.distribution_code_name.is_null());
        let name = unsafe {
            CStr::from_ptr(traits.inner.distribution_code_name)
                .to_str()
                .unwrap()
        };
        assert_eq!(name, "xime");
    }

    #[test]
    fn test_traits_set_distribution_version() {
        let mut traits = Traits::new();
        traits.set_distribution_version("0.13.3");
        assert!(!traits.inner.distribution_version.is_null());
        let version = unsafe {
            CStr::from_ptr(traits.inner.distribution_version)
                .to_str()
                .unwrap()
        };
        assert_eq!(version, "0.13.3");
    }

    #[test]
    fn test_traits_set_app_name() {
        let mut traits = Traits::new();
        traits.set_app_name("xime-daemon");
        assert!(!traits.inner.app_name.is_null());
        let name = unsafe { CStr::from_ptr(traits.inner.app_name).to_str().unwrap() };
        assert_eq!(name, "xime-daemon");
    }

    #[test]
    fn test_traits_set_min_log_level() {
        let mut traits = Traits::new();
        traits.set_min_log_level(2);
        assert_eq!(traits.inner.min_log_level, 2);
    }

    #[test]
    fn test_traits_builder_chain() {
        let mut traits = Traits::new();
        traits
            .set_shared_data_dir("/usr/share/rime-data")
            .set_user_data_dir("/home/user/.config/rime")
            .set_distribution_name("Xime")
            .set_distribution_code_name("xime")
            .set_distribution_version("0.13.3")
            .set_app_name("xime-daemon")
            .set_min_log_level(2);

        assert!(!traits.inner.shared_data_dir.is_null());
        assert!(!traits.inner.user_data_dir.is_null());
        assert!(!traits.inner.distribution_name.is_null());
        assert!(!traits.inner.distribution_code_name.is_null());
        assert!(!traits.inner.distribution_version.is_null());
        assert!(!traits.inner.app_name.is_null());
        assert_eq!(traits.inner.min_log_level, 2);
    }

    #[test]
    fn test_traits_debug_format() {
        let mut traits = Traits::new();
        traits.set_shared_data_dir("/test/path");
        let debug_str = format!("{:?}", traits);
        assert!(debug_str.contains("shared_data_dir"));
        assert!(debug_str.contains("/test/path"));
    }
}
