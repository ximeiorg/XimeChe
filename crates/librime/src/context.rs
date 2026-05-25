use crate::get_api;
use std::ffi::CStr;

pub struct Context {
    inner: librime_sys2::RimeContext,
}

impl Context {
    pub(crate) fn new(inner: librime_sys2::RimeContext) -> Self {
        Self { inner }
    }

    pub fn composition(&self) -> Composition {
        let comp = self.inner.composition;
        Composition {
            length: comp.length as usize,
            cursor_pos: comp.cursor_pos as usize,
            sel_start: comp.sel_start as usize,
            sel_end: comp.sel_end as usize,
            preedit: if comp.preedit.is_null() {
                None
            } else {
                Some(unsafe { CStr::from_ptr(comp.preedit).to_str().unwrap() })
            },
        }
    }

    pub fn menu(&self) -> Menu {
        let menu = self.inner.menu;
        Menu {
            page_size: menu.page_size as usize,
            page_no: menu.page_no as usize,
            is_last_page: menu.is_last_page != 0,
            highlighted_candidate_index: menu.highlighted_candidate_index as usize,
            num_candidates: menu.num_candidates as usize,
            candidates: unsafe {
                let mut candidates = Vec::new();
                if !menu.candidates.is_null() {
                    for i in 0..menu.num_candidates as usize {
                        let candidate = &*menu.candidates.add(i);
                        candidates.push(Candidate {
                            text: if candidate.text.is_null() {
                                ""
                            } else {
                                CStr::from_ptr(candidate.text).to_str().unwrap()
                            },
                            comment: if candidate.comment.is_null() {
                                None
                            } else {
                                Some(CStr::from_ptr(candidate.comment).to_str().unwrap())
                            },
                        });
                    }
                }
                candidates
            },
            select_keys: if menu.select_keys.is_null() {
                None
            } else {
                Some(unsafe { CStr::from_ptr(menu.select_keys).to_str().unwrap() })
            },
        }
    }

    pub fn commit_text_preview(&self) -> Option<&str> {
        if self.inner.commit_text_preview.is_null() {
            None
        } else {
            Some(unsafe {
                CStr::from_ptr(self.inner.commit_text_preview)
                    .to_str()
                    .unwrap()
            })
        }
    }

    pub fn raw(&self) -> &librime_sys2::RimeContext {
        &self.inner
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            let api = get_api();
            if !api.is_null() {
                if let Some(free_context) = (*api).free_context {
                    free_context(&mut self.inner);
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Composition {
    pub length: usize,
    pub cursor_pos: usize,
    pub sel_start: usize,
    pub sel_end: usize,
    pub preedit: Option<&'static str>,
}

#[derive(Debug, Clone)]
pub struct Menu {
    pub page_size: usize,
    pub page_no: usize,
    pub is_last_page: bool,
    pub highlighted_candidate_index: usize,
    pub num_candidates: usize,
    pub candidates: Vec<Candidate>,
    pub select_keys: Option<&'static str>,
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub text: &'static str,
    pub comment: Option<&'static str>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_composition_default_values() {
        let comp = Composition {
            length: 0,
            cursor_pos: 0,
            sel_start: 0,
            sel_end: 0,
            preedit: None,
        };
        assert_eq!(comp.length, 0);
        assert_eq!(comp.cursor_pos, 0);
        assert!(comp.preedit.is_none());
    }

    #[test]
    fn test_composition_with_preedit() {
        let comp = Composition {
            length: 4,
            cursor_pos: 4,
            sel_start: 0,
            sel_end: 4,
            preedit: Some("nihao"),
        };
        assert_eq!(comp.length, 4);
        assert_eq!(comp.preedit, Some("nihao"));
    }

    #[test]
    fn test_menu_default_values() {
        let menu = Menu {
            page_size: 5,
            page_no: 0,
            is_last_page: true,
            highlighted_candidate_index: 0,
            num_candidates: 0,
            candidates: Vec::new(),
            select_keys: None,
        };
        assert_eq!(menu.page_size, 5);
        assert!(menu.is_last_page);
        assert!(menu.candidates.is_empty());
        assert!(menu.select_keys.is_none());
    }

    #[test]
    fn test_menu_with_candidates() {
        let candidates = vec![
            Candidate {
                text: "你好",
                comment: None,
            },
            Candidate {
                text: "世界",
                comment: Some("shijie"),
            },
        ];
        let menu = Menu {
            page_size: 5,
            page_no: 0,
            is_last_page: false,
            highlighted_candidate_index: 0,
            num_candidates: 2,
            candidates,
            select_keys: Some("12345"),
        };
        assert!(!menu.is_last_page);
        assert_eq!(menu.num_candidates, 2);
        assert_eq!(menu.candidates[0].text, "你好");
        assert!(menu.candidates[0].comment.is_none());
        assert_eq!(menu.candidates[1].text, "世界");
        assert_eq!(menu.candidates[1].comment, Some("shijie"));
        assert_eq!(menu.select_keys, Some("12345"));
    }

    #[test]
    fn test_candidate_with_comment() {
        let candidate = Candidate {
            text: "测试",
            comment: Some("ceshi"),
        };
        assert_eq!(candidate.text, "测试");
        assert_eq!(candidate.comment, Some("ceshi"));
    }

    #[test]
    fn test_composition_debug() {
        let comp = Composition {
            length: 2,
            cursor_pos: 2,
            sel_start: 0,
            sel_end: 2,
            preedit: Some("wo"),
        };
        let debug = format!("{:?}", comp);
        assert!(debug.contains("preedit"));
        assert!(debug.contains("wo"));
    }

    #[test]
    fn test_menu_debug() {
        let menu = Menu {
            page_size: 5,
            page_no: 0,
            is_last_page: true,
            highlighted_candidate_index: 0,
            num_candidates: 0,
            candidates: Vec::new(),
            select_keys: None,
        };
        let debug = format!("{:?}", menu);
        assert!(debug.contains("page_size"));
        assert!(debug.contains("page_no"));
    }

    #[test]
    fn test_candidate_debug() {
        let candidate = Candidate {
            text: "测试",
            comment: None,
        };
        let debug = format!("{:?}", candidate);
        assert!(debug.contains("测试"));
    }

    #[test]
    fn test_commit_text_preview_none() {
        // Create a minimal RimeContext with null commit_text_preview
        let inner = librime_sys2::RimeContext {
            data_size: std::mem::size_of::<librime_sys2::RimeContext>() as std::os::raw::c_int,
            composition: librime_sys2::RimeComposition {
                length: 0,
                cursor_pos: 0,
                sel_start: 0,
                sel_end: 0,
                preedit: std::ptr::null_mut(),
            },
            menu: librime_sys2::RimeMenu {
                page_size: 0,
                page_no: 0,
                is_last_page: 0,
                highlighted_candidate_index: 0,
                num_candidates: 0,
                candidates: std::ptr::null_mut(),
                select_keys: std::ptr::null_mut(),
            },
            commit_text_preview: std::ptr::null_mut(),
            select_labels: std::ptr::null_mut(),
        };
        let ctx = Context::new(inner);
        assert!(ctx.commit_text_preview().is_none());
    }

    #[test]
    fn test_raw_access() {
        let inner = librime_sys2::RimeContext {
            data_size: std::mem::size_of::<librime_sys2::RimeContext>() as std::os::raw::c_int,
            composition: librime_sys2::RimeComposition {
                length: 0,
                cursor_pos: 0,
                sel_start: 0,
                sel_end: 0,
                preedit: std::ptr::null_mut(),
            },
            menu: librime_sys2::RimeMenu {
                page_size: 0,
                page_no: 0,
                is_last_page: 0,
                highlighted_candidate_index: 0,
                num_candidates: 0,
                candidates: std::ptr::null_mut(),
                select_keys: std::ptr::null_mut(),
            },
            commit_text_preview: std::ptr::null_mut(),
            select_labels: std::ptr::null_mut(),
        };
        let ctx = Context::new(inner);
        let raw = ctx.raw();
        assert_eq!(
            raw.data_size,
            std::mem::size_of::<librime_sys2::RimeContext>() as std::os::raw::c_int
        );
    }
}
