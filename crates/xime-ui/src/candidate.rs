#[derive(Debug, Clone, Default)]
pub struct CandidateItem {
    pub text: String,
    pub comment: String,
    pub index: usize,
}

#[derive(Debug, Clone)]
pub struct CandidateList {
    items: Vec<CandidateItem>,
    page_size: usize,
    current_page: usize,
    highlighted_index: usize,
    select_keys: String,
}

impl CandidateList {
    pub fn new(page_size: usize) -> Self {
        Self {
            items: Vec::new(),
            page_size,
            current_page: 0,
            highlighted_index: 0,
            select_keys: "12345".to_string(),
        }
    }

    pub fn set_candidates(&mut self, texts: Vec<(&str, Option<&str>)>, select_keys: Option<&str>) {
        self.items = texts.iter().enumerate().map(|(i, (text, comment))| {
            CandidateItem {
                text: text.to_string(),
                comment: comment.map(|c| c.to_string()).unwrap_or_default(),
                index: i,
            }
        }).collect();
        
        if let Some(keys) = select_keys {
            self.select_keys = keys.to_string();
        }
        
        self.current_page = 0;
        self.highlighted_index = 0;
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.current_page = 0;
        self.highlighted_index = 0;
    }

    pub fn page_up(&mut self) {
        if self.current_page > 0 {
            self.current_page -= 1;
            self.highlighted_index = 0;
        }
    }

    pub fn page_down(&mut self) {
        let total_pages = (self.items.len() + self.page_size - 1) / self.page_size;
        if self.current_page < total_pages - 1 {
            self.current_page += 1;
            self.highlighted_index = 0;
        }
    }

    pub fn select_highlighted(&self) -> Option<&str> {
        let idx = self.current_page * self.page_size + self.highlighted_index;
        self.items.get(idx).map(|item| item.text.as_str())
    }

    pub fn select_by_key(&mut self, key: char) -> Option<&str> {
        let pos = self.select_keys.find(key);
        if let Some(pos) = pos {
            let idx = self.current_page * self.page_size + pos;
            self.items.get(idx).map(|item| item.text.as_str())
        } else {
            None
        }
    }

    pub fn move_highlight(&mut self, direction: MoveDirection) {
        match direction {
            MoveDirection::Up => {
                if self.highlighted_index > 0 {
                    self.highlighted_index -= 1;
                }
            }
            MoveDirection::Down => {
                let page_items = self.get_page_items().len();
                if self.highlighted_index < page_items - 1 {
                    self.highlighted_index += 1;
                }
            }
        }
    }

    fn get_page_items(&self) -> &[CandidateItem] {
        let start = self.current_page * self.page_size;
        let end = std::cmp::min(start + self.page_size, self.items.len());
        &self.items[start..end]
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn get_page_info(&self) -> PageInfo {
        PageInfo {
            page_size: self.page_size,
            current_page: self.current_page,
            total_pages: (self.items.len() + self.page_size - 1) / self.page_size,
            is_last_page: self.current_page >= (self.items.len() + self.page_size - 1) / self.page_size - 1,
            highlighted_index: self.highlighted_index,
            select_keys: self.select_keys.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MoveDirection {
    Up,
    Down,
}

#[derive(Debug, Clone)]
pub struct PageInfo {
    pub page_size: usize,
    pub current_page: usize,
    pub total_pages: usize,
    pub is_last_page: bool,
    pub highlighted_index: usize,
    pub select_keys: String,
}