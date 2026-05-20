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
