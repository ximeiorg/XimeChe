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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn test_status_new_inactive() {
        let inner = librime_sys2::RimeStatus {
            data_size: std::mem::size_of::<librime_sys2::RimeStatus>() as std::os::raw::c_int,
            schema_id: std::ptr::null_mut(),
            schema_name: std::ptr::null_mut(),
            is_disabled: 0,
            is_composing: 0,
            is_ascii_mode: 1,
            is_full_shape: 0,
            is_simplified: 0,
            is_traditional: 0,
            is_ascii_punct: 0,
        };
        let status = Status::new(inner);

        assert!(!status.is_disabled);
        assert!(!status.is_composing);
        assert!(status.is_ascii_mode);
        assert!(!status.is_full_shape);
        assert!(!status.is_simplified);
        assert!(!status.is_traditional);
        assert!(!status.is_ascii_punct);
        assert_eq!(status.schema_id(), "");
        assert_eq!(status.schema_name(), "");
    }

    #[test]
    fn test_status_new_composing() {
        let inner = librime_sys2::RimeStatus {
            data_size: std::mem::size_of::<librime_sys2::RimeStatus>() as std::os::raw::c_int,
            schema_id: std::ptr::null_mut(),
            schema_name: std::ptr::null_mut(),
            is_disabled: 1,
            is_composing: 1,
            is_ascii_mode: 0,
            is_full_shape: 1,
            is_simplified: 1,
            is_traditional: 0,
            is_ascii_punct: 1,
        };
        let status = Status::new(inner);

        assert!(status.is_disabled);
        assert!(status.is_composing);
        assert!(!status.is_ascii_mode);
        assert!(status.is_full_shape);
        assert!(status.is_simplified);
        assert!(!status.is_traditional);
        assert!(status.is_ascii_punct);
    }

    #[test]
    fn test_status_new_all_enabled() {
        let inner = librime_sys2::RimeStatus {
            data_size: std::mem::size_of::<librime_sys2::RimeStatus>() as std::os::raw::c_int,
            schema_id: std::ptr::null_mut(),
            schema_name: std::ptr::null_mut(),
            is_disabled: 1,
            is_composing: 1,
            is_ascii_mode: 1,
            is_full_shape: 1,
            is_simplified: 1,
            is_traditional: 1,
            is_ascii_punct: 1,
        };
        let status = Status::new(inner);

        assert!(status.is_disabled);
        assert!(status.is_composing);
        assert!(status.is_ascii_mode);
        assert!(status.is_full_shape);
        assert!(status.is_simplified);
        assert!(status.is_traditional);
        assert!(status.is_ascii_punct);
    }

    #[test]
    fn test_status_schema_id_with_value() {
        let schema_id = CString::new("wubi86").unwrap();
        let inner = librime_sys2::RimeStatus {
            data_size: std::mem::size_of::<librime_sys2::RimeStatus>() as std::os::raw::c_int,
            schema_id: schema_id.into_raw(),
            schema_name: std::ptr::null_mut(),
            is_disabled: 0,
            is_composing: 0,
            is_ascii_mode: 0,
            is_full_shape: 0,
            is_simplified: 0,
            is_traditional: 0,
            is_ascii_punct: 0,
        };
        let status = Status::new(inner);
        assert_eq!(status.schema_id(), "wubi86");
    }

    #[test]
    fn test_status_schema_name_with_value() {
        let schema_name = CString::new("五笔86").unwrap();
        let inner = librime_sys2::RimeStatus {
            data_size: std::mem::size_of::<librime_sys2::RimeStatus>() as std::os::raw::c_int,
            schema_id: std::ptr::null_mut(),
            schema_name: schema_name.into_raw(),
            is_disabled: 0,
            is_composing: 0,
            is_ascii_mode: 0,
            is_full_shape: 0,
            is_simplified: 0,
            is_traditional: 0,
            is_ascii_punct: 0,
        };
        let status = Status::new(inner);
        assert_eq!(status.schema_name(), "五笔86");
    }
}
